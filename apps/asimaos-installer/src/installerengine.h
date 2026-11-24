#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QObject>
#include <QPointer>
#include <QProcess>
#include <QSet>
#include <QTimer>
#include <QUrl>
#include <QVariantList>
#include <QVariantMap>

// Data and actions behind the Asima OS Installer.
//
// System mode: reads this computer (disks via lsblk, firmware, TPM, memory, GPU) and drives the
// installer engine `asimaos-install --json` (as root, or through pkexec).
// Demo mode: realistic sample data (design work, screenshots, running outside the live ISO).
class InstallerEngine : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Backend)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    Q_PROPERTY(QVariantMap system READ system CONSTANT)
    Q_PROPERTY(QVariantList requirements READ requirements NOTIFY disksChanged)
    Q_PROPERTY(QVariantList languages READ languages CONSTANT)
    Q_PROPERTY(QVariantList keyboards READ keyboards CONSTANT)
    Q_PROPERTY(QVariantList timezones READ timezones CONSTANT)
    Q_PROPERTY(QVariantList disks READ disks NOTIFY disksChanged)
    Q_PROPERTY(QVariantMap defaults READ defaults CONSTANT)
    Q_PROPERTY(QVariantMap install READ install NOTIFY installChanged)

public:
    // The one instance main() makes; QML gets it as the Backend singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline InstallerEngine *instance = nullptr;
    static InstallerEngine *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    InstallerEngine(bool demo, bool dryRun, QObject *parent = nullptr);

    bool demo() const { return m_demo; }
    QVariantMap system() const { return m_system; }
    QVariantList requirements() const;
    QVariantList languages() const { return m_languages; }
    QVariantList keyboards() const { return m_keyboards; }
    QVariantList timezones() const { return m_timezones; }
    QVariantList disks() const { return m_disks; }
    QVariantMap defaults() const { return m_defaults; }
    QVariantMap install() const { return m_install; }

    Q_INVOKABLE void refreshDisks();
    Q_INVOKABLE QString localTime(const QString &timezone) const;
    // Account checks shared with the engine's own rules (asimaos-install --user / --hostname).
    Q_INVOKABLE QString usernameProblem(const QString &name) const;    // empty when fine
    Q_INVOKABLE QString hostnameProblem(const QString &name) const;    // empty when fine
    Q_INVOKABLE QString suggestUsername(const QString &fullName) const;
    Q_INVOKABLE QString suggestHostname(const QString &username) const;
    Q_INVOKABLE void startInstall(const QVariantMap &options);
    Q_INVOKABLE void copyText(const QString &text);
    Q_INVOKABLE bool saveText(const QUrl &file, const QString &text);
    Q_INVOKABLE void restart();
    Q_INVOKABLE void quit();
    // "Repair this computer": a terminal with the repair tool's guided mode.
    Q_INVOKABLE void openRepair();

    // Demo states for screenshots: installing, error, done; "-plain" installs without encryption.
    void setDemoState(const QString &state);

signals:
    void disksChanged();
    void installChanged();
    void toast(const QString &message, const QString &tone);

private:
    void loadLists();
    void loadSystem();
    void loadDemoSystem();
    void resetInstall(bool encrypt = true);
    void setStep(const QString &id, const QString &state);
    void handleEvent(const QByteArray &line);
    void fail(const QString &message);
    void demoTick();
    QString findEngine() const;

    bool m_demo;
    bool m_dryRun;
    QVariantMap m_system, m_defaults, m_install;
    QVariantList m_languages, m_keyboards, m_timezones, m_disks;
    QPointer<QProcess> m_process;
    QByteArray m_buffer;
    QStringList m_log;
    QTimer m_demoTimer;
    double m_demoProgress = 0.0;
    bool m_encrypt = true;
    QSet<QString> m_systemNames;   // accounts and groups the system already has
};
