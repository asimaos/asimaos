#include "devicestate.h"
#include "sysinfo.h"
#include "util.h"

#include <QObject>
#include <QStringList>
#include <QSysInfo>

using Util::item;

namespace {

QString duration(double hours)
{
    if (hours <= 0 || hours > 48)
        return {};
    const int minutes = int(hours * 60.0);
    return minutes >= 60 ? QObject::tr("%1 h %2 min").arg(minutes / 60).arg(minutes % 60) : QObject::tr("%1 min").arg(minutes);
}

} // namespace

namespace DeviceState {

QVariantMap device()
{
    const QVariantMap type = SysInfo::deviceType();
    QString os;
    for (const QString &line : SysInfo::readTrimmed(QStringLiteral("/usr/lib/os-release")).split(QLatin1Char('\n')))
        if (line.startsWith(QLatin1String("PRETTY_NAME=")))
            os = line.mid(12).remove(QLatin1Char('"'));
    return item({{"model", type.value("model")}, {"type", type.value("type")}, {"typeLabel", type.value("label")},
                 {"handheldFamily", type.value("handheldFamily")},
                 {"vendor", SysInfo::dmi().value("vendor")}, {"cpu", SysInfo::cpuModel()}, {"cores", SysInfo::cpuCores()},
                 {"threads", SysInfo::cpuThreads()}, {"memory", SysInfo::memoryBytes()}, {"gpus", SysInfo::gpus()},
                 {"bios", SysInfo::dmi().value("bios")}, {"os", os}, {"kernel", QStringLiteral("Linux ") + QSysInfo::kernelVersion()},
                 {"storage", QString()}, {"screen", QString()}, {"wireless", QString()}});
}

QVariantMap battery()
{
    const QVariantList bats = SysInfo::batteries();
    if (bats.isEmpty())
        return item({{"present", false}});
    const QVariantMap b = bats.first().toMap();
    const QString status = b.value("status").toString();
    const double power = b.value("powerW").toDouble();
    const double now = b.value("energyWh").toDouble(), full = b.value("fullWh").toDouble();
    QString timeText;
    if (power > 0.5 && status == QLatin1String("Discharging"))
        timeText = QObject::tr("%1 left").arg(duration(now / power));
    else if (power > 0.5 && status == QLatin1String("Charging"))
        timeText = QObject::tr("Full in %1").arg(duration((full - now) / power));
    const double limit = b.value("chargeLimit").toDouble();
    return item({{"present", true}, {"percent", qRound(b.value("capacity").toDouble())}, {"status", status},
                 {"timeText", timeText}, {"powerW", power}, {"health", qRound(b.value("health").toDouble())},
                 {"cycles", b.value("cycles")}, {"designWh", b.value("designWh")}, {"fullWh", full},
                 {"limitSupported", limit > 0}, {"limitEnabled", limit > 0 && limit < 100}, {"chargeLimit", limit > 0 ? int(limit) : 100}});
}

QVariantMap thermals()
{
    QVariantMap t;
    for (const QVariant &v : SysInfo::temperatures()) {
        const QVariantMap s = v.toMap();
        const QString kind = s.value("kind").toString();
        const QString label = s.value("label").toString();
        const bool preferred = QStringList{"Tctl", "Package id 0", "edge", "Composite"}.contains(label);
        if (!t.contains(kind) || preferred)
            t.insert(kind, qRound(s.value("celsius").toDouble()));
    }
    t.insert("fans", SysInfo::fans());
    return t;
}

QVariantList controllers()
{
    // A controller's battery is matched through the HID device they share.
    const QVariantList batteries = SysInfo::peripheralBatteries();
    QVariantList pads;
    for (const QVariant &v : SysInfo::gamepads()) {
        QVariantMap pad = v.toMap();
        pad.insert("battery", -1);
        const QString hid = pad.value("hid").toString();
        for (const QVariant &bv : batteries) {
            const QVariantMap b = bv.toMap();
            const QString bh = b.value("hid").toString();
            if (!hid.isEmpty() && !bh.isEmpty() && (bh.startsWith(hid) || hid.startsWith(bh))) {
                pad.insert("battery", b.value("capacity"));
                pad.insert("charging", b.value("status").toString() == QLatin1String("Charging"));
            }
        }
        pads.append(pad);
    }
    return pads;
}

Demo demo(const QString &kind)
{
    if (kind == QLatin1String("laptop")) {
        // A thin-and-light laptop: infrared camera, fingerprint in the power button, keyboard backlight.
        return {
            item({{"model", "Lenovo ThinkPad X1 Carbon Gen 13"}, {"vendor", "LENOVO"}, {"type", "laptop"}, {"typeLabel", QObject::tr("Laptop")},
                  {"cpu", "Intel Core Ultra 7 258V"}, {"cores", 8}, {"threads", 8}, {"memory", 34359738368.0},
                  {"gpus", QVariantList{item({{"name", "Intel Arc 140V"}, {"primary", true}, {"driver", "xe"}})}},
                  {"storage", QObject::tr("1 TB NVMe SSD")}, {"screen", QObject::tr("14″ · 2880 × 1800 · 120 Hz")}, {"bios", "N4AET52W"},
                  {"wireless", QObject::tr("Wi-Fi 7 · Bluetooth 5.4")}, {"os", "Asima OS 2026.11.15.1"}, {"kernel", "Linux 7.3.4-arch1-1"}}),
            item({{"present", true}, {"percent", 64}, {"status", "Discharging"}, {"timeText", QObject::tr("9 h 40 min left")},
                  {"powerW", 5.1}, {"health", 98}, {"cycles", 61}, {"designWh", 57.0}, {"fullWh", 55.9},
                  {"limitSupported", true}, {"limitEnabled", true}, {"chargeLimit", 80}}),
            item({{"cpu", 47}, {"ssd", 38}, {"battery", 31}, {"fans", QVariantList{item({{"label", QObject::tr("Fan")}, {"rpm", 2400}})}}}),
            item({{"available", true}, {"profile", "balanced"},
                  {"wake", QVariantList{item({{"title", QObject::tr("Lid opened")}, {"detail", QObject::tr("23 times this week")}}),
                                        item({{"title", QObject::tr("USB-C dock")}, {"detail", QObject::tr("Twice this week")}})}}}),
        };
    }
    return {
        item({{"model", "ASUS ROG Ally X"}, {"vendor", "ASUSTeK"}, {"type", "handheld"}, {"typeLabel", QObject::tr("Handheld")},
              {"handheldFamily", "ally"},
              {"cpu", "AMD Ryzen Z1 Extreme"}, {"cores", 8}, {"threads", 16}, {"memory", 25769803776.0},
              {"gpus", QVariantList{item({{"name", "AMD Radeon 780M"}, {"primary", true}, {"driver", "amdgpu"}})}},
              {"storage", QObject::tr("1 TB NVMe SSD")}, {"screen", QObject::tr("7″ · 1920 × 1080 · 120 Hz")}, {"bios", "RC72LA.309"},
              {"wireless", QObject::tr("Wi-Fi 6E · Bluetooth 5.3")}, {"os", "Asima OS 2026.11.15.1"}, {"kernel", "Linux 7.3.4-arch1-1"}}),
        item({{"present", true}, {"percent", 78}, {"status", "Discharging"}, {"timeText", QObject::tr("3 h 10 min left")},
              {"powerW", 14.2}, {"health", 96}, {"cycles", 142}, {"designWh", 80.0}, {"fullWh", 76.8},
              {"limitSupported", true}, {"limitEnabled", true}, {"chargeLimit", 80}}),
        item({{"cpu", 61}, {"gpu", 58}, {"ssd", 44}, {"battery", 33},
              {"fans", QVariantList{item({{"label", QObject::tr("Left fan")}, {"rpm", 3150}}), item({{"label", QObject::tr("Right fan")}, {"rpm", 3020}})}}}),
        item({{"available", true}, {"profile", "balanced"},
              {"wake", QVariantList{item({{"title", QObject::tr("Power button")}, {"detail", QObject::tr("12 times this week")}}),
                                    item({{"title", QObject::tr("DualSense Wireless Controller")}, {"detail", QObject::tr("Twice this week")}})}}}),
    };
}

} // namespace DeviceState
