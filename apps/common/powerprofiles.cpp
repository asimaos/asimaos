#include "powerprofiles.h"

#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDBusReply>
#include <QDBusVariant>
#include <QObject>
#include <QVariantList>

#include <utility>

namespace PowerProfiles {

QVariantMap read()
{
    for (const auto &[service, path] : {std::pair{QStringLiteral("org.freedesktop.UPower.PowerProfiles"), QStringLiteral("/org/freedesktop/UPower/PowerProfiles")},
                                        std::pair{QStringLiteral("net.hadess.PowerProfiles"), QStringLiteral("/net/hadess/PowerProfiles")}}) {
        QDBusInterface props(service, path, QStringLiteral("org.freedesktop.DBus.Properties"), QDBusConnection::systemBus());
        const QDBusReply<QDBusVariant> reply = props.call(QStringLiteral("Get"), service, QStringLiteral("ActiveProfile"));
        if (reply.isValid())
            return QVariantMap{{"available", true}, {"profile", reply.value().variant().toString()}, {"service", service},
                               {"path", path}, {"wake", QVariantList{}}};
    }
    return QVariantMap{{"available", false}, {"profile", QString()}, {"wake", QVariantList{}}};
}

QString set(const QVariantMap &current, const QString &profile)
{
    if (!current.value("available").toBool())
        return QObject::tr("Power modes are not available on this computer.");
    const QString service = current.value("service").toString();
    QDBusInterface props(service, current.value("path").toString(), QStringLiteral("org.freedesktop.DBus.Properties"), QDBusConnection::systemBus());
    const QDBusMessage reply = props.call(QStringLiteral("Set"), service, QStringLiteral("ActiveProfile"), QVariant::fromValue(QDBusVariant(profile)));
    return reply.type() == QDBusMessage::ErrorMessage ? QObject::tr("Couldn't change the power mode: %1").arg(reply.errorMessage()) : QString();
}

} // namespace PowerProfiles
