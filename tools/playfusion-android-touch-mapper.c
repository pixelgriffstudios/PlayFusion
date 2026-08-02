#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/input.h>
#include <linux/uinput.h>
#include <math.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/poll.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#define MAX_KEYBOARDS 16
#define CURSOR_MAGIC 0x50464355u
#define CURSOR_WIDTH 1920
#define CURSOR_HEIGHT 1080

struct cursor_state {
    uint32_t magic;
    int32_t x;
    int32_t y;
    int32_t visible;
    int32_t touching;
};

static volatile sig_atomic_t running = 1;
static void stop_running(int signal_number) { (void)signal_number; running = 0; }

static int read_name(int number, char *buffer, size_t size) {
    char path[128];
    snprintf(path, sizeof(path), "/sys/class/input/event%d/device/name", number);
    FILE *file = fopen(path, "r");
    if (!file) return -1;
    if (!fgets(buffer, (int)size, file)) { fclose(file); return -1; }
    fclose(file);
    buffer[strcspn(buffer, "\r\n")] = '\0';
    return 0;
}

static int find_controller(void) {
    int selected = -1;
    for (int number = 0; number < 128; number++) {
        char name[256] = {0};
        if (read_name(number, name, sizeof(name)) != 0) continue;
        if (strstr(name, "X-Box 360") || strstr(name, "Xbox 360") ||
            strstr(name, "Xbox One") || strstr(name, "Gamepad")) {
            selected = number;
        }
    }
    return selected;
}

static int supports_key(int fd, int key) {
    unsigned long bits[(KEY_MAX + (8 * sizeof(unsigned long))) /
                       (8 * sizeof(unsigned long))];
    memset(bits, 0, sizeof(bits));
    if (ioctl(fd, EVIOCGBIT(EV_KEY, sizeof(bits)), bits) < 0) return 0;
    const size_t word_bits = 8 * sizeof(unsigned long);
    return (bits[key / word_bits] >> (key % word_bits)) & 1UL;
}

static size_t open_keyboards(int *fds, size_t capacity) {
    size_t count = 0;
    for (int number = 0; number < 128 && count < capacity; number++) {
        char path[64];
        snprintf(path, sizeof(path), "/dev/input/event%d", number);
        int fd = open(path, O_RDONLY | O_NONBLOCK);
        if (fd < 0) continue;
        if (supports_key(fd, KEY_ESC)) {
            fds[count++] = fd;
        } else {
            close(fd);
        }
    }
    return count;
}

static void emit_event(int fd, unsigned short type, unsigned short code, int value) {
    struct input_event event;
    memset(&event, 0, sizeof(event));
    gettimeofday(&event.time, NULL);
    event.type = type; event.code = code; event.value = value;
    (void)write(fd, &event, sizeof(event));
}

static int create_pointer(void) {
    int fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
    if (fd < 0) return -1;
    ioctl(fd, UI_SET_EVBIT, EV_KEY);
    ioctl(fd, UI_SET_KEYBIT, BTN_LEFT);
    ioctl(fd, UI_SET_EVBIT, EV_REL);
    ioctl(fd, UI_SET_RELBIT, REL_X);
    ioctl(fd, UI_SET_RELBIT, REL_Y);
    struct uinput_setup setup;
    memset(&setup, 0, sizeof(setup));
    snprintf(setup.name, UINPUT_MAX_NAME_SIZE, "PlayFusion Android Touch Pointer");
    setup.id.bustype = BUS_USB; setup.id.vendor = 0x5046;
    setup.id.product = 0x0001; setup.id.version = 1;
    if (ioctl(fd, UI_DEV_SETUP, &setup) < 0 || ioctl(fd, UI_DEV_CREATE) < 0) {
        close(fd); return -1;
    }
    usleep(250000);
    return fd;
}

