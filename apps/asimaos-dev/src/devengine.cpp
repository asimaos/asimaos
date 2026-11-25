#include "devengine.h"
#include "serviceclient.h"
#include "util.h"

#include <QDBusConnection>
#include <QDBusError>
#include <QDBusMessage>
#include <QDBusObjectPath>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDir>
#include <QStandardPaths>
#include <QThread>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QTimer>
#include <QJsonDocument>
#include <QJsonObject>
#include <QDesktopServices>
#include <algorithm>
#include <QProcess>
#include <QRegularExpression>
#include <QUrl>

using Util::item;

DevEngine::DevEngine(bool demo, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
{
    if (!m_demo) {
        readWorkspaces();
        readPassthrough();
        readStorage();
        return;
    }
    const auto gb = [](double n) { return n * 1e9; };
    m_storage = item({{"available", true}, {"counted", true},
        {"areas", QVariantList{
            item({{"id", "containers:asha"}, {"person", "asha"}, {"separate", true}, {"used", gb(14.2)}, {"limit", gb(40)}}),
            item({{"id", "vm:asha"}, {"person", "asha"}, {"separate", true}, {"used", gb(48.3)}, {"limit", gb(120)}}),
            item({{"id", "nix"}, {"person", QVariant()}, {"separate", true}, {"used", gb(6.4)}, {"limit", QVariant()}}),
            item({{"id", "containers"}, {"person", QVariant()}, {"separate", true}, {"used", gb(0.2)}, {"limit", QVariant()}}),
            item({{"id", "vm"}, {"person", QVariant()}, {"separate", true}, {"used", gb(0.0)}, {"limit", QVariant()}})}}});
    m_passthrough = item({{"iommu", true}, {"kept", QString()},
        {"gpus", QVariantList{
            item({{"address", "0000:06:00.0"}, {"slot", "0000:06:00"}, {"name", "AMD Radeon 780M"}, {"screen", true}, {"driver", "amdgpu"},
                  {"kept", false}, {"ready", false}, {"shared", QStringList{}}}),
            item({{"address", "0000:01:00.0"}, {"slot", "0000:01:00"}, {"name", "NVIDIA GeForce RTX 4060"}, {"screen", false},
                  {"driver", "nvidia"}, {"kept", false}, {"ready", false}, {"shared", QStringList{}}})}}});
    m_workspaces = item({
        {"podman", true}, {"distrobox", true},
        {"containers", QVariantList{
            item({{"name", "ubuntu-dev"}, {"image", "Ubuntu 24.04"}, {"state", "running"}, {"size", 2.1e9}}),
            item({{"name", "fedora"}, {"image", "Fedora 45"}, {"state", "exited"}, {"size", 1.6e9}}),
            item({{"name", "arch-aur"}, {"image", "Arch Linux"}, {"state", "exited"}, {"size", 2.7e9}})}},
        {"vms", QVariantList{item({{"name", "Windows 11"}, {"detail", tr("UEFI · TPM · 8 GB · 4 cores")}, {"state", "shut off"}, {"size", 42e9}})}},
    });
}

void DevEngine::readWorkspaces()
{
    QVariantList containers;
    const bool podman = Util::has(QStringLiteral("podman"));
    if (podman) {
        const QJsonArray list = QJsonDocument::fromJson(Util::run(QStringLiteral("podman"), {QStringLiteral("ps"), QStringLiteral("-a"),
                                                         QStringLiteral("--format"), QStringLiteral("json")}).toUtf8()).array();
        for (const QJsonValue &v : list) {
            const QJsonObject c = v.toObject();
            containers.append(item({{"name", c.value("Names").toArray().first().toString()}, {"image", c.value("Image").toString().section(QLatin1Char('/'), -1)},
                                    {"state", c.value("State").toString()}, {"size", -1.0}}));
        }
    }
    m_workspaces = item({{"podman", podman}, {"distrobox", Util::has(QStringLiteral("distrobox"))}, {"containers", containers},
                         {"vms", readVms()}, {"libvirt", Util::has(QStringLiteral("virsh"))}});
}

void DevEngine::openUrl(const QString &url)
{
    if (m_demo || !QDesktopServices::openUrl(QUrl(url)))
        emit toast(tr("Opens %1.").arg(url), QStringLiteral("info"));
}

void DevEngine::createContainer(const QString &image, const QString &name)
{
    if (m_demo) {
        QVariantList list = m_workspaces.value("containers").toList();
        list.prepend(item({{"name", name}, {"image", image}, {"state", "created"}, {"size", 0.4e9}}));
        m_workspaces.insert("containers", list);
        emit changed();
        emit toast(tr("Container \"%1\" is ready. Open it to start working.").arg(name), QStringLiteral("ok"));
        return;
    }
    if (!Util::has(QStringLiteral("distrobox"))) {
        emit toast(tr("Distrobox isn't installed on this system."), QStringLiteral("warn"));
        return;
    }
    auto *p = new QProcess(this);
    connect(p, &QProcess::finished, this, [this, p, name](int code) {
        emit toast(code == 0 ? tr("Container \"%1\" is ready.").arg(name) : tr("Creating \"%1\" failed.").arg(name),
                   code == 0 ? QStringLiteral("ok") : QStringLiteral("error"));
        readWorkspaces();
        emit changed();
        p->deleteLater();
    });
    p->start(QStringLiteral("distrobox"), {QStringLiteral("create"), QStringLiteral("--yes"), QStringLiteral("--name"), name,
                                           QStringLiteral("--image"), image});
    emit toast(tr("Creating \"%1\"… this downloads the image once.").arg(name), QStringLiteral("info"));
}

void DevEngine::openContainer(const QString &name)
{
    if (m_demo || !QProcess::startDetached(QStringLiteral("konsole"), {QStringLiteral("-e"), QStringLiteral("distrobox"), QStringLiteral("enter"), name}))
        emit toast(tr("Opens a terminal inside \"%1\".").arg(name), QStringLiteral("info"));
}

void DevEngine::openApp(const QString &command)
{
    const QStringList parts = QProcess::splitCommand(command);
    if (m_demo || parts.isEmpty() || !QProcess::startDetached(parts.first(), parts.mid(1)))
        emit toast(tr("Opens %1.").arg(parts.value(0)), QStringLiteral("info"));
}

// ───────────────────────────────────────────────────────────────────────── system units

// Starts a system unit through systemd; polkit asks for the administrator's password first.
void DevEngine::startUnit(const QString &unit, const QString &started)
{
    QDBusMessage m = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.systemd1"), QStringLiteral("/org/freedesktop/systemd1"),
                                                    QStringLiteral("org.freedesktop.systemd1.Manager"), QStringLiteral("StartUnit"));
    m << unit << QStringLiteral("replace");
    m.setInteractiveAuthorizationAllowed(true);
    // The password dialog may stay open a while.
    auto *w = new QDBusPendingCallWatcher(QDBusConnection::systemBus().asyncCall(m, 10 * 60 * 1000), this);
    connect(w, &QDBusPendingCallWatcher::finished, this, [this, w, started] {
        const QDBusPendingReply<QDBusObjectPath> r = *w;
        w->deleteLater();
        if (r.isError()) {
            const bool refused = r.error().type() == QDBusError::AccessDenied
                              || r.error().name() == QLatin1String("org.freedesktop.DBus.Error.InteractiveAuthorizationRequired");
            emit toast(refused ? tr("Not allowed: the administrator's password is needed.") : r.error().message(), QStringLiteral("warn"));
        } else {
            emit toast(started, QStringLiteral("info"));
        }
        // The unit needs a moment to switch to "activating".
        QTimer::singleShot(800, this, [this] { readPassthrough(); emit changed(); });
    });
}

