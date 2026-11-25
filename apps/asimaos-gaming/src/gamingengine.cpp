#include "gamingengine.h"
#include "devicestate.h"
#include "powerprofiles.h"
#include "serviceclient.h"
#include "sysinfo.h"
#include "util.h"

#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDBusVariant>
#include <QDir>
#include <QFile>
#include <QStandardPaths>
#include <QFileInfo>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMap>
#include <QRegularExpression>
#include <QProcess>

using Util::item;

namespace {
// Handheld presets, in watts.
const QHash<QString, int> kPresetWatts{{"silent", 10}, {"balanced", 17}, {"turbo", 25}, {"max", 35}};

// Multicolour LEDs the kernel drives (readable without root): a controller connecting adds one.
int countRgbLights()
{
    int n = 0;
    const QDir leds(QStringLiteral("/sys/class/leds"));
    for (const QString &led : leds.entryList(QDir::Dirs | QDir::System | QDir::NoDotAndDotDot)) {
        const QStringList ch = SysInfo::readTrimmed(leds.filePath(led + QStringLiteral("/multi_index"))).split(QLatin1Char(' '));
        if (ch.contains(QLatin1String("red")) && ch.contains(QLatin1String("green")) && ch.contains(QLatin1String("blue")))
            ++n;
    }
    return n;
}
} // namespace

