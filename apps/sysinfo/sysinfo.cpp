#include "sysinfo.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QObject>
#include <QSet>

#include <algorithm>

namespace SysInfo {

namespace {

QVariantMap item(std::initializer_list<std::pair<QString, QVariant>> values)
{
    QVariantMap m;
    for (const auto &[k, v] : values)
        m.insert(k, v);
    return m;
}

QStringList entries(const QString &dir, const QStringList &filters = {})
{
    return QDir(dir).entryList(filters, QDir::Dirs | QDir::Files | QDir::System | QDir::NoDotAndDotDot, QDir::Name);
}

double number(const QString &path)
{
    bool ok = false;
    const double v = readTrimmed(path).toDouble(&ok);
    return ok ? v : -1;
}

} // namespace

QString readTrimmed(const QString &path)
{
    QFile f(path);
    return f.open(QIODevice::ReadOnly) ? QString::fromUtf8(f.readAll()).trimmed() : QString();
}

QString gpuName(const QString &vendor, const QString &device)
{
    const QString v = vendor.mid(2).toLower();
    const QString d = device.mid(2).toLower();
    const QString shortVendor = v == QLatin1String("10de") ? QStringLiteral("NVIDIA")
                              : v == QLatin1String("1002") ? QStringLiteral("AMD")
                              : v == QLatin1String("8086") ? QStringLiteral("Intel") : QString();
    QFile f(QStringLiteral("/usr/share/hwdata/pci.ids"));
    if (f.open(QIODevice::ReadOnly)) {
        bool inVendor = false;
        while (!f.atEnd()) {
            const QByteArray line = f.readLine();
            if (line.startsWith('#') || line.trimmed().isEmpty())
                continue;
            if (line[0] != '\t') {
                if (inVendor)
                    break;
                inVendor = line.startsWith(v.toLatin1());
                continue;
            }
            if (inVendor && line.size() > 6 && line[1] != '\t' && line.mid(1, 4) == d.toLatin1()) {
                QString name = QString::fromUtf8(line.mid(5)).trimmed();
                const int a = name.indexOf(QLatin1Char('['));
                const int b = name.lastIndexOf(QLatin1Char(']'));
                if (a >= 0 && b > a)
                    name = name.mid(a + 1, b - a - 1);
                return shortVendor.isEmpty() ? name : shortVendor + QLatin1Char(' ') + name;
            }
        }
    }
    return shortVendor.isEmpty() ? QString() : shortVendor + QStringLiteral(" graphics");
}

QVariantList gpus()
{
    QVariantList out;
    QSet<QString> seen;
    for (const QString &card : entries(QStringLiteral("/sys/class/drm"), {QStringLiteral("card?"), QStringLiteral("card??")})) {
        const QString base = QStringLiteral("/sys/class/drm/") + card + QStringLiteral("/device/");
        const QString vendor = readTrimmed(base + QStringLiteral("vendor"));
        if (vendor.isEmpty())
            continue;
        const QString pci = QFileInfo(base).canonicalFilePath();
        if (seen.contains(pci))
            continue;
        seen.insert(pci);
        out.append(item({{"name", gpuName(vendor, readTrimmed(base + QStringLiteral("device")))},
                         {"vendor", vendor},
                         {"driver", QFileInfo(base + QStringLiteral("driver")).symLinkTarget().section(QLatin1Char('/'), -1)},
                         {"primary", readTrimmed(base + QStringLiteral("boot_vga")) == QLatin1String("1")}}));
    }
    return out;
}

QString cpuModel()
{
    QFile f(QStringLiteral("/proc/cpuinfo"));
    if (!f.open(QIODevice::ReadOnly))
        return {};
    while (!f.atEnd()) {
        const QString line = QString::fromUtf8(f.readLine());
        if (line.startsWith(QLatin1String("model name")))
            return line.section(QLatin1Char(':'), 1).simplified().remove(QStringLiteral("(R)")).remove(QStringLiteral("(TM)"));
    }
    return {};
}

int cpuThreads()
{
    QFile f(QStringLiteral("/proc/cpuinfo"));
    int n = 0;
    if (f.open(QIODevice::ReadOnly))
        while (!f.atEnd())
            n += f.readLine().startsWith("processor") ? 1 : 0;
    return n;
}

int cpuCores()
{
    QSet<QString> cores;
    for (const QString &cpu : entries(QStringLiteral("/sys/devices/system/cpu"), {QStringLiteral("cpu[0-9]*")})) {
        const QString base = QStringLiteral("/sys/devices/system/cpu/") + cpu + QStringLiteral("/topology/");
        cores.insert(readTrimmed(base + QStringLiteral("physical_package_id")) + QLatin1Char(':') + readTrimmed(base + QStringLiteral("core_id")));
    }
    return int(cores.size());
}

double memoryBytes()
{
    for (const QString &line : readTrimmed(QStringLiteral("/proc/meminfo")).split(QLatin1Char('\n')))
        if (line.startsWith(QLatin1String("MemTotal:")))
            return line.section(QLatin1Char(' '), -2, -2).toDouble() * 1024.0;
    return 0;
}

QVariantMap dmi()
{
    const QString d = QStringLiteral("/sys/class/dmi/id/");
    return item({{"vendor", readTrimmed(d + QStringLiteral("sys_vendor"))},
                 {"product", readTrimmed(d + QStringLiteral("product_name"))},
                 {"family", readTrimmed(d + QStringLiteral("product_family"))},
                 {"board", readTrimmed(d + QStringLiteral("board_name"))},
                 {"chassis", readTrimmed(d + QStringLiteral("chassis_type")).toInt()},
                 {"bios", readTrimmed(d + QStringLiteral("bios_version"))}});
}

QVariantMap deviceType()
{
    const QVariantMap d = dmi();
    const QString vendor = d.value("vendor").toString();
    const QString product = d.value("product").toString();
    const QString family = d.value("family").toString();
    const QString all = vendor + QLatin1Char(' ') + product + QLatin1Char(' ') + family + QLatin1Char(' ') + d.value("board").toString();

    // Known gaming handhelds (DMI names).
    struct Handheld { const char *match, *name, *family; };
    static const Handheld known[] = {
        {"Jupiter", "Steam Deck", "deck"}, {"Galileo", "Steam Deck OLED", "deck"},
        {"RC72LA", "ASUS ROG Ally X", "ally"}, {"RC71L", "ASUS ROG Ally", "ally"},
        {"Legion Go S", "Lenovo Legion Go S", "legion"}, {"Legion Go", "Lenovo Legion Go", "legion"}, {"83E1", "Lenovo Legion Go", "legion"},
        {"Claw", "MSI Claw", "claw"}, {"AYANEO", "AYANEO handheld", "ayaneo"}, {"AYADEVICE", "AYANEO handheld", "ayaneo"},
        {"ONEXPLAYER", "OneXPlayer", "onexplayer"}, {"ONEXFLY", "OneXFly", "onexplayer"}, {"G1618", "GPD WIN 4", "gpd"},
        {"G1617", "GPD WIN Mini", "gpd"},
    };
    for (const Handheld &h : known)
        if (all.contains(QLatin1String(h.match), Qt::CaseInsensitive))
            return item({{"type", "handheld"}, {"label", QObject::tr("Handheld")}, {"model", QString::fromUtf8(h.name)},
                         {"handheldFamily", h.family}});

    const int chassis = d.value("chassis").toInt();
    QString type = QStringLiteral("desktop");
    if (QList<int>{8, 9, 10, 14}.contains(chassis)) type = QStringLiteral("laptop");
    else if (QList<int>{30, 31, 32}.contains(chassis)) type = QStringLiteral("tablet");
    const QString label = type == QLatin1String("laptop") ? QObject::tr("Laptop")
                        : type == QLatin1String("tablet") ? QObject::tr("Tablet / convertible") : QObject::tr("Desktop");
    QString model = (vendor + QLatin1Char(' ') + (family.isEmpty() || family.contains(QLatin1String("To be filled")) ? product : family)).simplified();
    if (model.isEmpty() || model.contains(QLatin1String("To be filled")))
        model = QObject::tr("This computer");
    return item({{"type", type}, {"label", label}, {"model", model}, {"handheldFamily", QString()}});
}

namespace {
QVariantList supplies(bool device)
{
    QVariantList out;
    const QString root = QStringLiteral("/sys/class/power_supply/");
    for (const QString &s : entries(root)) {
        const QString b = root + s + QLatin1Char('/');
        if (readTrimmed(b + QStringLiteral("type")) != QLatin1String("Battery"))
            continue;
        const bool isDevice = readTrimmed(b + QStringLiteral("scope")) == QLatin1String("Device");
        if (isDevice != device)
            continue;
        double full = number(b + QStringLiteral("energy_full"));
        double design = number(b + QStringLiteral("energy_full_design"));
        double now = number(b + QStringLiteral("energy_now"));
        if (full < 0) {   // some batteries report charge (µAh) instead of energy (µWh)
            full = number(b + QStringLiteral("charge_full"));
            design = number(b + QStringLiteral("charge_full_design"));
            now = number(b + QStringLiteral("charge_now"));
        }
        out.append(item({
            {"name", s}, {"model", readTrimmed(b + QStringLiteral("model_name"))},
            {"vendor", readTrimmed(b + QStringLiteral("manufacturer"))},
            {"capacity", number(b + QStringLiteral("capacity"))},
            {"status", readTrimmed(b + QStringLiteral("status"))},
            {"health", full > 0 && design > 0 ? 100.0 * full / design : -1},
            {"cycles", number(b + QStringLiteral("cycle_count"))},
            {"powerW", number(b + QStringLiteral("power_now")) / 1e6},
            {"energyWh", now / 1e6}, {"fullWh", full / 1e6}, {"designWh", design / 1e6},
            {"chargeLimit", number(b + QStringLiteral("charge_control_end_threshold"))},
            {"hid", QFileInfo(b + QStringLiteral("device")).canonicalFilePath()},
        }));
    }
    return out;
}
} // namespace

QVariantList batteries() { return supplies(false); }
QVariantList peripheralBatteries() { return supplies(true); }

QVariantList temperatures()
{
    QVariantList out;
    const QString root = QStringLiteral("/sys/class/hwmon/");
    for (const QString &h : entries(root)) {
        const QString b = root + h + QLatin1Char('/');
        const QString name = readTrimmed(b + QStringLiteral("name"));
        QString kind = QStringLiteral("board");
        if (QStringList{"k10temp", "coretemp", "zenpower", "cpu_thermal"}.contains(name)) kind = QStringLiteral("cpu");
        else if (QStringList{"amdgpu", "nouveau", "i915", "xe", "radeon"}.contains(name)) kind = QStringLiteral("gpu");
        else if (name == QLatin1String("nvme")) kind = QStringLiteral("ssd");
        else if (name.startsWith(QLatin1String("BAT"))) kind = QStringLiteral("battery");
        for (const QString &t : entries(b, {QStringLiteral("temp*_input")})) {
            const double milli = number(b + t);
            if (milli <= 0)
                continue;
            const QString label = readTrimmed(b + QString(t).replace(QStringLiteral("_input"), QStringLiteral("_label")));
            out.append(item({{"kind", kind}, {"chip", name}, {"label", label.isEmpty() ? name : label}, {"celsius", milli / 1000.0}}));
        }
    }
    return out;
}

QVariantList fans()
{
    QVariantList out;
    const QString root = QStringLiteral("/sys/class/hwmon/");
    for (const QString &h : entries(root)) {
        const QString b = root + h + QLatin1Char('/');
        for (const QString &f : entries(b, {QStringLiteral("fan*_input")})) {
            const QString label = readTrimmed(b + QString(f).replace(QStringLiteral("_input"), QStringLiteral("_label")));
            out.append(item({{"label", label.isEmpty() ? readTrimmed(b + QStringLiteral("name")) : label}, {"rpm", number(b + f)}}));
        }
    }
    return out;
}

QVariantList gamepads()
{
    QVariantList out;
    const QString root = QStringLiteral("/sys/class/input/");
    for (const QString &ev : entries(root, {QStringLiteral("event*")})) {
        const QString dev = root + ev + QStringLiteral("/device/");
        // Key capability bitmap: hex words, most significant first. BTN_GAMEPAD = 0x130.
        const QStringList words = readTrimmed(dev + QStringLiteral("capabilities/key")).split(QLatin1Char(' '), Qt::SkipEmptyParts);
        const int bit = 0x130, wordBits = 64, index = bit / wordBits;
        if (words.size() <= index)
            continue;
        const quint64 word = words.at(words.size() - 1 - index).toULongLong(nullptr, 16);
        if (!(word & (quint64(1) << (bit % wordBits))))
            continue;
        const QString vendor = readTrimmed(dev + QStringLiteral("id/vendor"));
        const QString bus = readTrimmed(dev + QStringLiteral("id/bustype"));
        const QString kind = vendor == QLatin1String("045e") ? QStringLiteral("xbox")
                           : vendor == QLatin1String("054c") ? QStringLiteral("playstation")
                           : vendor == QLatin1String("057e") ? QStringLiteral("nintendo")
                           : vendor == QLatin1String("2dc8") ? QStringLiteral("8bitdo")
                           : QStringList{"28de", "0b05", "17ef", "1ee9", "2f24"}.contains(vendor) ? QStringLiteral("builtin")
                           : QStringLiteral("generic");
        out.append(item({{"name", readTrimmed(dev + QStringLiteral("name"))}, {"path", QStringLiteral("/dev/input/") + ev},
                         {"vendor", vendor}, {"product", readTrimmed(dev + QStringLiteral("id/product"))},
                         {"bus", bus == QLatin1String("0005") ? QStringLiteral("bluetooth") : bus == QLatin1String("0003") ? QStringLiteral("usb") : QStringLiteral("builtin")},
                         {"hid", QFileInfo(dev + QStringLiteral("device")).canonicalFilePath()}, {"kind", kind}}));
    }
    return out;
}

} // namespace SysInfo
