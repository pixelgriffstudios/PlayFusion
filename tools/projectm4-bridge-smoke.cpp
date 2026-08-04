#include <GL/gl.h>
#include <GL/glx.h>
#include <X11/Xlib.h>
#include <dlfcn.h>

#include <chrono>
#include <cstdint>
#include <fstream>
#include <iostream>
#include <thread>
#include <vector>

int main() {
    Display* display = XOpenDisplay(nullptr);
    if (!display) {
        std::cerr << "cannot open display\n";
        return 2;
    }
    const int attributes[] = {GLX_RGBA, GLX_DOUBLEBUFFER, GLX_DEPTH_SIZE, 24, None};
    XVisualInfo* visual = glXChooseVisual(display, DefaultScreen(display), const_cast<int*>(attributes));
    if (!visual) {
        std::cerr << "cannot choose visual\n";
        return 3;
    }
    Colormap colormap = XCreateColormap(
        display, RootWindow(display, visual->screen), visual->visual, AllocNone
    );
    XSetWindowAttributes window_attributes{};
    window_attributes.colormap = colormap;
    window_attributes.event_mask = StructureNotifyMask;
    Window window = XCreateWindow(
        display, RootWindow(display, visual->screen), 0, 0, 640, 360, 0,
        visual->depth, InputOutput, visual->visual, CWColormap | CWEventMask,
        &window_attributes
    );
    GLXContext context = glXCreateContext(display, visual, nullptr, True);
    XFree(visual);
    if (!window || !context || !glXMakeCurrent(display, window, context)) {
        std::cerr << "cannot activate main test context\n";
        return 4;
    }

    void* library = dlopen("/tmp/libplayfusion-projectm-native-4.so", RTLD_NOW);
    if (!library) {
        std::cerr << dlerror() << '\n';
        return 5;
    }
    using Create = void* (*)(const char*, const char*, int, int);
    using Render = void (*)(void*);
    using Texture = unsigned int (*)(void*);
    using Destroy = void (*)(void*);
    auto create = reinterpret_cast<Create>(dlsym(library, "playfusion_projectm_create"));
    auto render = reinterpret_cast<Render>(dlsym(library, "playfusion_projectm_render"));
    auto texture = reinterpret_cast<Texture>(dlsym(library, "playfusion_projectm_texture"));
    auto destroy = reinterpret_cast<Destroy>(dlsym(library, "playfusion_projectm_destroy"));

    void* instance = create(
        "/var/kazeta/state/projectm-home/.projectM/config.inp", nullptr, 640, 360
    );
    if (!instance) {
        std::cerr << "bridge create returned null\n";
        return 6;
    }
    for (int frame = 0; frame < 90; ++frame) {
        render(instance);
        std::this_thread::sleep_for(std::chrono::milliseconds(16));
    }
    const GLuint texture_id = texture(instance);
    if (!glIsTexture(texture_id)) {
        std::cerr << "shared output texture is not visible in main context\n";
        return 7;
    }
    std::vector<std::uint8_t> pixels(640 * 360 * 4);
    glBindTexture(GL_TEXTURE_2D, texture_id);
    glGetTexImage(GL_TEXTURE_2D, 0, GL_RGBA, GL_UNSIGNED_BYTE, pixels.data());
    std::uint64_t sample_sum = 0;
    for (std::size_t index = 0; index < pixels.size(); index += 4096) {
        sample_sum += pixels[index];
    }
    std::ofstream image("/tmp/projectm4-smoke.ppm", std::ios::binary);
    image << "P6\n640 360\n255\n";
    for (int y = 359; y >= 0; --y) {
        for (int x = 0; x < 640; ++x) {
            const std::size_t index = static_cast<std::size_t>((y * 640 + x) * 4);
            image.write(reinterpret_cast<const char*>(&pixels[index]), 3);
        }
    }
    std::cout << "texture=" << texture_id << " sample_sum=" << sample_sum << '\n';
    destroy(instance);
    dlclose(library);
    glXMakeCurrent(display, None, nullptr);
    glXDestroyContext(display, context);
    XDestroyWindow(display, window);
    XFreeColormap(display, colormap);
    XCloseDisplay(display);
    return sample_sum == 0 ? 8 : 0;
}
