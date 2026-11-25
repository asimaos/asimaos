#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QJsonObject>
#include <QObject>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

class ServiceClient;

// Data and actions behind Asima Maintenance: the system's health, Secure Boot and the disk's
// secrets, snapshots and backups, changed system files, Flatpak clean-up and the factory reset.
//
// System mode: talks to asimaos-maintenanced (io.github.asimaos.Maintenance1, see apps/DESIGN.md):
// status as JSON, every change as an asynchronous call that polkit authorises. Without the
// service it shows what an unprivileged app can read, and says that changes need the service.
// Demo mode: realistic sample data (design work, screenshots, running outside Asima OS).
class MaintenanceEngine : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Backend)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    Q_PROPERTY(QVariantMap system READ system NOTIFY systemChanged)
    Q_PROPERTY(QVariantMap storage READ storage NOTIFY storageChanged)
    Q_PROPERTY(QVariantList snapshots READ snapshots NOTIFY snapshotsChanged)
    Q_PROPERTY(QVariantMap backup READ backup NOTIFY backupChanged)
    // Secure Boot and the Asima OS key, encryption, TPM unlock, system protection (Security page).
    Q_PROPERTY(QVariantMap protection READ protection NOTIFY protectionChanged)
    // The person using the app: suggested account for the fresh system after a factory reset.
    Q_PROPERTY(QVariantMap account READ account CONSTANT)
    // Factory reset preparation: { prepared, recoveryKey } (the key is shown once).
    Q_PROPERTY(QVariantMap reset READ reset NOTIFY resetChanged)
    Q_PROPERTY(QVariantList health READ health NOTIFY healthChanged)
    Q_PROPERTY(QVariantList activity READ activity NOTIFY activityChanged)
    // The firmware's start order: { known, asimaosFirst, first } (Overview: "Start Asima OS first").
    Q_PROPERTY(QVariantMap bootOrder READ bootOrder NOTIFY bootOrderChanged)
    // Files in /etc that differ from the factory version (System changes page).
    Q_PROPERTY(QVariantList changes READ changes NOTIFY changesChanged)
    // Flatpak apps: { apps, size, unused } (bytes; -1 when unknown).
    Q_PROPERTY(QVariantMap flatpak READ flatpak NOTIFY flatpakChanged)
    Q_PROPERTY(bool checking READ checking NOTIFY checkingChanged)
    // Failed starts and kernel crashes (Health page): { kind, title, detail, when, log }.
    Q_PROPERTY(QVariantList crashes READ crashes NOTIFY crashesChanged)
    // The backups that can be restored (after loadBackups()): { id, when, detail }.
    Q_PROPERTY(QVariantList backupList READ backupList NOTIFY backupListChanged)
    // The problem report (after makeReport()), plain text the person edits before saving.
    Q_PROPERTY(QString report READ report NOTIFY reportChanged)
    Q_PROPERTY(bool reporting READ reporting NOTIFY reportChanged)
    Q_PROPERTY(QString uiHint READ uiHint NOTIFY uiHintChanged)

