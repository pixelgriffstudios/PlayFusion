#define GL_GLEXT_PROTOTYPES 1

#include <projectM-4/playlist.h>
#include <projectM-4/projectM.h>
#include <pulse/error.h>
#include <pulse/simple.h>

#include <GL/gl.h>
#include <GL/glx.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cctype>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <mutex>
#include <stdexcept>
#include <string>
#include <thread>
#include <utility>
#include <vector>

namespace {
constexpr int kAudioFrames = 512;
constexpr int kChannels = 2;
constexpr int kPresetCommandPollFrames = 15;
constexpr const char* kPresetCommandPath =
    "/var/kazeta/state/projectm-home/requested-preset.txt";
constexpr const char* kActivePresetPath =
    "/var/kazeta/state/projectm-home/active-preset.txt";
constexpr const char* kBridgeInfoPath =
    "/var/kazeta/state/projectm-home/bridge-info.txt";

struct AudioBlock {
    std::array<std::int16_t, kAudioFrames * kChannels> samples{};
};

struct ProjectMSettings {
    std::string preset_path = "/usr/share/projectM/presets";
    double preset_duration = 25.0;
    double transition_duration = 4.0;
    float beat_sensitivity = 1.0f;
    int fps = 60;
    std::size_t mesh_x = 128;
    std::size_t mesh_y = 72;
};

std::string trim(std::string value) {
    const auto not_space = [](unsigned char value) {
        return !std::isspace(value);
    };
    value.erase(value.begin(), std::find_if(value.begin(), value.end(), not_space));
    value.erase(std::find_if(value.rbegin(), value.rend(), not_space).base(), value.end());
    return value;
}

ProjectMSettings read_settings(const char* config_path) {
    ProjectMSettings settings;
    if (!config_path) {
        return settings;
    }
    std::ifstream config(config_path);
    std::string line;
    while (std::getline(config, line)) {
        const auto separator = line.find('=');
        if (separator == std::string::npos) {
            continue;
        }
        const std::string key = trim(line.substr(0, separator));
        const std::string value = trim(line.substr(separator + 1));
        try {
            if (key == "Preset Path") {
                settings.preset_path = value;
            } else if (key == "Preset Duration") {
                settings.preset_duration = std::stod(value);
            } else if (key == "Smooth Transition Duration") {
                settings.transition_duration = std::stod(value);
            } else if (key == "Beat Sensitivity") {
                settings.beat_sensitivity = std::stof(value);
            } else if (key == "FPS") {
                settings.fps = std::stoi(value);
            } else if (key == "Mesh X") {
                settings.mesh_x = static_cast<std::size_t>(std::stoul(value));
            } else if (key == "Mesh Y") {
                settings.mesh_y = static_cast<std::size_t>(std::stoul(value));
            }
        } catch (...) {
            // Keep safe defaults for malformed optional values.
        }
    }
    return settings;
}

class NativeProjectM {
public:
    NativeProjectM(const char* config_path, const char* monitor, int width, int height)
        : monitor_(monitor ? monitor : ""),
          settings_(read_settings(config_path)) {
        const int requested_width = std::max(320, width);
        const int requested_height = std::max(180, height);
        width_ = requested_width;
        height_ = requested_height;
        initialize_hidden_context();
        start_audio();
    }

    ~NativeProjectM() {
        capture_running_.store(false);
        if (capture_thread_.joinable()) {
            capture_thread_.join();
        }
        destroy_hidden_context();
    }

