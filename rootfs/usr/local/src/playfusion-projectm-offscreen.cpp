#include <QGuiApplication>
#include <QOffscreenSurface>
#include <QOpenGLContext>
#include <QOpenGLFramebufferObject>
#include <QOpenGLFunctions>
#include <QSurfaceFormat>

#include <libprojectM/projectM.hpp>
#include <pulse/error.h>
#include <pulse/simple.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <string>
#include <thread>
#include <vector>

#include <fcntl.h>
#include <new>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

namespace {
constexpr int kWidth = 1280;
constexpr int kHeight = 720;
constexpr int kChannels = 2;
constexpr int kFramesPerRead = 512;
constexpr int kCornerRadius = 44;
constexpr std::size_t kHeaderSize = 24;

struct SharedHeader {
    char magic[4];
    std::uint32_t width;
    std::uint32_t height;
    std::uint32_t reserved;
    alignas(8) std::atomic<std::uint64_t> sequence;
};
static_assert(sizeof(SharedHeader) == kHeaderSize);

bool inside_rounded_rect(int x, int y) {
    const int radius = kCornerRadius;
    const int nearest_x = std::clamp(x, radius, kWidth - radius - 1);
    const int nearest_y = std::clamp(y, radius, kHeight - radius - 1);
    const int dx = x - nearest_x;
    const int dy = y - nearest_y;
    return dx * dx + dy * dy <= radius * radius;
}

}  // namespace

int main(int argc, char** argv) {
    if (argc < 3) {
        std::fprintf(
            stderr,
            "usage: %s CONFIG FRAME_FILE [PULSE_MONITOR]\n",
            argv[0]
        );
        return 2;
    }

    const std::string config_path = argv[1];
    const std::string frame_path = argv[2];
    const char* monitor = argc >= 4 && argv[3][0] != '\0' ? argv[3] : nullptr;

    QCoreApplication::setAttribute(Qt::AA_UseDesktopOpenGL);
    QGuiApplication application(argc, argv);

    const std::size_t pixel_bytes =
        static_cast<std::size_t>(kWidth) * kHeight * 4;
    const std::size_t mapping_bytes = kHeaderSize + pixel_bytes;
    const int frame_fd = ::open(
        frame_path.c_str(),
        O_RDWR | O_CREAT | O_TRUNC | O_CLOEXEC,
        0600
    );
    if (frame_fd < 0 || ::ftruncate(frame_fd, mapping_bytes) != 0) {
        std::fprintf(stderr, "unable to create shared projectM frame\n");
        return 7;
    }
    void* mapping = ::mmap(
        nullptr,
        mapping_bytes,
        PROT_READ | PROT_WRITE,
        MAP_SHARED,
        frame_fd,
        0
    );
    if (mapping == MAP_FAILED) {
        std::fprintf(stderr, "unable to map shared projectM frame\n");
        ::close(frame_fd);
        return 8;
    }
    auto* header = static_cast<SharedHeader*>(mapping);
    std::memcpy(header->magic, "PFPM", 4);
    header->width = kWidth;
    header->height = kHeight;
    header->reserved = 0;
    new (&header->sequence) std::atomic<std::uint64_t>(0);
    auto* pixels =
        reinterpret_cast<std::uint8_t*>(mapping) + kHeaderSize;

    QSurfaceFormat format;
    format.setRenderableType(QSurfaceFormat::OpenGL);
    format.setVersion(2, 1);
    format.setProfile(QSurfaceFormat::CompatibilityProfile);
    format.setDepthBufferSize(24);
    format.setStencilBufferSize(8);

    QOpenGLContext context;
    context.setFormat(format);
    if (!context.create()) {
        std::fprintf(stderr, "unable to create off-screen OpenGL context\n");
        return 3;
    }

    QOffscreenSurface surface;
    surface.setFormat(context.format());
    surface.create();
    if (!surface.isValid() || !context.makeCurrent(&surface)) {
        std::fprintf(stderr, "unable to activate off-screen OpenGL surface\n");
        return 4;
    }

    QOpenGLFramebufferObjectFormat framebuffer_format;
    framebuffer_format.setAttachment(QOpenGLFramebufferObject::CombinedDepthStencil);
    framebuffer_format.setInternalTextureFormat(GL_RGBA8);
    QOpenGLFramebufferObject framebuffer(kWidth, kHeight, framebuffer_format);
    if (!framebuffer.isValid() || !framebuffer.bind()) {
        std::fprintf(stderr, "unable to create projectM framebuffer\n");
        return 5;
    }

    projectM visualizer(config_path);
    visualizer.projectM_resetGL(kWidth, kHeight);

    pa_sample_spec sample_spec{};
    sample_spec.format = PA_SAMPLE_S16LE;
    sample_spec.rate = 48000;
    sample_spec.channels = kChannels;
    int pulse_error = 0;
    pa_simple* pulse = pa_simple_new(
        nullptr,
        "PlayFusion projectM",
        PA_STREAM_RECORD,
        monitor,
        "cabinet visualization",
        &sample_spec,
        nullptr,
        nullptr,
        &pulse_error
    );
    if (pulse == nullptr) {
        std::fprintf(
            stderr,
            "unable to open PulseAudio monitor: %s\n",
            pa_strerror(pulse_error)
        );
        return 6;
    }

    std::array<std::int16_t, kFramesPerRead * kChannels> audio{};
    int audio_reads = 0;

    while (true) {
        if (pa_simple_read(
                pulse,
                audio.data(),
                audio.size() * sizeof(std::int16_t),
                &pulse_error
            ) < 0) {
            std::fprintf(
                stderr,
                "PulseAudio read failed: %s\n",
                pa_strerror(pulse_error)
            );
            break;
        }

        visualizer.pcm()->addPCM16Data(audio.data(), kFramesPerRead);
        // Two 512-frame monitor reads yield about 46.9 visual frames per
        // second at 48 kHz. This is notably smoother in the cabinet while
        // leaving headroom for the PlayFusion UI on Vega integrated graphics.
        if (++audio_reads < 2) {
            continue;
        }
        audio_reads = 0;

        const std::uint64_t next_sequence =
            (header->sequence.load(std::memory_order_relaxed) & ~1ULL) + 2;
        header->sequence.store(
            next_sequence - 1,
            std::memory_order_release
        );
        framebuffer.bind();
        visualizer.renderFrame();
        context.functions()->glReadPixels(
            0,
            0,
            kWidth,
            kHeight,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            pixels
        );

        for (int y = 0; y < kHeight; ++y) {
            for (int x = 0; x < kWidth; ++x) {
                const std::size_t pixel =
                    static_cast<std::size_t>((y * kWidth + x) * 4);
                pixels[pixel + 3] =
                    inside_rounded_rect(x, y) ? 255 : 0;
            }
        }
        header->sequence.store(next_sequence, std::memory_order_release);
    }

    pa_simple_free(pulse);
    ::munmap(mapping, mapping_bytes);
    ::close(frame_fd);
    return 0;
}
