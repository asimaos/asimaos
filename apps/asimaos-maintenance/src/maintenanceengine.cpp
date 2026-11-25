#include "maintenanceengine.h"
#include "serviceclient.h"
#include "util.h"

#include <QDBusMessage>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QClipboard>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLocale>
#include <QProcess>
#include <QRandomGenerator>
#include <QRegularExpression>
#include <QStorageInfo>
#include <QSysInfo>
#include <QTimeZone>
#include <QTimer>
#include <QUrl>

#include <algorithm>

using Util::item;

namespace {

const QString kEtcUpper = QStringLiteral("/run/initramfs/data/overlay/etc/upper");
const QString kFactoryEtc = QStringLiteral("/usr/share/factory/etc");

QString readFirstLine(const QString &path)
{
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly))
        return {};
    return QString::fromUtf8(f.readLine()).trimmed();
}

QString readAll(const QString &path)
{
    QFile f(path);
    return f.open(QIODevice::ReadOnly) ? QString::fromUtf8(f.readAll()) : QString();
}

// Recovery keys use the same 8 × 8 "modhex" alphabet as systemd-cryptenroll.
QString demoRecoveryKey()
{
    static const char alphabet[] = "cbdefghijklnrtuv";
    QStringList groups;
    for (int g = 0; g < 8; ++g) {
        QString group;
        for (int c = 0; c < 8; ++c)
            group.append(QLatin1Char(alphabet[QRandomGenerator::global()->bounded(16)]));
        groups << group;
    }
    return groups.join(QLatin1Char('-'));
}

} // namespace

