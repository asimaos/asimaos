#include "managerengine.h"
#include "devicestate.h"
#include "powerprofiles.h"
#include "serviceclient.h"
#include "sysinfo.h"
#include "util.h"

#include <QClipboard>
#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDBusReply>
#include <QDBusVariant>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QGuiApplication>
#include <QJsonArray>
#include <QProcess>
#include <QStandardPaths>
#include <QTimer>
#include <QUrl>

#include <algorithm>

using Util::item;

ManagerEngine::ManagerEngine(bool demo, QObject *parent)
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
    auto *service = new ServiceClient(QStringLiteral("Manager1"), this);
    if (!service->available()) {
        delete service;
        return;
    }
    m_service = service;
    connect(m_service, &ServiceClient::failed, this, [this](const QString &message) { emit toast(message, QStringLiteral("warn")); });
    m_service->connectSignal(QStringLiteral("Changed"), this, SLOT(onServiceChanged(QString)));
    onServiceChanged(QString());
}

// ───────────────────────────────────────────────────────────────────────── asimaos-managerd

void ManagerEngine::onServiceChanged(const QString &)
{
    m_service->call(QStringLiteral("Status"), {}, [this](const QDBusMessage &r) { applyServiceStatus(ServiceClient::json(r)); });
}

void ManagerEngine::applyServiceStatus(const QJsonObject &s)
{
    // Shield (D55): the zone for other networks, each network in use with its zone, the ones it
    // remembers, the VPN in use and the connection attempts it stopped since the start.
    m_shield = s.value("shield").toObject().toVariantMap();
    m_shield.insert("managed", true);
    const QJsonObject dev = s.value("device").toObject();
    const QJsonObject limit = dev.value("chargeLimit").toObject();
    if (limit.value("supported").toBool()) {
        const int value = limit.value("value").toInt(100);
        m_battery.insert("limitSupported", true);
        m_battery.insert("chargeLimit", value);
        m_battery.insert("limitEnabled", value < 100);
        m_battery.insert("limitFixed", limit.value("fixed").toBool());
    }
    const QJsonObject bio = s.value("biometrics").toObject();
    if (!bio.isEmpty()) {
        m_biometrics.insert("useLogin", bio.value("useLogin").toBool(true));
        m_biometrics.insert("useUnlock", bio.value("useUnlock").toBool(true));
        m_biometrics.insert("useWebcam", bio.value("useWebcam").toBool(false));
    }
    emit changed();
}

// Translates a page's setting into the service's vocabulary.
void ManagerEngine::applySetting(const QString &area, const QString &key, const QVariant &value)
{
    auto set = [this](const QString &a, const QString &k, const QVariant &v) {
        m_service->call(QStringLiteral("Set"), {a, k, QVariant::fromValue(QDBusVariant(v))});
    };
    if (area == QLatin1String("shield")) {
        m_service->call(QStringLiteral("SetShield"), {key, QVariant::fromValue(QDBusVariant(value))});
    } else if (area == QLatin1String("power") && key == QLatin1String("chargeLimit")) {
        set(area, key, value.toInt());
    } else if (area == QLatin1String("power") && key == QLatin1String("limitEnabled")) {
        const int limit = m_battery.value("chargeLimit").toInt();
        set(area, QStringLiteral("chargeLimit"), value.toBool() ? (limit > 0 && limit < 100 ? limit : 80) : 100);
    } else if (area == QLatin1String("biometrics")) {
        set(area, key, value.toBool());
    }
}

void ManagerEngine::needsService(const QString &what)
{
    emit toast(tr("%1 needs the Asima Manager service (asimaos-managerd), which isn't available on this computer.").arg(what), QStringLiteral("info"));
}

// ───────────────────────────────────────────────────────────────────────── demo device

void ManagerEngine::loadDemo()
{
    const DeviceState::Demo d = DeviceState::demo(QStringLiteral("handheld"));
    m_device = d.device;
    m_battery = d.battery;
    m_thermals = d.thermals;
    m_power = d.power;
    m_display = item({{"rgbLights", 2}, {"kbdAvailable", false}, {"kbdLevel", 0}, {"kbdMax", 3}});
    m_biometrics = item({{"useLogin", true}, {"useUnlock", true}, {"useWebcam", false}});
    m_shield = item({{"managed", true}, {"zone", "home"}, {"killSwitch", false}, {"available", true},
                     {"networks", QVariantList{item({{"uuid", "8c1f0a52-demo"}, {"name", "Home Wi-Fi"}, {"kind", "wifi"},
                                                      {"device", "wlan0"}, {"zone", "home"}, {"own", true}})}},
                     {"remembered", QVariantList{item({{"uuid", "51d2e9b0-demo"}, {"name", "Café Lumen"}, {"zone", "public"}}),
                                                 item({{"uuid", "e07a4c13-demo"}, {"name", "Office"}, {"zone", "developer"}})}},
                     {"vpn", QVariantList{}}, {"stopped", 37},
                     {"since", double(QDateTime::currentSecsSinceEpoch() - 2 * 3600)}});
}