    void render() {
        if (!display_ || !hidden_context_ || !pbuffer_ || !projectm_) {
            return;
        }
        Display* current_display = glXGetCurrentDisplay();
        GLXContext current_context = glXGetCurrentContext();
        GLXDrawable current_drawable = glXGetCurrentDrawable();
        GLXDrawable current_read_drawable = glXGetCurrentReadDrawable();
        if (!glXMakeContextCurrent(display_, pbuffer_, pbuffer_, hidden_context_)) {
            return;
        }

        poll_requested_preset();
        submit_audio();
        glViewport(0, 0, width_, height_);
        projectm_opengl_render_frame(projectm_);

        // projectM composites to framebuffer 0. In this context framebuffer 0
        // belongs to an invisible pbuffer, so shader warm-up and feedback setup
        // can never flash through the PlayFusion menu.
        glBindFramebuffer(GL_READ_FRAMEBUFFER, 0);
        GLint projectm_draw_buffer = GL_FRONT;
        glGetIntegerv(GL_DRAW_BUFFER, &projectm_draw_buffer);
        glReadBuffer(projectm_draw_buffer == GL_BACK ? GL_BACK : GL_FRONT);
        glPixelStorei(GL_PACK_ALIGNMENT, 1);
        glReadPixels(
            0, 0, width_, height_, GL_RGBA, GL_UNSIGNED_BYTE, frame_pixels_.data()
        );
        record_active_preset();

        if (current_display && current_context) {
            glXMakeContextCurrent(
                current_display, current_drawable, current_read_drawable, current_context
            );
            GLint previous_texture = 0;
            glGetIntegerv(GL_TEXTURE_BINDING_2D, &previous_texture);
            glBindTexture(GL_TEXTURE_2D, texture_);
            glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
            glTexSubImage2D(
                GL_TEXTURE_2D, 0, 0, 0, width_, height_,
                GL_RGBA, GL_UNSIGNED_BYTE, frame_pixels_.data()
            );
            glBindTexture(GL_TEXTURE_2D, static_cast<GLuint>(previous_texture));
        }
    }

    GLuint texture() const { return texture_; }
    const std::uint8_t* pixels() const { return frame_pixels_.data(); }
    int width() const { return width_; }
    int height() const { return height_; }

private:
    void initialize_hidden_context() {
        display_ = glXGetCurrentDisplay();
        main_context_ = glXGetCurrentContext();
        main_drawable_ = glXGetCurrentDrawable();
        main_read_drawable_ = glXGetCurrentReadDrawable();
        if (!display_ || !main_context_ || !main_drawable_) {
            throw std::runtime_error("PlayFusion ProjectM requires a current GLX context");
        }
        const std::string main_gl_version = reinterpret_cast<const char*>(
            glGetString(GL_VERSION)
        );

        // The output texture belongs exclusively to Macroquad's context. No
        // OpenGL object is shared with the hidden renderer; completed pixels
        // are transferred after glReadPixels has synchronized the pbuffer.
        glGenTextures(1, &texture_);
        glBindTexture(GL_TEXTURE_2D, texture_);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
        glTexImage2D(
            GL_TEXTURE_2D, 0, GL_RGBA8, width_, height_, 0,
            GL_RGBA, GL_UNSIGNED_BYTE, nullptr
        );
        frame_pixels_.resize(
            static_cast<std::size_t>(width_) * static_cast<std::size_t>(height_) * 4
        );

        const int config_attributes[] = {
            GLX_X_RENDERABLE, True,
            GLX_DRAWABLE_TYPE, GLX_PBUFFER_BIT,
            GLX_RENDER_TYPE, GLX_RGBA_BIT,
            GLX_RED_SIZE, 8,
            GLX_GREEN_SIZE, 8,
            GLX_BLUE_SIZE, 8,
            GLX_ALPHA_SIZE, 8,
            GLX_DEPTH_SIZE, 24,
            GLX_STENCIL_SIZE, 8,
            GLX_DOUBLEBUFFER, False,
            None
        };
        int config_count = 0;
        GLXFBConfig* configs = glXChooseFBConfig(
            display_, DefaultScreen(display_), config_attributes, &config_count
        );
        if (!configs || config_count == 0) {
            if (configs) {
                XFree(configs);
            }
            throw std::runtime_error("No GLX pbuffer framebuffer configuration");
        }

        const int pbuffer_attributes[] = {
            GLX_PBUFFER_WIDTH, width_,
            GLX_PBUFFER_HEIGHT, height_,
            GLX_PRESERVED_CONTENTS, True,
            None
        };
        pbuffer_ = glXCreatePbuffer(display_, configs[0], pbuffer_attributes);
        hidden_context_ = glXCreateNewContext(
            display_, configs[0], GLX_RGBA_TYPE, nullptr, True
        );
        XFree(configs);
        if (!pbuffer_ || !hidden_context_) {
            throw std::runtime_error("Could not create isolated GLX pbuffer context");
        }
        if (!glXMakeContextCurrent(display_, pbuffer_, pbuffer_, hidden_context_)) {
            throw std::runtime_error("Could not activate shared GLX pbuffer context");
        }
        const std::string hidden_gl_version = reinterpret_cast<const char*>(
            glGetString(GL_VERSION)
        );
        std::ofstream(kBridgeInfoPath, std::ios::trunc)
            << "engine=projectM 4.1.6\n"
            << "transfer=isolated-pbuffer-cpu-upload\n"
            << "size=" << width_ << 'x' << height_ << '\n'
            << "main_gl=" << main_gl_version << '\n'
            << "hidden_gl=" << hidden_gl_version << '\n';

        projectm_ = projectm_create();
        if (!projectm_) {
            restore_main_context();
            throw std::runtime_error("projectM 4.1.6 initialization failed");
        }
        projectm_set_window_size(projectm_, width_, height_);
        projectm_set_fps(projectm_, settings_.fps);
        projectm_set_mesh_size(projectm_, settings_.mesh_x, settings_.mesh_y);
        projectm_set_aspect_correction(projectm_, true);
        projectm_set_beat_sensitivity(projectm_, settings_.beat_sensitivity);
        projectm_set_preset_duration(projectm_, settings_.preset_duration);
        projectm_set_soft_cut_duration(projectm_, settings_.transition_duration);
        projectm_set_hard_cut_enabled(projectm_, false);

        const char* texture_paths[] = {
            "/usr/share/projectM/textures",
            "/usr/share/projectM/presets"
        };
        projectm_set_texture_search_paths(projectm_, texture_paths, 2);

        playlist_ = projectm_playlist_create(projectm_);
        if (!playlist_) {
            restore_main_context();
            throw std::runtime_error("projectM 4.1.6 playlist initialization failed");
        }
        load_playlist();
        projectm_playlist_set_shuffle(playlist_, true);
        projectm_playlist_set_retry_count(playlist_, 8);
        if (projectm_playlist_size(playlist_) == 0) {
            restore_main_context();
            throw std::runtime_error("projectM playlist contains no presets");
        }
        projectm_playlist_set_position(playlist_, 0, true);

        restore_main_context();
    }

