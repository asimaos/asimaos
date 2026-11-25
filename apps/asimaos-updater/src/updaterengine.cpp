#include "updaterengine.h"
#include "serviceclient.h"
#include "util.h"

#include <QDBusMessage>
#include <QDBusUnixFileDescriptor>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QLocale>
#include <QProcess>
#include <QSysInfo>

#include <algorithm>

using Util::item;

UpdaterEngine::UpdaterEngine(bool demo, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
{
    m_installTimer.setInterval(90);
    connect(&m_installTimer, &QTimer::timeout, this, &UpdaterEngine::installTick);
    if (m_demo) {
        loadDemo();
        return;
    }
    loadSystem();
    auto *service = new ServiceClient(QStringLiteral("Updater1"), this);
    if (!service->available()) {
        delete service;
        return;
    }
    m_service = service;
    connect(m_service, &ServiceClient::failed, this, [this](const QString &message) { emit toast(message, QStringLiteral("warn")); });
    m_service->connectSignal(QStringLiteral("JobProgress"), this, SLOT(onJobProgress(QString,double,QString,QString)));
    m_service->connectSignal(QStringLiteral("JobFinished"), this, SLOT(onJobFinished(QString,bool,QString)));
    refreshFromService();
}

// ───────────────────────────────────────────────────────────────────────── asimaos-updaterd

void UpdaterEngine::refreshFromService()
{
    m_service->call(QStringLiteral("Status"), {}, [this](const QDBusMessage &r) { applyServiceStatus(ServiceClient::json(r)); });
}

void UpdaterEngine::applyServiceStatus(const QJsonObject &s)
{
    const QString running = s.value("version").toString();
    // The start menu (docs/architecture.md §8): "current" starts by itself, "backup" is the one to go back to,
    // "failed" is an update that didn't start. previous = the other working version in the menu.
    QString previous, pending, runningRole;
    bool currentGood = false;   // the current entry started fine before (not a fresh update)
    for (const QJsonValue &v : s.value("versions").toArray()) {
        const QJsonObject o = v.toObject();
        if (o.value("running").toBool())
            runningRole = o.value("role").toString();
        if (o.value("role").toString() == QLatin1String("current"))
            currentGood = o.value("blessed").toBool();
    }
    // Running the backup while the current version is good: it was chosen for one start.
    const bool startedOnce = runningRole == QLatin1String("backup") && currentGood;
    m_versions.clear();
    for (const QJsonValue &v : s.value("versions").toArray()) {
        const QJsonObject o = v.toObject();
        const bool isRunning = o.value("running").toBool();
        const bool blessed = o.value("blessed").toBool();
        const QString ver = o.value("version").toString();
        const QString menu = o.value("role").toString();
        const int triesLeft = o.value("triesLeft").toInt(-1);
        const bool failed = menu == QLatin1String("failed") || (!blessed && triesLeft == 0);
        const bool next = !isRunning && menu == QLatin1String("current") && !failed;   // starts next time
        const bool image = o.value("image").toBool();
        if (next)
            pending = ver;
        if (!isRunning && !failed && (menu == QLatin1String("backup") || (menu == QLatin1String("current") && runningRole == QLatin1String("backup"))))
            previous = ver;
        const QString boot = !o.value("bootable").toBool() ? tr("Its start menu entry is missing")
                             : failed ? tr("Didn't start")
                             : blessed ? tr("Started fine")
                             : next ? tr("Starts after a restart")
                             : tr("%n attempt(s) left", nullptr, triesLeft);
        const qint64 installedAt = o.value("installed").toVariant().toLongLong();
        const QString role = isRunning ? QStringLiteral("running") : next ? QStringLiteral("next")
                             : failed ? QStringLiteral("failed") : menu == QLatin1String("backup") ? QStringLiteral("backup")
                             : QStringLiteral("kept");
        m_versions.append(QVariantMap{
            {"version", ver}, {"role", role}, {"menuRole", menu}, {"nextOnce", o.value("nextOnce").toBool()},
            {"fresh", next && !blessed}, {"startedOnce", isRunning && startedOnce},
            {"menuTitle", menu == QLatin1String("current") ? tr("Current") : menu == QLatin1String("backup") ? tr("Backup")
                          : menu == QLatin1String("failed") ? tr("Failed") : tr("Not in the start menu")},
            {"tone", isRunning ? (blessed ? "ok" : "info") : (failed || !image ? "error" : "info")},
            {"statusText", isRunning ? (blessed ? tr("Running · good") : tr("Running · being checked"))
                           : !image ? tr("Damaged") : failed ? tr("Failed to start")
                           : o.value("nextOnce").toBool() ? tr("Starts once next time")
                           : next && blessed ? tr("Starts next time") : next ? tr("Installed · restart") : tr("Ready")},
            {"installed", installedAt > 0 ? QLocale().toString(QDateTime::fromSecsSinceEpoch(installedAt).date(), QLocale::ShortFormat) : QString()},
            {"size", o.value("size").toDouble()}, {"boot", boot},
            {"detail", isRunning ? (startedOnce ? tr("The backup version, started once. The current version starts again next time.")
                                    : menu == QLatin1String("backup") ? tr("The version you are using now. It stays as the backup after the restart.")
                                    : tr("The version you are using now."))
                       : next && blessed ? tr("The current version: it starts again at the next start.")
                       : next ? tr("Installed: it starts at the next restart.")
                       : failed ? tr("This update didn't start, so your computer went back by itself. It is removed with the next update.")
                       : tr("Kept unchanged so you can go back to it.")},
        });
    }
    m_system.insert("version", running);
    m_system.insert("previousVersion", previous);
    m_system.insert("pendingVersion", pending);
    m_system.insert("managed", true);
    m_system.insert("settingsRollback", s.value("settingsRollback").toString());
    emit versionsChanged();
    emit systemChanged();
}

void UpdaterEngine::onJobProgress(const QString &job, double progress, const QString &step, const QString &detail)
{
    if (job != m_job)
        return;
    const auto st = [progress](double from, double to) {
        return progress >= to ? QStringLiteral("done") : progress >= from ? QStringLiteral("active") : QStringLiteral("pending");
    };
    m_update.insert("state", "installing");
    m_update.insert("progress", progress);
    m_update.insert("steps", QVariantList{
        QVariantMap{{"title", tr("Check signature")}, {"detail", tr("Signed by Asima OS")}, {"state", "done"}},
        QVariantMap{{"title", tr("Write Asima OS")}, {"detail", step == QLatin1String("write") ? detail : QString()}, {"state", st(0.05, 0.9)}},
        QVariantMap{{"title", tr("Keep two versions")}, {"detail", tr("Your current version stays as the fallback")}, {"state", st(0.9, 1.0)}},
    });
    emit pendingUpdateChanged();
}

void UpdaterEngine::onJobFinished(const QString &job, bool ok, const QString &message)
{
    if (job != m_job)
        return;
    if (ok)
        onJobProgress(job, 1.0, QStringLiteral("finish"), QString());
    m_job.clear();
    if (ok) {
        m_update.insert("state", "done");
        emit toast(message, QStringLiteral("ok"));
    } else {
        m_update.insert("state", "error");
        m_update.insert("errorTitle", tr("The update was not installed"));
        m_update.insert("errorText", message + QLatin1Char(' ') + tr("Nothing was changed."));
    }
    emit pendingUpdateChanged();
    refreshFromService();
}

// ───────────────────────────────────────────────────────────────────────── demo data

void UpdaterEngine::loadDemo()
{
    m_system = item({
        {"name", "Asima OS"}, {"version", "2026.11.15.1"}, {"previousVersion", "2026.11.01.2"},
        {"kernel", "Linux 7.3.4-arch1-1"}, {"hostname", "asimaos"}, {"managed", true},
    });
    m_versions = {
        item({{"version", "2026.11.15.1"}, {"role", "running"}, {"menuRole", "current"}, {"menuTitle", tr("Current")},
              {"tone", "ok"}, {"statusText", tr("Running · good")},
              {"installed", tr("15 Nov 2026")}, {"size", 4.9e9}, {"boot", tr("Started on the first try")},
              {"detail", tr("Marked good after the login screen stayed healthy for 15 seconds.")}}),
        item({{"version", "2026.11.01.2"}, {"role", "backup"}, {"menuRole", "backup"}, {"menuTitle", tr("Backup")},
              {"tone", "info"}, {"statusText", tr("Ready")},
              {"installed", tr("1 Nov 2026")}, {"size", 4.8e9}, {"boot", tr("Last ran on 15 Nov")},
              {"detail", tr("Kept unchanged so you can go back to it at any time.")}}),
    };
    m_update = item({{"state", "idle"}});
}

void UpdaterEngine::setDemoState(const QString &state)
{
    if (state == QLatin1String("update-ready") || state == QLatin1String("update-error")) {
        setUpdateState(QStringLiteral("ready"));
        if (state == QLatin1String("update-error")) {
            m_update.insert("state", "error");
            m_update.insert("errorTitle", tr("This file isn't signed by Asima OS"));
            m_update.insert("errorText", tr("Its signature doesn't match the Asima OS release key, so nothing was changed. "
                                             "Download the update again from the official source."));
            emit pendingUpdateChanged();
        }
    } else if (state == QLatin1String("update-installing")) {
        setUpdateState(QStringLiteral("ready"));
        m_progress = 0.62;
        setUpdateState(QStringLiteral("installing"));
    } else if (state == QLatin1String("update-done")) {
        setUpdateState(QStringLiteral("ready"));
        m_progress = 1.0;
        setUpdateState(QStringLiteral("done"));
    }
}

// ───────────────────────────────────────────────────────────────────────── system data

void UpdaterEngine::loadSystem()
{
    const QVariantMap os = Util::osRelease();
    const QString version = os.value("IMAGE_VERSION", os.value("VERSION_ID", tr("unknown"))).toString();

    // Installed versions = images in @slots (mounted read-only at /run/initramfs/slots).
    QString previous;
    const QStringList images = QDir(QStringLiteral("/run/initramfs/slots"))
        .entryList({QStringLiteral("usr_*.erofs")}, QDir::Files, QDir::Name | QDir::Reversed);
    for (const QString &image : images) {
        const QString v = image.mid(4, image.size() - 4 - 6);
        const qint64 size = QFileInfo(QStringLiteral("/run/initramfs/slots/") + image).size();
        const bool running = v == version;
        if (!running && previous.isEmpty())
            previous = v;
        m_versions.append(item({
            {"version", v}, {"role", running ? "running" : "kept"}, {"menuTitle", QString()},
            {"tone", running ? "ok" : "info"}, {"statusText", running ? tr("Running") : tr("Installed")},
            {"installed", QString()}, {"size", double(size)}, {"boot", QString()},
            {"detail", running ? tr("The version you are using now.") : tr("Kept unchanged so you can go back to it.")},
        }));
    }
    m_system = item({
        {"name", os.value("NAME", "Asima OS")}, {"version", version}, {"previousVersion", previous},
        {"kernel", QStringLiteral("Linux ") + QSysInfo::kernelVersion()}, {"hostname", QSysInfo::machineHostName()},
        {"managed", false},
    });
    m_update = item({{"state", "idle"}});
}

// ───────────────────────────────────────────────────────────────────────── actions

void UpdaterEngine::needsService(const QString &action)
{
    emit toast(tr("%1 needs the Asima Updater service, which isn't available on this computer.").arg(action), QStringLiteral("info"));
}

void UpdaterEngine::setUpdateState(const QString &state)
{
    if (state == QLatin1String("ready")) {
        m_update = item({
            {"state", "ready"},
            {"fileName", "asimaos-2026.11.29.1-x86_64.aosupd"},
            {"fileSize", 1.93e9},
            {"version", "2026.11.29.1"},
            {"unpackedSize", 5.1e9},
            {"checks", QVariantList{
                item({{"title", tr("Signed by Asima OS")}, {"detail", tr("Release key")}, {"tone", "ok"}, {"value", tr("Valid")}}),
                item({{"title", tr("Complete and undamaged")}, {"detail", tr("Every part matches its signed checksum")}, {"tone", "ok"}, {"value", tr("Intact")}}),
                item({{"title", tr("Newer than this system")}, {"detail", "2026.11.15.1 → 2026.11.29.1"}, {"tone", "ok"}, {"value", tr("Newer")}}),
                item({{"title", tr("Release still valid")}, {"detail", tr("Until %1").arg("2027-02-28")}, {"tone", "ok"}, {"value", tr("Valid")}}),
                item({{"title", tr("Enough free space")}, {"detail", tr("Needs %1 · %2 free").arg(Util::bytesText(10e9), Util::bytesText(687e9))}, {"tone", "ok"}, {"value", tr("OK")}}),
            }},
            {"notes", QStringList{
                tr("KDE Plasma 6.8.3 and KDE Gear 26.08.3"),
                tr("Linux 7.3.6 with security fixes"),
                tr("Mesa 26.2.8 for AMD and Intel graphics"),
                tr("NVIDIA 590.48, matching the Flathub runtime"),
                tr("Faster start-up for the Asima OS apps"),
            }},
        });
        emit pendingUpdateChanged();
        return;
    }

    if (state == QLatin1String("installing") || state == QLatin1String("done")) {
        const double p = state == QLatin1String("done") ? 1.0 : m_progress;
        auto st = [p](double from, double to) {
            return p >= to ? QStringLiteral("done") : p >= from ? QStringLiteral("active") : QStringLiteral("pending");
        };
        const double written = qBound(0.0, (p - 0.14) / 0.68, 1.0) * 5.1;
        m_update.insert("state", state);
        m_update.insert("progress", p);
        m_update.insert("steps", QVariantList{
            item({{"title", tr("Check signature")}, {"detail", tr("Signed by Asima OS")}, {"state", st(0.0, 0.08)}}),
            item({{"title", tr("Check free space")}, {"detail", tr("687 GB free")}, {"state", st(0.08, 0.14)}}),
            item({{"title", tr("Write system image")},
                  {"detail", st(0.14, 0.82) == QLatin1String("pending") ? QString()
                                                                      : tr("%1 GB of 5.1 GB").arg(written, 0, 'f', 1)},
                  {"state", st(0.14, 0.82)}}),
            item({{"title", tr("Write integrity data")}, {"detail", QString()}, {"state", st(0.82, 0.95)}}),
            item({{"title", tr("Add boot entry")}, {"detail", tr("Your current version stays as the fallback")}, {"state", st(0.95, 1.0)}}),
        });
        emit pendingUpdateChanged();
    }
}

void UpdaterEngine::selectUpdateFile(const QUrl &file)
{
    if (m_demo) {
        setUpdateState(QStringLiteral("ready"));
        return;
    }
    if (!m_service)
        return needsService(tr("Checking an update file"));
    QFile f(file.toLocalFile());
    if (!f.open(QIODevice::ReadOnly)) {
        emit toast(tr("That file can't be opened."), QStringLiteral("warn"));
        return;
    }
    m_updatePath = f.fileName();
    const QString name = QFileInfo(f).fileName();
    const double size = double(f.size());
    // The service reads the file through this descriptor: it never needs access to your home.
    m_service->call(QStringLiteral("VerifyUpdate"), {QVariant::fromValue(QDBusUnixFileDescriptor(f.handle()))},
                    [this, name, size](const QDBusMessage &r) {
        const QJsonObject v = ServiceClient::json(r);
        QStringList problems, failed;
        for (const QJsonValue &p : v.value("problems").toArray())
            problems << Util::sentence(p.toString());
        for (const QJsonValue &f : v.value("failed").toArray())
            failed << f.toString();
        // The service's sentence for a failed check, by its code.
        auto why = [&](const QString &code) {
            const qsizetype i = failed.indexOf(code);
            return i >= 0 && i < problems.size() ? problems.at(i) : QString();
        };
        auto check = [](const QString &title, const QString &detail, bool ok, const QString &value) {
            return QVariantMap{{"title", title}, {"detail", detail}, {"tone", ok ? "ok" : "error"}, {"value", value}};
        };
        const bool signature = v.value("signature").toBool();
        const QString refused = v.value("refused").toString();
        const QString ver = v.value("version").toString(), cur = v.value("current").toString();
        // The running version's own file: not an update, but it can write a fresh copy of this
        // version (D21: after a power cut, or damaged system files). A fresh copy replaces one
        // file at a time, so it needs half the space of an update (the service checks it again).
        const bool sameVersion = signature && v.value("sameVersion").toBool()
                                 && std::all_of(failed.cbegin(), failed.cend(), [](const QString &c) {
                                        return c == QLatin1String("newer") || c == QLatin1String("space"); });
        const double needBytes = v.value("needBytes").toDouble() / (sameVersion ? 2 : 1);
        const bool space = sameVersion ? v.value("freeBytes").toDouble() >= needBytes : !failed.contains(QLatin1String("space"));
        const bool reinstall = sameVersion && space;
        QVariantList checks;
        if (signature)
            checks << check(tr("Signed by Asima OS"), tr("Release key"), true, tr("Valid"));
        else if (refused == QLatin1String("not-signed"))
            checks << check(tr("Signed by Asima OS"),
                            tr("Not signed with a release key this system knows. An update made after a change of the key installs with “Repair this computer” on the USB stick, which keeps your files."),
                            false, tr("Invalid"));
        else
            checks << check(tr("Signed by Asima OS"), tr("Not checked"), false, tr("Unknown"));
        if (refused == QLatin1String("damaged"))
            checks << check(tr("Complete and undamaged"), why(QStringLiteral("damaged")).isEmpty() ? problems.value(0) : why(QStringLiteral("damaged")),
                            false, tr("Damaged"));
        else if (v.value("partsChecked").toBool())
            checks << check(tr("Complete and undamaged"), tr("Every part matches its signed checksum"), true, tr("Intact"));
        if (signature) {
            if (failed.contains(QLatin1String("arch")))
                checks << check(tr("Made for this computer"), why(QStringLiteral("arch")), false, tr("Wrong type"));
            if (failed.contains(QLatin1String("min-from")))
                checks << check(tr("Installs on this version"), why(QStringLiteral("min-from")), false, tr("Too old"));
            else if (sameVersion)
                checks << check(tr("The version running now"), tr("Writes a fresh copy of %1").arg(ver), true, tr("Same"));
            else
                checks << check(tr("Newer than this system"), QStringLiteral("%1 → %2").arg(cur, ver), !failed.contains(QLatin1String("newer")),
                                failed.contains(QLatin1String("newer")) ? tr("Not newer") : tr("Newer"));
            const QString expires = v.value("expires").toString();
            checks << check(tr("Release still valid"), expires.isEmpty() ? tr("No end date") : tr("Until %1").arg(expires),
                            !failed.contains(QLatin1String("expired")), failed.contains(QLatin1String("expired")) ? tr("Expired") : tr("Valid"));
            checks << check(tr("Enough free space"),
                            tr("Needs %1 · %2 free").arg(Util::bytesText(needBytes), Util::bytesText(v.value("freeBytes").toDouble())),
                            space, space ? tr("OK") : tr("Too little"));
        }
        const bool ok = v.value("ok").toBool() || reinstall;
        m_update = QVariantMap{
            {"state", ok ? "ready" : "error"}, {"reinstall", reinstall},
            {"fileName", name}, {"fileSize", size}, {"version", ver},
            {"unpackedSize", v.value("unpackedSize").toDouble()},
            {"checks", checks},
            {"notes", [&v] {
                QStringList notes;
                for (QString line : v.value("notes").toString().split(QLatin1Char('\n'))) {
                    line = line.trimmed();
                    if (line.startsWith(QLatin1String("- ")) || line.startsWith(QLatin1String("* ")))
                        line = line.mid(2);
                    if (!line.isEmpty() && !line.startsWith(QLatin1Char('#')))
                        notes << line;
                }
                return notes;
            }()},
        };
        if (!ok) {
            m_update.insert("errorTitle", refused == QLatin1String("not-signed")     ? tr("This file isn't signed by Asima OS")
                                          : refused == QLatin1String("damaged")      ? tr("This file is damaged or incomplete")
                                          : refused == QLatin1String("newer-format") ? tr("This update is too new for this version")
                                                                                     : tr("This update can't be installed"));
            m_update.insert("errorText", problems.join(QLatin1Char('\n')));
        }
        emit pendingUpdateChanged();
    });
}

void UpdaterEngine::clearUpdate()
{
    m_installTimer.stop();
    m_update = item({{"state", "idle"}});
    emit pendingUpdateChanged();
}

void UpdaterEngine::installUpdate()
{
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("Installing an update"));
        QFile f(m_updatePath);
        if (!f.open(QIODevice::ReadOnly)) {
            emit toast(tr("The update file is gone. Choose it again."), QStringLiteral("warn"));
            return;
        }
        const bool reinstall = m_update.value("reinstall").toBool();
        m_service->call(reinstall ? QStringLiteral("ReinstallVersion") : QStringLiteral("ApplyUpdate"),
                        {QVariant::fromValue(QDBusUnixFileDescriptor(f.handle()))},
                        [this](const QDBusMessage &r) {
            m_job = r.arguments().value(0).toString();
            onJobProgress(m_job, 0.02, QStringLiteral("write"), tr("Starting…"));
        });
        return;
    }
    m_progress = 0.0;
    setUpdateState(QStringLiteral("installing"));
    m_installTimer.start();
}