// A thin-and-light laptop: infrared camera, fingerprint in the power button, keyboard backlight.
// "ideapad": a laptop whose battery care is on or off at a level it sets itself (Lenovo).
void ManagerEngine::setDemoDevice(const QString &kind)
{
    if (!m_demo || (kind != QLatin1String("laptop") && kind != QLatin1String("ideapad")))
        return;
    const DeviceState::Demo d = DeviceState::demo(QStringLiteral("laptop"));
    m_device = d.device;
    m_battery = d.battery;
    m_thermals = d.thermals;
    m_power = d.power;
    m_display = item({{"rgbLights", 0}, {"kbdAvailable", true}, {"kbdLevel", 1}, {"kbdMax", 2}});
    if (kind == QLatin1String("ideapad")) {
        m_device.insert("model", QStringLiteral("Lenovo IdeaPad Gaming 3"));
        m_battery.insert("limitFixed", true);
    }
    emit changed();
}

// ───────────────────────────────────────────────────────────────────────── this computer

void ManagerEngine::loadSystem()
{
    m_device = DeviceState::device();

    // Keyboard backlight through UPower: works without root.
    QDBusInterface kbd(QStringLiteral("org.freedesktop.UPower"), QStringLiteral("/org/freedesktop/UPower/KbdBacklight"),
                       QStringLiteral("org.freedesktop.UPower.KbdBacklight"), QDBusConnection::systemBus());
    const QDBusReply<int> kbdMax = kbd.call(QStringLiteral("GetMaxBrightness"));
    const QDBusReply<int> kbdNow = kbd.call(QStringLiteral("GetBrightness"));
    // RGB lights are Asima Gaming's; the Display page only points there when there are some.
    int rgbLights = 0;
    const QDir leds(QStringLiteral("/sys/class/leds"));
    for (const QString &led : leds.entryList(QDir::Dirs | QDir::System | QDir::NoDotAndDotDot)) {
        const QStringList channels = SysInfo::readTrimmed(leds.filePath(led + QStringLiteral("/multi_index"))).split(QLatin1Char(' '));
        if (channels.contains(QLatin1String("red")) && channels.contains(QLatin1String("green")) && channels.contains(QLatin1String("blue")))
            ++rgbLights;
    }
    m_display = item({{"rgbLights", rgbLights}, {"kbdAvailable", kbdMax.isValid() && kbdMax.value() > 0},
                      {"kbdLevel", kbdNow.isValid() ? kbdNow.value() : 0}, {"kbdMax", kbdMax.isValid() ? kbdMax.value() : 0}});

    m_biometrics = item({{"useLogin", true}, {"useUnlock", true}, {"useWebcam", false}});

    QString zone = QStringLiteral("home");
    for (const QString &line : SysInfo::readTrimmed(QStringLiteral("/etc/asimaos/shield.conf")).split(QLatin1Char('\n')))
        if (line.startsWith(QLatin1String("ZONE=")))
            zone = line.mid(5);
    m_shield = item({{"managed", false}, {"zone", zone}, {"killSwitch", false}, {"networks", QVariantList{}}});

    readLive();
    m_power = PowerProfiles::read();
    m_liveTimer.start();
}

void ManagerEngine::readLive()
{
    if (m_demo)
        return;
    m_battery = DeviceState::battery();
    m_thermals = DeviceState::thermals();
    // Installed for the person or for everyone, Flatpak exports the desktop file to XDG_DATA_DIRS.
    m_hubInstalled = !QStandardPaths::locate(QStandardPaths::ApplicationsLocation, QStringLiteral("io.github.asimaos.Hub.desktop")).isEmpty();
}

// ───────────────────────────────────────────────────────────────────────── actions

void ManagerEngine::setPowerProfile(const QString &profile)
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

