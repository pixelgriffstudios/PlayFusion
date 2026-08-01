#include <X11/Xlib.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static unsigned long number(const char *value, const char *label) {
    char *end = NULL;
    errno = 0;
    unsigned long result = strtoul(value, &end, 0);
    if (errno || !end || *end != '\0') {
        fprintf(stderr, "Invalid %s: %s\n", label, value);
        exit(2);
    }
    return result;
}

int main(int argc, char **argv) {
    if (argc != 7) {
        fprintf(stderr, "Usage: %s CHILD PARENT X Y WIDTH HEIGHT\n", argv[0]);
        return 2;
    }
    Display *display = XOpenDisplay(NULL);
    if (!display) {
        fprintf(stderr, "Unable to open X display\n");
        return 1;
    }

    Window child = (Window)number(argv[1], "child window");
    Window parent = (Window)number(argv[2], "parent window");
    int x = (int)number(argv[3], "x");
    int y = (int)number(argv[4], "y");
    unsigned int width = (unsigned int)number(argv[5], "width");
    unsigned int height = (unsigned int)number(argv[6], "height");

    XUnmapWindow(display, child);
    XReparentWindow(display, child, parent, x, y);
    XMoveResizeWindow(
        display,
        child,
        x,
        y,
        width > 1 ? width - 1 : width,
        height > 1 ? height - 1 : height);
    XMapRaised(display, child);
    XSync(display, False);
    usleep(30000);
    XMoveResizeWindow(display, child, x, y, width, height);
    XSetInputFocus(display, parent, RevertToParent, CurrentTime);
    XSync(display, False);
    XCloseDisplay(display);
    return 0;
}