MaintenanceEngine::MaintenanceEngine(bool demo, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
{
    if (m_demo) {
        loadDemo();
        return;
    }
    loadSystem();
    auto *service = new ServiceClient(QStringLiteral("Maintenance1"), this);
    if (!service->available()) {
        delete service;
        return;
    }
    m_service = service;
    connect(m_service, &ServiceClient::failed, this, [this](const QString &message) { emit toast(message, QStringLiteral("warn")); });
    refreshFromService();
}

// ───────────────────────────────────────────────────────────────────────── asimaos-maintenanced

void MaintenanceEngine::refreshFromService()
{
    m_service->call(QStringLiteral("Status"), {}, [this](const QDBusMessage &r) { applyServiceStatus(ServiceClient::json(r)); });
}

void MaintenanceEngine::applyServiceStatus(const QJsonObject &s)
{
    m_system.insert("version", s.value("version").toString());
    // The settings came from a version older than the last update's migration steps reach (D52).
    m_system.insert("settingsBehind", s.value("settingsBehind").toString().section(QLatin1Char(' '), 0, 0));
    m_bootOrder = s.value("bootOrder").toObject().toVariantMap();
    emit bootOrderChanged();

    // Changed files whose factory default an update changed.
    m_newDefaults.clear();
    for (const QJsonValue &v : s.value("etcNewDefaults").toArray())
        m_newDefaults << v.toString();
    bool marked = false;
    for (QVariant &c : m_changes) {
        QVariantMap m = c.toMap();
        const bool isNew = m_newDefaults.contains(m.value("path").toString());
        if (m.value("newDefault").toBool() != isNew) {
            m.insert("newDefault", isNew);
            c = m;
            marked = true;
        }
    }
    if (marked)
        emit changesChanged();

    const QJsonObject sec = s.value("security").toObject();
    const QString pendingCode = m_protection.value("pendingCode").toString();
    const QString retireCode = m_protection.value("retireCode").toString();
    m_protection = sec.toVariantMap();
    m_protection.insert("pendingCode", sec.value("keyPending").toBool() ? pendingCode : QString());
    m_protection.insert("retireCode", sec.value("retirePending").toBool() ? retireCode : QString());

    const QJsonObject st = s.value("storage").toObject();
    const double total = st.value("total").toDouble(), free = st.value("free").toDouble();
    m_storage.insert("total", total);
    m_storage.insert("free", free);
    m_storage.insert("used", total - free);
    m_storage.insert("reserveOk", st.value("reserve").toBool());

    const bool verified = sec.value("verity").toBool();
    QStringList facts;
    if (verified) facts << tr("Verified system");
    if (sec.value("encrypted").toBool()) facts << tr("Encrypted disk");
    if (sec.value("secureBoot").toString() == QLatin1String("on")) facts << tr("Secure Boot on");
    m_system.insert("health", verified ? "ok" : "warn");
    m_system.insert("healthShort", verified ? tr("System healthy") : tr("Needs a look"));
    m_system.insert("healthTitle", verified ? tr("Everything looks good") : tr("Needs a look"));
    m_system.insert("healthText", verified ? tr("System files are verified, and Asima OS looks after itself in the background.")
                                           : tr("System files couldn't be confirmed as verified. Run a health check."));
    m_system.insert("facts", facts);

    // Snapshots: home/<YYYY-MM-DD-HHMM> (UTC) every day, update/<version> before each update.
    m_snapshots.clear();
    int daily = 0, updates = 0;
    for (const QJsonValue &v : s.value("snapshots").toArray()) {
        const QJsonObject o = v.toObject();
        const bool update = o.value("kind").toString() == QLatin1String("update");
        QDateTime when = QDateTime::fromString(o.value("when").toString(), QStringLiteral("yyyy-MM-dd-HHmm"));
        when.setTimeZone(QTimeZone::UTC);
        (update ? updates : daily)++;
        m_snapshots.append(QVariantMap{
            {"kind", update ? "update" : "daily"},
            {"label", update ? tr("Before updating to %1").arg(o.value("label").toString()) : tr("Snapshot of your files")},
            {"when", when.isValid() ? QLocale().toString(when.toLocalTime(), QLocale::ShortFormat) : tr("Kept for going back")},
            {"size", o.value("size").toDouble(-1)},
        });
    }
    const QJsonObject b = s.value("backup").toObject();
    const auto date = [](const QJsonValue &secs) {
        return secs.toDouble() > 0 ? QLocale().toString(QDateTime::fromSecsSinceEpoch(qint64(secs.toDouble())), QLocale::ShortFormat) : QString();
    };
    const bool lastOk = b.value("lastOk").toBool();
    m_backup = QVariantMap{
        {"configured", b.value("configured").toBool()},
        {"destination", b.value("destination").toString()},
        {"destinationDetail", tr("Encrypted with restic · every day · checked by restoring a sample")},
        {"schedule", tr("Every day")},
        {"running", b.value("running").toBool()},
        {"lastRun", b.value("running").toBool() ? tr("Running now…") : date(b.value("lastRun"))},
        {"lastText", b.value("lastRun").toDouble() <= 0 ? tr("Not yet") : lastOk ? tr("Finished") : Util::sentence(b.value("lastMessage").toString())},
        {"lastTone", b.value("lastRun").toDouble() <= 0 ? "neutral" : lastOk ? "ok" : "error"},
        {"restoreTest", b.value("lastTest").toDouble() <= 0 ? tr("Not yet")
                        : b.value("lastTestOk").toBool() ? tr("Passed · %1").arg(date(b.value("lastTest"))) : tr("Failed")},
        {"restoreTone", b.value("lastTest").toDouble() <= 0 ? "neutral" : b.value("lastTestOk").toBool() ? "ok" : "error"},
        {"kept", b.value("kept").toInt() > 0 ? tr("%n version(s) · 7 daily, 4 weekly", nullptr, b.value("kept").toInt()) : tr("7 daily · 4 weekly")},
        {"repoSize", b.value("repoSize").toDouble()},
        {"snapshotsDaily", daily},
        {"snapshotsUpdate", updates},
        {"snapshotsSize", -1},
    };

    m_backup.insert("snapshotSchedule", s.value("snapshotSchedule").toString(QStringLiteral("daily")));
    // The disk-encryption header's copy: only on an encrypted disk, with each backup.
    m_backup.insert("headerText", b.value("headerSaved").toDouble() > 0 ? tr("Copied · %1").arg(date(b.value("headerSaved")))
                                  : tr("With the next backup"));
    const QJsonObject restore = b.value("restore").toObject();
    m_backup.insert("restoreRunning", restore.value("running").toBool());
    m_backup.insert("restoreText", restore.isEmpty() ? QString()
                    : restore.value("running").toBool() ? tr("Restoring…")
                    : restore.value("ok").toBool() ? tr("Restored into the folder “%1” in your home").arg(restore.value("folder").toString())
                    : Util::sentence(restore.value("message").toString()));
    m_backup.insert("restoreTone", restore.isEmpty() || restore.value("running").toBool() ? "neutral"
                    : restore.value("ok").toBool() ? "ok" : "error");
    if (restore.value("running").toBool())
        QTimer::singleShot(5000, this, &MaintenanceEngine::refreshFromService);

    // Failed starts and kernel crashes, newest first.
    m_crashes.clear();
    for (const QJsonValue &v : s.value("crashes").toArray()) {
        const QJsonObject o = v.toObject();
        const QString when = QLocale().toString(QDateTime::fromSecsSinceEpoch(qint64(o.value("at").toDouble())), QLocale::ShortFormat);
        if (o.value("kind").toString() == QLatin1String("start")) {
            m_crashes.append(QVariantMap{
                {"kind", "start"},
                {"title", tr("Asima OS %1 didn't start properly").arg(o.value("version").toString())},
                {"detail", Util::sentence(o.value("detail").toString())},
                {"when", when}, {"log", QString()},
            });
        } else {
            m_crashes.append(QVariantMap{
                {"kind", "crash"},
                {"title", tr("The system stopped with an error")},
                {"detail", tr("The kernel's last messages were kept")},
                {"when", when}, {"log", o.value("log").toString()},
            });
        }
    }
    emit crashesChanged();

    if (!s.value("reset").toObject().value("prepared").toBool() && m_reset.value("prepared").toBool())
        m_reset = QVariantMap{{"prepared", false}, {"recoveryKey", QString()}};

    emit snapshotsChanged();
    emit backupChanged();
    emit systemChanged();
    emit protectionChanged();
    emit storageChanged();
    emit resetChanged();
}

// ───────────────────────────────────────────────────────────────────────── demo data

// A failed start of an update (it went back by itself) and an older kernel crash, for the Health
// page's demo.
static QVariantList demoCrashes()
{
    return {
        item({{"kind", "start"}, {"title", MaintenanceEngine::tr("Asima OS %1 didn't start properly").arg("2026.11.17.1")},
              {"detail", MaintenanceEngine::tr("The login screen did not start.")}, {"when", MaintenanceEngine::tr("17 Nov, 08:12")}, {"log", QString()}}),
        item({{"kind", "crash"}, {"title", MaintenanceEngine::tr("The system stopped with an error")},
              {"detail", MaintenanceEngine::tr("The kernel's last messages were kept")}, {"when", MaintenanceEngine::tr("2 Nov, 22:47")},
              {"log", QStringLiteral("BUG: kernel NULL pointer dereference, address: 0000000000000008\n"
                                     "RIP: 0010:example_driver_irq+0x1c/0x90 [example]\n"
                                     "Kernel panic - not syncing: Fatal exception in interrupt")}}),
    };
}

void MaintenanceEngine::loadDemo()
{
    m_system = item({
        {"name", "Asima OS"},
        {"version", "2026.11.15.1"},
        {"kernel", "Linux 7.3.4-arch1-1"},
        {"desktop", "KDE Plasma 6.8"},
        {"hostname", "asimaos"},
        {"health", "ok"},
        {"healthTitle", tr("Everything looks good")},
        {"healthText", tr("System files are verified, your disk is encrypted and the last boot was healthy.")},
        {"facts", QStringList{tr("Verified system"), tr("Encrypted disk"), tr("Secure Boot on")}},
        {"bootedAt", tr("Today, 09:12")},
        {"lastCheck", tr("3 days ago")},
        {"maintenance", QVariantList{
            item({{"title", tr("Disk check (scrub)")}, {"when", tr("Weekly · Sunday 02:00")}, {"last", tr("3 days ago · 0 errors")}, {"icon", "drive-harddisk-symbolic"}}),
            item({{"title", tr("TRIM for the SSD")}, {"when", tr("Weekly · Sunday 03:00")}, {"last", tr("3 days ago")}, {"icon", "view-refresh"}}),
            item({{"title", tr("Free-space balance")}, {"when", tr("Monthly · 1st day")}, {"last", tr("2 Nov")}, {"icon", "view-statistics"}}),
            item({{"title", tr("SSD wear log")}, {"when", tr("Weekly")}, {"last", tr("3 days ago")}, {"icon", "speedometer"}}),
        }},
    });

    m_storage = item({
        {"total", 1000.2e9}, {"used", 312.6e9}, {"free", 687.6e9},
        {"os", 9.7e9}, {"snapshots", 6.2e9}, {"reserve", 2.147e9}, {"reserveOk", true},
        {"device", tr("Samsung 990 PRO · 1 TB NVMe")},
    });

    m_snapshots = {
        item({{"when", tr("Today, 03:00")}, {"kind", "daily"}, {"label", tr("Daily snapshot")}, {"size", 0.212e9}}),
        item({{"when", tr("Yesterday, 03:00")}, {"kind", "daily"}, {"label", tr("Daily snapshot")}, {"size", 0.198e9}}),
        item({{"when", tr("15 Nov, 08:41")}, {"kind", "update"}, {"label", tr("Before updating to 2026.11.15.1")}, {"size", 0.96e9}}),
        item({{"when", tr("14 Nov, 03:00")}, {"kind", "daily"}, {"label", tr("Daily snapshot")}, {"size", 0.31e9}}),
        item({{"when", tr("13 Nov, 03:00")}, {"kind", "daily"}, {"label", tr("Daily snapshot")}, {"size", 0.27e9}}),
        item({{"when", tr("1 Nov, 19:20")}, {"kind", "update"}, {"label", tr("Before updating to 2026.11.01.2")}, {"size", 0.88e9}}),
    };

    m_backup = item({
        {"configured", true},
        {"destination", tr("Archive SSD")},
        {"destinationDetail", tr("USB drive · 2 TB · encrypted")},
        {"schedule", tr("Every day at 21:00")},
        {"lastRun", tr("Yesterday, 21:04")},
        {"lastText", tr("Finished · 1.4 GB new")},
        {"lastTone", "ok"},
        {"restoreTest", tr("Passed · 13 Nov")},
        {"restoreTone", "ok"},
        {"kept", tr("7 daily · 4 weekly")},
        {"headerText", tr("Copied · Yesterday, 21:04")},
        {"repoSize", 38.2e9},
        {"snapshotsDaily", 14},
        {"snapshotsUpdate", 2},
        {"snapshotsSize", 6.2e9},
        {"snapshotSchedule", "daily"},
        {"restoreText", QString()},
        {"restoreTone", "neutral"},
    });

    m_crashes = {};   // a healthy computer; the "health-report" state shows a failed start and a crash

    m_protection = item({
        {"managed", true}, {"secureBoot", "on"}, {"keyApproved", true}, {"keyPending", false}, {"pendingCode", QString()},
        {"encrypted", true}, {"tpm", true}, {"tpmEnrolled", true}, {"tpmPin", false}, {"tpmKnown", true},
        {"lockdown", "integrity"}, {"verity", true},
        {"lsms", QStringList{"capability", "landlock", "lockdown", "yama", "apparmor", "bpf"}},
    });
    m_account = item({{"fullName", "Asha Roy"}, {"username", "asha"}});
    m_reset = item({{"prepared", false}, {"recoveryKey", QString()}});

    m_health = {
        item({{"title", tr("System files")}, {"detail", tr("Verified by dm-verity every time they are read")}, {"tone", "ok"}, {"value", tr("Verified")}, {"icon", "kt-check-data"}}),
        item({{"title", tr("Last boot")}, {"detail", tr("2026.11.15.1 started on the first try")}, {"tone", "ok"}, {"value", tr("Good")}, {"icon", "system-reboot"}}),
        item({{"title", tr("Disk check")}, {"detail", tr("Weekly scrub · last run Sunday · 0 errors")}, {"tone", "ok"}, {"value", tr("No errors")}, {"icon", "drive-harddisk-symbolic"}}),
        item({{"title", tr("Free space")}, {"detail", tr("687 GB free · 2 GB emergency reserve intact")}, {"tone", "ok"}, {"value", tr("Plenty")}, {"icon", "folder-open"}}),
        item({{"title", tr("SSD wear")}, {"detail", tr("12.4 TB written · 4% of rated life used")}, {"tone", "ok"}, {"value", "4%"}, {"icon", "speedometer"}}),
        item({{"title", tr("Memory guard")}, {"detail", tr("Keeps the desktop responsive under heavy load")}, {"tone", "ok"}, {"value", tr("Active")}, {"icon", "system-run"}}),
        item({{"title", tr("Failed boots")}, {"detail", tr("None recorded since 1 Nov")}, {"tone", "ok"}, {"value", tr("None")}, {"icon", "task-complete"}}),
    };

    m_activity = {
        item({{"when", tr("Today, 09:12")}, {"title", tr("Asima OS 2026.11.15.1 started")}, {"detail", tr("Boot checks passed · marked good")}, {"tone", "ok"}, {"icon", "system-reboot"}}),
        item({{"when", tr("Today, 03:00")}, {"title", tr("Snapshot of your files")}, {"detail", tr("212 MB · kept 14 days")}, {"tone", "info"}, {"icon", "view-history"}}),
        item({{"when", tr("Yesterday, 21:04")}, {"title", tr("Backup finished")}, {"detail", tr("Archive SSD · 1.4 GB new")}, {"tone", "ok"}, {"icon", "backup"}}),
        item({{"when", tr("Sunday, 02:00")}, {"title", tr("Disk check finished")}, {"detail", tr("No errors found")}, {"tone", "ok"}, {"icon", "drive-harddisk-symbolic"}}),
    };

    m_changes = {
        item({{"path", "/etc/hostname"}, {"kind", "modified"},
              {"diff", "--- factory/etc/hostname\n+++ /etc/hostname\n@@ -1 +1 @@\n-asimaos\n+ally-x"}}),
        item({{"path", "/etc/locale.conf"}, {"kind", "modified"},
              {"diff", "--- factory/etc/locale.conf\n+++ /etc/locale.conf\n@@ -1 +1 @@\n-LANG=C.UTF-8\n+LANG=en_IN.UTF-8"}}),
        item({{"path", "/etc/vconsole.conf"}, {"kind", "modified"},
              {"newDefault", true}, {"diff", "--- factory/etc/vconsole.conf\n+++ /etc/vconsole.conf\n@@ -1 +1,3 @@\n KEYMAP=us\n+XKBLAYOUT=in\n+XKBVARIANT=eng"}}),
        item({{"path", "/etc/asimaos/install.conf"}, {"kind", "added"},
              {"diff", "--- /dev/null\n+++ /etc/asimaos/install.conf\n@@ -0,0 +1,3 @@\n+# Written by the Asima OS Installer.\n+INSTALLED_VERSION=2026.11.15.1\n+TPM_ENROLL=yes"}}),
        item({{"path", "/etc/NetworkManager/system-connections/Home Wi-Fi.nmconnection"}, {"kind", "added"}, {"secret", true}, {"diff", ""}}),
        item({{"path", "/etc/ssh/sshd_config.d/50-asimaos.conf"}, {"kind", "deleted"},
              {"diff", "--- factory/etc/ssh/sshd_config.d/50-asimaos.conf\n+++ (removed)\n@@ -1,2 +0,0 @@\n-PasswordAuthentication no\n-KbdInteractiveAuthentication no"}}),
    };

    m_flatpak = item({{"apps", 27}, {"size", 9.1e9}, {"unused", 1.2e9}});
}

void MaintenanceEngine::setDemoState(const QString &state)
{
    if (state == QLatin1String("windows-first")) {
        m_bootOrder = item({{"known", true}, {"asimaosFirst", false}, {"first", "Windows Boot Manager"}});
        emit bootOrderChanged();
    } else if (state == QLatin1String("health-checking")) {
        m_checking = true;
        emit checkingChanged();
    } else if (state.startsWith(QLatin1String("security-"))) {
        // A computer that hasn't been set up yet: Secure Boot off, TPM unlock off.
        m_protection.insert("secureBoot", "off");
        m_protection.insert("keyApproved", false);
        m_protection.insert("tpmEnrolled", false);
        if (state == QLatin1String("security-pending"))
            approveAsimaKey();
        if (state == QLatin1String("security-plain"))
            m_protection.insert("encrypted", false);
        if (state == QLatin1String("security-pin")) {
            // Set up, with a PIN typed at every start (the optional way; automatic is the default).
            m_protection.insert("secureBoot", "on");
            m_protection.insert("keyApproved", true);
            m_protection.insert("tpmEnrolled", true);
            m_protection.insert("tpmPin", true);
        }
        if (state == QLatin1String("security-ownkeys") || state == QLatin1String("security-ownkeys-setup")) {
            const bool setup = state == QLatin1String("security-ownkeys-setup");
            m_protection.insert("secureBoot", setup ? "off" : "on");
            m_protection.insert("keyApproved", true);
            m_protection.insert("ownKeys", item({{"prepared", true}, {"setupMode", setup}, {"own", false},
                {"certificates", QVariantList{
                    item({{"id", "k1"}, {"list", "KEK"}, {"name", "Microsoft Corporation KEK CA 2011"}}),
                    item({{"id", "k2"}, {"list", "KEK"}, {"name", "Microsoft Corporation KEK 2K CA 2023"}}),
                    item({{"id", "k3"}, {"list", "KEK"}, {"name", "Lenovo Ltd. KEK CA 2012"}}),
                    item({{"id", "d1"}, {"list", "db"}, {"name", "Microsoft Windows Production PCA 2011"}}),
                    item({{"id", "d2"}, {"list", "db"}, {"name", "Microsoft Corporation UEFI CA 2011"}}),
                    item({{"id", "d3"}, {"list", "db"}, {"name", "Windows UEFI CA 2023"}}),
                    item({{"id", "d4"}, {"list", "db"}, {"name", "Microsoft UEFI CA 2023"}}),
                    item({{"id", "d5"}, {"list", "db"}, {"name", "My old Secure Boot key"}}),
                }}}));
        }
        if (state == QLatin1String("security-keys") || state == QLatin1String("security-retiring")) {
            // On: two keys of earlier builds and another system's are approved too.
            m_protection.insert("secureBoot", "on");
            m_protection.insert("keyApproved", true);
            m_protection.insert("otherKeys", QVariantList{
                item({{"id", "a1"}, {"name", "Asima OS Secure Boot (2026-11-16)"}, {"since", "Nov 16 10:12:00 2026 GMT"}}),
                item({{"id", "b2"}, {"name", "Asima OS Secure Boot (2026-11-18)"}, {"since", "Nov 18 08:40:00 2026 GMT"}}),
                item({{"id", "c3"}, {"name", "Ubuntu MOK signing key"}, {"since", "Mar 3 17:02:00 2025 GMT"}}),
            });
            if (state == QLatin1String("security-retiring"))
                retireKeys({QStringLiteral("a1"), QStringLiteral("b2")});
        }
        emit protectionChanged();
    } else if (state == QLatin1String("health-report")) {
        // An update that didn't start (and went back by itself) and an older crash: the Health
        // page's lower cards, with the report made.
        m_crashes = demoCrashes();
        for (QVariant &h : m_health) {
            QVariantMap m = h.toMap();
            if (m.value("icon") == QLatin1String("task-complete")) {
                m.insert("detail", tr("1 failed start (went back by itself), 1 crash since 1 Nov"));
                m.insert("tone", "warn");
                m.insert("value", tr("2"));
                h = m;
            }
        }
        emit crashesChanged();
        emit healthChanged();
        makeReport();
        m_uiHint = state;
        emit uiHintChanged();
    } else if (state == QLatin1String("backup-restore")) {
        loadBackups();
        m_uiHint = QStringLiteral("backup-tab");
        emit uiHintChanged();
    } else if (state == QLatin1String("reset-key")) {
        prepareFactoryReset({{QStringLiteral("username"), QStringLiteral("asha")}});
    } else if (state == QLatin1String("backup-setup")) {
        // No backup yet: the Backup tab shows how to set one up.
        m_backup.insert("configured", false);
        m_backup.remove("destination");
        emit backupChanged();
        m_uiHint = QStringLiteral("backup-tab");
        emit uiHintChanged();
    } else {
        m_uiHint = state;
        emit uiHintChanged();
    }
}

// ───────────────────────────────────────────────────────────────────────── system data

void MaintenanceEngine::loadSystem()
{
    const QVariantMap os = Util::osRelease();
    m_system = item({
        {"name", os.value("NAME", "Asima OS")},
        {"version", os.value("IMAGE_VERSION", os.value("VERSION_ID", tr("unknown")))},
        {"kernel", QStringLiteral("Linux ") + QSysInfo::kernelVersion()},
        {"desktop", QString()},
        {"hostname", QSysInfo::machineHostName()},
        {"health", "info"},
        {"healthTitle", tr("Limited view")},
        {"healthText", tr("Detailed checks and all changes need the Asima Maintenance service, which isn't available on this computer.")},
        {"facts", QStringList{}},
        {"maintenance", QVariantList{}},
    });

    const QStorageInfo home(QStringLiteral("/home"));
    m_storage = item({
        {"total", double(home.bytesTotal())},
        {"used", double(home.bytesTotal() - home.bytesAvailable())},
        {"free", double(home.bytesAvailable())},
        {"reserveOk", QFileInfo::exists(QStringLiteral("/run/initramfs/reserve/.emergency_ballast"))},
        {"device", QString::fromUtf8(home.device())},
    });

    // Security facts an unprivileged process can read.
    QFile sb(QStringLiteral("/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c"));
    const bool sbOn = sb.open(QIODevice::ReadOnly) && sb.readAll().right(1) == QByteArray(1, '\x01');
    const bool tpm = QFileInfo::exists(QStringLiteral("/sys/class/tpm/tpm0"));
    const bool luks = QFileInfo::exists(QStringLiteral("/dev/mapper/crypt_pool"));
    static const QRegularExpression bracket(QStringLiteral("\\[(\\w+)\\]"));
    const QString lockdown = bracket.match(readFirstLine(QStringLiteral("/sys/kernel/security/lockdown"))).captured(1);
    const QStringList lsms = readFirstLine(QStringLiteral("/sys/kernel/security/lsm")).split(QLatin1Char(','), Qt::SkipEmptyParts);

    // Is the Asima OS key approved (MOK)? mokutil reads the MOK list without root. The certificate
    // also ships in /usr for this check (the ESP copy is root-only).
    const QString asimaosKey = QStringLiteral("/usr/share/asimaos/keys/asimaos.der");
    bool keyApproved = false;
    if (QFileInfo::exists(asimaosKey)) {
        QProcess mok;
        mok.start(QStringLiteral("mokutil"), {QStringLiteral("--test-key"), asimaosKey});
        if (mok.waitForFinished(3000))
            keyApproved = QString::fromUtf8(mok.readAll()).contains(QLatin1String("already enrolled"));
    }
    const bool efi = QFileInfo::exists(QStringLiteral("/sys/firmware/efi"));
    m_protection = item({
        {"managed", false}, {"secureBoot", !efi ? "unsupported" : sbOn ? "on" : "off"}, {"keyApproved", keyApproved || sbOn},
        {"keyPending", false}, {"pendingCode", QString()},
        // Plain pools carry their own partition label (decision D9).
        {"encrypted", luks}, {"tpm", tpm}, {"tpmEnrolled", false}, {"tpmPin", false}, {"tpmKnown", false},
        {"lockdown", lockdown.isEmpty() ? QStringLiteral("none") : lockdown},
        {"verity", QFileInfo::exists(QStringLiteral("/dev/mapper/usr"))}, {"lsms", lsms},
    });
    const QString user = qEnvironmentVariable("USER");
    QString fullName;
    for (const QString &line : readAll(QStringLiteral("/etc/passwd")).split(QLatin1Char('\n')))
        if (line.startsWith(user + QLatin1Char(':')))
            fullName = line.section(QLatin1Char(':'), 4, 4).section(QLatin1Char(','), 0, 0);
    m_account = item({{"fullName", fullName}, {"username", user}});
    m_reset = item({{"prepared", false}, {"recoveryKey", QString()}});

    const bool verity = QFileInfo::exists(QStringLiteral("/dev/mapper/usr"));
    m_health = {
        item({{"title", tr("System files")}, {"detail", verity ? tr("Verified by dm-verity every time they are read") : tr("Not running from a verified Asima OS image")},
              {"tone", verity ? "ok" : "warn"}, {"value", verity ? tr("Verified") : tr("Not verified")}, {"icon", "kt-check-data"}}),
        item({{"title", tr("Detailed checks")}, {"detail", tr("Boot status, disk checks and SSD wear need the Asima Maintenance service.")},
              {"tone", "neutral"}, {"value", tr("Unavailable")}, {"icon", "speedometer"}}),
    };

    m_backup = item({{"configured", false}, {"snapshotsDaily", 0}, {"snapshotsUpdate", 0}, {"snapshotsSize", 0}});
    readChanges();
    readFlatpak();
}

void MaintenanceEngine::readChanges()
{
    m_changes.clear();
    if (!QDir(kEtcUpper).exists())
        return;
    // Files the system itself keeps up to date: not interesting as "your changes".
    static const QStringList noise{"machine-id", ".updated", "ld.so.cache", "adjtime", ".pwd.lock", "passwd-", "shadow-",
                                   "group-", "gshadow-", "subuid-", "subgid-", "resolv.conf"};
    QDirIterator it(kEtcUpper, QDir::Files | QDir::System | QDir::Hidden | QDir::NoDotAndDotDot, QDirIterator::Subdirectories);
    while (it.hasNext()) {
        const QString full = it.next();
        const QString rel = full.mid(kEtcUpper.size() + 1);
        if (noise.contains(rel))
            continue;
        const QFileInfo fi(full);
        QString kind;
        if (fi.isFile() || fi.isSymLink())
            kind = QFileInfo::exists(kFactoryEtc + QLatin1Char('/') + rel) ? QStringLiteral("modified") : QStringLiteral("added");
        else
            kind = QStringLiteral("deleted");     // overlayfs whiteout (character device 0/0)
        m_changes.append(item({{"path", QStringLiteral("/etc/") + rel}, {"kind", kind}, {"secret", fi.isFile() && !fi.isReadable()},
                               {"newDefault", m_newDefaults.contains(QStringLiteral("/etc/") + rel)}}));
    }
    std::sort(m_changes.begin(), m_changes.end(), [](const QVariant &a, const QVariant &b) {
        return a.toMap().value("path").toString() < b.toMap().value("path").toString();
    });
}

void MaintenanceEngine::readFlatpak()
{
    const QStringList apps = Util::has(QStringLiteral("flatpak"))
        ? Util::run(QStringLiteral("flatpak"), {QStringLiteral("list"), QStringLiteral("--app"), QStringLiteral("--columns=application")})
              .split(QLatin1Char('\n'), Qt::SkipEmptyParts)
        : QStringList();
    m_flatpak = item({{"apps", int(apps.size())}, {"size", -1.0}, {"unused", -1.0}});
    m_flatpak.insert("hub", apps.contains(QStringLiteral("io.github.asimaos.Hub")));
}

// ───────────────────────────────────────────────────────────────────────── actions

void MaintenanceEngine::needsService(const QString &action)
{
    emit toast(tr("%1 needs the Asima Maintenance service, which isn't available on this computer.").arg(action), QStringLiteral("info"));
}

void MaintenanceEngine::backupNow()
{
    if (m_demo)
        emit toast(tr("Backup started. You can keep working."), QStringLiteral("ok"));
    else if (!m_service)
        needsService(tr("Backing up"));
    else
        m_service->call(QStringLiteral("BackupNow"), {}, [this](const QDBusMessage &) {
            emit toast(tr("Backup started. You can keep working."), QStringLiteral("ok"));
            m_backup.insert("running", true);
            m_backup.insert("lastRun", tr("Running now…"));
            emit backupChanged();
            QTimer::singleShot(15000, this, &MaintenanceEngine::refreshFromService);
        });
}

void MaintenanceEngine::snapshotNow()
{
    if (m_demo)
        emit toast(tr("Snapshot taken. Your files are kept as they are now."), QStringLiteral("ok"));
    else if (!m_service)
        needsService(tr("Taking a snapshot"));
    else
        m_service->call(QStringLiteral("SnapshotNow"), {}, [this](const QDBusMessage &) {
            emit toast(tr("Snapshot taken. Your files are kept as they are now."), QStringLiteral("ok"));
            refreshFromService();
        });
}

void MaintenanceEngine::configureBackup(const QString &destination, const QString &password)
{
    if (m_demo) {
        m_backup.insert("configured", true);
        m_backup.insert("destination", destination);
        emit backupChanged();
        emit toast(tr("Backup is set up. The first one runs tonight."), QStringLiteral("ok"));
        return;
    }
    if (!m_service)
        return needsService(tr("Setting up a backup"));
    // The password goes straight to the service (root-only file there) and is not kept here.
    const QString settings = QString::fromUtf8(QJsonDocument(QJsonObject{{"repository", destination}, {"password", password}})
                                                   .toJson(QJsonDocument::Compact));
    m_service->call(QStringLiteral("ConfigureBackup"), {settings}, [this](const QDBusMessage &) {
        emit toast(tr("Backup is set up. The first one runs tonight; \"Back up now\" starts it at once."), QStringLiteral("ok"));
        refreshFromService();
    });
}

void MaintenanceEngine::openSnapshots()
{
    QDesktopServices::openUrl(QUrl::fromLocalFile(QStringLiteral("/home/.snapshots")));
}

void MaintenanceEngine::setSnapshotSchedule(const QString &schedule)
{
    const auto done = [this, schedule] {
        m_backup.insert("snapshotSchedule", schedule);
        emit backupChanged();
        emit toast(schedule == QLatin1String("off") ? tr("Automatic snapshots are off. “Snapshot now” still works.")
                                                    : tr("Snapshots will be taken %1.").arg(
                                                          schedule == QLatin1String("weekly") ? tr("every week")
                                                          : schedule == QLatin1String("monthly") ? tr("every month") : tr("every day")),
                   QStringLiteral("ok"));
    };
    if (m_demo)
        done();
    else if (!m_service)
        needsService(tr("Changing the snapshot schedule"));
    else
        m_service->call(QStringLiteral("SetSnapshotSchedule"), {schedule}, [done](const QDBusMessage &) { done(); });
}

void MaintenanceEngine::loadBackups()
{
    const auto fill = [this](const QJsonArray &list) {
        m_backupList.clear();
        for (const QJsonValue &v : list) {
            const QJsonObject o = v.toObject();
            const QDateTime when = QDateTime::fromString(o.value("time").toString(), Qt::ISODateWithMs).toLocalTime();
            m_backupList.append(QVariantMap{
                {"id", o.value("id").toString()},
                {"when", when.isValid() ? tr("%1, %2").arg(QLocale().toString(when.date(), QLocale::LongFormat),
                                                         QLocale().toString(when.time(), QLocale::ShortFormat))
                                    : o.value("time").toString()},
                {"detail", tr("Backup %1").arg(o.value("id").toString())},
            });
        }
        emit backupListChanged();
    };
    if (m_demo) {
        QJsonArray demo;
        demo.append(QJsonObject{{"id", "5d1f0a3c"}, {"time", "2026-11-17T21:04:12Z"}});
        demo.append(QJsonObject{{"id", "a90b44e1"}, {"time", "2026-11-16T21:02:40Z"}});
        demo.append(QJsonObject{{"id", "77c2d9f0"}, {"time", "2026-11-10T21:05:03Z"}});
        fill(demo);
    } else if (!m_service) {
        needsService(tr("Restoring"));
    } else {
        m_service->call(QStringLiteral("ListBackups"), {}, [fill](const QDBusMessage &r) {
            fill(QJsonDocument::fromJson(r.arguments().value(0).toString().toUtf8()).array());
        });
    }
}

void MaintenanceEngine::restoreBackup(const QString &id)
{
    const QString user = m_account.value("username").toString();
    if (m_demo) {
        m_backup.insert("restoreText", tr("Restored into the folder “%1” in your home").arg(tr("Restored 2026-11-18")));
        m_backup.insert("restoreTone", "ok");
        emit backupChanged();
        emit toast(tr("Your files are back in a new folder. Nothing was overwritten."), QStringLiteral("ok"));
    } else if (!m_service) {
        needsService(tr("Restoring"));
    } else {
        m_service->call(QStringLiteral("RestoreBackup"), {id, user}, [this](const QDBusMessage &) {
            m_backup.insert("restoreRunning", true);
            m_backup.insert("restoreText", tr("Restoring…"));
            m_backup.insert("restoreTone", "neutral");
            emit backupChanged();
            emit toast(tr("Restoring into a new folder in your home. Nothing is overwritten."), QStringLiteral("ok"));
            QTimer::singleShot(5000, this, &MaintenanceEngine::refreshFromService);
        });
    }
}

void MaintenanceEngine::makeReport()
{
    if (m_demo) {
        m_report = QStringLiteral(
            "# Asima OS problem report\n\nVersion 2026.11.18.1 · Secure Boot on\n\n"
            "## Health check\n\nok   system-files\nok   boot: 2026.11.18.1\nok   space: 687.6 GB free\nok   filesystem\n"
            "FAIL services: bluetooth.service\nok   secure-boot: on\n\n"
            "## Failed starts and crashes\n\nfailed start of 2026.11.17.1: the login screen did not start (desktop)\n\n"
            "## Errors since this start\n\n[   12.410213] bluetoothd[812]: Failed to set mode: Failed (0x03)\n");
        emit reportChanged();
        return;
    }
    if (!m_service) {
        needsService(tr("A problem report"));
        return;
    }
    m_reporting = true;
    emit reportChanged();
    m_service->call(QStringLiteral("ProblemReport"), {}, [this](const QDBusMessage &r) {
        m_report = r.arguments().value(0).toString();
        m_reporting = false;
        emit reportChanged();
    });
    connect(m_service, &ServiceClient::failed, this, [this] {
        if (m_reporting) {
            m_reporting = false;
            emit reportChanged();
        }
    }, Qt::SingleShotConnection);
}

bool MaintenanceEngine::saveReport(const QString &text, const QUrl &file)
{
    QFile f(file.isLocalFile() ? file.toLocalFile() : file.toString());
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate) || f.write(text.toUtf8()) < 0) {
        emit toast(tr("The report could not be saved there."), QStringLiteral("error"));
        return false;
    }
    emit toast(tr("Report saved. Nothing was sent anywhere."), QStringLiteral("ok"));
    return true;
}

