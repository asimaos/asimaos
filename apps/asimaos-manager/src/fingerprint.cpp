#include "fingerprint.h"

#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusObjectPath>
#include <QDBusReply>
#include <QDBusVariant>
#include <QDir>

namespace {
const QString kService = QStringLiteral("net.reactivated.Fprint");
const QString kDeviceIface = QStringLiteral("net.reactivated.Fprint.Device");

// Reads a D-Bus property explicitly (fprintd's names contain dashes, e.g. "num-enroll-stages").
QVariant deviceProperty(const QString &path, const QString &name)
{
    QDBusInterface props(kService, path, QStringLiteral("org.freedesktop.DBus.Properties"), QDBusConnection::systemBus());
    const QDBusReply<QDBusVariant> reply = props.call(QStringLiteral("Get"), kDeviceIface, name);
    return reply.isValid() ? reply.value().variant() : QVariant();
}

QVariantMap entry(const QString &id, const QString &title)
{
    return {{QStringLiteral("id"), id}, {QStringLiteral("title"), title}};
}
} // namespace

Fingerprint::Fingerprint(bool demo, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
{
    m_demoTimer.setInterval(520);
    connect(&m_demoTimer, &QTimer::timeout, this, [this] {
        onEnrollStatus(m_stage + 1 >= m_stages ? QStringLiteral("enroll-completed") : QStringLiteral("enroll-stage-passed"),
                       m_stage + 1 >= m_stages);
    });
    refresh();
    if (m_demo)
        return;
    // fprintd has no "reader added" signal: when the USB devices change, ask it again once the
    // new reader had a moment to appear there.
    const auto usbDevices = [] { return QDir(QStringLiteral("/sys/bus/usb/devices")).entryList(QDir::Dirs | QDir::System | QDir::NoDotAndDotDot); };
    m_usb = usbDevices();
    m_watch.setInterval(3000);
    connect(&m_watch, &QTimer::timeout, this, [this, usbDevices] {
        const QStringList now = usbDevices();
        if (now == m_usb || m_enrolling)
            return;
        m_usb = now;
        QTimer::singleShot(1500, this, &Fingerprint::refresh);
    });
    m_watch.start();
}

QVariantList Fingerprint::fingers() const
{
    return {
        entry("right-index-finger", tr("Right index finger")), entry("left-index-finger", tr("Left index finger")),
        entry("right-thumb", tr("Right thumb")), entry("left-thumb", tr("Left thumb")),
        entry("right-middle-finger", tr("Right middle finger")), entry("left-middle-finger", tr("Left middle finger")),
        entry("right-ring-finger", tr("Right ring finger")), entry("left-ring-finger", tr("Left ring finger")),
        entry("right-little-finger", tr("Right little finger")), entry("left-little-finger", tr("Left little finger")),
    };
}

QString Fingerprint::fingerTitle(const QString &id) const
{
    for (const QVariant &f : fingers())
        if (f.toMap().value("id").toString() == id)
            return f.toMap().value("title").toString();
    return id;
}

QString Fingerprint::user() const
{
    return qEnvironmentVariable("USER");
}

void Fingerprint::refresh()
{
    if (m_demo) {
        m_available = true;
        m_reader = QStringLiteral("Goodix MOC fingerprint sensor");
        if (m_enrolled.isEmpty())
            m_enrolled = {QStringLiteral("right-index-finger"), QStringLiteral("left-index-finger")};
        emit changed();
        return;
    }
    QDBusInterface manager(kService, QStringLiteral("/net/reactivated/Fprint/Manager"),
                           QStringLiteral("net.reactivated.Fprint.Manager"), QDBusConnection::systemBus());
    const QDBusReply<QDBusObjectPath> device = manager.call(QStringLiteral("GetDefaultDevice"));
    m_available = device.isValid();
    m_enrolled.clear();
    if (m_available) {
        m_device = device.value().path();
        QDBusInterface dev(kService, m_device, kDeviceIface, QDBusConnection::systemBus());
        m_reader = deviceProperty(m_device, QStringLiteral("name")).toString();
        m_stages = qMax(1, deviceProperty(m_device, QStringLiteral("num-enroll-stages")).toInt());
        const QDBusReply<QStringList> list = dev.call(QStringLiteral("ListEnrolledFingers"), user());
        if (list.isValid())
            m_enrolled = list.value();
    }
    emit changed();
}

void Fingerprint::enroll(const QString &finger)
{
    if (m_enrolling)
        return;
    m_finger = finger;
    m_stage = 0;
    m_enrolling = true;
    m_message = tr("Touch the sensor with your %1").arg(fingerTitle(finger).toLower());
    emit changed();

    if (m_demo) {
        m_stages = 8;
        m_demoTimer.start();
        return;
    }
    QDBusInterface dev(kService, m_device, kDeviceIface, QDBusConnection::systemBus());
    const QDBusMessage claim = dev.call(QStringLiteral("Claim"), user());
    if (claim.type() == QDBusMessage::ErrorMessage) {
        m_enrolling = false;
        m_message = tr("The fingerprint reader is busy or not allowed: %1").arg(claim.errorMessage());
        emit changed();
        emit finished(false, m_message);
        return;
    }
    QDBusConnection::systemBus().connect(kService, m_device, kDeviceIface, QStringLiteral("EnrollStatus"),
                                         this, SLOT(onEnrollStatus(QString, bool)));
    const QDBusMessage start = dev.call(QStringLiteral("EnrollStart"), finger);
    if (start.type() == QDBusMessage::ErrorMessage) {
        m_message = tr("Could not start: %1").arg(start.errorMessage());
        endEnroll();
        emit finished(false, m_message);
    }
}

void Fingerprint::onEnrollStatus(const QString &result, bool done)
{
    if (result == QLatin1String("enroll-stage-passed")) {
        m_stage = qMin(m_stage + 1, m_stages);
        m_message = tr("Lift your finger and touch again (%1 of %2)").arg(m_stage).arg(m_stages);
    } else if (result == QLatin1String("enroll-completed")) {
        m_stage = m_stages;
        m_message = tr("Your %1 is ready to use.").arg(fingerTitle(m_finger).toLower());
    } else if (result == QLatin1String("enroll-retry-scan") || result == QLatin1String("enroll-remove-and-retry")) {
        m_message = tr("That didn't register. Touch the sensor again.");
    } else if (result == QLatin1String("enroll-swipe-too-short")) {
        m_message = tr("Too quick. Swipe a little slower.");
    } else if (result == QLatin1String("enroll-finger-not-centered")) {
        m_message = tr("Place your finger in the middle of the sensor.");
    } else if (result == QLatin1String("enroll-duplicate")) {
        m_message = tr("This finger is already set up.");
    } else if (result == QLatin1String("enroll-data-full")) {
        m_message = tr("The reader is full. Remove a fingerprint first.");
    } else {
        m_message = tr("The fingerprint reader stopped (%1).").arg(result);
    }
    emit changed();
    if (done) {
        const bool ok = result == QLatin1String("enroll-completed");
        endEnroll();
        if (ok && !m_enrolled.contains(m_finger))
            m_enrolled.append(m_finger);
        if (!m_demo)
            refresh();
        emit changed();
        emit finished(ok, m_message);
    }
}

void Fingerprint::endEnroll()
{
    m_demoTimer.stop();
    if (!m_demo && m_enrolling) {
        QDBusInterface dev(kService, m_device, kDeviceIface, QDBusConnection::systemBus());
        dev.call(QStringLiteral("EnrollStop"));
        dev.call(QStringLiteral("Release"));
        QDBusConnection::systemBus().disconnect(kService, m_device, kDeviceIface, QStringLiteral("EnrollStatus"),
                                                this, SLOT(onEnrollStatus(QString, bool)));
    }
    m_enrolling = false;
}

void Fingerprint::cancel()
{
    endEnroll();
    m_message.clear();
    emit changed();
}

void Fingerprint::removeAll()
{
    if (m_demo) {
        m_enrolled.clear();
        emit changed();
        emit finished(true, tr("All fingerprints were removed."));
        return;
    }
    QDBusInterface dev(kService, m_device, kDeviceIface, QDBusConnection::systemBus());
    dev.call(QStringLiteral("Claim"), user());
    const QDBusMessage reply = dev.call(QStringLiteral("DeleteEnrolledFingers2"));
    dev.call(QStringLiteral("Release"));
    refresh();
    const bool ok = reply.type() != QDBusMessage::ErrorMessage;
    emit finished(ok, ok ? tr("All fingerprints were removed.") : reply.errorMessage());
}

void Fingerprint::setDemoState(const QString &state)
{
    if (state == QLatin1String("enrolling")) {
        enroll(QStringLiteral("right-thumb"));
        m_demoTimer.stop();
        m_stage = 5;
        m_message = tr("Lift your finger and touch again (%1 of %2)").arg(m_stage).arg(m_stages);
        emit changed();
    }
}