// ───────────────────────────────────────────────────────────────────────── virtual machines

namespace {
const QString kSession = QStringLiteral("qemu:///session");

QString vmDir()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation) + QStringLiteral("/libvirt/images");
}
} // namespace

int DevEngine::hostCpus() const
{
    return qMax(1, QThread::idealThreadCount());
}

int DevEngine::hostMemoryGb() const
{
    QFile f(QStringLiteral("/proc/meminfo"));
    if (!f.open(QIODevice::ReadOnly))
        return 8;
    const QString first = QString::fromLatin1(f.readLine());   // MemTotal: N kB
    return qMax(2, int(first.section(QLatin1Char(':'), 1).trimmed().section(QLatin1Char(' '), 0, 0).toLongLong() / (1024 * 1024)));
}

QVariantList DevEngine::readVms() const
{
    QVariantList vms;
    if (!Util::has(QStringLiteral("virsh")))
        return vms;
    const QStringList names = Util::run(QStringLiteral("virsh"), {QStringLiteral("-c"), kSession, QStringLiteral("list"), QStringLiteral("--all"),
                                                                  QStringLiteral("--name")}).split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    for (const QString &name : names) {
        QVariantMap info;
        for (const QString &line : Util::run(QStringLiteral("virsh"), {QStringLiteral("-c"), kSession, QStringLiteral("dominfo"), name}).split(QLatin1Char('\n')))
            info.insert(line.section(QLatin1Char(':'), 0, 0).trimmed(), line.section(QLatin1Char(':'), 1).trimmed());
        const qint64 kib = info.value("Max memory").toString().section(QLatin1Char(' '), 0, 0).toLongLong();
        const QString state = info.value("State").toString();
        vms.append(item({{"name", name.trimmed()},
                         {"detail", tr("%1 GB · %n core(s)", "", info.value("CPU(s)").toInt()).arg(qRound(kib / 1048576.0))},
                         {"state", state == QLatin1String("running") ? QStringLiteral("running") : QStringLiteral("shut off")},
                         {"size", QFileInfo(vmDir() + QLatin1Char('/') + name.trimmed() + QStringLiteral(".qcow2")).size()}}));
    }
    return vms;
}