void MaintenanceEngine::copyText(const QString &text)
{
    if (auto *cb = QGuiApplication::clipboard())
        cb->setText(text);
    emit toast(tr("Copied."), QStringLiteral("ok"));
}

void MaintenanceEngine::openHub()
{
    // Installed: open it. Otherwise its always-latest download on GitHub Releases, as Asima
    // Manager offers it (D54); opening the downloaded file installs it with Discover.
    if (m_flatpak.value("hub").toBool()) {
        if (m_demo || !QProcess::startDetached(QStringLiteral("flatpak"), {QStringLiteral("run"), QStringLiteral("io.github.asimaos.Hub")}))
            emit toast(tr("Opens Asima Hub."), QStringLiteral("info"));
        return;
    }
    static const QUrl latest(QStringLiteral("https://github.com/asimaos/asimaos-hub/releases/latest/download/AsimaHub-x86_64.flatpak"));
    if (m_demo || !QDesktopServices::openUrl(latest))
        emit toast(tr("Opens the download of Asima Hub in the browser."), QStringLiteral("info"));
    else
        emit toast(tr("The browser downloads Asima Hub. Open the file to install it with Discover."), QStringLiteral("info"));
}

void MaintenanceEngine::runHealthCheck()
{
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("A full health check"));
        m_checking = true;
        emit checkingChanged();
        m_service->call(QStringLiteral("RunHealthCheck"), {}, [this](const QDBusMessage &r) {
            static const QHash<QString, std::pair<const char *, const char *>> names{
                {"system-files", {QT_TR_NOOP("System files"), "kt-check-data"}},
                {"boot", {QT_TR_NOOP("Last start"), "system-reboot"}},
                {"space", {QT_TR_NOOP("Free space"), "drive-harddisk-symbolic"}},
                {"filesystem", {QT_TR_NOOP("File system"), "drive-harddisk-symbolic"}},
                {"services", {QT_TR_NOOP("Background services"), "system-run"}},
                {"secure-boot", {QT_TR_NOOP("Secure Boot"), "view-certificate"}},
                {"reserve", {QT_TR_NOOP("Emergency space"), "edit-clear"}},
                {"scrub", {QT_TR_NOOP("Disk check (scrub)"), "drive-harddisk-symbolic"}},
                {"ssd", {QT_TR_NOOP("SSD wear"), "speedometer"}},
            };
            const QJsonObject h = ServiceClient::json(r);
            m_health.clear();
            for (const QJsonValue &v : h.value("checks").toArray()) {
                const QJsonObject c = v.toObject();
                const auto n = names.value(c.value("id").toString(), {"", "help-about"});
                const bool ok = c.value("ok").toBool();
                // The service sends the free space in bytes; shown in GB like the rest of the app.
                QString detail = c.value("detail").toString();
                if (c.value("id").toString() == QLatin1String("space") && !detail.isEmpty())
                    detail = tr("%1 free").arg(QLocale().formattedDataSize(detail.toLongLong(), 1, QLocale::DataSizeSIFormat));
                m_health.append(QVariantMap{{"title", tr(n.first)}, {"detail", detail},
                                            {"tone", ok ? "ok" : "warn"}, {"value", ok ? tr("OK") : tr("Look")}, {"icon", n.second}});
            }
            m_checking = false;
            emit checkingChanged();
            emit healthChanged();
            emit toast(h.value("ok").toBool() ? tr("All checks passed.") : tr("Some checks need a look."),
                       h.value("ok").toBool() ? QStringLiteral("ok") : QStringLiteral("warn"));
        });
        return;
    }
    m_checking = true;
    emit checkingChanged();
    QTimer::singleShot(2600, this, [this] {
        m_checking = false;
        emit checkingChanged();
        emit toast(tr("All checks passed."), QStringLiteral("ok"));
    });
}

