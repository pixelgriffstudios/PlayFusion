#define GL_GLEXT_PROTOTYPES 1

#include <libprojectM/projectM.hpp>
#include <pulse/error.h>
#include <pulse/simple.h>

#include <GL/gl.h>
#include <GL/glext.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cstdint>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace {
constexpr int kAudioFrames = 512;
constexpr int kChannels = 2;

struct AudioBlock {
    std::array<std::int16_t, kAudioFrames * kChannels> samples{};
};

class NativeProjectM {
public:
    NativeProjectM(
        const char* config_path,
        const char* monitor,
        int width,
        int height
    )
        : monitor_(monitor ? monitor : ""),
          visualizer_(std::make_unique<projectM>(config_path)) {
        resize(width, height);
        start_audio();
    }

    ~NativeProjectM() {
        capture_running_.store(false);
        if (capture_thread_.joinable()) {
            capture_thread_.join();
        }
        destroy_gl();
    }

    void render() {
        std::vector<AudioBlock> audio;
        {
            std::lock_guard<std::mutex> lock(audio_mutex_);
            audio.swap(pending_audio_);
        }
        for (const AudioBlock& block : audio) {
            visualizer_->pcm()->addPCM16Data(
                block.samples.data(),
                kAudioFrames
            );
        }

        GLint old_framebuffer = 0;
        GLint old_viewport[4]{};
        glGetIntegerv(GL_FRAMEBUFFER_BINDING, &old_framebuffer);
        glGetIntegerv(GL_VIEWPORT, old_viewport);
        glBindFramebuffer(GL_FRAMEBUFFER, framebuffer_);
        glViewport(0, 0, width_, height_);
        visualizer_->renderFrame();
        glBindFramebuffer(GL_FRAMEBUFFER, old_framebuffer);
        glViewport(
            old_viewport[0],
            old_viewport[1],
            old_viewport[2],
            old_viewport[3]
        );
    }

    GLuint texture() const {
        return texture_;
    }

private:
    void resize(int width, int height) {
        width_ = std::max(320, width);
        height_ = std::max(180, height);
        destroy_gl();

        glGenTextures(1, &texture_);
        glBindTexture(GL_TEXTURE_2D, texture_);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
        glTexImage2D(
            GL_TEXTURE_2D,
            0,
            GL_RGBA8,
            width_,
            height_,
            0,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            nullptr
        );

        glGenRenderbuffers(1, &depth_);
        glBindRenderbuffer(GL_RENDERBUFFER, depth_);
        glRenderbufferStorage(
            GL_RENDERBUFFER,
            GL_DEPTH24_STENCIL8,
            width_,
            height_
        );

        glGenFramebuffers(1, &framebuffer_);
        glBindFramebuffer(GL_FRAMEBUFFER, framebuffer_);
        glFramebufferTexture2D(
            GL_FRAMEBUFFER,
            GL_COLOR_ATTACHMENT0,
            GL_TEXTURE_2D,
            texture_,
            0
        );
        glFramebufferRenderbuffer(
            GL_FRAMEBUFFER,
            GL_DEPTH_STENCIL_ATTACHMENT,
            GL_RENDERBUFFER,
            depth_
        );
        glBindFramebuffer(GL_FRAMEBUFFER, 0);
        visualizer_->projectM_resetGL(width_, height_);
    }

    void destroy_gl() {
        if (framebuffer_ != 0) {
            glDeleteFramebuffers(1, &framebuffer_);
            framebuffer_ = 0;
        }
        if (depth_ != 0) {
            glDeleteRenderbuffers(1, &depth_);
            depth_ = 0;
        }
        if (texture_ != 0) {
            glDeleteTextures(1, &texture_);
            texture_ = 0;
        }
    }

    void start_audio() {
        capture_running_.store(true);
        capture_thread_ = std::thread([this]() {
            pa_sample_spec sample_spec{};
            sample_spec.format = PA_SAMPLE_S16LE;
            sample_spec.rate = 48000;
            sample_spec.channels = kChannels;
            int pulse_error = 0;
            const char* source = monitor_.empty() ? nullptr : monitor_.c_str();
            pa_simple* pulse = pa_simple_new(
                nullptr,
                "PlayFusion projectM",
                PA_STREAM_RECORD,
                source,
                "native cabinet visualization",
                &sample_spec,
                nullptr,
                nullptr,
                &pulse_error
            );
            if (!pulse) {
                return;
            }

            while (capture_running_.load()) {
                AudioBlock block;
                if (pa_simple_read(
                        pulse,
                        block.samples.data(),
                        block.samples.size() * sizeof(std::int16_t),
                        &pulse_error
                    ) < 0) {
                    break;
                }
                std::lock_guard<std::mutex> lock(audio_mutex_);
                if (pending_audio_.size() >= 8) {
                    pending_audio_.erase(pending_audio_.begin());
                }
                pending_audio_.push_back(std::move(block));
            }
            pa_simple_free(pulse);
        });
    }

    int width_ = 0;
    int height_ = 0;
    GLuint framebuffer_ = 0;
    GLuint depth_ = 0;
    GLuint texture_ = 0;
    std::string monitor_;
    std::unique_ptr<projectM> visualizer_;
    std::atomic<bool> capture_running_{false};
    std::thread capture_thread_;
    std::mutex audio_mutex_;
    std::vector<AudioBlock> pending_audio_;
};
}  // namespace

extern "C" void* playfusion_projectm_create(
    const char* config_path,
    const char* monitor,
    int width,
    int height
) {
    try {
        return new NativeProjectM(config_path, monitor, width, height);
    } catch (...) {
        return nullptr;
    }
}

extern "C" void playfusion_projectm_render(void* handle) {
    if (handle) {
        static_cast<NativeProjectM*>(handle)->render();
    }
}

extern "C" unsigned int playfusion_projectm_texture(void* handle) {
    return handle
        ? static_cast<NativeProjectM*>(handle)->texture()
        : 0;
}

extern "C" void playfusion_projectm_destroy(void* handle) {
    delete static_cast<NativeProjectM*>(handle);
}