GamingEngine::GamingEngine(bool demo, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
{
    m_liveTimer.setInterval(3000);
    connect(&m_liveTimer, &QTimer::timeout, this, [this] { readLive(); emit changed(); });
    if (m_demo) {
        loadDemo();
        return;
    }
    loadSystem();
    auto *service = new ServiceClient(QStringLiteral("Gaming1"), this);
    if (!service->available()) {
        delete service;
        return;
    }
    m_service = service;
    connect(m_service, &ServiceClient::failed, this, [this](const QString &message) { emit toast(message, QStringLiteral("warn")); });
    m_service->connectSignal(QStringLiteral("Changed"), this, SLOT(onServiceChanged(QString)));
    onServiceChanged(QString());
}

// ───────────────────────────────────────────────────────────────────────── asimaos-gamingd

void GamingEngine::onServiceChanged(const QString &)
{
    m_service->call(QStringLiteral("Status"), {}, [this](const QDBusMessage &r) { applyServiceStatus(ServiceClient::json(r)); });
}

void GamingEngine::applyServiceStatus(const QJsonObject &s)
{
    const QJsonObject tdp = s.value("device").toObject().value("tdp").toObject();
    if (!tdp.isEmpty()) {
        m_handheld.insert("tdpMin", tdp.value("min").toInt());
        m_handheld.insert("tdpMax", tdp.value("max").toInt());
        m_handheld.insert("tdp", tdp.value("current").toInt());
    }
    const QJsonObject fan = s.value("device").toObject().value("fan").toObject();
    m_handheld.insert("fanModes", fan.value("modes").toArray().toVariantList());
    if (!fan.isEmpty())
        m_handheld.insert("fan", fan.value("current").toString());
    // Game mode starts by itself for this person (the service names whose it is).
    m_handheld.insert("gameMode", s.value("device").toObject().value("gameMode").toString() == qEnvironmentVariable("USER"));
    if (s.value("device").toObject().contains("rumble"))
        m_controllerOptions.insert("rumble", s.value("device").toObject().value("rumble").toInt());
    // The handheld's built-in controller through InputPlumber (D55): which controller Steam and
    // the games see, or nothing where InputPlumber doesn't manage one.
    const QJsonObject controller = s.value("controller").toObject();
    m_handheld.insert("controllerName", controller.value("name").toString());
    m_handheld.insert("controller", controller.value("controller").toString());
    m_handheld.insert("controllerDefault", controller.value("default").toString());
    const QJsonObject lighting = s.value("lighting").toObject();
    if (!lighting.isEmpty())
        m_lighting = lighting.toVariantMap();
    emit changed();
}

void GamingEngine::needsService(const QString &what)
{
    emit toast(tr("%1 needs the Asima Gaming service (asimaos-gamingd), which isn't available on this computer.").arg(what), QStringLiteral("info"));
}

// ───────────────────────────────────────────────────────────────────────── demo device

void GamingEngine::loadDemo()
{
    const DeviceState::Demo d = DeviceState::demo(QStringLiteral("handheld"));
    m_device = d.device;
    m_battery = d.battery;
    m_thermals = d.thermals;
    m_power = d.power;
    m_gaming = item({
        {"gamemode", true}, {"gamemodeClients", 1}, {"ntsync", true}, {"overlay", false},
        {"scheduler", "lavd"}, {"schedulerAvailable", true},
        {"hybrid", false}, {"shaderCache", 3.1e9}, {"steam", true},
    });
    m_controllers = {
        item({{"path", "/dev/input/event12"}, {"name", tr("ROG Ally X built-in controls")}, {"kind", "builtin"}, {"bus", "builtin"}, {"battery", -1}}),
        item({{"path", "/dev/input/event21"}, {"name", "DualSense Wireless Controller"}, {"kind", "playstation"}, {"bus", "bluetooth"},
              {"battery", 64}, {"charging", false}, {"light", "#2F80ED"}}),
        item({{"path", "/dev/input/event23"}, {"name", "Xbox Wireless Controller"}, {"kind", "xbox"}, {"bus", "bluetooth"},
              {"battery", 90}, {"charging", false}}),
    };
    m_controllerOptions = item({{"desktopControls", true}, {"rumble", 80}, {"steamInput", true}});
    m_handheld = item({
        {"supported", true}, {"family", "ally"}, {"preset", "balanced"}, {"tdp", 17}, {"tdpMin", 7}, {"tdpMax", 35},
        {"fan", "auto"}, {"fanModes", QVariantList{"auto", "max"}}, {"refresh", 120}, {"refreshOptions", QVariantList{60, 120}}, {"frameLimit", 0},
        {"fsr", false}, {"desktopControls", true}, {"rotationSensor", true},
        {"controllerName", "ASUS ROG Ally X"}, {"controller", "deck"}, {"controllerDefault", "xbox-elite"},
    });
    m_lighting = item({{"color", "#2F80ED"}, {"brightness", 70},
                       {"lights", QVariantList{item({{"name", tr("Joystick rings")}, {"device", "ROG Ally X"}}),
                                               item({{"name", tr("Light bar")}, {"device", "DualSense Wireless Controller"}})}}});
}

// A laptop with one controller and no handheld controls.
void GamingEngine::setDemoDevice(const QString &kind)
{
    if (!m_demo || kind != QLatin1String("laptop"))
        return;
    const DeviceState::Demo d = DeviceState::demo(kind);
    m_device = d.device;
    m_battery = d.battery;
    m_thermals = d.thermals;
    m_power = d.power;
    m_gaming.insert("gamemodeClients", 0);
    m_gaming.insert("scheduler", "bpfland");
    m_controllers = {item({{"path", "/dev/input/event23"}, {"name", "Xbox Wireless Controller"}, {"kind", "xbox"},
                           {"bus", "bluetooth"}, {"battery", 90}, {"charging", false}})};
    m_handheld = item({{"supported", false}});
    m_lighting = item({{"color", "#2F80ED"}, {"brightness", 0}, {"lights", QVariantList{}}});
    emit changed();
}

// ───────────────────────────────────────────────────────────────────────── this computer

// ───────────────────────────────────────────────────────────────────────── screen and frames

namespace {
// The built-in screen through KScreen (Plasma owns the display layout): its name, the refresh
// rates of its current resolution, the current one, and the mode id of each rate.
struct Panel { QString name; QVariantList rates; int current = 0; QMap<int, QString> modes; };

Panel builtInPanel()
{
    Panel p;
    const QJsonObject all = QJsonDocument::fromJson(Util::run(QStringLiteral("kscreen-doctor"), {QStringLiteral("-j")}).toUtf8()).object();
    for (const QJsonValue &v : all.value("outputs").toArray()) {
        const QJsonObject o = v.toObject();
        const QString name = o.value("name").toString();
        const bool panel = o.value("type").toString().contains(QLatin1String("Panel"))
                           || name.startsWith(QLatin1String("eDP")) || name.startsWith(QLatin1String("DSI")) || name.startsWith(QLatin1String("LVDS"));
        if (!panel || !o.value("enabled").toBool())
            continue;
        const QString currentId = o.value("currentModeId").toString();
        QJsonObject currentSize;
        for (const QJsonValue &m : o.value("modes").toArray())
            if (m.toObject().value("id").toString() == currentId) {
                currentSize = m.toObject().value("size").toObject();
                p.current = qRound(m.toObject().value("refreshRate").toDouble());
            }
        for (const QJsonValue &m : o.value("modes").toArray()) {
            const QJsonObject mode = m.toObject();
            const int rate = qRound(mode.value("refreshRate").toDouble());
            if (mode.value("size").toObject() == currentSize && !p.modes.contains(rate))
                p.modes.insert(rate, mode.value("id").toString());
        }
        for (int rate : p.modes.keys())
            p.rates.append(rate);
        p.name = name;
        break;
    }
    return p;
}

// MangoHud for Flatpak Steam: its configuration inside Steam's sandbox, and the MANGOHUD=1 override
// that loads it. The frame limit needs MangoHud too; without the overlay it stays out of sight.
QString mangoHudConf()
{
    return QDir::homePath() + QStringLiteral("/.var/app/com.valvesoftware.Steam/config/MangoHud/MangoHud.conf");
}

int savedFrameLimit()
{
    static const QRegularExpression re(QStringLiteral("^fps_limit=(\\d+)"), QRegularExpression::MultilineOption);
    return re.match(SysInfo::readTrimmed(mangoHudConf())).captured(1).toInt();
}
} // namespace

bool GamingEngine::applyMangoHud(bool overlay, int frameLimit)
{
    QDir().mkpath(QFileInfo(mangoHudConf()).absolutePath());
    QFile f(mangoHudConf());
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate))
        return false;
    QString text = QStringLiteral("# Asima OS: Asima Gaming's overlay and frame limit for Flatpak Steam.\n");
    if (frameLimit > 0)
        text += QStringLiteral("fps_limit=%1\n").arg(frameLimit);
    if (!overlay)
        text += QStringLiteral("no_display\n");
    f.write(text.toUtf8());
    f.close();
    const bool load = overlay || frameLimit > 0;
    const QStringList args = load ? QStringList{"override", "--user", "--env=MANGOHUD=1", "com.valvesoftware.Steam"}
                                  : QStringList{"override", "--user", "--unset-env=MANGOHUD", "com.valvesoftware.Steam"};
    return QProcess::execute(QStringLiteral("flatpak"), args) == 0;
}

