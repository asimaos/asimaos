#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QJsonObject>
#include <QObject>
#include <QTimer>
#include <QUrl>
#include <QVariantList>
#include <QVariantMap>

class ServiceClient;

// Data and actions behind Asima Updater: update files and the versions in the start menu.
//
// System mode: talks to asimaos-updaterd (io.github.asimaos.Updater1, see apps/DESIGN.md): status as
// JSON, every change as an asynchronous call that polkit authorises. Without the service it shows
// what an unprivileged app can read, and says that changes need the service.
// Demo mode: realistic sample data (design work, screenshots, running outside Asima OS).
class UpdaterEngine : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Backend)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    // name, version, previousVersion (the backup), pendingVersion (starts next), kernel, hostname
    Q_PROPERTY(QVariantMap system READ system NOTIFY systemChanged)
    Q_PROPERTY(QVariantList versions READ versions NOTIFY versionsChanged)
    Q_PROPERTY(QVariantMap pendingUpdate READ pendingUpdate NOTIFY pendingUpdateChanged)

public:
    // The one instance main() makes; QML gets it as the Backend singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline UpdaterEngine *instance = nullptr;
    static UpdaterEngine *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit UpdaterEngine(bool demo, QObject *parent = nullptr);

    bool demo() const { return m_demo; }
    QVariantMap system() const { return m_system; }
    QVariantList versions() const { return m_versions; }
    QVariantMap pendingUpdate() const { return m_update; }

    Q_INVOKABLE void selectUpdateFile(const QUrl &file);
    Q_INVOKABLE void clearUpdate();
    // Installs the chosen update, or writes a fresh copy when it is the running version (D21).
    Q_INVOKABLE void installUpdate();
    Q_INVOKABLE void restartNow();
    // The start menu: "Asima OS (current)" on top, "Asima OS (backup)" below it.
    Q_INVOKABLE void startVersionOnce(const QString &version);    // the next start only
    Q_INVOKABLE void makeVersionCurrent(const QString &version);  // from the next start on

    // Puts the demo into a state for screenshots: update-ready, update-installing, update-done,
    // update-error.
    void setDemoState(const QString &state);

signals:
    void systemChanged();
    void versionsChanged();
    void pendingUpdateChanged();
    void toast(const QString &message, const QString &tone);

private slots:
    void onJobProgress(const QString &job, double progress, const QString &step, const QString &detail);
    void onJobFinished(const QString &job, bool ok, const QString &message);

private:
    void refreshFromService();
    void applyServiceStatus(const QJsonObject &s);
    void loadDemo();
    void loadSystem();
    void needsService(const QString &action);
    void setUpdateState(const QString &state);
    void installTick();

    ServiceClient *m_service = nullptr;   // asimaos-updaterd; none in demo mode or when it's missing
    bool m_demo;
    QString m_updatePath, m_job;
    QVariantMap m_system, m_update;
    QVariantList m_versions;
    QTimer m_installTimer;
    double m_progress = 0.0;
};
