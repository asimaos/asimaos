#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QObject>
#include <QUrl>
#include <QVariantMap>

class ServiceClient;

// Data and actions behind Asima Dev: containers (Distrobox on Podman), virtual machines and their
// disk use (D55). Nix programs and Android apps are in Asima App Center (D56). Everything here
// runs as you, without root, so Asima Dev needs no system service of its own: keeping a graphics
// card for virtual machines starts a system unit through systemd, which asks for the
// administrator's password (polkit), and disk limits go through Asima Maintenance's service,
// which owns the disk.
// Demo mode: sample containers, virtual machines and graphics cards.
class DevEngine : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Backend)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    // { podman, distrobox, containers: [{ name, image, state, size }], vms: [{ name, detail, state, size }] }
    Q_PROPERTY(QVariantMap workspaces READ workspaces NOTIFY changed)
    // GPU passthrough: { iommu, kept, gpus: [{ address, slot, name, screen, driver,
    // kept, ready, shared: [names] }] }.
    Q_PROPERTY(QVariantMap passthrough READ passthrough NOTIFY changed)
    // Disk use and limits (D55, through asimaos-maintenanced): { available, counted, areas: [{ id,
    // person, separate, used, limit }] }: containers, virtual machines and Nix programs, the
    // system's and yours.
    Q_PROPERTY(QVariantMap storage READ storage NOTIFY changed)

public:
    // The one instance main() makes; QML gets it as the Backend singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline DevEngine *instance = nullptr;
    static DevEngine *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit DevEngine(bool demo, QObject *parent = nullptr);

    bool demo() const { return m_demo; }
    QVariantMap workspaces() const { return m_workspaces; }
    QVariantMap passthrough() const { return m_passthrough; }
    QVariantMap storage() const { return m_storage; }

    Q_INVOKABLE void createContainer(const QString &image, const QString &name);
    // Virtual machines in your own libvirt session: kind "windows" (UEFI, Secure Boot,
    // TPM 2.0) or "linux"; the installer disc is `iso`. Start, stop and remove them here; their
    // screen opens in Virtual Machine Manager.
    // `gpu`: the slot of a graphics card kept for VMs (passthrough), or "".
    Q_INVOKABLE void createVm(const QString &name, const QString &kind, const QUrl &iso, int memoryGb, int cpus, int diskGb,
                              const QString &gpu = QString());
    // Keep a second graphics card for virtual machines, or give it back (both after a restart).
    Q_INVOKABLE void keepGpu(const QString &address);
    Q_INVOKABLE void releaseGpu();
    Q_INVOKABLE void startVm(const QString &name);
    Q_INVOKABLE void stopVm(const QString &name);
    Q_INVOKABLE void removeVm(const QString &name);
    Q_INVOKABLE int hostCpus() const;
    Q_INVOKABLE int hostMemoryGb() const;
    Q_INVOKABLE void openContainer(const QString &name);
    Q_INVOKABLE void openApp(const QString &command);
    Q_INVOKABLE void openUrl(const QString &url);
    // Disk use: read again; a limit in GB for an area ("nix", "vm:asha" …), 0 for none, after
    // the administrator's password.
    Q_INVOKABLE void readStorage();
    Q_INVOKABLE void setStorageLimit(const QString &area, double gb);
    void setDemoGpuReady();                       // screenshots: the second card kept and on vfio-pci

signals:
    void changed();
    void toast(const QString &message, const QString &tone);

private:
    void readWorkspaces();
    QVariantList readVms() const;
    void readPassthrough();
    void virsh(const QStringList &args, const QString &done, const QString &failed);
    void startUnit(const QString &unit, const QString &started);

    bool m_demo;
    QVariantMap m_workspaces;
    QVariantMap m_passthrough;
    QVariantMap m_storage;
    ServiceClient *m_maintenance = nullptr;   // asimaos-maintenanced, for disk use and limits
};