void GamingEngine::loadSystem()
{
    m_device = DeviceState::device();

    // Gaming plumbing that can be read directly.
    const QString scxState = SysInfo::readTrimmed(QStringLiteral("/sys/kernel/sched_ext/state"));
    const QString scxOps = SysInfo::readTrimmed(QStringLiteral("/sys/kernel/sched_ext/root/ops"));
    QDBusInterface gamemode(QStringLiteral("com.feralinteractive.GameMode"), QStringLiteral("/com/feralinteractive/GameMode"),
                            QStringLiteral("com.feralinteractive.GameMode"), QDBusConnection::sessionBus());
    const int clients = gamemode.isValid() ? gamemode.property("ClientCount").toInt() : 0;
    QDBusInterface switcheroo(QStringLiteral("net.hadess.SwitcherooControl"), QStringLiteral("/net/hadess/SwitcherooControl"),
                              QStringLiteral("net.hadess.SwitcherooControl"), QDBusConnection::systemBus());
    const QString overrides = Util::has(QStringLiteral("flatpak"))
        ? Util::run(QStringLiteral("flatpak"), {QStringLiteral("override"), QStringLiteral("--user"), QStringLiteral("--show"), QStringLiteral("com.valvesoftware.Steam")})
        : QString();
    m_gaming = item({{"gamemode", gamemode.isValid()}, {"gamemodeClients", clients},
                     {"ntsync", QFileInfo::exists(QStringLiteral("/dev/ntsync"))},
                     {"overlay", overrides.contains(QLatin1String("MANGOHUD=1")) && !SysInfo::readTrimmed(mangoHudConf()).contains(QLatin1String("no_display"))},
                     {"scheduler", scxState == QLatin1String("enabled") ? scxOps.section(QLatin1Char('_'), 0, 0) : QStringLiteral("default")},
                     {"schedulerAvailable", Util::has(QStringLiteral("scx_lavd")) || Util::has(QStringLiteral("scx_bpfland"))},
                     {"hybrid", switcheroo.isValid() && switcheroo.property("HasDualGpu").toBool()},
                     {"shaderCache", -1.0}, {"steam", QDir(QStringLiteral("/var/lib/flatpak/app/com.valvesoftware.Steam")).exists()}});

    // A controller as mouse and keyboard: the person's own user service (asimaos-controller-mouse).
    QProcess unit;
    unit.start(QStringLiteral("systemctl"), {QStringLiteral("--user"), QStringLiteral("is-enabled"), QStringLiteral("asimaos-controller-mouse.service")});
    const bool desktopControls = unit.waitForFinished(3000) && unit.readAllStandardOutput().trimmed() == "enabled";
    m_controllerOptions = item({{"desktopControls", desktopControls}, {"rumble", 80}, {"steamInput", true}});
    const bool handheld = m_device.value("type").toString() == QLatin1String("handheld");
    const Panel panel = builtInPanel();
    m_handheld = item({{"supported", handheld}, {"family", m_device.value("handheldFamily")}, {"preset", QString()}, {"tdp", 15},
                       {"tdpMin", 5}, {"tdpMax", 30}, {"fan", "auto"}, {"fanModes", QVariantList{}},
                       {"refresh", panel.current}, {"refreshOptions", panel.rates},
                       {"frameLimit", savedFrameLimit()}, {"fsr", overrides.contains(QLatin1String("WINE_FULLSCREEN_FSR=1"))},
                       {"desktopControls", desktopControls}, {"controllerName", QString()},
                       {"rotationSensor", !QDir(QStringLiteral("/sys/bus/iio/devices")).entryList({QStringLiteral("iio:device*")}, QDir::AllEntries | QDir::System).isEmpty()}});

    readLive();
    m_power = PowerProfiles::read();
    m_liveTimer.start();
}