// ───────────────────────────────────────────────────────────────────────── security

QString MaintenanceEngine::approveAsimaKey()
{
    if (!m_demo) {
        if (!m_service) {
            needsService(tr("Approving the Asima OS key"));
            return {};
        }
        m_service->call(QStringLiteral("ApproveAsimaKey"), {}, [this](const QDBusMessage &r) {
            m_protection.insert("keyPending", true);
            m_protection.insert("pendingCode", r.arguments().value(0).toString());
            emit protectionChanged();
        });
        return {};
    }
    // Digits only: MokManager reads the keyboard with the US layout, digits are the same everywhere.
    const QString code = QStringLiteral("%1 %2").arg(QRandomGenerator::global()->bounded(1000, 10000))
                                                 .arg(QRandomGenerator::global()->bounded(1000, 10000));
    m_protection.insert("keyPending", true);
    m_protection.insert("pendingCode", code);
    emit protectionChanged();
    return code;
}

void MaintenanceEngine::cancelKeyApproval()
{
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("Cancelling the key approval"));
        return m_service->call(QStringLiteral("CancelKeyApproval"), {}, [this](const QDBusMessage &) {
            emit toast(tr("Cancelled. Nothing changes at the next start."), QStringLiteral("info"));
            refreshFromService();
        });
    }
    m_protection.insert("keyPending", false);
    m_protection.insert("pendingCode", QString());
    emit protectionChanged();
    emit toast(tr("Cancelled. Nothing changes at the next start."), QStringLiteral("info"));
}

