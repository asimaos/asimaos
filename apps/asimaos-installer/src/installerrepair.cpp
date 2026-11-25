// "Repair this computer" with screens (D55). The installer's Repair button opens them; the repair
// engine on the USB stick (asimaos-repair, services/asimaos-engine/src/repair.rs) does the work and
// reports one JSON object per line: the disks with Asima OS, what one holds once its passphrase
// opened it ("--probe", nothing changes), then the repair's steps. Nobody's files are touched.

#include "installerengine.h"

#include <QCoreApplication>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTimer>

#include <unistd.h>

namespace {

QString repairTool()
{
    const QStringList candidates{
        qEnvironmentVariable("ASIMAOS_REPAIR_ENGINE"),
        QStringLiteral("/run/initramfs/live/asimaos/asimaos-repair"),
        QCoreApplication::applicationDirPath() + QStringLiteral("/asimaos-repair"),
    };
    for (const QString &c : candidates)
        if (!c.isEmpty() && QFileInfo(c).isExecutable())
            return c;
    return {};
}

QVariantMap repairStep(const QString &id, const QString &title)
{
    return QVariantMap{{"id", id}, {"title", title}, {"state", "pending"}};
}

// The engine's messages are written for a terminal ("the pool could not be opened: …").
QString sentence(QString s)
{
    if (!s.isEmpty())
        s[0] = s[0].toUpper();
    if (!s.isEmpty() && !s.endsWith(QLatin1Char('.')))
        s += QLatin1Char('.');
    return s;
}

} // namespace

void InstallerEngine::findRepairDisks()
{
    m_repair = QVariantMap{{"phase", "disks"}, {"state", "working"}};
    emit repairChanged();
    if (m_demo)
        return demoRepair(QStringLiteral("disks"), {});
    runRepair(QStringLiteral("disks"), {QStringLiteral("--json")}, QString());
}

void InstallerEngine::probeRepair(const QString &disk, const QString &passphrase)
{
    m_repair.insert("phase", "probe");
    m_repair.insert("state", "working");
    m_repair.insert("disk", disk);
    m_repair.remove("probe");
    m_repair.remove("message");
    emit repairChanged();
    if (m_demo)
        return demoRepair(QStringLiteral("probe"), {{"passphrase", passphrase}});
    QStringList args{disk, QStringLiteral("--probe"), QStringLiteral("--yes"), QStringLiteral("--json")};
    if (!passphrase.isEmpty())
        args << QStringLiteral("--passphrase-stdin");
    runRepair(QStringLiteral("probe"), args, passphrase);
}

void InstallerEngine::startRepair(const QVariantMap &options)
{
    const bool stick = options.value("installStick").toBool();
    const bool account = options.value("restoreAccount").toBool();
    const bool encrypted = m_repair.value("probe").toMap().value("encrypted").toBool();
    m_repair.insert("phase", "run");
    m_repair.insert("state", "working");
    m_repair.insert("steps", repairSteps(encrypted, stick, account));
    m_repair.insert("installStick", stick);
    m_repair.remove("message");
    m_repair.remove("log");
    emit repairChanged();
    if (m_demo)
        return demoRepair(QStringLiteral("run"), options);
    QStringList args{options.value("disk").toString(), QStringLiteral("--yes"), QStringLiteral("--json")};
    if (encrypted)
        args << QStringLiteral("--passphrase-stdin");
    if (stick)
        args << QStringLiteral("--install-this-version");
    if (account)
        args << QStringLiteral("--restore-account");
    runRepair(QStringLiteral("run"), args, encrypted ? options.value("passphrase").toString() : QString());
}

// The engine's steps (repair.rs), in its order.
QVariantList InstallerEngine::repairSteps(bool encrypted, bool stick, bool account) const
{
    QVariantList steps{repairStep(QStringLiteral("check"), tr("Find Asima OS on the disk"))};
    if (encrypted)
        steps << repairStep(QStringLiteral("unlock"), tr("Open the encrypted disk"));
    if (stick)
        steps << repairStep(QStringLiteral("install"), tr("Install Asima OS %1 from this USB stick").arg(m_repair.value("stick").toString()));
    steps << repairStep(QStringLiteral("esp"), tr("Check the boot partition"))
          << repairStep(QStringLiteral("boot"), tr("Put back the start menu"));
    if (account)
        steps << repairStep(QStringLiteral("account"), tr("Bring back the settings and accounts"));
    steps << repairStep(QStringLiteral("finish"), tr("Finish"));
    return steps;
}