static int scaled_motion(int value, int minimum, int maximum) {
    const double deadzone = 0.18;
    int span = abs(minimum) > abs(maximum) ? abs(minimum) : abs(maximum);
    if (span < 1) span = 32767;
    double normalized = (double)value / (double)span;
    double magnitude = fabs(normalized);
    if (magnitude <= deadzone) return 0;
    magnitude = (magnitude - deadzone) / (1.0 - deadzone);
    /* A television UI needs deliberate, controller-friendly movement.  The
       previous 19 px/tick peak crossed 1080p in under a second. */
    int result = (int)lround(pow(magnitude, 1.65) * 4.0);
    if (result < 1) result = 1;
    return normalized < 0.0 ? -result : result;
}

static void request_exit(void) {
    pid_t pid = fork();
    if (pid == 0) {
        execl("/usr/bin/playfusion-waydroid-exit",
              "playfusion-waydroid-exit", (char *)NULL);
        _exit(127);
    }
}

static void request_back(void) {
    pid_t pid = fork();
    if (pid == 0) {
        execl("/usr/bin/playfusion-waydroid-back",
              "playfusion-waydroid-back", (char *)NULL);
        _exit(127);
    }
}

int main(int argc, char **argv) {
    signal(SIGINT, stop_running); signal(SIGTERM, stop_running);
    signal(SIGHUP, stop_running);
    int touch_mode = !(argc > 1 && strcmp(argv[1], "--exit-only") == 0);
    int controller = -1, pointer = -1, state_fd = -1;
    struct cursor_state *cursor = MAP_FAILED;
    char event_path[64] = {0};
    if (touch_mode) {
        int event_number = find_controller();
        if (event_number < 0) {
            fprintf(stderr, "No Xbox-compatible controller found.\n"); return 2;
        }
        snprintf(event_path, sizeof(event_path), "/dev/input/event%d", event_number);
        controller = open(event_path, O_RDONLY | O_NONBLOCK);
        if (controller < 0) {
            fprintf(stderr, "Unable to open %s: %s\n", event_path, strerror(errno));
            return 3;
        }
        pointer = create_pointer();
        if (pointer < 0) {
            fprintf(stderr, "Unable to create touch pointer: %s\n", strerror(errno));
            close(controller); return 4;
        }
        const char *runtime_dir = getenv("XDG_RUNTIME_DIR");
        char state_path[256];
        if (!runtime_dir || !*runtime_dir) {
            snprintf(state_path, sizeof(state_path), "/run/user/%u/playfusion-android-cursor.state",
                     (unsigned)getuid());
        } else {
            snprintf(state_path, sizeof(state_path), "%s/playfusion-android-cursor.state", runtime_dir);
        }
        state_fd = open(state_path, O_CREAT | O_RDWR | O_TRUNC, 0600);
        if (state_fd >= 0 && ftruncate(state_fd, (off_t)sizeof(*cursor)) == 0) {
            cursor = mmap(NULL, sizeof(*cursor), PROT_READ | PROT_WRITE,
                          MAP_SHARED, state_fd, 0);
        }
        if (cursor != MAP_FAILED) {
            cursor->magic = CURSOR_MAGIC;
            cursor->x = CURSOR_WIDTH / 2;
            cursor->y = CURSOR_HEIGHT / 2;
            cursor->visible = 1;
            cursor->touching = 0;
        }
    }
    int keyboards[MAX_KEYBOARDS];
    size_t keyboard_count = open_keyboards(keyboards, MAX_KEYBOARDS);
    struct input_absinfo lx_info = {.minimum = -32768, .maximum = 32767};
    struct input_absinfo ly_info = {.minimum = -32768, .maximum = 32767};
    if (controller >= 0) {
        ioctl(controller, EVIOCGABS(ABS_X), &lx_info);
        ioctl(controller, EVIOCGABS(ABS_Y), &ly_info);
    }
    int lx = 0, ly = 0, touching = 0;
    struct pollfd poll_fds[1 + MAX_KEYBOARDS];
    size_t poll_count = 0;
    if (controller >= 0)
        poll_fds[poll_count++] = (struct pollfd){.fd = controller, .events = POLLIN};
    for (size_t index = 0; index < keyboard_count; index++)
        poll_fds[poll_count++] = (struct pollfd){.fd = keyboards[index], .events = POLLIN};
    fprintf(stderr, "Touch=%s controller=%s keyboards=%zu.\n",
            touch_mode ? "yes" : "no", touch_mode ? event_path : "native", keyboard_count);
    while (running) {
        int ready = poll(poll_fds, poll_count, 16);
        if (ready < 0 && errno != EINTR) break;
        if (ready > 0 && controller >= 0 && (poll_fds[0].revents & POLLIN)) {
            struct input_event events[32];
            ssize_t bytes;
            while ((bytes = read(controller, events, sizeof(events))) > 0) {
                size_t count = (size_t)bytes / sizeof(events[0]);
                for (size_t index = 0; index < count; index++) {
                    struct input_event *event = &events[index];
                    if (event->type == EV_ABS && event->code == ABS_X) lx = event->value;
                    if (event->type == EV_ABS && event->code == ABS_Y) ly = event->value;
                    if (event->type == EV_KEY && event->code == BTN_SOUTH) {
                        touching = event->value != 0;
                        if (cursor != MAP_FAILED) cursor->touching = touching;
                        emit_event(pointer, EV_KEY, BTN_LEFT, touching);
                        emit_event(pointer, EV_SYN, SYN_REPORT, 0);
                    }
                    if (event->type == EV_KEY && event->code == BTN_EAST &&
                        event->value == 1) {
                        request_back();
                    }
                }
            }
        }
        size_t keyboard_start = controller >= 0 ? 1 : 0;
        for (size_t index = keyboard_start; index < poll_count; index++) {
            if (!(poll_fds[index].revents & POLLIN)) continue;
            struct input_event events[16];
            ssize_t bytes;
            while ((bytes = read(poll_fds[index].fd, events, sizeof(events))) > 0) {
                size_t count = (size_t)bytes / sizeof(events[0]);
                for (size_t event_index = 0; event_index < count; event_index++) {
                    if (events[event_index].type == EV_KEY &&
                        events[event_index].code == KEY_ESC &&
                        events[event_index].value == 1) {
                        request_exit();
                        running = 0;
                    }
                }
            }
        }
        int dx = scaled_motion(lx, lx_info.minimum, lx_info.maximum);
        int dy = scaled_motion(ly, ly_info.minimum, ly_info.maximum);
        if (dx || dy) {
            if (cursor != MAP_FAILED) {
                cursor->x += dx;
                cursor->y += dy;
                if (cursor->x < 0) cursor->x = 0;
                if (cursor->y < 0) cursor->y = 0;
                if (cursor->x >= CURSOR_WIDTH) cursor->x = CURSOR_WIDTH - 1;
                if (cursor->y >= CURSOR_HEIGHT) cursor->y = CURSOR_HEIGHT - 1;
            }
            emit_event(pointer, EV_REL, REL_X, dx);
            emit_event(pointer, EV_REL, REL_Y, dy);
            emit_event(pointer, EV_SYN, SYN_REPORT, 0);
        }
    }
    if (touching) {
        emit_event(pointer, EV_KEY, BTN_LEFT, 0);
        emit_event(pointer, EV_SYN, SYN_REPORT, 0);
    }
    if (cursor != MAP_FAILED) {
        cursor->visible = 0;
        munmap(cursor, sizeof(*cursor));
    }
    if (state_fd >= 0) close(state_fd);
    if (pointer >= 0) ioctl(pointer, UI_DEV_DESTROY);
    if (pointer >= 0) close(pointer);
    if (controller >= 0) close(controller);
    for (size_t index = 0; index < keyboard_count; index++) close(keyboards[index]);
    return 0;
}