void MaintenanceEngine::retireKeys(const QStringList &ids)
{
    const auto pending = [this](const QString &code) {
        m_protection.insert("retirePending", true);
        m_protection.insert("retireCode", code);
        emit protectionChanged();
    };
    if (m_demo)
        return pending(QStringLiteral("%1 %2").arg(QRandomGenerator::global()->bounded(1000, 10000))
                                               .arg(QRandomGenerator::global()->bounded(1000, 10000)));
    if (!m_service)
        return needsService(tr("Retiring keys"));
    m_service->call(QStringLiteral("RetireKeys"), {ids}, [pending](const QDBusMessage &r) {
        pending(r.arguments().value(0).toString());
    });
}

void MaintenanceEngine::prepareOwnKeys()
{
    if (m_demo)
        return setDemoState(QStringLiteral("security-ownkeys"));
    if (!m_service)
        return needsService(tr("Own Secure Boot keys"));
    m_service->call(QStringLiteral("PrepareOwnKeys"), {}, [this](const QDBusMessage &) { refreshFromService(); });
}

void MaintenanceEngine::enrollOwnKeys(const QStringList &drop)
{
    const auto done = [this] {
        emit toast(tr("The keys are in the firmware. Restart, and switch Secure Boot on in the firmware settings if it is off."), QStringLiteral("ok"));
    };
    if (m_demo) {
        QVariantMap own = m_protection.value("ownKeys").toMap();
        own.insert("own", true);
        own.insert("setupMode", false);
        m_protection.insert("ownKeys", own);
        emit protectionChanged();
        return done();
    }
    if (!m_service)
        return needsService(tr("Own Secure Boot keys"));
    m_service->call(QStringLiteral("EnrollOwnKeys"), {drop}, [this, done](const QDBusMessage &) {
        done();
        refreshFromService();
    });
}

