#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QHash>
#include <QJsonDocument>
#include <QNetworkAccessManager>
#include <QObject>
#include <QPointer>
#include <QProcess>
#include <QSet>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>

#include <functional>

class SnapdClient;
class ServiceClient;

// Data and actions behind Asima App Center (D56): software of every platform that runs on Linux,
// in one place. Each kind of software is a row; purposes are the tabs along the top.
//
//   flatpak    Flathub (its API for the catalogue; `flatpak` installs for everyone)
//   snap       the Snap Store, through snapd's own API (polkit asks for the password)
//   appimage   AppImageHub's list; the file goes to ~/Applications with a menu entry
//   android    F-Droid's apps in Waydroid (Android is set up here, D56; moved from Asima Dev)
//   windows    Windows programs in Bottles (Wine), from Flathub
//   containers Distrobox containers and the programs exported from them; Toolbx and JuNest too
//   nix        your Nix profile (moved from Asima Dev)
//   homebrew   Homebrew on Linux (/home/linuxbrew)
//   python     Python tools with pipx
//   node       Node tools with npm (Node.js in your home folder)
//   rust       Rust tools with cargo (rustup in your home folder)
//   go         Go tools with `go install` (Go in your home folder)
//   conda      Conda environments (Miniforge in your home folder)
//
// Everything runs as you, except what needs the administrator: Flatpak and Snap ask through
// polkit themselves, Homebrew's folder and Android are set up by system units, and Flatpak repair
// for everyone goes through Asima Maintenance's service. Changes run one at a time (the jobs).
// The app goes online for the catalogues and downloads (D56); OS updates stay with Asima Hub.
// Demo mode: sample rows, no network.
class AppCenterEngine : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Backend)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    // [{ id, title, icon }]: the tabs.
    Q_PROPERTY(QVariantList purposes READ purposes CONSTANT)
    Q_PROPERTY(QString purpose READ purpose NOTIFY rowsChanged)
    Q_PROPERTY(QString query READ query NOTIFY rowsChanged)
    // The rows of the tab or the search: [{ kind, title, subtitle, icon, state (loading | ready |
    // setup | empty | error), message, items: [{ kind, id, name, summary, icon, version, installed }] }].
    Q_PROPERTY(QVariantList rows READ rows NOTIFY rowsChanged)
    // What is installed, one row per kind that has something: [{ kind, title, icon, items, tools: [{ id, title }] }].
    Q_PROPERTY(QVariantList installed READ installed NOTIFY installedChanged)
    // { checking, total, kinds: [{ kind, title, count, names }] }
    Q_PROPERTY(QVariantMap updates READ updates NOTIFY updatesChanged)
    // { busy, title, detail, waiting }
    Q_PROPERTY(QVariantMap job READ job NOTIFY jobChanged)
    // The app groups from the signed feed (D56, from Asima Hub): [{ id, title, summary, apps: [{ kind, id, name }] }].
    Q_PROPERTY(QVariantList groups READ groups NOTIFY groupsChanged)
    // An app file opened with Asima App Center: { path, name, kind, title, detail, size, ok, problem }.
    Q_PROPERTY(QVariantMap file READ file NOTIFY fileChanged)
    // Each kind: [{ id, title, icon, ready }].
    Q_PROPERTY(QVariantList kinds READ kinds NOTIFY installedChanged)
    // Flatpak apps' caches in memory (D55): on or off, for this person.
    Q_PROPERTY(bool cachesInMemory READ cachesInMemory NOTIFY installedChanged)