void InstallerEngine::runRepair(const QString &phase, const QStringList &engineArgs, const QString &secret)
{
    const QString tool = repairTool();
    if (tool.isEmpty())
        return repairFailed(tr("Repairing works from the Asima OS USB stick: its repair tool was not found."));
    QString program = tool;
    QStringList args = engineArgs;
    if (::geteuid() != 0) {
        args.prepend(tool);
        program = QStringLiteral("pkexec");
    }
    m_repairBuffer.clear();
    auto *p = new QProcess(this);
    m_repairProcess = p;
    connect(p, &QProcess::readyReadStandardOutput, this, [this, p] {
        m_repairBuffer += p->readAllStandardOutput();
        for (qsizetype nl = m_repairBuffer.indexOf('\n'); nl >= 0; nl = m_repairBuffer.indexOf('\n')) {
            const QByteArray line = m_repairBuffer.left(nl).trimmed();
            m_repairBuffer.remove(0, nl + 1);
            if (!line.isEmpty())
                repairEvent(line);
        }
    });
    connect(p, &QProcess::errorOccurred, this, [this, p](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart)
            repairFailed(tr("The repair tool could not be started: %1").arg(p->errorString()));
    });
    connect(p, &QProcess::finished, this, [this, p, phase](int code, QProcess::ExitStatus status) {
        if (m_repair.value("state") == QLatin1String("working")) {
            // The disks and the probe end with their report; a repair ends with "done".
            if (status == QProcess::NormalExit && code == 0 && phase != QLatin1String("run")) {
                m_repair.insert("state", "ok");
                emit repairChanged();
            } else {
                repairFailed(status == QProcess::NormalExit && code == 126
                                 ? tr("Permission to repair was not given.")
                                 : tr("The repair tool stopped unexpectedly (code %1).").arg(code));
            }
        }
        p->deleteLater();
    });
    p->start(program, args);
    // The passphrase goes through a pipe, never on the command line or in the environment.
    if (!secret.isEmpty())
        p->write(secret.toUtf8() + '\n');
    p->closeWriteChannel();
}

void InstallerEngine::repairEvent(const QByteArray &line)
{
    const QJsonObject e = QJsonDocument::fromJson(line).object();
    const QString event = e.value(QStringLiteral("event")).toString();
    if (event == QLatin1String("disks")) {
        m_repair.insert("disks", e.value(QStringLiteral("disks")).toArray().toVariantList());
        m_repair.insert("stick", e.value(QStringLiteral("stick")).toString());
        m_repair.insert("state", "ok");
    } else if (event == QLatin1String("probe")) {
        m_repair.insert("probe", e.toVariantMap());
        m_repair.insert("state", "ok");
    } else if (event == QLatin1String("step") && m_repair.value("phase") == QLatin1String("run")) {
        const QString id = e.value(QStringLiteral("id")).toString();
        QVariantList steps = m_repair.value("steps").toList();
        for (QVariant &s : steps) {
            QVariantMap m = s.toMap();
            if (m.value("id").toString() == id)
                m.insert("state", e.value(QStringLiteral("state")).toString());
            s = m;
        }
        m_repair.insert("steps", steps);
    } else if (event == QLatin1String("log")) {
        QStringList log = m_repair.value("log").toStringList();
        log << sentence(e.value(QStringLiteral("message")).toString());
        m_repair.insert("log", log);
    } else if (event == QLatin1String("error")) {
        return repairFailed(sentence(e.value(QStringLiteral("message")).toString()));
    } else if (event == QLatin1String("done")) {
        m_repair.insert("state", "ok");
        m_repair.insert("version", e.value(QStringLiteral("version")).toString());
    } else {
        return;
    }
    emit repairChanged();
}

