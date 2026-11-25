#include "snapd.h"

#include <QFileInfo>
#include <QJsonDocument>
#include <QLocalSocket>
#include <QTimer>

namespace {
const QString kSocket = QStringLiteral("/run/snapd.socket");
}

SnapdClient::SnapdClient(QObject *parent)
    : QObject(parent)
{
}

bool SnapdClient::available()
{
    return QFileInfo::exists(kSocket);
}

void SnapdClient::request(const QString &method, const QString &path, const QByteArray &json,
                          std::function<void(const QJsonObject &)> done)
{
    auto *s = new QLocalSocket(this);
    auto *reply = new QByteArray;
    connect(s, &QLocalSocket::connected, this, [s, method, path, json] {
        // HTTP/1.0: snapd closes the connection after its answer, which ends the reply.
        QByteArray req = method.toLatin1() + ' ' + path.toUtf8() + " HTTP/1.0\r\nHost: snapd\r\n"
                         "X-Allow-Interaction: true\r\n";
        if (!json.isEmpty())
            req += "Content-Type: application/json\r\nContent-Length: " + QByteArray::number(json.size()) + "\r\n";
        s->write(req + "\r\n" + json);
    });
    connect(s, &QLocalSocket::readyRead, this, [s, reply] { *reply += s->readAll(); });
    connect(s, &QLocalSocket::disconnected, this, [s, reply, done] {
        *reply += s->readAll();
        const qsizetype body = reply->indexOf("\r\n\r\n");
        done(body < 0 ? QJsonObject() : QJsonDocument::fromJson(reply->mid(body + 4)).object());
        delete reply;
        s->deleteLater();
    });
    connect(s, &QLocalSocket::errorOccurred, this, [s, reply, done](QLocalSocket::LocalSocketError e) {
        if (e == QLocalSocket::PeerClosedError)
            return;   // the normal end: "disconnected" follows
        done(QJsonObject());
        s->disconnect();
        delete reply;
        s->deleteLater();
    });
    s->connectToServer(kSocket);
}

void SnapdClient::change(const QString &method, const QString &path, const QByteArray &json,
                         std::function<void(bool, const QString &)> done)
{
    request(method, path, json, [this, done](const QJsonObject &r) {
        if (r.value(QStringLiteral("type")).toString() == QLatin1String("async"))
            return follow(r.value(QStringLiteral("change")).toString(), done);
        const QString message = r.value(QStringLiteral("result")).toObject().value(QStringLiteral("message")).toString();
        done(false, message.isEmpty() ? tr("The Snap service (snapd) didn't answer.") : message);
    });
}

void SnapdClient::follow(const QString &id, std::function<void(bool, const QString &)> done)
{
    request(QStringLiteral("GET"), QStringLiteral("/v2/changes/") + id, {}, [this, id, done](const QJsonObject &r) {
        const QJsonObject c = r.value(QStringLiteral("result")).toObject();
        if (c.isEmpty())
            return done(false, tr("The Snap service (snapd) didn't answer."));
        if (!c.value(QStringLiteral("ready")).toBool()) {
            QTimer::singleShot(1000, this, [this, id, done] { follow(id, done); });
            return;
        }
        const bool ok = c.value(QStringLiteral("status")).toString() == QLatin1String("Done");
        done(ok, ok ? QString() : c.value(QStringLiteral("err")).toString());
    });
}