void UpdaterEngine::installTick()
{
    m_progress = qMin(1.0, m_progress + 0.011);
    if (m_progress >= 1.0) {
        m_installTimer.stop();
        setUpdateState(QStringLiteral("done"));
        emit toast(tr("Asima OS 2026.11.29.1 is ready. Restart to start using it."), QStringLiteral("ok"));
        return;
    }
    setUpdateState(QStringLiteral("installing"));
}

void UpdaterEngine::restartNow()
{
    if (m_demo)
        emit toast(tr("Demo mode: the computer would restart into the new version now."), QStringLiteral("info"));
    else if (!QProcess::startDetached(QStringLiteral("systemctl"), {QStringLiteral("reboot")}))
        emit toast(tr("The computer couldn't be restarted. Use the power menu."), QStringLiteral("warn"));
}

void UpdaterEngine::startVersionOnce(const QString &version)
{
    if (m_demo)
        emit toast(tr("The next start will use %1 once. After that, %2 is used again.").arg(version, m_system.value("version").toString()),
                   QStringLiteral("ok"));
    else if (!m_service)
        needsService(tr("Starting another version"));
    else
        m_service->call(QStringLiteral("SetDefaultVersion"), {version, true}, [this, version](const QDBusMessage &) {
            emit toast(tr("The next start uses %1 once.").arg(version), QStringLiteral("ok"));
            refreshFromService();
        });
}

void UpdaterEngine::makeVersionCurrent(const QString &version)
{
    if (m_demo)
        emit toast(tr("%1 will be used from the next start. Nothing was deleted.").arg(version), QStringLiteral("ok"));
    else if (!m_service)
        needsService(tr("Going back to another version"));
    else
        m_service->call(QStringLiteral("SetDefaultVersion"), {version, false}, [this, version](const QDBusMessage &) {
            emit toast(tr("%1 is the current version from the next start. Nothing was deleted.").arg(version), QStringLiteral("ok"));
            refreshFromService();
        });
}

