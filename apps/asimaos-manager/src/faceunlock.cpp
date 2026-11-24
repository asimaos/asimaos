#include "faceunlock.h"
#include "sysinfo.h"

#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusInterface>
#include <QDBusReply>
#include <QDir>

namespace {
const QString kService = QStringLiteral("io.github.asimaos.Face1");
const QString kPath = QStringLiteral("/io/github/asimaos/Face1");
const QString kIface = QStringLiteral("io.github.asimaos.Face1");

// Tests run the service on a private session bus (ASIMAOS_SERVICES_BUS=session).
QDBusConnection faceBus()
{
    return qEnvironmentVariable("ASIMAOS_SERVICES_BUS") == QLatin1String("session") ? QDBusConnection::sessionBus()
                                                                                   : QDBusConnection::systemBus();
}
} // namespace

FaceUnlock::FaceUnlock(bool demo, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
{
    // Demo scan: a steady sweep through the steps of a real, guided scan (D47).
    m_demoTimer.setInterval(110);
    connect(&m_demoTimer, &QTimer::timeout, this, [this] {
        const double next = qMin(1.0, m_progress + 0.018);
        if (next >= 1.0) {
            m_demoTimer.stop();
            onDone(true, QString());
            return;
        }
        static const char *steps[] = {"straight", "turn", "turn-other", "nod", "nod-other", "blink"};
        onProgress(next, QString::fromLatin1(steps[qMin(5, int(next * 6))]));
    });

    if (!m_demo) {
        QDBusConnection bus = faceBus();
        bus.connect(kService, kPath, kIface, QStringLiteral("EnrollProgress"), this, SLOT(onProgress(double,QString)));
        bus.connect(kService, kPath, kIface, QStringLiteral("EnrollDone"), this, SLOT(onDone(bool,QString)));
    }
    refresh();
}

// Infrared cameras announce themselves in their V4L2 name ("Integrated IR Camera", "… Infrared …").
QString FaceUnlock::findInfraredCamera()
{
    const QDir dir(QStringLiteral("/sys/class/video4linux"));
    for (const QString &node : dir.entryList(QDir::Dirs | QDir::System | QDir::NoDotAndDotDot)) {
        const QString name = SysInfo::readTrimmed(dir.filePath(node) + QStringLiteral("/name"));
        if (name.contains(QLatin1String("IR"), Qt::CaseSensitive) || name.contains(QLatin1String("Infrared"), Qt::CaseInsensitive))
            return name;
    }
    return {};
}

QString FaceUnlock::message() const
{
    if (m_hint == QLatin1String("look") || m_hint == QLatin1String("straight"))
        return tr("Look straight at the screen");
    if (m_hint == QLatin1String("turn"))
        return tr("Slowly turn your head to one side");
    if (m_hint == QLatin1String("turn-other"))
        return tr("Now slowly turn it to the other side");
    if (m_hint == QLatin1String("nod"))
        return tr("Tilt your head up or down a little");
    if (m_hint == QLatin1String("nod-other"))
        return tr("Now the other way");
    if (m_hint == QLatin1String("blink"))
        return tr("Look at the screen and blink three times");
    if (m_hint == QLatin1String("closer"))
        return tr("Move a little closer");
    if (m_hint == QLatin1String("farther"))
        return tr("Move back a little");
    if (m_hint == QLatin1String("hold"))
        return tr("Hold still…");
    return {};
}

void FaceUnlock::refresh()
{
    if (m_demo) {
        emit changed();
        return;
    }
    QDBusConnectionInterface *bus = faceBus().interface();
    m_service = bus && (bus->isServiceRegistered(kService).value() || bus->activatableServiceNames().value().contains(kService));
    m_faces.clear();
    m_camera.clear();
    m_kind.clear();
    m_webcam.clear();
    if (m_service) {
        QDBusInterface face(kService, kPath, kIface, faceBus());
        m_camera = face.property("Camera").toString();
        m_kind = face.property("CameraKind").toString();
        m_webcam = face.property("Webcam").toString();
        const QDBusReply<QStringList> faces = face.call(QStringLiteral("ListFaces"));
        if (faces.isValid())
            m_faces = faces.value();
    }
    if (m_camera.isEmpty() && !m_service) {
        m_camera = findInfraredCamera();
        m_kind = m_camera.isEmpty() ? QString() : QStringLiteral("infrared");
    }
    emit changed();
}

void FaceUnlock::setDemo(const QString &camera, const QString &state)
{
    m_camera = camera == QLatin1String("infrared") ? tr("Integrated IR camera") : QString();
    m_kind = m_camera.isEmpty() ? QString() : camera;
    m_webcam = camera == QLatin1String("webcam") ? QStringLiteral("HD Webcam C615") : QString();
    m_service = true;
    if (!m_camera.isEmpty() && state == QLatin1String("enrolling")) {
        m_enrolling = true;
        m_progress = 0.58;
        m_hint = QStringLiteral("nod");
    }
    emit changed();
}

void FaceUnlock::enroll()
{
    if (m_enrolling || !hasCamera())
        return;
    if (!m_service) {
        emit finished(false, tr("Face sign-in needs the Asima OS face service (asimaos-faced), which isn't available on this computer."));
        return;
    }
    m_enrolling = true;
    m_progress = 0.0;
    m_hint = QStringLiteral("look");
    emit changed();
    const QString label = m_faces.isEmpty() ? tr("Your face") : tr("Alternative look");
    if (m_demo) {
        m_demoTimer.start();
        return;
    }
    QDBusInterface face(kService, kPath, kIface, faceBus());
    const QDBusMessage reply = face.call(QStringLiteral("Enroll"), label);
    if (reply.type() == QDBusMessage::ErrorMessage)
        onDone(false, reply.errorMessage());
}

void FaceUnlock::cancel()
{
    if (!m_enrolling)
        return;
    m_demoTimer.stop();
    if (!m_demo)
        QDBusInterface(kService, kPath, kIface, faceBus()).call(QStringLiteral("Cancel"));
    m_enrolling = false;
    m_progress = 0.0;
    m_hint.clear();
    emit changed();
}

void FaceUnlock::removeAll()
{
    if (!m_demo) {
        const QDBusMessage reply = QDBusInterface(kService, kPath, kIface, faceBus()).call(QStringLiteral("DeleteAll"));
        if (reply.type() == QDBusMessage::ErrorMessage) {
            emit finished(false, reply.errorMessage());
            return;
        }
    }
    m_faces.clear();
    emit changed();
    emit finished(true, tr("Face data removed from this device."));
}

void FaceUnlock::onProgress(double progress, const QString &hint)
{
    if (!m_enrolling)
        return;
    m_progress = qBound(0.0, progress, 1.0);
    m_hint = hint;
    emit changed();
}

void FaceUnlock::onDone(bool ok, const QString &message)
{
    if (!m_enrolling)
        return;
    m_enrolling = false;
    m_progress = ok ? 1.0 : 0.0;
    m_hint.clear();
    if (ok && m_demo)
        m_faces.append(m_faces.isEmpty() ? tr("Your face") : tr("Alternative look"));
    else if (ok)
        refresh();
    emit changed();
    emit finished(ok, ok ? tr("Face sign-in is ready.") : (message.isEmpty() ? tr("Face setup didn't finish. Try again.") : message));
}
