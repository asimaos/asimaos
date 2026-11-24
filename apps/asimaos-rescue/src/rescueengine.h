#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QObject>
#include <QPointer>
#include <QProcess>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>

// Data and actions behind Asima Rescue, the repair system on the ESP (a signed UKI that runs
// entirely in RAM; it never mounts the installed /usr).
//
// System mode: drives the rescue engine `asimaos-rescue-engine` (it runs as root inside the rescue
// system): `status --json` once, then one ACTION --json per job, with the same JSON event lines as
// the installer engine (step / progress / log / error / done). Secrets go through stdin.
// Demo mode: a computer whose last start failed, with snapshots and an update file on a USB stick.
class RescueEngine : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Backend)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    Q_PROPERTY(QVariantMap status READ status NOTIFY statusChanged)
    Q_PROPERTY(QVariantList snapshots READ snapshots NOTIFY statusChanged)
    Q_PROPERTY(QVariantList updateFiles READ updateFiles NOTIFY statusChanged)
    Q_PROPERTY(QVariantMap job READ job NOTIFY jobChanged)

public:
    // The one instance main() makes; QML gets it as the Backend singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline RescueEngine *instance = nullptr;
    static RescueEngine *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit RescueEngine(bool demo, QObject *parent = nullptr);

    bool demo() const { return m_demo; }
    QVariantMap status() const { return m_status; }
    QVariantList snapshots() const { return m_snapshots; }
    QVariantList updateFiles() const { return m_updateFiles; }
    QVariantMap job() const { return m_job; }

    Q_INVOKABLE void refresh();
    // Unlocks the encrypted pool with the passphrase or the recovery key. Result arrives as unlocked().
    Q_INVOKABLE void unlock(const QString &secret);
    // Actions: check, repair, free-space, restore {snapshot}, reinstall {file}, start-previous,
    // approve-key, remove-tpm, factory-reset {fullName, username, password, passphrase}, wipe {serialTail}.
    Q_INVOKABLE void start(const QString &action, const QVariantMap &args = {});
    Q_INVOKABLE void clearJob();
    Q_INVOKABLE void restart();
    Q_INVOKABLE void powerOff();

    // Demo states for screenshots: unlocked, check-running, check-done, wipe-running.
    void setDemoState(const QString &state);

signals:
    void statusChanged();
    void jobChanged();
    void unlocked(bool ok, const QString &message);

private:
    void loadDemo();
    void loadSystem();
    QString engine() const;
    void beginJob(const QString &action, const QString &title, const QStringList &stepTitles);
    void setStep(int index, const QString &state);
    void handleEvent(const QByteArray &line);
    void finish(bool ok, const QString &message);
    void demoTick();

    bool m_demo;
    QVariantMap m_status, m_job;
    QVariantList m_snapshots, m_updateFiles;
    QStringList m_stepIds;
    QPointer<QProcess> m_process;
    QByteArray m_buffer;
    QTimer m_demoTimer;
    double m_demoProgress = 0.0;
};