void MaintenanceEngine::cancelRetireKeys()
{
    const auto done = [this] {
        m_protection.insert("retirePending", false);
        m_protection.insert("retireCode", QString());
        emit protectionChanged();
        emit toast(tr("Cancelled. Nothing changes at the next start."), QStringLiteral("info"));
    };
    if (m_demo)
        return done();
    if (!m_service)
        return needsService(tr("Cancelling"));
    m_service->call(QStringLiteral("CancelRetireKeys"), {}, [done](const QDBusMessage &) { done(); });
}

void MaintenanceEngine::restartNow()
{
    if (m_demo)
        emit toast(tr("Demo mode: the computer would restart now."), QStringLiteral("info"));
    else if (!QProcess::startDetached(QStringLiteral("systemctl"), {QStringLiteral("reboot")}))
        emit toast(tr("The computer couldn't be restarted. Use the power menu."), QStringLiteral("warn"));
}

void MaintenanceEngine::restartToFirmware()
{
    if (m_demo) {
        emit toast(tr("Demo mode: the computer would now restart into its firmware settings."), QStringLiteral("info"));
        return;
    }
    // logind lets the person at the computer do this; no Asima OS service needed.
    if (!QProcess::startDetached(QStringLiteral("systemctl"), {QStringLiteral("reboot"), QStringLiteral("--firmware-setup")}))
        emit toast(tr("The firmware settings could not be opened. Restart and press the setup key (often F2 or Del)."), QStringLiteral("warn"));
}

