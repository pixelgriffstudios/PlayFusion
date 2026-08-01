#include <QApplication>
#include <QBitmap>
#include <QPaintEvent>
#include <QPainter>
#include <QPixmap>
#include <QRegion>
#include <QWidget>
#include <cstdlib>

class BezelWindow final : public QWidget {
public:
    BezelWindow(const QString &image_path, int width, int height, const QRect &hole)
        : image_(image_path) {
        setWindowTitle(QStringLiteral("PlayFusion Jukebox Bezel"));
        setWindowFlags(Qt::FramelessWindowHint | Qt::Tool);
        setAttribute(Qt::WA_TranslucentBackground);
        setAttribute(Qt::WA_NoSystemBackground);
        setAttribute(Qt::WA_TransparentForMouseEvents);
        setFixedSize(width, height);

        scaled_ = image_.scaled(width, height, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
        QBitmap alpha_mask = scaled_.createMaskFromColor(
            Qt::transparent, Qt::MaskInColor);
        QRegion visible(alpha_mask);
        visible -= QRegion(hole);
        setMask(visible);
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
        painter.drawPixmap(rect(), scaled_);
    }

private:
    QPixmap image_;
    QPixmap scaled_;
};

int main(int argc, char **argv) {
    if (argc != 8) {
        return 2;
    }
    QApplication app(argc, argv);
    BezelWindow window(
        QString::fromLocal8Bit(argv[1]),
        std::atoi(argv[2]),
        std::atoi(argv[3]),
        QRect(
            std::atoi(argv[4]),
            std::atoi(argv[5]),
            std::atoi(argv[6]),
            std::atoi(argv[7])));
    window.show();
    return app.exec();
}
