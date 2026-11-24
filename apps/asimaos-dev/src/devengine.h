#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QObject>
#include <QUrl>
#include <QVariantMap>

// Data and actions behind Asima Dev: containers (Distrobox on Podman), virtual machines, the
// programs installed with Nix (D28: terminal and developer tools come from Nix or Distrobox) and
// Android apps (Waydroid, D34). Everything here runs as you, without root, so Asima Dev needs no
// system service: setting Android up or removing it starts one of two system units through
// systemd, which asks for the administrator's password (polkit).
// Demo mode: sample containers, a virtual machine and Android.
class DevEngine : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Backend)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    // { podman, distrobox, containers: [{ name, image, state, size }], vms: [{ name, detail, state, size }] }
    Q_PROPERTY(QVariantMap workspaces READ workspaces NOTIFY changed)
    // { available, version, busy, programs: [{ name, detail }] }: your Nix profile.
    Q_PROPERTY(QVariantMap nix READ nix NOTIFY changed)
    // GPU passthrough: { iommu, kept, gpus: [{ address, slot, name, screen, driver,
    // kept, ready, shared: [names] }] }.
    Q_PROPERTY(QVariantMap passthrough READ passthrough NOTIFY changed)
    // Android apps (Waydroid): { available, ready, settingUp, setupFailed, removing, softwareOnly, running }.
    Q_PROPERTY(QVariantMap android READ android NOTIFY changed)

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
    QVariantMap nix() const { return m_nix; }
    QVariantMap android() const { return m_android; }
    QVariantMap passthrough() const { return m_passthrough; }

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
    // Nix: install a program from nixpkgs by name, remove one, update all, free up space.
    Q_INVOKABLE void nixInstall(const QString &name);
    Q_INVOKABLE void nixRemove(const QString &name);
    Q_INVOKABLE void nixUpgrade();
    Q_INVOKABLE void nixClean();
    Q_INVOKABLE void openUrl(const QString &url);
    // Android apps: setting up downloads Android once (a system unit); the rest runs as you.
    Q_INVOKABLE void androidSetup();
    Q_INVOKABLE void androidRemove();
    Q_INVOKABLE void androidOpen();
    Q_INVOKABLE void androidStop();
    Q_INVOKABLE void androidInstall(const QUrl &apk);
    void setDemoAndroid(const QString &state);   // "" (ready) or "setup" (not set up yet)
    void setDemoGpuReady();                       // screenshots: the second card kept and on vfio-pci

signals:
    void changed();
    void toast(const QString &message, const QString &tone);

private:
    void readWorkspaces();
    QVariantList readVms() const;
    void readPassthrough();
    void virsh(const QStringList &args, const QString &done, const QString &failed);
    void readNix();
    void runNix(const QString &program, const QStringList &args, const QString &started, const QString &done, const QString &failed);
    void readAndroid();
    void startUnit(const QString &unit, const QString &started);

    bool m_demo;
    QVariantMap m_workspaces;
    QVariantMap m_nix;
    QVariantMap m_android;
    QVariantMap m_passthrough;
};