void DevEngine::virsh(const QStringList &args, const QString &done, const QString &failed)
{
    auto *p = new QProcess(this);
    connect(p, &QProcess::finished, this, [this, p, done, failed](int code) {
        emit toast(code == 0 ? done : failed + QLatin1Char(' ') + QString::fromUtf8(p->readAllStandardError()).trimmed().section(QLatin1Char('\n'), -1),
                   code == 0 ? QStringLiteral("ok") : QStringLiteral("error"));
        readWorkspaces();
        emit changed();
        p->deleteLater();
    });
    p->start(QStringLiteral("virsh"), QStringList{QStringLiteral("-c"), kSession} + args);
}

void DevEngine::createVm(const QString &name, const QString &kind, const QUrl &isoUrl, int memoryGb, int cpus, int diskGb,
                         const QString &gpu)
{
    static const QRegularExpression valid(QStringLiteral("^[A-Za-z0-9][A-Za-z0-9 ._-]{0,39}$"));
    if (!valid.match(name).hasMatch()) {
        emit toast(tr("Use letters, digits, spaces, dots and dashes for the name."), QStringLiteral("warn"));
        return;
    }
    const QString iso = isoUrl.isLocalFile() ? isoUrl.toLocalFile() : isoUrl.toString();
    const bool windows = kind == QLatin1String("windows");
    if (m_demo) {
        QVariantList list = m_workspaces.value("vms").toList();
        list.prepend(item({{"name", name}, {"detail", tr("%1 GB · %n core(s)", "", cpus).arg(memoryGb)}, {"state", "shut off"}, {"size", 0.2e9}}));
        m_workspaces.insert("vms", list);
        emit changed();
        emit toast(tr("\"%1\" is ready. Start it to install from the disc.").arg(name), QStringLiteral("ok"));
        return;
    }
    if (!Util::has(QStringLiteral("virsh")) || !Util::has(QStringLiteral("qemu-img"))) {
        emit toast(tr("The virtual machine tools (libvirt, QEMU) are missing."), QStringLiteral("warn"));
        return;
    }
    if (iso.isEmpty() || !QFileInfo(iso).isFile()) {
        emit toast(tr("Choose the installer disc (an .iso file) first."), QStringLiteral("warn"));
        return;
    }
    QDir().mkpath(vmDir());
    const QString disk = vmDir() + QLatin1Char('/') + QString(name).replace(QLatin1Char(' '), QLatin1Char('-')) + QStringLiteral(".qcow2");
    if (QFileInfo::exists(disk)) {
        emit toast(tr("A virtual machine disk with this name exists already."), QStringLiteral("warn"));
        return;
    }
    const QString esc = name.toHtmlEscaped();
    // Passthrough: every function of the kept card, already on vfio-pci since the start.
    QString hostdevs;
    if (!gpu.isEmpty()) {
        static const QRegularExpression slotRx(QStringLiteral("^([0-9a-f]{4}):([0-9a-f]{2}):([0-9a-f]{2})$"));
        const QRegularExpressionMatch m = slotRx.match(gpu);
        const QDir pci(QStringLiteral("/sys/bus/pci/devices"));
        const QStringList functions = pci.entryList({gpu + QStringLiteral(".*")}, QDir::Dirs | QDir::System | QDir::NoDotAndDotDot);
        if (!m.hasMatch() || functions.isEmpty()) {
            emit toast(tr("That graphics card isn't ready for virtual machines yet."), QStringLiteral("warn"));
            return;
        }
        for (const QString &f : functions)
            hostdevs += QStringLiteral("\n    <hostdev mode='subsystem' type='pci' managed='no'><source><address domain='0x%1' bus='0x%2' "
                                       "slot='0x%3' function='0x%4'/></source></hostdev>")
                            .arg(m.captured(1), m.captured(2), m.captured(3), f.section(QLatin1Char('.'), -1));
    }
    // Windows has no virtio drivers on its disc: SATA and an Intel network card work out of the box.
    const QString xml = QStringLiteral(R"(<domain type='kvm'>
  <name>%1</name>
  <memory unit='GiB'>%2</memory>
  <vcpu>%3</vcpu>
  <os firmware='efi'>
    <type arch='x86_64' machine='q35'>hvm</type>%4
    <boot dev='cdrom'/>
    <boot dev='hd'/>
  </os>
  <features><acpi/><apic/>%5</features>
  <cpu mode='host-passthrough'/>
  <clock offset='%6'/>
  <devices>
    <disk type='file' device='disk'><driver name='qemu' type='qcow2' discard='unmap'/><source file='%7'/><target dev='%8' bus='%9'/></disk>
    <disk type='file' device='cdrom'><driver name='qemu' type='raw'/><source file='%10'/><target dev='sdc' bus='sata'/><readonly/></disk>
    <interface type='user'><backend type='passt'/><model type='%11'/></interface>%12%14
    <graphics type='spice' autoport='yes'><listen type='none'/></graphics>
    <video><model type='%13'/></video>
    <channel type='spicevmc'><target type='virtio' name='com.redhat.spice.0'/></channel>
    <input type='tablet' bus='usb'/>
    <sound model='ich9'/>
  </devices>
</domain>
)")
        .arg(esc).arg(memoryGb).arg(cpus)
        .arg(windows ? QStringLiteral("\n    <firmware><feature enabled='yes' name='secure-boot'/><feature enabled='yes' name='enrolled-keys'/></firmware>") : QString())
        .arg(windows ? QStringLiteral("<smm state='on'/><hyperv mode='passthrough'/>") : QString())
        .arg(windows ? QStringLiteral("localtime") : QStringLiteral("utc"))
        .arg(disk.toHtmlEscaped())
        .arg(windows ? QStringLiteral("sda") : QStringLiteral("vda"))
        .arg(windows ? QStringLiteral("sata") : QStringLiteral("virtio"))
        .arg(iso.toHtmlEscaped())
        .arg(windows ? QStringLiteral("e1000e") : QStringLiteral("virtio"))
        .arg(windows ? QStringLiteral("\n    <tpm model='tpm-crb'><backend type='emulator' version='2.0'/></tpm>") : QString())
        .arg(windows ? QStringLiteral("vga") : QStringLiteral("virtio"))
        .arg(hostdevs);
    const QString xmlFile = vmDir() + QLatin1Char('/') + QString(name).replace(QLatin1Char(' '), QLatin1Char('-')) + QStringLiteral(".xml");
    QFile f(xmlFile);
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        emit toast(tr("Couldn't write the virtual machine's description."), QStringLiteral("error"));
        return;
    }
    f.write(xml.toUtf8());
    f.close();
    auto *img = new QProcess(this);
    connect(img, &QProcess::finished, this, [this, img, name, xmlFile](int code) {
        img->deleteLater();
        if (code != 0) {
            emit toast(tr("The disk for \"%1\" couldn't be made.").arg(name), QStringLiteral("error"));
            return;
        }
        virsh({QStringLiteral("define"), xmlFile}, tr("\"%1\" is ready. Start it to install from the disc.").arg(name),
              tr("\"%1\" couldn't be made.").arg(name));
    });
    img->start(QStringLiteral("qemu-img"), {QStringLiteral("create"), QStringLiteral("-f"), QStringLiteral("qcow2"), disk,
                                            QStringLiteral("%1G").arg(diskGb)});
    emit toast(tr("Making \"%1\"…").arg(name), QStringLiteral("info"));
}

