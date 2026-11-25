#pragma once

#include <QByteArray>
#include <QJsonObject>
#include <QObject>
#include <QString>

#include <functional>

// snapd's REST API over /run/snapd.socket (what the snap command and Discover use). Changes are
// sent with "X-Allow-Interaction: true", so snapd asks polkit, and the Asima OS password dialog
// asks for the administrator's password; a change is then followed until it is ready.
class SnapdClient : public QObject
{
    Q_OBJECT

public:
    explicit SnapdClient(QObject *parent = nullptr);

    static bool available();
    // One request; `done` gets snapd's answer ({ type, status-code, result, change }), or an
    // empty object when snapd can't be reached.
    void request(const QString &method, const QString &path, const QByteArray &json,
                 std::function<void(const QJsonObject &)> done);
    // A change (install, remove, refresh) to its end: `done(ok, message)`.
    void change(const QString &method, const QString &path, const QByteArray &json,
                std::function<void(bool, const QString &)> done);

private:
    void follow(const QString &id, std::function<void(bool, const QString &)> done);
};