public:
    static inline AppCenterEngine *instance = nullptr;
    static AppCenterEngine *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    // window: false for the background check (no catalogues, no feed).
    explicit AppCenterEngine(bool demo, bool window = true, QObject *parent = nullptr);

    bool demo() const { return m_demo; }
    QVariantList purposes() const;
    QString purpose() const { return m_purpose; }
    QString query() const { return m_query; }
    QVariantList rows() const { return m_rows; }
    QVariantList installed() const { return m_installed; }
    QVariantMap updates() const { return m_updates; }
    QVariantMap job() const;
    QVariantList groups() const { return m_groups; }
    QVariantMap file() const { return m_file; }
    QVariantList kinds() const;
    bool cachesInMemory() const;

    Q_INVOKABLE void setPurpose(const QString &purpose);
    Q_INVOKABLE void search(const QString &text);
    Q_INVOKABLE void install(const QVariantMap &item);
    Q_INVOKABLE void remove(const QVariantMap &item);
    Q_INVOKABLE void open(const QVariantMap &item);
    // Sets a kind up (Android, Homebrew, Node.js, Rust, Go, pipx, Conda, Bottles).
    Q_INVOKABLE void setUp(const QString &kind);
    // tool: cleanup, repair, unused (Flatpak), menu (AppImage), garbage (Nix), stop, remove (Android) …
    Q_INVOKABLE void tool(const QString &kind, const QString &tool);
    Q_INVOKABLE void refreshInstalled();
    Q_INVOKABLE void checkUpdates();
    Q_INVOKABLE void updateKind(const QString &kind);
    Q_INVOKABLE void updateAll();
    Q_INVOKABLE void installGroup(const QString &id);
    // An app file: what it is, then installing it.
    Q_INVOKABLE void openFile(const QString &path);
    Q_INVOKABLE void installFile();
    Q_INVOKABLE void openUrl(const QString &url);
    Q_INVOKABLE void openUpdater();
    // Keeps each Flatpak app's cache in memory (/run/asimaos/appcache), or on the disk again.
    Q_INVOKABLE void setCachesInMemory(bool on);

    // Without a window (the user timer asimaos-appcenter-updates): counts the updates, then
    // tells the person in a notification; `done` runs when that is finished.
    void checkUpdatesQuietly(std::function<void(int)> done);
    void setDemoState(const QString &state);

signals:
    void rowsChanged();
    void installedChanged();
    void updatesChanged();
    void jobChanged();
    void groupsChanged();
    void fileChanged();
    void toast(const QString &message, const QString &tone);

private:
    // One step of a change: a program (as you), a download, a call to snapd or a system unit.
    struct Step {
        QString title;
        QString program;
        QStringList args;
        QString url, path, sha256, sha256Url;   // a download, checked when a checksum is given
        QString snapMethod, snapPath;
        QByteArray snapBody;
        QString unit;                           // started through systemd (polkit), waited for
        std::function<bool()> action;           // a small step in this process (files, menus)
    };
    struct Job {
        QString kind, title, done, failed;
        QList<Step> steps;
    };

    void loadRows();
    void setRow(const QString &kind, const QVariantList &items, const QString &state = QStringLiteral("ready"),
                const QString &message = QString());
    QVariantMap rowFor(const QString &kind) const;
    QStringList kindsFor(const QString &purpose) const;
    void browse(const QString &kind, const QString &purpose);
    void searchKind(const QString &kind, const QString &text);
    bool ready(const QString &kind) const;
    QVariantList installedItems(const QString &kind) const;
    void markInstalled(QVariantList &items) const;
    QList<Step> installSteps(const QVariantMap &item) const;
    QList<Step> removeSteps(const QVariantMap &item) const;
    QList<Step> updateSteps(const QString &kind) const;
    QList<Step> setUpSteps(const QString &kind) const;
    void countUpdates(const QString &kind, std::function<void(int, QStringList)> done);
    void loadGroups();
    void loadDemo();
    void applyCaches(const QVariantList &apps, bool on);

    void enqueue(const Job &job);
    void runNext();
    void runStep();
    void stepDone(bool ok, const QString &detail = QString());

    // HTTPS with a small cache (~/.cache/asimaos-appcenter), so the tabs open fast and offline.
    void getJson(const QString &url, int maxAgeHours, std::function<void(const QJsonDocument &, bool)> done,
                 const QByteArray &postJson = QByteArray());
    QString cacheFile(const QString &url) const;

    bool m_demo;
    QString m_purpose = QStringLiteral("all");
    QString m_query;
    QVariantList m_rows, m_installed, m_groups;
    QVariantMap m_updates, m_file;
    QHash<QString, QSet<QString>> m_installedIds;
    QNetworkAccessManager m_net;
    SnapdClient *m_snapd = nullptr;
    ServiceClient *m_maintenance = nullptr;
    QList<Job> m_jobs;
    int m_step = -1;
    QString m_detail;
    QPointer<QProcess> m_process;
    int m_generation = 0;   // rows of an older tab or search don't land in a newer one
    bool m_demoCaches = false;
};