void MaintenanceEngine::enrollTpm(const QString &passphrase)
{
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("Setting up automatic unlock"));
        return m_service->call(QStringLiteral("EnrollTpm"), {passphrase}, [this](const QDBusMessage &) {
            emit toast(tr("Automatic unlock is on. Your passphrase keeps working too."), QStringLiteral("ok"));
            refreshFromService();
        });
    }
    m_protection.insert("tpmEnrolled", true);
    m_protection.insert("tpmPin", false);
    emit protectionChanged();
    emit toast(tr("Automatic unlock is on. Your passphrase keeps working too."), QStringLiteral("ok"));
}

void MaintenanceEngine::enrollTpmPin(const QString &passphrase, const QString &pin)
{
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("Setting up unlock with a PIN"));
        return m_service->call(QStringLiteral("EnrollTpmPin"), {passphrase, pin}, [this](const QDBusMessage &) {
            emit toast(tr("Unlock with a PIN is on: type it at every start. Your passphrase keeps working too."), QStringLiteral("ok"));
            refreshFromService();
        });
    }
    m_protection.insert("tpmEnrolled", true);
    m_protection.insert("tpmPin", true);
    emit protectionChanged();
    emit toast(tr("Unlock with a PIN is on: type it at every start. Your passphrase keeps working too."), QStringLiteral("ok"));
}

