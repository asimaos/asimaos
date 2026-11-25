#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QJsonObject>
#include <QObject>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>

class ServiceClient;

// Data and actions behind Asima Gaming: power mode and CPU scheduler for games, the in-game
// helpers (overlay, GameMode, ntsync), controllers, a handheld's own controls, and RGB lighting
// on devices that have lights the kernel drives (D34).
//
// System mode: reads what the hardware and session expose without root (sysfs, sched-ext,
// GameMode, power-profiles, Flatpak overrides). Settings that need root go to asimaos-gamingd
// (io.github.asimaos.Gaming1), authorised by polkit. Demo mode: a gaming handheld or a laptop.
class GamingEngine : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Backend)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    Q_PROPERTY(QVariantMap device READ device NOTIFY changed)
    Q_PROPERTY(QVariantMap battery READ battery NOTIFY changed)
    Q_PROPERTY(QVariantMap thermals READ thermals NOTIFY changed)
    Q_PROPERTY(QVariantMap power READ power NOTIFY changed)
    Q_PROPERTY(QVariantMap gaming READ gaming NOTIFY changed)
    Q_PROPERTY(QVariantList controllers READ controllers NOTIFY changed)
    // Xbox Wireless Adapters plugged in: [{ product, firmware }] (D55).
    Q_PROPERTY(QVariantList dongles READ dongles NOTIFY changed)
    Q_PROPERTY(QVariantMap controllerOptions READ controllerOptions NOTIFY changed)
    Q_PROPERTY(QVariantMap handheld READ handheld NOTIFY changed)
    // RGB lights: { lights: [{ name, device }], color: "#RRGGBB", brightness: 0–100 }.
    Q_PROPERTY(QVariantMap lighting READ lighting NOTIFY changed)

public:
    // The one instance main() makes; QML gets it as the Backend singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline GamingEngine *instance = nullptr;
    static GamingEngine *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit GamingEngine(bool demo, QObject *parent = nullptr);
    void setDemoDevice(const QString &kind);   // "handheld" (default) or "laptop"

    bool demo() const { return m_demo; }
    QVariantMap device() const { return m_device; }
    QVariantMap battery() const { return m_battery; }
    QVariantMap thermals() const { return m_thermals; }
    QVariantMap power() const { return m_power; }
    QVariantMap gaming() const { return m_gaming; }
    QVariantList controllers() const { return m_controllers; }
    QVariantList dongles() const { return m_dongles; }
    QVariantMap controllerOptions() const { return m_controllerOptions; }
    QVariantMap handheld() const { return m_handheld; }
    QVariantMap lighting() const { return m_lighting; }

    Q_INVOKABLE void setPowerProfile(const QString &profile);
    // area: gaming | handheld | controllers
    Q_INVOKABLE void openDisplaySettings();      // Plasma's display settings (screen rotation)
    Q_INVOKABLE void set(const QString &area, const QString &key, const QVariant &value);
    // Every RGB light to one colour; brightness 0 turns them off. Kept for lights that come later.
    Q_INVOKABLE void setLighting(const QString &color, int brightness);
    Q_INVOKABLE void openSettings(const QString &module);
    Q_INVOKABLE void openApp(const QString &command);

signals:
    void changed();
    void toast(const QString &message, const QString &tone);

private slots:
    void onServiceChanged(const QString &area);

private:
    bool applyMangoHud(bool overlay, int frameLimit);   // Flatpak Steam's MangoHud config and override
    void applyServiceStatus(const QJsonObject &s);
    void loadDemo();
    void loadSystem();
    void readLive();          // battery, temperatures, controllers: refreshed every few seconds
    void needsService(const QString &what);

    ServiceClient *m_service = nullptr;   // asimaos-gamingd; none in demo mode or when it's missing
    bool m_demo;
    QVariantMap m_device, m_battery, m_thermals, m_power, m_gaming, m_controllerOptions, m_handheld, m_lighting;
    int m_rgbLights = -1;   // RGB lights in sysfs at the last look: a change asks the service again
    QVariantList m_controllers;
    QVariantList m_dongles;
    QTimer m_liveTimer;
};