    void load_playlist() {
        namespace fs = std::filesystem;
        std::vector<std::string> presets;
        std::error_code error;
        for (const auto& entry : fs::directory_iterator(settings_.preset_path, error)) {
            const std::string extension = entry.path().extension().string();
            if (extension == ".milk" || extension == ".prjm") {
                presets.push_back(entry.path().string());
            }
        }
        std::sort(presets.begin(), presets.end());
        for (const std::string& preset : presets) {
            projectm_playlist_add_preset(playlist_, preset.c_str(), false);
        }
    }

    void restore_main_context() {
        if (display_ && main_context_) {
            glXMakeContextCurrent(
                display_, main_drawable_, main_read_drawable_, main_context_
            );
        }
    }

    void destroy_hidden_context() {
        if (!display_) {
            return;
        }
        Display* current_display = glXGetCurrentDisplay();
        GLXContext current_context = glXGetCurrentContext();
        GLXDrawable current_drawable = glXGetCurrentDrawable();
        GLXDrawable current_read_drawable = glXGetCurrentReadDrawable();

        if (hidden_context_ && pbuffer_) {
            glXMakeContextCurrent(display_, pbuffer_, pbuffer_, hidden_context_);
            if (playlist_) {
                projectm_playlist_destroy(playlist_);
                playlist_ = nullptr;
            }
            if (projectm_) {
                projectm_destroy(projectm_);
                projectm_ = nullptr;
            }
        }

        if (current_display && current_context && current_context != hidden_context_) {
            glXMakeContextCurrent(
                current_display, current_drawable, current_read_drawable, current_context
            );
        } else {
            glXMakeContextCurrent(display_, None, None, nullptr);
        }
        if (texture_ && glXGetCurrentContext()) {
            glDeleteTextures(1, &texture_);
            texture_ = 0;
        }
        if (hidden_context_) {
            glXDestroyContext(display_, hidden_context_);
            hidden_context_ = nullptr;
        }
        if (pbuffer_) {
            glXDestroyPbuffer(display_, pbuffer_);
            pbuffer_ = 0;
        }
    }