void GamingEngine::readLive()
{
    if (m_demo)
        return;
    m_battery = DeviceState::battery();
    m_thermals = DeviceState::thermals();
    m_controllers = DeviceState::controllers();
    const int lights = countRgbLights();
    if (lights != m_rgbLights) {
        m_rgbLights = lights;
        if (m_service)
            onServiceChanged(QString());
    }
}

// ───────────────────────────────────────────────────────────────────────── actions

void GamingEngine::setPowerProfile(const QString &profile)
{
    if (m_demo) {
        m_power.insert("profile", profile);
        emit changed();
        return;
    }
    const QString error = PowerProfiles::set(m_power, profile);
    if (!error.isEmpty())
        emit toast(error, QStringLiteral("warn"));
    m_power = PowerProfiles::read();
    emit changed();
}

void GamingEngine::set(const QString &area, const QString &key, const QVariant &value)
{
    QVariantMap *target = area == QLatin1String("handheld") ? &m_handheld
                        : area == QLatin1String("gaming") ? &m_gaming
                        : area == QLatin1String("controllers") ? &m_controllerOptions : nullptr;
    if (!target)
        return;

    if (!m_demo) {
        if ((area == QLatin1String("gaming") && key == QLatin1String("overlay")) || key == QLatin1String("frameLimit")) {
            // Performance overlay and frame limit for Flatpak Steam: MangoHud through a per-user
            // Flatpak override and its configuration in Steam's sandbox, no root needed.
            const bool overlay = key == QLatin1String("overlay") ? value.toBool() : m_gaming.value("overlay").toBool();
            const int limit = key == QLatin1String("frameLimit") ? value.toInt() : m_handheld.value("frameLimit").toInt();
            if (!applyMangoHud(overlay, limit)) {
                emit toast(tr("Couldn't change MangoHud's settings for Steam."), QStringLiteral("error"));
                return;
            }
        } else if (key == QLatin1String("fsr")) {
            // AMD FSR 1 for Proton games that run below the screen's resolution in full screen.
            const QStringList args = value.toBool()
                ? QStringList{"override", "--user", "--env=WINE_FULLSCREEN_FSR=1", "com.valvesoftware.Steam"}
                : QStringList{"override", "--user", "--unset-env=WINE_FULLSCREEN_FSR", "com.valvesoftware.Steam"};
            if (QProcess::execute(QStringLiteral("flatpak"), args) != 0) {
                emit toast(tr("Couldn't change the FSR setting for Steam."), QStringLiteral("error"));
                return;
            }
        } else if (key == QLatin1String("refresh")) {
            // Through KScreen, the display's owner: the same resolution at another refresh rate.
            const Panel panel = builtInPanel();
            const QString mode = panel.modes.value(value.toInt());
            if (mode.isEmpty() || QProcess::execute(QStringLiteral("kscreen-doctor"),
                    {QStringLiteral("output.%1.mode.%2").arg(panel.name, mode)}) != 0) {
                emit toast(tr("Couldn't change the refresh rate."), QStringLiteral("error"));
                return;
            }
        } else if (m_service && area == QLatin1String("handheld") && key == QLatin1String("controller")) {
            m_service->call(QStringLiteral("Set"), {area, key, QVariant::fromValue(QDBusVariant(value.toString()))});
        } else if (m_service && key == QLatin1String("gameMode")) {
            m_service->call(QStringLiteral("Set"), {QStringLiteral("gaming"), key, QVariant::fromValue(QDBusVariant(value.toBool()))});
        } else if (m_service && (key == QLatin1String("fan") || (area == QLatin1String("controllers") && key == QLatin1String("rumble")))) {
            const QVariant v = key == QLatin1String("fan") ? QVariant(value.toString()) : QVariant(value.toInt());
            m_service->call(QStringLiteral("Set"), {area, key, QVariant::fromValue(QDBusVariant(v))});
        } else if (m_service && area == QLatin1String("handheld") && (key == QLatin1String("tdp") || key == QLatin1String("preset"))) {
            int w = key == QLatin1String("tdp") ? value.toInt() : kPresetWatts.value(value.toString(), 17);
            w = qBound(m_handheld.value("tdpMin", 3).toInt(), w, m_handheld.value("tdpMax", w).toInt());
            m_service->call(QStringLiteral("Set"), {area, QStringLiteral("tdp"), QVariant::fromValue(QDBusVariant(w))});
        } else if (key == QLatin1String("desktopControls")) {
            // The person's own user service: no administrator needed.
            const QStringList args{QStringLiteral("--user"), value.toBool() ? QStringLiteral("enable") : QStringLiteral("disable"),
                                   QStringLiteral("--now"), QStringLiteral("asimaos-controller-mouse.service")};
            if (QProcess::execute(QStringLiteral("systemctl"), args) != 0) {
                emit toast(tr("Couldn't change the controller setting."), QStringLiteral("error"));
                return;
            }
            m_controllerOptions.insert(key, value);   // one setting, shown on two pages
            m_handheld.insert(key, value);
        } else if (m_service && area == QLatin1String("gaming") && key == QLatin1String("scheduler")) {
            m_service->call(QStringLiteral("Set"), {area, key, QVariant::fromValue(QDBusVariant(value.toString()))});
        } else {
            needsService(tr("Changing this setting"));
            return;
        }
    }

    target->insert(key, value);
    if (area == QLatin1String("handheld") && key == QLatin1String("preset"))
        target->insert("tdp", kPresetWatts.value(value.toString(), target->value("tdp").toInt()));
    if (area == QLatin1String("handheld") && key == QLatin1String("tdp"))
        target->insert("preset", QString());
    emit changed();
}

