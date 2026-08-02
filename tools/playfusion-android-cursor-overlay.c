#define _GNU_SOURCE
#include <X11/Xlib.h>
#include <X11/extensions/shape.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#define CURSOR_MAGIC 0x50464355u
#define SIZE 34

struct cursor_state {
    uint32_t magic;
    int32_t x;
    int32_t y;
    int32_t visible;
    int32_t touching;
};

static void pause_ms(long milliseconds) {
    struct timespec delay = {milliseconds / 1000, (milliseconds % 1000) * 1000000L};
    nanosleep(&delay, NULL);
}

int main(void) {
    const char *runtime_dir = getenv("XDG_RUNTIME_DIR");
    char state_path[256];
    if (!runtime_dir || !*runtime_dir)
        snprintf(state_path, sizeof(state_path), "/run/user/%u/playfusion-android-cursor.state",
                 (unsigned)getuid());
    else
        snprintf(state_path, sizeof(state_path), "%s/playfusion-android-cursor.state", runtime_dir);

    int state_fd = -1;
    for (int attempt = 0; attempt < 300 && state_fd < 0; attempt++) {
        state_fd = open(state_path, O_RDONLY);
        if (state_fd < 0) pause_ms(10);
    }
    if (state_fd < 0) return 2;
    struct cursor_state *state = mmap(NULL, sizeof(*state), PROT_READ, MAP_SHARED, state_fd, 0);
    if (state == MAP_FAILED) { close(state_fd); return 3; }

    Display *display = NULL;
    for (int attempt = 0; attempt < 300 && !display; attempt++) {
        display = XOpenDisplay(NULL);
        if (!display) pause_ms(10);
    }
    if (!display) { munmap(state, sizeof(*state)); close(state_fd); return 4; }

    int screen = DefaultScreen(display);
    Window root = RootWindow(display, screen);
    XSetWindowAttributes attrs;
    memset(&attrs, 0, sizeof(attrs));
    attrs.override_redirect = True;
    attrs.background_pixel = BlackPixel(display, screen);
    attrs.save_under = True;
    Window window = XCreateWindow(display, root, 0, 0, SIZE, SIZE, 0,
                                  CopyFromParent, InputOutput, CopyFromParent,
                                  CWOverrideRedirect | CWBackPixel | CWSaveUnder, &attrs);

    Pixmap shape = XCreatePixmap(display, window, SIZE, SIZE, 1);
    GC shape_gc = XCreateGC(display, shape, 0, NULL);
    XSetForeground(display, shape_gc, 0);
    XFillRectangle(display, shape, shape_gc, 0, 0, SIZE, SIZE);
    XSetForeground(display, shape_gc, 1);
    XPoint arrow[] = {{1,1},{1,29},{8,22},{13,33},{19,30},{14,19},{27,19}};
    XFillPolygon(display, shape, shape_gc, arrow, 7, Complex, CoordModeOrigin);
    XShapeCombineMask(display, window, ShapeBounding, 0, 0, shape, ShapeSet);

    Pixmap empty_input = XCreatePixmap(display, window, SIZE, SIZE, 1);
    GC input_gc = XCreateGC(display, empty_input, 0, NULL);
    XSetForeground(display, input_gc, 0);
    XFillRectangle(display, empty_input, input_gc, 0, 0, SIZE, SIZE);
    XShapeCombineMask(display, window, ShapeInput, 0, 0, empty_input, ShapeSet);

    Colormap cmap = DefaultColormap(display, screen);
    XColor cyan, exact;
    if (!XAllocNamedColor(display, cmap, "#00E7FF", &cyan, &exact))
        cyan.pixel = WhitePixel(display, screen);
    GC draw_gc = XCreateGC(display, window, 0, NULL);
    XSetForeground(display, draw_gc, cyan.pixel);
    XMapRaised(display, window);

    int last_x = -1, last_y = -1;
    while (state->magic == CURSOR_MAGIC && state->visible) {
        int x = state->x, y = state->y;
        if (x != last_x || y != last_y) {
            XMoveWindow(display, window, x, y);
            XRaiseWindow(display, window);
            last_x = x; last_y = y;
        }
        XClearWindow(display, window);
        XFillPolygon(display, window, draw_gc, arrow, 7, Complex, CoordModeOrigin);
        XFlush(display);
        pause_ms(16);
    }

    XDestroyWindow(display, window);
    XFreeGC(display, draw_gc);
    XFreeGC(display, shape_gc);
    XFreeGC(display, input_gc);
    XFreePixmap(display, shape);
    XFreePixmap(display, empty_input);
    XCloseDisplay(display);
    munmap(state, sizeof(*state));
    close(state_fd);
    return 0;
}
