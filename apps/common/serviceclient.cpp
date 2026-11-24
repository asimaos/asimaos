#include "serviceclient.h"

#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QJsonDocument>

ServiceClient::ServiceClient(const QString &name, QObject *parent)
    : QObject(parent)
    , m_service(QStringLiteral("io.github.asimaos.") + name)
    , m_path(QStringLiteral("/io/github/asimaos/") + name)
{
    QDBusConnectionInterface *iface = bus().interface();
    m_available = iface && (iface->isServiceRegistered(m_service).value() || iface->activatableServiceNames().value().contains(m_service));
}

QDBusConnection ServiceClient::bus()
{
    return qEnvironmentVariable("ASIMAOS_SERVICES_BUS") == QLatin1String("session") ? QDBusConnection::sessionBus()
                                                                                   : QDBusConnection::systemBus();
}

void ServiceClient::call(const QString &method, const QVariantList &args, std::function<void(const QDBusMessage &)> done)
{
    QDBusMessage msg = QDBusMessage::createMethodCall(m_service, m_path, m_service, method);
    msg.setArguments(args);
    auto *watcher = new QDBusPendingCallWatcher(bus().asyncCall(msg, 10 * 60 * 1000), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, done](QDBusPendingCallWatcher *w) {
        const QDBusMessage reply = w->reply();
        w->deleteLater();
        if (reply.type() == QDBusMessage::ErrorMessage) {
            emit failed(reply.errorName() == QLatin1String("org.freedesktop.DBus.Error.AccessDenied")
                            ? tr("Not changed: permission wasn't given.") : reply.errorMessage());
            return;
        }
        if (done)
            done(reply);
    });
}

bool ServiceClient::connectSignal(const QString &signal, QObject *receiver, const char *slot)
{
    return bus().connect(m_service, m_path, m_service, signal, receiver, slot);
}

QJsonObject ServiceClient::json(const QDBusMessage &reply)
{
    return QJsonDocument::fromJson(reply.arguments().value(0).toString().toUtf8()).object();
}
