#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QJsonObject>
#include <QObject>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>

class ServiceClient;

// Data and actions behind Asima Manager: the settings of this computer. Power modes and battery
// care, the display and keyboard light, sign-in with face and fingerprint, the hardware, and
// Asima Shield. RGB lighting is in Asima Gaming and Android apps in Asima Dev (D34).
//
// System mode: reads what the hardware and session expose without root (sysfs, power-profiles,
// UPower). Settings that need root go to asimaos-managerd (io.github.asimaos.Manager1), authorised by
// polkit. Demo mode: a complete sample device, a gaming handheld or a laptop.
class ManagerEngine : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Backend)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    Q_PROPERTY(QVariantMap device READ device NOTIFY changed)
    Q_PROPERTY(QVariantMap battery READ battery NOTIFY changed)
    Q_PROPERTY(QVariantMap thermals READ thermals NOTIFY changed)
    Q_PROPERTY(QVariantMap power READ power NOTIFY changed)
    Q_PROPERTY(QVariantMap display READ display NOTIFY changed)
    Q_PROPERTY(QVariantMap biometrics READ biometrics NOTIFY changed)
    Q_PROPERTY(QVariantMap shield READ shield NOTIFY changed)
    // Asima Hub, the online app (D39), is installed (its Flatpak's desktop file is there).
    Q_PROPERTY(bool hubInstalled READ hubInstalled NOTIFY changed)

public:
    // The one instance main() makes; QML gets it as the Backend singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline ManagerEngine *instance = nullptr;
    static ManagerEngine *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit ManagerEngine(bool demo, QObject *parent = nullptr);
    void setDemoDevice(const QString &kind);   // "handheld" (default) or "laptop"

    bool demo() const { return m_demo; }
    QVariantMap device() const { return m_device; }
    QVariantMap battery() const { return m_battery; }
    QVariantMap thermals() const { return m_thermals; }
    QVariantMap power() const { return m_power; }
    QVariantMap display() const { return m_display; }
    QVariantMap biometrics() const { return m_biometrics; }
    QVariantMap shield() const { return m_shield; }
    bool hubInstalled() const { return m_hubInstalled; }

    Q_INVOKABLE void setPowerProfile(const QString &profile);
    // Settings kept by asimaos-managerd (area: shield | power | biometrics).
    Q_INVOKABLE void set(const QString &area, const QString &key, const QVariant &value);
    Q_INVOKABLE void setKeyboardLight(int level);
    // A screen's brightness (Plasma's org.kde.ScreenBrightness; external monitors over DDC/CI).
    Q_INVOKABLE void setMonitorBrightness(const QString &name, int value);
    Q_INVOKABLE void openSettings(const QString &module);
    Q_INVOKABLE void copyReport();
    // Opens another Asima OS app, e.g. "asimaos-gaming --page lighting".
    Q_INVOKABLE void openApp(const QString &command);
    // Asima Hub is never part of the system (D3) and Asima Manager never goes online (D2): this
    // only opens the Hub's always-latest download in the browser; Discover installs the file (D39).
    Q_INVOKABLE void getHub();
    Q_INVOKABLE void openHub();

signals:
    void changed();
    void toast(const QString &message, const QString &tone);

private slots:
    void onServiceChanged(const QString &area);

private:
    void applyServiceStatus(const QJsonObject &s);
    void applySetting(const QString &area, const QString &key, const QVariant &value);
    void loadDemo();
    void loadSystem();
    void readLive();          // battery, temperatures and screen brightness: refreshed every few seconds
    QVariantList readMonitors() const;
    void needsService(const QString &what);

    ServiceClient *m_service = nullptr;   // asimaos-managerd; none in demo mode or when it's missing
    bool m_demo;
    QVariantMap m_device, m_battery, m_thermals, m_power, m_display, m_biometrics, m_shield;
    bool m_hubInstalled = false;
    QTimer m_liveTimer;
};