    void submit_audio() {
        std::vector<AudioBlock> audio;
        {
            std::lock_guard<std::mutex> lock(audio_mutex_);
            audio.swap(pending_audio_);
        }
        for (const AudioBlock& block : audio) {
            projectm_pcm_add_int16(
                projectm_, block.samples.data(), kAudioFrames, PROJECTM_STEREO
            );
        }
    }

    void poll_requested_preset() {
        if (++command_poll_frame_ < kPresetCommandPollFrames) {
            return;
        }
        command_poll_frame_ = 0;
        std::ifstream request_file(kPresetCommandPath);
        std::string requested;
        if (!request_file || !std::getline(request_file, requested) || requested.empty() ||
            requested == last_request_) {
            return;
        }
        last_request_ = requested;

        const uint32_t count = projectm_playlist_size(playlist_);
        for (uint32_t index = 0; index < count; ++index) {
            char* item = projectm_playlist_item(playlist_, index);
            const std::string candidate = item ? item : "";
            projectm_playlist_free_string(item);
            const std::string filename = std::filesystem::path(candidate).filename().string();
            if (candidate == requested || filename == requested) {
                projectm_playlist_set_position(playlist_, index, true);
                write_active_preset(candidate);
                return;
            }
        }
        write_active_preset("NOT FOUND: " + requested);
    }

    void record_active_preset() {
        const uint32_t position = projectm_playlist_get_position(playlist_);
        if (have_position_ && position == last_position_) {
            return;
        }
        have_position_ = true;
        last_position_ = position;
        char* item = projectm_playlist_item(playlist_, position);
        const std::string candidate = item ? item : "";
        projectm_playlist_free_string(item);
        write_active_preset(candidate);
    }

    static void write_active_preset(const std::string& value) {
        std::ofstream(kActivePresetPath, std::ios::trunc) << value << '\n';
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
                nullptr, "PlayFusion projectM 4.1.6", PA_STREAM_RECORD,
                source, "offscreen menu visualization", &sample_spec,
                nullptr, nullptr, &pulse_error
            );
            if (!pulse) {
                return;
            }
            while (capture_running_.load()) {
                AudioBlock block;
                if (pa_simple_read(
                        pulse, block.samples.data(),
                        block.samples.size() * sizeof(std::int16_t), &pulse_error
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
    std::string monitor_;
    ProjectMSettings settings_;
    Display* display_ = nullptr;
    GLXContext main_context_ = nullptr;
    GLXDrawable main_drawable_ = 0;
    GLXDrawable main_read_drawable_ = 0;
    GLXContext hidden_context_ = nullptr;
    GLXPbuffer pbuffer_ = 0;
    GLuint texture_ = 0;
    std::vector<std::uint8_t> frame_pixels_;
    projectm_handle projectm_ = nullptr;
    projectm_playlist_handle playlist_ = nullptr;
    std::atomic<bool> capture_running_{false};
    std::thread capture_thread_;
    std::mutex audio_mutex_;
    std::vector<AudioBlock> pending_audio_;
    int command_poll_frame_ = 0;
    std::string last_request_;
    bool have_position_ = false;
    uint32_t last_position_ = 0;
};
}  // namespace

extern "C" void* playfusion_projectm_create(
    const char* config_path, const char* monitor, int width, int height
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
    return handle ? static_cast<NativeProjectM*>(handle)->texture() : 0;
}

extern "C" const std::uint8_t* playfusion_projectm_pixels(void* handle) {
    return handle ? static_cast<NativeProjectM*>(handle)->pixels() : nullptr;
}

extern "C" int playfusion_projectm_width(void* handle) {
    return handle ? static_cast<NativeProjectM*>(handle)->width() : 0;
}

extern "C" int playfusion_projectm_height(void* handle) {
    return handle ? static_cast<NativeProjectM*>(handle)->height() : 0;
}

extern "C" void playfusion_projectm_destroy(void* handle) {
    delete static_cast<NativeProjectM*>(handle);
}