public:
    // The one instance main() makes; QML gets it as the Backend singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline MaintenanceEngine *instance = nullptr;
    static MaintenanceEngine *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit MaintenanceEngine(bool demo, QObject *parent = nullptr);

    bool demo() const { return m_demo; }
    QVariantMap system() const { return m_system; }
    QVariantMap storage() const { return m_storage; }
    QVariantList snapshots() const { return m_snapshots; }
    QVariantMap backup() const { return m_backup; }
    QVariantMap protection() const { return m_protection; }
    QVariantMap account() const { return m_account; }
    QVariantMap reset() const { return m_reset; }
    QVariantList health() const { return m_health; }
    QVariantList activity() const { return m_activity; }
    QVariantList changes() const { return m_changes; }
    QVariantMap bootOrder() const { return m_bootOrder; }
    QVariantMap flatpak() const { return m_flatpak; }
    bool checking() const { return m_checking; }
    QVariantList crashes() const { return m_crashes; }
    QVariantList backupList() const { return m_backupList; }
    QString report() const { return m_report; }
    bool reporting() const { return m_reporting; }
    QString uiHint() const { return m_uiHint; }

    Q_INVOKABLE void backupNow();
    Q_INVOKABLE void snapshotNow();
    // A folder (USB drive, mounted share) or a restic address, and the backup's password.
    Q_INVOKABLE void configureBackup(const QString &destination, const QString &password);
    Q_INVOKABLE void openSnapshots();
    // Automatic snapshots: "daily", "weekly", "monthly" or "off".
    Q_INVOKABLE void setSnapshotSchedule(const QString &schedule);
    // Restoring: the list of backups, then one into a new folder in the person's home.
    Q_INVOKABLE void loadBackups();
    Q_INVOKABLE void restoreBackup(const QString &id);
    // The problem report: made on this computer; saved or copied by the person.
    Q_INVOKABLE void makeReport();
    Q_INVOKABLE bool saveReport(const QString &text, const QUrl &file);
    Q_INVOKABLE void copyText(const QString &text);
    // Asima Hub (the online app): opens it, or its download when it isn't installed (D54).
    Q_INVOKABLE void openHub();
    Q_INVOKABLE void runHealthCheck();
    // Security (D10, D16). Secrets are passed straight on to asimaos-maintenanced and never kept.
    Q_INVOKABLE QString approveAsimaKey();          // queues the MOK import; returns the one-time code
    Q_INVOKABLE void cancelKeyApproval();
    // Approved keys besides this version's (protection.otherKeys): removed and blocked (MOKX) at
    // the next start, confirmed with a one-time code (protection.retireCode).
    Q_INVOKABLE void retireKeys(const QStringList &ids);
    // Own Secure Boot keys in the firmware (protection.ownKeys): save the firmware's lists and
    // make this computer's keys; then, in Setup Mode, write them (leaving out `drop`).
    Q_INVOKABLE void prepareOwnKeys();
    Q_INVOKABLE void enrollOwnKeys(const QStringList &drop);
    Q_INVOKABLE void cancelRetireKeys();
    Q_INVOKABLE void restartNow();
    Q_INVOKABLE void restartToFirmware();            // works without the service (logind)
    Q_INVOKABLE void enrollTpm(const QString &passphrase);
    // The optional way (D49): the TPM opens the disk only together with a PIN typed at every start.
    Q_INVOKABLE void enrollTpmPin(const QString &passphrase, const QString &pin);
    Q_INVOKABLE void removeTpm();
    Q_INVOKABLE void changePassphrase(const QString &current, const QString &next);
    Q_INVOKABLE QString newRecoveryKey(const QString &current);
    // Reset to a fresh installation of the current version (D7, D12, D13).
    Q_INVOKABLE void prepareFactoryReset(const QVariantMap &fresh);   // account + new disk secrets first
    Q_INVOKABLE void cancelFactoryReset();
    Q_INVOKABLE void factoryReset();
    // System changes: the difference to the factory file, and restoring it at the next start.
    Q_INVOKABLE QString diff(const QString &path) const;
    Q_INVOKABLE void revertChange(const QString &path);
    // Puts the entry this start came from first in the firmware's start order.
    Q_INVOKABLE void startAsimaFirst();
    // Every setting back to this version's defaults at the next start (restarts the computer).
    Q_INVOKABLE void resetSettings();
    Q_INVOKABLE void openApp(const QString &command);

    // Puts the demo into a state for screenshots: backup-tab, backup-setup, health-checking,
    // security-setup, security-pending, security-plain, reset-key.
    void setDemoState(const QString &state);

signals:
    void bootOrderChanged();
    void systemChanged();
    void storageChanged();
    void snapshotsChanged();
    void backupChanged();
    void protectionChanged();
    void resetChanged();
    void healthChanged();
    void activityChanged();
    void changesChanged();
    void flatpakChanged();
    void checkingChanged();
    void uiHintChanged();
    void crashesChanged();
    void backupListChanged();
    void reportChanged();
    void toast(const QString &message, const QString &tone);
    void recoveryKeyMade(const QString &key);   // system mode: newRecoveryKey() answers later

private:
    void refreshFromService();
    void applyServiceStatus(const QJsonObject &s);
    void loadDemo();
    void loadSystem();
    void readChanges();
    void readFlatpak();
    void needsService(const QString &action);

    ServiceClient *m_service = nullptr;   // asimaos-maintenanced; none in demo mode or when it's missing
    bool m_demo;
    bool m_checking = false;
    bool m_reporting = false;
    QString m_report;
    QVariantList m_crashes, m_backupList;
    QString m_uiHint;
    QVariantMap m_system, m_storage, m_backup, m_protection, m_account, m_reset, m_flatpak;
    QVariantList m_snapshots, m_health, m_activity, m_changes;
    QVariantMap m_bootOrder;
    QStringList m_newDefaults;   // from asimaos-maintenanced
};
