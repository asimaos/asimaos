#include "rescueengine.h"

#include <QFileInfo>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRandomGenerator>
#include <QStandardPaths>

namespace {

QVariantMap item(std::initializer_list<std::pair<QString, QVariant>> values)
{
    QVariantMap m;
    for (const auto &[k, v] : values)
        m.insert(k, v);
    return m;
}

struct Step { const char *id; const char *title; };

// What each action does, step by step (the engine reports progress by these ids).
QList<Step> stepsFor(const QString &action)
{
    if (action == QLatin1String("check"))
        return {{"fs", QT_TRANSLATE_NOOP("RescueEngine", "Check the file system")}, {"images", QT_TRANSLATE_NOOP("RescueEngine", "Check both Asima OS versions")},
                {"boot", QT_TRANSLATE_NOOP("RescueEngine", "Check the boot files")}, {"space", QT_TRANSLATE_NOOP("RescueEngine", "Check the free space")}};
    if (action == QLatin1String("repair"))
        return {{"repair", QT_TRANSLATE_NOOP("RescueEngine", "Repair the file system")}, {"verify", QT_TRANSLATE_NOOP("RescueEngine", "Check it again")}};
    if (action == QLatin1String("free-space"))
        return {{"reserve", QT_TRANSLATE_NOOP("RescueEngine", "Release the emergency reserve")}};
    if (action == QLatin1String("restore"))
        return {{"save", QT_TRANSLATE_NOOP("RescueEngine", "Keep today's files as a snapshot")}, {"restore", QT_TRANSLATE_NOOP("RescueEngine", "Put the snapshot's files back")}};
    if (action == QLatin1String("reinstall"))
        return {{"verify", QT_TRANSLATE_NOOP("RescueEngine", "Check the update file's signature")}, {"write", QT_TRANSLATE_NOOP("RescueEngine", "Write Asima OS to the disk")},
                {"boot", QT_TRANSLATE_NOOP("RescueEngine", "Make it the version that starts")}};
    if (action == QLatin1String("start-previous"))
        return {{"boot", QT_TRANSLATE_NOOP("RescueEngine", "Start the previous version once")}};
    if (action == QLatin1String("approve-key"))
        return {{"key", QT_TRANSLATE_NOOP("RescueEngine", "Prepare the Asima OS key for approval")}};
    if (action == QLatin1String("remove-tpm"))
        return {{"tpm", QT_TRANSLATE_NOOP("RescueEngine", "Remove automatic unlock")}};
    if (action == QLatin1String("factory-reset"))
        return {{"account", QT_TRANSLATE_NOOP("RescueEngine", "Prepare the fresh account")}, {"wipe", QT_TRANSLATE_NOOP("RescueEngine", "Delete everything except Asima OS")},
                {"keys", QT_TRANSLATE_NOOP("RescueEngine", "Replace the passphrase and keys")}, {"trim", QT_TRANSLATE_NOOP("RescueEngine", "Discard the freed space")}};
    if (action == QLatin1String("wipe"))
        return {{"keys", QT_TRANSLATE_NOOP("RescueEngine", "Destroy the encryption keys")}, {"discard", QT_TRANSLATE_NOOP("RescueEngine", "Erase every block of the disk")}};
    return {};
}

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

RescueEngine::RescueEngine(bool demo, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
{
    m_demoTimer.setInterval(90);
    connect(&m_demoTimer, &QTimer::timeout, this, &RescueEngine::demoTick);
    m_job = item({{"state", "idle"}});
    if (m_demo)
        loadDemo();
    else
        loadSystem();
}

// ───────────────────────────────────────────────────────────────────────── data

void RescueEngine::loadDemo()
{
    m_status = item({
        {"version", "2026.11.15.1"}, {"previousVersion", "2026.11.01.2"},
        {"reason", tr("Asima OS 2026.11.15.1 didn't finish starting three times in a row.")},
        {"disk", item({{"path", "/dev/nvme0n1"}, {"model", "Samsung SSD 990 PRO 1TB"}, {"size", 1000204886016.0},
                       {"serial", "S6Z2NJ0W817K2Q"}})},
        {"encrypted", true}, {"unlocked", false}, {"secureBoot", "on"}, {"keyApproved", true},
        {"tpm", true}, {"tpmEnrolled", true}, {"freeBytes", 1.4e9}, {"totalBytes", 1000204886016.0},
        {"reserveBytes", 2e9},
    });
    m_snapshots = {
        item({{"id", "@snapshots/home-2026-11-16-0300"}, {"title", tr("Today, 03:00")}, {"kind", "daily"}, {"size", 212e6}}),
        item({{"id", "@snapshots/home-2026-11-15-0300"}, {"title", tr("Yesterday, 03:00")}, {"kind", "daily"}, {"size", 1.1e9}}),
        item({{"id", "@snapshots/update-2026.11.15.1"}, {"title", tr("Before the update to 2026.11.15.1")}, {"kind", "update"}, {"size", 3.4e9}}),
        item({{"id", "@snapshots/home-2026-11-14-0300"}, {"title", tr("14 November, 03:00")}, {"kind", "daily"}, {"size", 0.9e9}}),
    };
    m_updateFiles = {
        item({{"path", "/run/rescue/usb/asimaos-2026.11.15.1.aosupd"}, {"version", "2026.11.15.1"}, {"size", 3.9e9},
              {"older", false}, {"device", "SanDisk Ultra Fit"}}),
        item({{"path", "/run/rescue/usb/asimaos-2026.11.01.2.aosupd"}, {"version", "2026.11.01.2"}, {"size", 3.8e9},
              {"older", true}, {"device", "SanDisk Ultra Fit"}}),
    };
}

void RescueEngine::loadSystem()
{
    QProcess p;
    p.start(engine(), {QStringLiteral("status"), QStringLiteral("--json")});
    if (!p.waitForFinished(15000)) {
        m_status = item({{"error", tr("The rescue engine did not answer.")}});
        return;
    }
    const QJsonObject o = QJsonDocument::fromJson(p.readAllStandardOutput()).object();
    m_status = o.toVariantMap();
    m_snapshots = o.value(QStringLiteral("snapshots")).toArray().toVariantList();
    m_updateFiles = o.value(QStringLiteral("updateFiles")).toArray().toVariantList();
}

QString RescueEngine::engine() const
{
    if (const QString e = qEnvironmentVariable("ASIMAOS_RESCUE_ENGINE"); !e.isEmpty())
        return e;
    const QString inPath = QStandardPaths::findExecutable(QStringLiteral("asimaos-rescue-engine"));
    return inPath.isEmpty() ? QStringLiteral("/usr/lib/asimaos/asimaos-rescue-engine") : inPath;
}

void RescueEngine::refresh()
{
    if (!m_demo)
        loadSystem();
    emit statusChanged();
}

// ───────────────────────────────────────────────────────────────────────── unlock

void RescueEngine::unlock(const QString &secret)
{
    if (m_demo) {
        const bool ok = secret.length() >= 8;
        if (ok) {
            m_status.insert("unlocked", true);
            emit statusChanged();
        }
        emit unlocked(ok, ok ? tr("Your disk is unlocked.") : tr("That didn't unlock the disk. Check the keyboard layout (US) and try again."));
        return;
    }
    auto *p = new QProcess(this);
    connect(p, &QProcess::finished, this, [this, p](int code) {
        const bool ok = code == 0;
        if (ok)
            loadSystem();
        emit statusChanged();
        emit unlocked(ok, ok ? tr("Your disk is unlocked.") : tr("That didn't unlock the disk. Check the keyboard layout (US) and try again."));
        p->deleteLater();
    });
    p->start(engine(), {QStringLiteral("unlock"), QStringLiteral("--json"), QStringLiteral("--secret-stdin")});
    p->write(secret.toUtf8() + '\n');   // never on the command line
    p->closeWriteChannel();
}

// ───────────────────────────────────────────────────────────────────────── jobs

void RescueEngine::beginJob(const QString &action, const QString &title, const QStringList &stepTitles)
{
    QVariantList steps;
    for (int i = 0; i < stepTitles.size(); ++i)
        steps.append(item({{"id", m_stepIds.value(i)}, {"title", stepTitles.at(i)}, {"state", "pending"}, {"detail", QString()}}));
    m_job = item({{"action", action}, {"title", title}, {"state", "running"}, {"progress", 0.0}, {"detail", QString()},
                  {"steps", steps}, {"message", QString()}, {"code", QString()}, {"recoveryKey", QString()}});
    emit jobChanged();
}

void RescueEngine::setStep(int index, const QString &state)
{
    QVariantList steps = m_job.value("steps").toList();
    if (index < 0 || index >= steps.size())
        return;
    QVariantMap s = steps.at(index).toMap();
    s.insert("state", state);
    steps[index] = s;
    m_job.insert("steps", steps);
}

void RescueEngine::start(const QString &action, const QVariantMap &args)
{
    static const QHash<QString, const char *> titles{
        {QStringLiteral("check"), QT_TR_NOOP("Checking the disk")},
        {QStringLiteral("repair"), QT_TR_NOOP("Repairing the file system")},
        {QStringLiteral("free-space"), QT_TR_NOOP("Freeing emergency space")},
        {QStringLiteral("restore"), QT_TR_NOOP("Restoring your files")},
        {QStringLiteral("reinstall"), QT_TR_NOOP("Reinstalling Asima OS")},
        {QStringLiteral("start-previous"), QT_TR_NOOP("Starting the previous version")},
        {QStringLiteral("approve-key"), QT_TR_NOOP("Approving the Asima OS key")},
        {QStringLiteral("remove-tpm"), QT_TR_NOOP("Turning off automatic unlock")},
        {QStringLiteral("factory-reset"), QT_TR_NOOP("Factory reset")},
        {QStringLiteral("wipe"), QT_TR_NOOP("Erasing the disk")},
    };
    if (!titles.contains(action) || m_job.value("state") == QLatin1String("running"))
        return;
    m_stepIds.clear();
    QStringList stepTitles;
    for (const Step &s : stepsFor(action)) {
        m_stepIds << QString::fromLatin1(s.id);
        stepTitles << tr(s.title);
    }
    beginJob(action, tr(titles.value(action)), stepTitles);

    if (m_demo) {
        m_demoProgress = 0.0;
        m_demoTimer.start();
        return;
    }

    QStringList cli{action, QStringLiteral("--json")};
    QByteArray input;
    if (action == QLatin1String("restore"))
        cli << args.value("snapshot").toString();
    else if (action == QLatin1String("reinstall"))
        cli << args.value("file").toString();
    else if (action == QLatin1String("wipe"))
        cli << QStringLiteral("--serial-tail") << args.value("serialTail").toString();
    else if (action == QLatin1String("factory-reset")) {
        cli << QStringLiteral("--user") << args.value("username").toString()
            << QStringLiteral("--fullname") << args.value("fullName").toString() << QStringLiteral("--secrets-stdin");
        // Account password, then the new disk passphrase: one per line, never on the command line.
        input = args.value("password").toString().toUtf8() + '\n' + args.value("passphrase").toString().toUtf8() + '\n';
    }

    m_buffer.clear();
    m_process = new QProcess(this);
    connect(m_process, &QProcess::readyReadStandardOutput, this, [this] {
        m_buffer += m_process->readAllStandardOutput();
        for (int nl = m_buffer.indexOf('\n'); nl >= 0; nl = m_buffer.indexOf('\n')) {
            const QByteArray line = m_buffer.left(nl).trimmed();
            m_buffer.remove(0, nl + 1);
            if (!line.isEmpty())
                handleEvent(line);
        }
    });
    connect(m_process, &QProcess::finished, this, [this](int code) {
        if (m_job.value("state") == QLatin1String("running"))
            finish(false, tr("The rescue engine stopped unexpectedly (code %1).").arg(code));
        m_process->deleteLater();
        refresh();
    });
    m_process->start(engine(), cli);
    m_process->write(input);
    m_process->closeWriteChannel();
}

void RescueEngine::handleEvent(const QByteArray &line)
{
    const QJsonObject e = QJsonDocument::fromJson(line).object();
    const QString kind = e.value(QStringLiteral("event")).toString();
    if (kind == QLatin1String("step")) {
        setStep(m_stepIds.indexOf(e.value(QStringLiteral("id")).toString()), e.value(QStringLiteral("state")).toString());
    } else if (kind == QLatin1String("progress")) {
        m_job.insert("progress", e.value(QStringLiteral("value")).toDouble());
        m_job.insert("detail", e.value(QStringLiteral("detail")).toString());
    } else if (kind == QLatin1String("code")) {
        m_job.insert("code", e.value(QStringLiteral("code")).toString());
    } else if (kind == QLatin1String("recovery-key")) {
        m_job.insert("recoveryKey", e.value(QStringLiteral("key")).toString());
    } else if (kind == QLatin1String("error")) {
        finish(false, e.value(QStringLiteral("message")).toString());
        return;
    } else if (kind == QLatin1String("done")) {
        finish(true, e.value(QStringLiteral("message")).toString());
        return;
    }
    emit jobChanged();
}

void RescueEngine::finish(bool ok, const QString &message)
{
    m_demoTimer.stop();
    m_job.insert("state", ok ? "done" : "error");
    m_job.insert("message", message);
    if (ok)
        m_job.insert("progress", 1.0);
    emit jobChanged();
}

void RescueEngine::clearJob()
{
    if (m_job.value("state") == QLatin1String("running"))
        return;
    m_job = item({{"state", "idle"}});
    emit jobChanged();
}

void RescueEngine::demoTick()
{
    const int n = int(m_stepIds.size());
    m_demoProgress = qMin(1.0, m_demoProgress + 0.012);
    for (int i = 0; i < n; ++i) {
        const double from = double(i) / n, to = double(i + 1) / n;
        setStep(i, m_demoProgress >= to ? QStringLiteral("done") : m_demoProgress >= from ? QStringLiteral("active") : QStringLiteral("pending"));
    }
    m_job.insert("progress", m_demoProgress);
    const QString action = m_job.value("action").toString();
    if (action == QLatin1String("wipe"))
        m_job.insert("detail", tr("Erased %1 of 1.0 TB").arg(m_demoProgress, 0, 'f', 2));
    else if (action == QLatin1String("reinstall"))
        m_job.insert("detail", tr("Writing · %1 of 3.9 GB").arg(m_demoProgress * 3.9, 0, 'f', 1));
    if (m_demoProgress < 1.0) {
        emit jobChanged();
        return;
    }
    QString message;
    if (action == QLatin1String("check"))
        message = tr("The file system and both Asima OS versions are fine. The disk was nearly full: freeing the emergency space lets Asima OS start again.");
    else if (action == QLatin1String("free-space")) {
        message = tr("2 GB were freed. Asima OS has room to start; delete some files soon.");
        m_status.insert("freeBytes", m_status.value("freeBytes").toDouble() + 2e9);
        m_status.insert("reserveBytes", 0.0);
        emit statusChanged();
    } else if (action == QLatin1String("approve-key")) {
        m_job.insert("code", QStringLiteral("%1 %2").arg(QRandomGenerator::global()->bounded(1000, 10000))
                                                     .arg(QRandomGenerator::global()->bounded(1000, 10000)));
        message = tr("At the next start, type this code on the blue screen to approve the Asima OS key.");
    } else if (action == QLatin1String("factory-reset")) {
        m_job.insert("recoveryKey", demoRecoveryKey());
        message = tr("Asima OS is fresh. Restart and sign in with the account you chose.");
    } else if (action == QLatin1String("wipe"))
        message = tr("Nothing is left on this disk. To use the computer again, install an operating system from a USB stick.");
    else
        message = tr("Done.");
    finish(true, message);
}

void RescueEngine::restart()
{
    if (!m_demo)
        QProcess::startDetached(QStringLiteral("systemctl"), {QStringLiteral("reboot")});
}

void RescueEngine::powerOff()
{
    if (!m_demo)
        QProcess::startDetached(QStringLiteral("systemctl"), {QStringLiteral("poweroff")});
}

void RescueEngine::setDemoState(const QString &state)
{
    if (state != QLatin1String("locked")) {
        m_status.insert("unlocked", true);
        emit statusChanged();
    }
    const QString action = state.section(QLatin1Char('-'), 0, -2);
    const QString phase = state.section(QLatin1Char('-'), -1);
    if (phase == QLatin1String("running") || phase == QLatin1String("done")) {
        start(action);
        m_demoTimer.stop();
        m_demoProgress = phase == QLatin1String("done") ? 0.999 : 0.55;
        demoTick();
    }
}