void GamingEngine::openDisplaySettings()
{
    if (m_demo || !QProcess::startDetached(QStringLiteral("systemsettings"), {QStringLiteral("kcm_kscreen")}))
        emit toast(tr("Opens Plasma's display settings."), QStringLiteral("info"));
}

void GamingEngine::setLighting(const QString &color, int brightness)
{
    brightness = qBound(0, brightness, 100);
    if (!m_demo) {
        if (!m_service)
            return needsService(tr("Changing the lights"));
        m_service->call(QStringLiteral("Set"), {QStringLiteral("lighting"), QStringLiteral("color"),
                                               QVariant::fromValue(QDBusVariant(QStringLiteral("%1:%2").arg(color).arg(brightness)))});
    }
    m_lighting.insert("color", color.toUpper());
    m_lighting.insert("brightness", brightness);
    emit changed();
}

void GamingEngine::openSettings(const QString &module)
{
    if (m_demo || !QProcess::startDetached(QStringLiteral("systemsettings"), {module}))
        emit toast(tr("Opens System Settings (%1).").arg(module), QStringLiteral("info"));
}

void GamingEngine::openApp(const QString &command)
{
    const QStringList parts = QProcess::splitCommand(command);
    if (m_demo || parts.isEmpty() || !QProcess::startDetached(parts.first(), parts.mid(1)))
        emit toast(tr("Opens %1.").arg(parts.value(0)), QStringLiteral("info"));
}