void MaintenanceEngine::removeTpm()
{
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("Turning off automatic unlock"));
        return m_service->call(QStringLiteral("RemoveTpm"), {}, [this](const QDBusMessage &) {
            emit toast(tr("Automatic unlock is off. The disk asks for your passphrase at every start."), QStringLiteral("info"));
            refreshFromService();
        });
    }
    m_protection.insert("tpmEnrolled", false);
    m_protection.insert("tpmPin", false);
    emit protectionChanged();
    emit toast(tr("Automatic unlock is off. The disk asks for your passphrase at every start."), QStringLiteral("info"));
}

void MaintenanceEngine::changePassphrase(const QString &current, const QString &next)
{
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("Changing the disk passphrase"));
        return m_service->call(QStringLiteral("ChangePassphrase"), {current, next}, [this](const QDBusMessage &) {
            emit toast(tr("The disk passphrase is changed. Use the new one from the next start."), QStringLiteral("ok"));
        });
    }
    emit toast(tr("The disk passphrase is changed. Use the new one from the next start."), QStringLiteral("ok"));
}

QString MaintenanceEngine::newRecoveryKey(const QString &current)
{
    if (!m_demo) {
        if (!m_service) {
            needsService(tr("Making a new recovery key"));
            return {};
        }
        m_service->call(QStringLiteral("NewRecoveryKey"), {current}, [this](const QDBusMessage &r) {
            emit recoveryKeyMade(r.arguments().value(0).toString());
        });
        return {};
    }
    return demoRecoveryKey();
}

// ───────────────────────────────────────────────────────────────────────── reset

void MaintenanceEngine::prepareFactoryReset(const QVariantMap &fresh)
{
    // fresh: fullName, username, password, passphrase (new disk passphrase), current (to authorise).
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("Preparing the factory reset"));
        const QByteArray json = QJsonDocument(QJsonObject::fromVariantMap(fresh)).toJson(QJsonDocument::Compact);
        return m_service->call(QStringLiteral("PrepareFactoryReset"), {QString::fromUtf8(json)}, [this](const QDBusMessage &r) {
            const QJsonObject o = ServiceClient::json(r);
            m_reset = QVariantMap{{"prepared", true}, {"recoveryKey", o.value("recoveryKey").toString()},
                                  {"username", o.value("username").toString()}};
            emit resetChanged();
        });
    }
    if (fresh.value("username").toString().isEmpty())
        return;
    m_reset = item({{"prepared", true},
                    {"recoveryKey", m_protection.value("encrypted").toBool() ? demoRecoveryKey() : QString()},
                    {"username", fresh.value("username")}});
    emit resetChanged();
}

void MaintenanceEngine::cancelFactoryReset()
{
    if (!m_demo && m_service && m_reset.value("prepared").toBool())
        m_service->call(QStringLiteral("CancelFactoryReset"));
    m_reset = item({{"prepared", false}, {"recoveryKey", QString()}});
    emit resetChanged();
}

void MaintenanceEngine::factoryReset()
{
    if (m_demo)
        emit toast(tr("Demo mode: nothing was deleted."), QStringLiteral("info"));
    else if (!m_service)
        needsService(tr("Factory reset"));
    else    // the service marks the staged reset and restarts the computer
        m_service->call(QStringLiteral("ScheduleFactoryReset"));
}

// ───────────────────────────────────────────────────────────────────────── system changes, Flatpak

QString MaintenanceEngine::diff(const QString &path) const
{
    for (const QVariant &v : m_changes) {
        const QVariantMap c = v.toMap();
        if (c.value("path").toString() != path)
            continue;
        if (c.value("secret").toBool())
            return {};
        if (m_demo)
            return c.value("diff").toString();
        const QString rel = path.mid(5);   // strip "/etc/"
        const QString factory = kFactoryEtc + QLatin1Char('/') + rel;
        const QString current = kEtcUpper + QLatin1Char('/') + rel;
        const QString kind = c.value("kind").toString();
        return Util::run(QStringLiteral("diff"), {QStringLiteral("-u"), QStringLiteral("--label"), QStringLiteral("factory/etc/") + rel,
                                                  QStringLiteral("--label"), kind == QLatin1String("deleted") ? QStringLiteral("(removed)") : path,
                                                  kind == QLatin1String("added") ? QStringLiteral("/dev/null") : factory,
                                                  kind == QLatin1String("deleted") ? QStringLiteral("/dev/null") : current});
    }
    return {};
}

void MaintenanceEngine::revertChange(const QString &path)
{
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("Restoring a factory file"));
        return m_service->call(QStringLiteral("RestoreEtc"), {path}, [this, path](const QDBusMessage &) {
            emit toast(tr("%1 goes back to its factory version at the next start.").arg(path), QStringLiteral("ok"));
        });
    }
    for (int i = 0; i < m_changes.size(); ++i)
        if (m_changes.at(i).toMap().value("path").toString() == path) {
            m_changes.removeAt(i);
            break;
        }
    emit changesChanged();
    emit toast(tr("%1 is back to its factory version.").arg(path), QStringLiteral("ok"));
}

void MaintenanceEngine::startAsimaFirst()
{
    const auto done = [this] {
        m_bootOrder.insert("asimaosFirst", true);
        emit bootOrderChanged();
        emit toast(tr("Asima OS starts first again."), QStringLiteral("ok"));
    };
    if (m_demo)
        return done();
    if (!m_service)
        return needsService(tr("Changing the start order"));
    m_service->call(QStringLiteral("StartAsimaFirst"), {}, [done](const QDBusMessage &) { done(); });
}

void MaintenanceEngine::resetSettings()
{
    if (m_demo)
        return emit toast(tr("Demo mode: no setting was changed."), QStringLiteral("info"));
    if (!m_service)
        return needsService(tr("Resetting the settings"));
    // The service schedules the reset and restarts the computer.
    m_service->call(QStringLiteral("ResetSettings"));
}

void MaintenanceEngine::cleanFlatpak()
{
    if (m_demo) {
        m_flatpak.insert("unused", 0.0);
        emit flatpakChanged();
        emit toast(tr("Removed unused runtimes and freed 1.2 GB."), QStringLiteral("ok"));
        return;
    }
    auto *p = new QProcess(this);
    connect(p, &QProcess::finished, this, [this, p](int code) {
        emit toast(code == 0 ? tr("Unused Flatpak runtimes removed.") : tr("Cleaning up Flatpak failed."),
                   code == 0 ? QStringLiteral("ok") : QStringLiteral("error"));
        if (code == 0) {
            m_flatpak.insert("unused", 0.0);
            emit flatpakChanged();
        }
        p->deleteLater();
    });
    p->start(QStringLiteral("flatpak"), {QStringLiteral("uninstall"), QStringLiteral("--unused"), QStringLiteral("-y"), QStringLiteral("--noninteractive")});
}

void MaintenanceEngine::openApp(const QString &command)
{
    const QStringList parts = QProcess::splitCommand(command);
    if (m_demo || parts.isEmpty() || !QProcess::startDetached(parts.first(), parts.mid(1)))
        emit toast(tr("Opens %1.").arg(parts.value(0)), QStringLiteral("info"));
}