void DevEngine::startVm(const QString &name)
{
    if (m_demo)
        return;
    virsh({QStringLiteral("start"), name}, tr("\"%1\" is running. Open Virtual Machine Manager to see its screen.").arg(name),
          tr("\"%1\" couldn't start.").arg(name));
}

void DevEngine::stopVm(const QString &name)
{
    if (m_demo)
        return;
    virsh({QStringLiteral("shutdown"), name}, tr("\"%1\" is shutting down.").arg(name), tr("\"%1\" couldn't be shut down.").arg(name));
}

void DevEngine::removeVm(const QString &name)
{
    if (m_demo) {
        QVariantList list = m_workspaces.value("vms").toList();
        list.erase(std::remove_if(list.begin(), list.end(), [&](const QVariant &v) { return v.toMap().value("name") == name; }), list.end());
        m_workspaces.insert("vms", list);
        emit changed();
        return;
    }
    // Its disk and firmware variables go with it; the installer disc stays.
    virsh({QStringLiteral("undefine"), name, QStringLiteral("--nvram"), QStringLiteral("--tpm"), QStringLiteral("--remove-all-storage")},
          tr("\"%1\" is removed.").arg(name), tr("\"%1\" couldn't be removed.").arg(name));
}

// ───────────────────────────────────────────────────────────────────────── GPU passthrough

