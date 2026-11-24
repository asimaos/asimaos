#pragma once

#include <QDBusConnection>
#include <QJsonObject>
#include <QObject>
#include <QVariantList>

#include <functional>

class QDBusMessage;

// A client for one of the Asima OS system services: io.github.asimaos.<name> on the system bus (tests run
// the services on a private session bus: ASIMAOS_SERVICES_BUS=session). Calls are asynchronous and
// may wait up to 10 minutes, because polkit can ask for a password first.
class ServiceClient : public QObject
{
    Q_OBJECT

public:
    // name: "Updater1" → io.github.asimaos.Updater1 at /io/github/asimaos/Updater1.
    explicit ServiceClient(const QString &name, QObject *parent = nullptr);

    // Running, or started by D-Bus activation on the first call.
    bool available() const { return m_available; }
    void call(const QString &method, const QVariantList &args = {}, std::function<void(const QDBusMessage &)> done = {});
    // Connects one of the service's signals to a slot (SLOT() syntax).
    bool connectSignal(const QString &signal, QObject *receiver, const char *slot);

    // The first argument of a reply as a JSON object (the services answer status in JSON).
    static QJsonObject json(const QDBusMessage &reply);
    static QDBusConnection bus();

signals:
    // A call was refused or failed: a sentence for people.
    void failed(const QString &message);

private:
    QString m_service, m_path;
    bool m_available = false;
};