void InstallerEngine::repairFailed(const QString &message)
{
    m_repair.insert("state", "error");
    m_repair.insert("message", message);
    QVariantList steps = m_repair.value("steps").toList();
    for (QVariant &s : steps) {
        QVariantMap m = s.toMap();
        if (m.value("state").toString() == QLatin1String("active"))
            m.insert("state", "failed");
        s = m;
    }
    m_repair.insert("steps", steps);
    emit repairChanged();
}

// Demo mode (design work, screenshots): a computer with one encrypted disk holding two versions.
void InstallerEngine::demoRepair(const QString &phase, const QVariantMap &options)
{
    const QString stick = m_system.value("version").toString().isEmpty() ? QStringLiteral("2026.11.25.1")
                                                                          : m_system.value("version").toString();
    if (phase == QLatin1String("disks")) {
        m_repair.insert("disks", QVariantList{QVariantMap{{"path", "/dev/nvme0n1"}, {"size", "953.9G"},
                                                           {"model", "Samsung SSD 980 PRO 1TB"}, {"encrypted", true}}});
        m_repair.insert("stick", stick);
        m_repair.insert("state", "ok");
    } else if (phase == QLatin1String("probe")) {
        if (options.value("passphrase").toString() == QLatin1String("wrong"))
            return repairFailed(tr("The pool could not be opened: wrong passphrase or recovery key."));
        m_repair.insert("probe", QVariantMap{{"versions", QVariantList{"2026.11.22.1", "2026.11.15.1"}}, {"stick", stick},
                                             {"settings", "2026.11.22.1"}, {"snapshot", "2026-11-24-0300"}, {"encrypted", true}});
        m_repair.insert("state", "ok");
    } else {
        // The steps go by one after another.
        auto *t = new QTimer(this);
        t->setInterval(700);
        connect(t, &QTimer::timeout, this, [this, t] {
            QVariantList steps = m_repair.value("steps").toList();
            bool advanced = false;
            for (int i = 0; i < steps.size() && !advanced; ++i) {
                QVariantMap m = steps[i].toMap();
                if (m.value("state").toString() == QLatin1String("active")) {
                    m.insert("state", "done");
                    steps[i] = m;
                } else if (m.value("state").toString() == QLatin1String("pending")) {
                    m.insert("state", "active");
                    steps[i] = m;
                    advanced = true;
                }
            }
            m_repair.insert("steps", steps);
            if (!advanced) {
                m_repair.insert("state", "ok");
                m_repair.insert("version", m_repair.value("installStick").toBool() ? m_repair.value("stick") : QVariant("2026.11.22.1"));
                t->stop();
                t->deleteLater();
            }
            emit repairChanged();
        });
        t->start();
        return;
    }
    emit repairChanged();
}

// Screenshots: "disks", "probed", "running", "done" or "error".
void InstallerEngine::demoRepairState(const QString &what)
{
    demoRepair(QStringLiteral("disks"), {});
    m_repair.insert("phase", "disks");
    if (what == QLatin1String("disks"))
        return;
    m_repair.insert("disk", "/dev/nvme0n1");
    demoRepair(QStringLiteral("probe"), {});
    m_repair.insert("phase", "probe");
    if (what == QLatin1String("probed"))
        return;
    QVariantList steps = repairSteps(true, true, false);
    const int current = what == QLatin1String("running") ? 2 : what == QLatin1String("error") ? 3 : int(steps.size());
    for (int i = 0; i < steps.size(); ++i) {
        QVariantMap m = steps[i].toMap();
        m.insert("state", i < current ? "done" : i > current ? "pending" : what == QLatin1String("error") ? "failed" : "active");
        steps[i] = m;
    }
    m_repair.insert("phase", "run");
    m_repair.insert("steps", steps);
    m_repair.insert("installStick", true);
    if (what == QLatin1String("done")) {
        m_repair.insert("state", "ok");
        m_repair.insert("version", m_repair.value("stick"));
    } else if (what == QLatin1String("error")) {
        m_repair.insert("state", "error");
        m_repair.insert("message", tr("The boot partition could not be formatted."));
    } else {
        m_repair.insert("state", "working");
    }
    emit repairChanged();
}
