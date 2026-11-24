#include "controllertester.h"

#include <QSocketNotifier>

#include <cerrno>
#include <cmath>
#include <cstring>
#include <fcntl.h>
#include <linux/input.h>
#include <sys/ioctl.h>
#include <unistd.h>

namespace {

const char *buttonName(int code, bool swapXY)
{
    switch (code) {
    case BTN_SOUTH: return "a";
    case BTN_EAST: return "b";
    // Linux gamepad spec: NORTH = top, WEST = left. Xbox drivers report X/Y the other way round.
    case BTN_NORTH: return swapXY ? "x" : "y";
    case BTN_WEST: return swapXY ? "y" : "x";
    case BTN_TL: return "lb";
    case BTN_TR: return "rb";
    case BTN_TL2: return "ltButton";
    case BTN_TR2: return "rtButton";
    case BTN_SELECT: return "select";
    case BTN_START: return "start";
    case BTN_MODE: return "home";
    case BTN_THUMBL: return "ls";
    case BTN_THUMBR: return "rs";
    case BTN_DPAD_UP: return "up";
    case BTN_DPAD_DOWN: return "down";
    case BTN_DPAD_LEFT: return "left";
    case BTN_DPAD_RIGHT: return "right";
    default: return nullptr;
    }
}

} // namespace