void DevEngine::readPassthrough()
{
    const QDir pci(QStringLiteral("/sys/bus/pci/devices"));
    const auto text = [](const QString &path) { QFile f(path); return f.open(QIODevice::ReadOnly) ? QString::fromLatin1(f.readAll()).trimmed() : QString(); };
    // "lspci -mm": quoted fields; the vendor and the device name are the third and fourth.
    const auto nameOf = [](const QString &address) {
        const QString line = Util::run(QStringLiteral("lspci"), {QStringLiteral("-mm"), QStringLiteral("-s"), address}).trimmed();
        const QStringList q = line.split(QLatin1Char('"'));
        return q.size() >= 8 ? (q.at(3) + QLatin1Char(' ') + q.at(5)).replace(QStringLiteral(" Corporation"), QString()).trimmed() : address;
    };
    QString kept;
    for (const QString &line : text(QStringLiteral("/etc/asimaos/passthrough.conf")).split(QLatin1Char('\n')))
        if (line.startsWith(QLatin1String("GPU=")))
            kept = line.mid(4);
    QVariantList gpus;
    for (const QString &address : pci.entryList(QDir::Dirs | QDir::System | QDir::NoDotAndDotDot)) {
        const QString dev = pci.filePath(address);
        if (!text(dev + QStringLiteral("/class")).startsWith(QLatin1String("0x03")))
            continue;
        const QString slot = address.section(QLatin1Char('.'), 0, 0);
        const QString driver = QFileInfo(QFileInfo(dev + QStringLiteral("/driver")).symLinkTarget()).fileName();
        // Other devices in its IOMMU group (not its own functions, not bridges) would go with it.
        QStringList shared;
        const QDir group(dev + QStringLiteral("/iommu_group/devices"));
        for (const QString &other : group.entryList(QDir::Dirs | QDir::System | QDir::NoDotAndDotDot)) {
            if (other.startsWith(slot) || text(pci.filePath(other) + QStringLiteral("/class")).startsWith(QLatin1String("0x0604")))
                continue;
            shared << nameOf(other);
        }
        gpus.append(item({{"address", address}, {"slot", slot}, {"name", nameOf(address)},
                          {"screen", text(dev + QStringLiteral("/boot_vga")) == QLatin1String("1")}, {"driver", driver},
                          {"kept", slot == kept}, {"ready", driver == QLatin1String("vfio-pci")}, {"shared", shared}}));
    }
    m_passthrough = item({{"iommu", !QDir(QStringLiteral("/sys/kernel/iommu_groups")).entryList(QDir::Dirs | QDir::NoDotAndDotDot).isEmpty()},
                          {"kept", kept}, {"gpus", gpus}});
}

void DevEngine::keepGpu(const QString &address)
{
    if (m_demo) {
        QVariantList gpus = m_passthrough.value("gpus").toList();
        for (QVariant &g : gpus) {
            QVariantMap m = g.toMap();
            m.insert("kept", m.value("address") == address);
            g = m;
        }
        m_passthrough.insert("gpus", gpus);
        m_passthrough.insert("kept", address.section(QLatin1Char('.'), 0, 0));
        emit changed();
        emit toast(tr("The graphics card is kept for virtual machines from the next start."), QStringLiteral("ok"));
        return;
    }
    startUnit(QStringLiteral("asimaos-passthrough-on@%1.service").arg(address),
              tr("The graphics card is kept for virtual machines from the next start. Restart when you're ready."));
    QTimer::singleShot(3000, this, [this] { readPassthrough(); emit changed(); });
}

