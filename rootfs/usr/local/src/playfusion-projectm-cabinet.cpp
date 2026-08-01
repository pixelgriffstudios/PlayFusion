#include <QGuiApplication>
#include <QImage>
#include <QOpenGLContext>
#include <QOpenGLFramebufferObject>
#include <QOpenGLWindow>
#include <QPainter>
#include <QPainterPath>
#include <QPixmap>
#include <QSurfaceFormat>
#include <QTimer>

#include <libprojectM/projectM.hpp>
#include <pulse/error.h>
#include <pulse/simple.h>

#include <GL/gl.h>

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

class ProjectMCabinet final : public QOpenGLWindow {
public:
    ProjectMCabinet(
        std::string config_path,
        QString cabinet_path,
        std::string monitor
    )
        : config_path_(std::move(config_path)),
          monitor_(std::move(monitor)) {
        setTitle(QStringLiteral("PlayFusion Jukebox"));
        setFlags(
            Qt::FramelessWindowHint |
            Qt::WindowStaysOnTopHint |
            Qt::WindowDoesNotAcceptFocus |
            Qt::WindowTransparentForInput
        );

        QImage cabinet(cabinet_path);
        cabinet = cabinet.convertToFormat(QImage::Format_ARGB32_Premultiplied);
        if (!cabinet.isNull()) {
            const qreal x_scale = cabinet.width() / 640.0;
            const qreal y_scale = cabinet.height() / 360.0;
            QPainter painter(&cabinet);
            painter.setCompositionMode(QPainter::CompositionMode_Clear);
            QPainterPath opening;
            opening.addRoundedRect(
                QRectF(
                    149.0 * x_scale,
                    73.0 * y_scale,
                    341.0 * x_scale,
                    191.0 * y_scale
                ),
                22.0 * x_scale,
                22.0 * y_scale
            );
            painter.fillPath(opening, Qt::transparent);
            painter.end();
            cabinet_overlay_ = QPixmap::fromImage(cabinet);
        }

        connect(&render_timer_, &QTimer::timeout, this, [this]() {
            update();
        });
        render_timer_.setTimerType(Qt::PreciseTimer);
        render_timer_.setInterval(16);
    }

    ~ProjectMCabinet() override {
        capture_running_.store(false);
        if (capture_thread_.joinable()) {
            capture_thread_.join();
        }
    }

protected:
    void initializeGL() override {
        visualizer_ = std::make_unique<projectM>(config_path_);
        rebuild_framebuffer();
        start_audio_capture();
        render_timer_.start();
    }

    void resizeGL(int, int) override {
        rebuild_framebuffer();
    }

    void paintGL() override {
        if (!visualizer_ || !visual_framebuffer_) {
            return;
        }

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

        visual_framebuffer_->bind();
        visualizer_->renderFrame();
        visual_framebuffer_->release();

        const qreal ratio = devicePixelRatio();
        const int output_width = std::max(1, qRound(width() * ratio));
        const int output_height = std::max(1, qRound(height() * ratio));
        const int view_x = qRound(output_width * (149.0 / 640.0));
        const int view_y = qRound(output_height * (73.0 / 360.0));
        const int view_width = qRound(output_width * (341.0 / 640.0));
        const int view_height = qRound(output_height * (191.0 / 360.0));

        QOpenGLFramebufferObject::bindDefault();
        glViewport(0, 0, output_width, output_height);
        glDisable(GL_DEPTH_TEST);
        glDisable(GL_BLEND);
        glClearColor(0.0f, 0.0f, 0.0f, 1.0f);
        glClear(GL_COLOR_BUFFER_BIT);

        const float left = (2.0f * view_x / output_width) - 1.0f;
        const float right =
            (2.0f * (view_x + view_width) / output_width) - 1.0f;
        const float top = 1.0f - (2.0f * view_y / output_height);
        const float bottom =
            1.0f - (2.0f * (view_y + view_height) / output_height);

        glEnable(GL_TEXTURE_2D);
        glBindTexture(GL_TEXTURE_2D, visual_framebuffer_->texture());
        glColor4f(1.0f, 1.0f, 1.0f, 1.0f);
        glBegin(GL_QUADS);
        glTexCoord2f(0.0f, 1.0f);
        glVertex2f(left, top);
        glTexCoord2f(1.0f, 1.0f);
        glVertex2f(right, top);
        glTexCoord2f(1.0f, 0.0f);
        glVertex2f(right, bottom);
        glTexCoord2f(0.0f, 0.0f);
        glVertex2f(left, bottom);
        glEnd();
        glBindTexture(GL_TEXTURE_2D, 0);
        glDisable(GL_TEXTURE_2D);

        QPainter painter(this);
        painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
        painter.drawPixmap(QRect(0, 0, width(), height()), cabinet_overlay_);
    }

private:
    void rebuild_framebuffer() {
        if (!visualizer_ || width() <= 0 || height() <= 0) {
            return;
        }

        const qreal ratio = devicePixelRatio();
        const int view_width =
            std::max(320, qRound(width() * ratio * (341.0 / 640.0)));
        const int view_height =
            std::max(180, qRound(height() * ratio * (191.0 / 360.0)));
        QOpenGLFramebufferObjectFormat format;
        format.setAttachment(QOpenGLFramebufferObject::CombinedDepthStencil);
        format.setInternalTextureFormat(GL_RGBA8);
        visual_framebuffer_ =
            std::make_unique<QOpenGLFramebufferObject>(
                view_width,
                view_height,
                format
            );
        visualizer_->projectM_resetGL(view_width, view_height);
    }

    void start_audio_capture() {
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
                "cabinet visualization",
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

    std::string config_path_;
    std::string monitor_;
    QPixmap cabinet_overlay_;
    QTimer render_timer_;
    std::unique_ptr<projectM> visualizer_;
    std::unique_ptr<QOpenGLFramebufferObject> visual_framebuffer_;
    std::atomic<bool> capture_running_{false};
    std::thread capture_thread_;
    std::mutex audio_mutex_;
    std::vector<AudioBlock> pending_audio_;
};
}  // namespace

int main(int argc, char** argv) {
    if (argc < 4) {
        return 2;
    }

    QCoreApplication::setAttribute(Qt::AA_UseDesktopOpenGL);
    QSurfaceFormat format;
    format.setRenderableType(QSurfaceFormat::OpenGL);
    format.setVersion(2, 1);
    format.setProfile(QSurfaceFormat::CompatibilityProfile);
    format.setSwapInterval(1);
    format.setDepthBufferSize(24);
    format.setStencilBufferSize(8);
    QSurfaceFormat::setDefaultFormat(format);

    QGuiApplication application(argc, argv);
    ProjectMCabinet window(argv[1], argv[2], argv[3]);
    window.showFullScreen();
    window.raise();
    return application.exec();
}