ControllerTester::ControllerTester(bool demo, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
{
    resetState();
    m_emitTimer.setSingleShot(true);
    m_emitTimer.setInterval(16);   // at most ~60 updates per second to the UI
    connect(&m_emitTimer, &QTimer::timeout, this, &ControllerTester::stateChanged);
    m_demoTimer.setInterval(50);
    connect(&m_demoTimer, &QTimer::timeout, this, &ControllerTester::demoStep);
}

ControllerTester::~ControllerTester()
{
    stop();
}

bool ControllerTester::active() const
{
    return m_fd >= 0 || m_demoTimer.isActive();
}

void ControllerTester::resetState()
{
    m_state.clear();
    for (const char *k : {"a", "b", "x", "y", "lb", "rb", "ls", "rs", "start", "select", "home", "up", "down", "left", "right"})
        m_state.insert(QString::fromLatin1(k), false);
    for (const char *k : {"lx", "ly", "rx", "ry", "lt", "rt"})
        m_state.insert(QString::fromLatin1(k), 0.0);
}

bool ControllerTester::start(const QString &path, const QString &kind)
{
    stop();
    m_swapXY = kind == QLatin1String("xbox");
    if (m_demo) {
        m_path = path;
        m_demoTick = 0;
        m_demoTimer.start();
        emit activeChanged();
        return true;
    }
    const int fd = ::open(path.toLocal8Bit().constData(), O_RDONLY | O_NONBLOCK | O_CLOEXEC);
    if (fd < 0) {
        emit error(tr("Can't read the controller (%1).").arg(QString::fromLocal8Bit(std::strerror(errno))));
        return false;
    }
    for (int code : {ABS_X, ABS_Y, ABS_RX, ABS_RY, ABS_Z, ABS_RZ, ABS_HAT0X, ABS_HAT0Y, ABS_GAS, ABS_BRAKE}) {
        input_absinfo info {};
        if (::ioctl(fd, EVIOCGABS(code), &info) == 0 && info.maximum > info.minimum)
            m_ranges[code] = {info.minimum, info.maximum};
    }
    m_fd = fd;
    m_path = path;
    m_notifier = new QSocketNotifier(fd, QSocketNotifier::Read, this);
    connect(m_notifier, &QSocketNotifier::activated, this, &ControllerTester::readEvents);
    resetState();
    emit activeChanged();
    emit stateChanged();
    return true;
}

void ControllerTester::stop()
{
    const bool wasActive = active();
    m_demoTimer.stop();
    delete m_notifier;
    m_notifier = nullptr;
    if (m_fd >= 0) {
        ::close(m_fd);
        m_fd = -1;
    }
    m_path.clear();
    resetState();
    if (wasActive) {
        emit activeChanged();
        emit stateChanged();
    }
}

double ControllerTester::normalized(int code, int value, bool centred) const
{
    const Range r = m_ranges[code];
    const double span = double(r.max) - double(r.min);
    if (span <= 0)
        return 0.0;
    if (!centred)
        return qBound(0.0, (value - r.min) / span, 1.0);
    const double v = (value - (r.max + r.min) / 2.0) / (span / 2.0);
    return std::abs(v) < 0.06 ? 0.0 : qBound(-1.0, v, 1.0);   // small dead zone
}

void ControllerTester::readEvents()
{
    input_event events[64];
    ssize_t n = 0;
    while ((n = ::read(m_fd, events, sizeof events)) > 0) {
        for (size_t i = 0; i < size_t(n) / sizeof(input_event); ++i) {
            const input_event &e = events[i];
            if (e.type == EV_KEY) {
                if (const char *name = buttonName(e.code, m_swapXY))
                    m_state.insert(QString::fromLatin1(name), e.value != 0);
            } else if (e.type == EV_ABS) {
                switch (e.code) {
                case ABS_X: m_state.insert("lx", normalized(e.code, e.value, true)); break;
                case ABS_Y: m_state.insert("ly", normalized(e.code, e.value, true)); break;
                case ABS_RX: m_state.insert("rx", normalized(e.code, e.value, true)); break;
                case ABS_RY: m_state.insert("ry", normalized(e.code, e.value, true)); break;
                case ABS_Z: case ABS_BRAKE: m_state.insert("lt", normalized(e.code, e.value, false)); break;
                case ABS_RZ: case ABS_GAS: m_state.insert("rt", normalized(e.code, e.value, false)); break;
                case ABS_HAT0X: m_state.insert("left", e.value < 0); m_state.insert("right", e.value > 0); break;
                case ABS_HAT0Y: m_state.insert("up", e.value < 0); m_state.insert("down", e.value > 0); break;
                default: break;
                }
            }
        }
    }
    if (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK) {
        emit error(tr("The controller was disconnected."));
        stop();
        return;
    }
    if (!m_emitTimer.isActive())
        m_emitTimer.start();
}

bool ControllerTester::rumble(const QString &path, int milliseconds)
{
    if (m_demo)
        return true;
    const int fd = ::open(path.toLocal8Bit().constData(), O_RDWR | O_CLOEXEC);
    if (fd < 0) {
        emit error(tr("Can't reach the controller to rumble."));
        return false;
    }
    ff_effect effect {};
    effect.type = FF_RUMBLE;
    effect.id = -1;
    effect.u.rumble.strong_magnitude = 0xC000;
    effect.u.rumble.weak_magnitude = 0x8000;
    effect.replay.length = quint16(qBound(50, milliseconds, 2000));
    if (::ioctl(fd, EVIOCSFF, &effect) < 0) {
        ::close(fd);
        emit error(tr("This controller can't rumble."));
        return false;
    }
    input_event play {};
    play.type = EV_FF;
    play.code = quint16(effect.id);
    play.value = 1;
    const bool ok = ::write(fd, &play, sizeof play) == ssize_t(sizeof play);
    const int id = effect.id;
    QTimer::singleShot(milliseconds + 250, this, [fd, id] {
        ::ioctl(fd, EVIOCRMFF, id);
        ::close(fd);
    });
    return ok;
}

void ControllerTester::demoStep()
{
    // Sample input so the tester looks alive in demo mode.
    ++m_demoTick;
    const double t = m_demoTick * 0.07;
    m_state.insert("lx", 0.75 * std::cos(t));
    m_state.insert("ly", 0.75 * std::sin(t));
    m_state.insert("rx", 0.5 * std::sin(t * 1.4));
    m_state.insert("ry", -0.4 * std::cos(t * 0.9));
    m_state.insert("lt", (std::sin(t * 0.8) + 1.0) / 2.0);
    m_state.insert("rt", 0.85);
    static const char *sequence[] = {"a", "b", "y", "x", "rb", "up", "right", "down", "left", "start"};
    const int active = (m_demoTick / 12) % 10;
    for (int i = 0; i < 10; ++i)
        m_state.insert(QString::fromLatin1(sequence[i]), i == active);
    m_state.insert("rb", true);
    emit stateChanged();
}