void DevEngine::releaseGpu()
{
    if (m_demo) {
        QVariantList gpus = m_passthrough.value("gpus").toList();
        for (QVariant &g : gpus) {
            QVariantMap m = g.toMap();
            m.insert("kept", false);
            g = m;
        }
        m_passthrough.insert("gpus", gpus);
        m_passthrough.insert("kept", QString());
        emit changed();
        emit toast(tr("The graphics card goes back to Asima OS at the next start."), QStringLiteral("ok"));
        return;
    }
    startUnit(QStringLiteral("asimaos-passthrough-off.service"), tr("The graphics card goes back to Asima OS at the next start."));
    QTimer::singleShot(3000, this, [this] { readPassthrough(); emit changed(); });
}

void DevEngine::setDemoGpuReady()
{
    QVariantList gpus = m_passthrough.value("gpus").toList();
    for (QVariant &g : gpus) {
        QVariantMap m = g.toMap();
        const bool second = !m.value("screen").toBool();
        m.insert("kept", second);
        m.insert("ready", second);
        if (second)
            m.insert("driver", QStringLiteral("vfio-pci"));
        g = m;
    }
    m_passthrough.insert("gpus", gpus);
    m_passthrough.insert("kept", QStringLiteral("0000:01:00"));
    emit changed();
}

// ───────────────────────────────────────────────────────────────────────── disk use and limits

void DevEngine::readStorage()
{
    if (m_demo)
        return;
    if (!m_maintenance) {
        m_maintenance = new ServiceClient(QStringLiteral("Maintenance1"), this);
        connect(m_maintenance, &ServiceClient::failed, this, [this](const QString &message) { emit toast(message, QStringLiteral("error")); });
    }
    if (!m_maintenance->available()) {
        m_storage = item({{"available", false}});
        emit changed();
        return;
    }
    m_maintenance->call(QStringLiteral("Storage"), {}, [this](const QDBusMessage &reply) {
        const QJsonObject s = ServiceClient::json(reply);
        const QString me = qEnvironmentVariable("USER");
        QVariantList areas;
        for (const QJsonValue &v : s.value(QStringLiteral("areas")).toArray()) {
            const QJsonObject a = v.toObject();
            const QString kind = a.value(QStringLiteral("id")).toString().section(QLatin1Char(':'), 0, 0);
            const QString person = a.value(QStringLiteral("person")).toString();
            // Containers, virtual machines and Nix: the system's, and this person's own.
            if (!QStringList{QStringLiteral("containers"), QStringLiteral("vm"), QStringLiteral("nix")}.contains(kind)
                || (!person.isEmpty() && person != me))
                continue;
            areas << a.toVariantMap();
        }
        // Yours first.
        std::stable_sort(areas.begin(), areas.end(), [](const QVariant &x, const QVariant &y) {
            return !x.toMap().value("person").toString().isEmpty() && y.toMap().value("person").toString().isEmpty();
        });
        m_storage = item({{"available", true}, {"counted", s.value(QStringLiteral("counted")).toBool()}, {"areas", areas}});
        emit changed();
    });
}

void DevEngine::setStorageLimit(const QString &area, double gb)
{
    // GB as Asima UI shows sizes (1000-based).
    const qulonglong bytes = gb <= 0 ? 0 : qulonglong(gb * 1e9);
    if (m_demo) {
        QVariantList areas = m_storage.value("areas").toList();
        for (QVariant &a : areas) {
            QVariantMap m = a.toMap();
            if (m.value("id").toString() == area)
                m.insert("limit", bytes ? QVariant(double(bytes)) : QVariant());
            a = m;
        }
        m_storage.insert("areas", areas);
        emit changed();
        return;
    }
    if (!m_maintenance || !m_maintenance->available())
        return emit toast(tr("Disk limits need Asima Maintenance's service, which isn't available here."), QStringLiteral("info"));
    m_maintenance->call(QStringLiteral("SetStorageLimit"), {area, QVariant::fromValue(bytes)}, [this, bytes](const QDBusMessage &) {
        emit toast(bytes ? tr("The limit is set.") : tr("The limit is removed."), QStringLiteral("ok"));
        readStorage();
    });
}
