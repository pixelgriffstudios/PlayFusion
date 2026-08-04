#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/input.h>
#include <linux/uinput.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

#define HOLD_MILLISECONDS 1000
#define MENU_HOLD_MILLISECONDS 450

static volatile sig_atomic_t running = 1;

static void stop_running(int signal_number) {
    (void)signal_number;
    running = 0;
}

static int read_name(int number, char *buffer, size_t size) {
    char path[128];
    snprintf(path, sizeof(path), "/sys/class/input/event%d/device/name", number);
    FILE *file = fopen(path, "r");
    if (!file) return -1;
    if (!fgets(buffer, (int)size, file)) {
        fclose(file);
        return -1;
    }
    fclose(file);
    buffer[strcspn(buffer, "\r\n")] = '\0';
    return 0;
}

static int supports_key(int fd, int key) {
    unsigned long bits[(KEY_MAX + (8 * sizeof(unsigned long))) /
                       (8 * sizeof(unsigned long))];
    memset(bits, 0, sizeof(bits));
    if (ioctl(fd, EVIOCGBIT(EV_KEY, sizeof(bits)), bits) < 0) return 0;
    const size_t word_bits = 8 * sizeof(unsigned long);
    return (bits[key / word_bits] >> (key % word_bits)) & 1UL;
}

static int open_controller(char *selected_path, size_t selected_path_size) {
    int selected_fd = -1;
    selected_path[0] = '\0';
    for (int number = 0; number < 128; number++) {
        char name[256] = {0};
        if (read_name(number, name, sizeof(name)) != 0) continue;
        if (!strstr(name, "X-Box 360") && !strstr(name, "Xbox 360") &&
            !strstr(name, "Xbox One") && !strstr(name, "Gamepad")) {
            continue;
        }
        char path[64];
        snprintf(path, sizeof(path), "/dev/input/event%d", number);
        int fd = open(path, O_RDONLY | O_NONBLOCK);
        if (fd < 0) continue;
        if (!supports_key(fd, BTN_SELECT) || !supports_key(fd, BTN_START)) {
            close(fd);
            continue;
        }
        if (selected_fd >= 0) close(selected_fd);
        selected_fd = fd;
        snprintf(selected_path, selected_path_size, "%s", path);
    }
    return selected_fd;
}

static void emit_event(int fd, unsigned short type, unsigned short code, int value) {
    struct input_event event;
    memset(&event, 0, sizeof(event));
    gettimeofday(&event.time, NULL);
    event.type = type;
    event.code = code;
    event.value = value;
    (void)write(fd, &event, sizeof(event));
}

static int create_keyboard(void) {
    int fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
    if (fd < 0) return -1;
    if (ioctl(fd, UI_SET_EVBIT, EV_KEY) < 0 ||
        ioctl(fd, UI_SET_KEYBIT, KEY_ESC) < 0 ||
        ioctl(fd, UI_SET_KEYBIT, KEY_F1) < 0 ||
        ioctl(fd, UI_SET_EVBIT, EV_SYN) < 0) {
        close(fd);
        return -1;
    }
    struct uinput_setup setup;
    memset(&setup, 0, sizeof(setup));
    snprintf(setup.name, UINPUT_MAX_NAME_SIZE, "PlayFusion Game Exit Hotkey");
    setup.id.bustype = BUS_USB;
    setup.id.vendor = 0x5046;
    setup.id.product = 0x0002;
    setup.id.version = 1;
    if (ioctl(fd, UI_DEV_SETUP, &setup) < 0 || ioctl(fd, UI_DEV_CREATE) < 0) {
        close(fd);
        return -1;
    }
    usleep(250000);
    return fd;
}

static long elapsed_milliseconds(const struct timespec *start) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (now.tv_sec - start->tv_sec) * 1000L +
           (now.tv_nsec - start->tv_nsec) / 1000000L;
}

static void send_key(int keyboard, unsigned short key) {
    emit_event(keyboard, EV_KEY, key, 1);
    emit_event(keyboard, EV_SYN, SYN_REPORT, 0);
    usleep(30000);
    emit_event(keyboard, EV_KEY, key, 0);
    emit_event(keyboard, EV_SYN, SYN_REPORT, 0);
}

int main(int argc, char **argv) {
    signal(SIGINT, stop_running);
    signal(SIGTERM, stop_running);
    signal(SIGHUP, stop_running);

    int keyboard = create_keyboard();
    if (keyboard < 0) {
        fprintf(stderr, "Unable to create PlayFusion exit keyboard: %s\n", strerror(errno));
        return 2;
    }

    int retroarch_menu = argc > 1 && strcmp(argv[1], "--retroarch-menu") == 0;
    int controller = -1;
    char controller_path[64] = {0};
    int select_down = 0;
    int start_down = 0;
    int south_down = 0;
    int chord_started = 0;
    int chord_fired = 0;
    struct timespec chord_time = {0};
    int menu_chord_started = 0;
    int menu_chord_fired = 0;
    struct timespec menu_chord_time = {0};

    while (running) {
        if (controller < 0) {
            controller = open_controller(controller_path, sizeof(controller_path));
            if (controller < 0) {
                usleep(500000);
                continue;
            }
            fprintf(stderr, "Watching %s: hold View/Select + Menu/Start to exit.\n",
                    controller_path);
        }

        struct pollfd poll_fd = {.fd = controller, .events = POLLIN};
        int ready = poll(&poll_fd, 1, 20);
        if (ready < 0 && errno != EINTR) break;
        if (poll_fd.revents & (POLLERR | POLLHUP | POLLNVAL)) {
            close(controller);
            controller = -1;
            select_down = start_down = south_down = 0;
            chord_started = chord_fired = 0;
            menu_chord_started = menu_chord_fired = 0;
            continue;
        }
        if (ready > 0 && (poll_fd.revents & POLLIN)) {
            struct input_event events[32];
            ssize_t bytes;
            while ((bytes = read(controller, events, sizeof(events))) > 0) {
                size_t count = (size_t)bytes / sizeof(events[0]);
                for (size_t index = 0; index < count; index++) {
                    const struct input_event *event = &events[index];
                    if (event->type != EV_KEY) continue;
                    if (event->code == BTN_SELECT) select_down = event->value != 0;
                    if (event->code == BTN_START) start_down = event->value != 0;
                    if (event->code == BTN_SOUTH) south_down = event->value != 0;
                }
            }
        }

        if (select_down && start_down) {
            if (!chord_started) {
                clock_gettime(CLOCK_MONOTONIC, &chord_time);
                chord_started = 1;
                chord_fired = 0;
            } else if (!chord_fired &&
                       elapsed_milliseconds(&chord_time) >= HOLD_MILLISECONDS) {
                send_key(keyboard, KEY_ESC);
                chord_fired = 1;
            }
        } else {
            chord_started = 0;
            chord_fired = 0;
        }

        if (retroarch_menu && select_down && south_down) {
            if (!menu_chord_started) {
                clock_gettime(CLOCK_MONOTONIC, &menu_chord_time);
                menu_chord_started = 1;
                menu_chord_fired = 0;
            } else if (!menu_chord_fired &&
                       elapsed_milliseconds(&menu_chord_time) >= MENU_HOLD_MILLISECONDS) {
                send_key(keyboard, KEY_F1);
                menu_chord_fired = 1;
            }
        } else {
            menu_chord_started = 0;
            menu_chord_fired = 0;
        }
    }

    if (controller >= 0) close(controller);
    ioctl(keyboard, UI_DEV_DESTROY);
    close(keyboard);
    return 0;
}