void ManagerEngine::set(const QString &area, const QString &key, const QVariant &value)
{
    QVariantMap *target = area == QLatin1String("shield") ? &m_shield
                        : area == QLatin1String("biometrics") ? &m_biometrics
                        : area == QLatin1String("power") ? &m_battery : nullptr;
    if (!target)
        return;

    if (!m_demo) {
        if (m_service) {
            applySetting(area, key, value);   // the page updates now; Status() corrects it if refused
        } else {
            needsService(tr("Changing this setting"));
            return;
        }
    }
    if (area == QLatin1String("shield") && (key == QLatin1String("networkZone") || key == QLatin1String("forget"))) {
        // The page shows the change at once; the service's Status() follows.
        const QString uuid = value.toString().section(QLatin1Char(' '), 0, 0);
        QVariantList nets = m_shield.value("networks").toList();
        for (QVariant &n : nets) {
            QVariantMap m = n.toMap();
            if (m.value("uuid").toString() == uuid) {
                m.insert("zone", key == QLatin1String("forget") ? m_shield.value("zone") : QVariant(value.toString().section(QLatin1Char(' '), 1)));
                m.insert("own", key != QLatin1String("forget"));
                n = m;
            }
        }
        m_shield.insert("networks", nets);
        QVariantList remembered = m_shield.value("remembered").toList();
        if (key == QLatin1String("forget"))
            remembered.erase(std::remove_if(remembered.begin(), remembered.end(),
                                            [&](const QVariant &r) { return r.toMap().value("uuid").toString() == uuid; }),
                             remembered.end());
        m_shield.insert("remembered", remembered);
        emit changed();
        return;
    }
    target->insert(key, value);
    emit changed();
}

void ManagerEngine::setKeyboardLight(int level)
{
    if (!m_demo) {
        QDBusInterface kbd(QStringLiteral("org.freedesktop.UPower"), QStringLiteral("/org/freedesktop/UPower/KbdBacklight"),
                           QStringLiteral("org.freedesktop.UPower.KbdBacklight"), QDBusConnection::systemBus());
        kbd.call(QStringLiteral("SetBrightness"), level);
    }
    m_display.insert("kbdLevel", level);
    emit changed();
}

void ManagerEngine::openApp(const QString &command)
{
    const QStringList parts = QProcess::splitCommand(command);
    if (m_demo || parts.isEmpty() || !QProcess::startDetached(parts.first(), parts.mid(1)))
        emit toast(tr("Opens %1.").arg(parts.value(0)), QStringLiteral("info"));
}

void ManagerEngine::getHub()
{
    static const QUrl latest(QStringLiteral("https://github.com/asimaos/asimaos-hub/releases/latest/download/AsimaHub-x86_64.flatpak"));
    if (m_demo || !QDesktopServices::openUrl(latest))
        emit toast(tr("Opens the download of Asima Hub in the browser."), QStringLiteral("info"));
    else
        emit toast(tr("The browser downloads Asima Hub. Open the file to install it with Discover."), QStringLiteral("info"));
}

void ManagerEngine::openHub()
{
    if (m_demo || !QProcess::startDetached(QStringLiteral("flatpak"), {QStringLiteral("run"), QStringLiteral("io.github.asimaos.Hub")}))
        emit toast(tr("Opens Asima Hub."), QStringLiteral("info"));
}

void ManagerEngine::openSettings(const QString &module)
{
    if (m_demo || !QProcess::startDetached(QStringLiteral("systemsettings"), {module}))
        emit toast(tr("Opens System Settings (%1).").arg(module), QStringLiteral("info"));
}

void ManagerEngine::copyReport()
{
    QStringList lines{
        QStringLiteral("Device: %1 (%2)").arg(m_device.value("model").toString(), m_device.value("typeLabel").toString()),
        QStringLiteral("System: %1 · %2").arg(m_device.value("os").toString(), m_device.value("kernel").toString()),
        QStringLiteral("Processor: %1 · %2 cores / %3 threads").arg(m_device.value("cpu").toString()).arg(m_device.value("cores").toInt()).arg(m_device.value("threads").toInt()),
        QStringLiteral("Memory: %1 GB").arg(qRound(m_device.value("memory").toDouble() / (1024.0 * 1024 * 1024))),
    };
    for (const QVariant &g : m_device.value("gpus").toList())
        lines << QStringLiteral("Graphics: %1 (%2)").arg(g.toMap().value("name").toString(), g.toMap().value("driver").toString());
    if (m_battery.value("present").toBool())
        lines << QStringLiteral("Battery: %1% · health %2% · %3 cycles").arg(m_battery.value("percent").toInt())
                     .arg(m_battery.value("health").toInt()).arg(m_battery.value("cycles").toInt());
    QGuiApplication::clipboard()->setText(lines.join(QLatin1Char('\n')));
    emit toast(tr("System report copied."), QStringLiteral("ok"));
}
