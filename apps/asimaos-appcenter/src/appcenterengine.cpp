#include "appcenterengine.h"
#include "serviceclient.h"
#include "snapd.h"
#include "util.h"

#include <QCryptographicHash>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusObjectPath>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QNetworkReply>
#include <QRegularExpression>
#include <QSettings>
#include <QStandardPaths>
#include <QUrl>
#include <QUrlQuery>

#include <openssl/evp.h>

#include <algorithm>
#include <memory>

#include <unistd.h>

using Util::item;

namespace {

const QString kFlathubApi = QStringLiteral("https://flathub.org/api/v2");
const QString kAppImageFeed = QStringLiteral("https://appimage.github.io/feed.json");
const QString kAppImageMedia = QStringLiteral("https://appimage.github.io/database/");
const QString kFeed = QStringLiteral("https://asimaos.pages.dev/feed/feed.json");
const QString kFeedKey = QStringLiteral("/usr/share/asimaos/keys/feed.pub");
const QString kBottles = QStringLiteral("com.usebottles.bottles");
const QString kBottle = QStringLiteral("Windows apps");
const QString kWaydroidConfig = QStringLiteral("/var/lib/waydroid/waydroid.cfg");
const QString kBrew = QStringLiteral("/home/linuxbrew/.linuxbrew/bin/brew");

QString home() { return QDir::homePath(); }
QString share() { return QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation); }
QString tools() { return share() + QStringLiteral("/asimaos"); }
QString appsDir() { return home() + QStringLiteral("/Applications"); }
QString menuDir() { return share() + QStringLiteral("/applications"); }

// Each kind's program, where Asima App Center puts it or else on the PATH.
QString program(const QString &kind)
{
    const QHash<QString, QString> local{
        {QStringLiteral("homebrew"), kBrew},
        {QStringLiteral("python"), home() + QStringLiteral("/.local/bin/pipx")},
        {QStringLiteral("node"), tools() + QStringLiteral("/node/bin/npm")},
        {QStringLiteral("rust"), home() + QStringLiteral("/.cargo/bin/cargo")},
        {QStringLiteral("go"), tools() + QStringLiteral("/go/bin/go")},
        {QStringLiteral("conda"), home() + QStringLiteral("/miniforge3/bin/conda")},
    };
    const QHash<QString, QString> names{
        {QStringLiteral("homebrew"), QStringLiteral("brew")}, {QStringLiteral("python"), QStringLiteral("pipx")},
        {QStringLiteral("node"), QStringLiteral("npm")},      {QStringLiteral("rust"), QStringLiteral("cargo")},
        {QStringLiteral("go"), QStringLiteral("go")},         {QStringLiteral("conda"), QStringLiteral("conda")},
        {QStringLiteral("nix"), QStringLiteral("nix")},       {QStringLiteral("containers"), QStringLiteral("distrobox")},
    };
    if (QFileInfo(local.value(kind)).isExecutable())
        return local.value(kind);
    const QString found = QStandardPaths::findExecutable(names.value(kind));
    return found;
}

// The environment for the tools installed in the home folder (PATH; Node's global folder).
QProcessEnvironment toolEnv()
{
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    const QStringList dirs{home() + QStringLiteral("/.local/bin"), home() + QStringLiteral("/.cargo/bin"),
                           tools() + QStringLiteral("/go/bin"), home() + QStringLiteral("/go/bin"),
                           tools() + QStringLiteral("/node/bin"), QStringLiteral("/home/linuxbrew/.linuxbrew/bin"),
                           home() + QStringLiteral("/miniforge3/bin")};
    env.insert(QStringLiteral("PATH"), dirs.join(QLatin1Char(':')) + QLatin1Char(':') + env.value(QStringLiteral("PATH")));
    env.insert(QStringLiteral("HOMEBREW_NO_ANALYTICS"), QStringLiteral("1"));
    return env;
}

// Runs a program as you and hands its output over when it ends (`ok`: exit code 0).
void runAsync(QObject *ctx, const QString &prog, const QStringList &args, std::function<void(const QString &, bool)> done)
{
    if (prog.isEmpty())
        return done(QString(), false);
    auto *p = new QProcess(ctx);
    p->setProcessEnvironment(toolEnv());
    QObject::connect(p, &QProcess::finished, ctx, [p, done](int code, QProcess::ExitStatus status) {
        done(QString::fromUtf8(p->readAllStandardOutput()), status == QProcess::NormalExit && code == 0);
        p->deleteLater();
    });
    QObject::connect(p, &QProcess::errorOccurred, ctx, [p, done](QProcess::ProcessError e) {
        if (e == QProcess::FailedToStart) {
            done(QString(), false);
            p->deleteLater();
        }
    });
    p->start(prog, args);
}

QVariantMap appItem(const QString &kind, const QString &id, const QString &name, const QString &summary,
                    const QString &icon = QString(), const QString &version = QString())
{
    return item({{"kind", kind}, {"id", id}, {"name", name.isEmpty() ? id : name}, {"summary", summary},
                 {"icon", icon}, {"version", version}, {"installed", false}});
}

// A name for files and menu entries: small letters, digits and dashes.
QString slug(const QString &name)
{
    QString s = name.toLower();
    s.replace(QRegularExpression(QStringLiteral("[^a-z0-9]+")), QStringLiteral("-"));
    return s.mid(0, 60).remove(QRegularExpression(QStringLiteral("^-+|-+$")));
}

bool validName(const QString &n)
{
    static const QRegularExpression re(QStringLiteral("^[A-Za-z0-9@][A-Za-z0-9._+/@-]{0,199}$"));
    return re.match(n).hasMatch() && !n.contains(QLatin1String(".."));
}

// The feed key's public half (32 bytes, hex, in /usr/share/asimaos/keys/feed.pub).
QByteArray feedKey()
{
    QFile f(kFeedKey);
    if (!f.open(QIODevice::ReadOnly))
        return {};
    for (const QByteArray &line : f.readAll().split('\n'))
        if (!line.trimmed().isEmpty() && !line.startsWith('#'))
            return QByteArray::fromHex(line.trimmed());
    return {};
}

bool verifyEd25519(const QByteArray &data, const QByteArray &sig, const QByteArray &key)
{
    if (sig.size() != 64 || key.size() != 32)
        return false;
    EVP_PKEY *k = EVP_PKEY_new_raw_public_key(EVP_PKEY_ED25519, nullptr, reinterpret_cast<const unsigned char *>(key.constData()), 32);
    if (!k)
        return false;
    EVP_MD_CTX *ctx = EVP_MD_CTX_new();
    const bool ok = ctx && EVP_DigestVerifyInit(ctx, nullptr, nullptr, nullptr, k) == 1
                    && EVP_DigestVerify(ctx, reinterpret_cast<const unsigned char *>(sig.constData()), 64,
                                        reinterpret_cast<const unsigned char *>(data.constData()), size_t(data.size())) == 1;
    EVP_MD_CTX_free(ctx);
    EVP_PKEY_free(k);
    return ok;
}

// An AppImage's menu entry that Asima App Center wrote: its keys.
QHash<QString, QString> desktopKeys(const QString &file)
{
    QHash<QString, QString> keys;
    QFile f(file);
    if (f.open(QIODevice::ReadOnly))
        for (const QString &line : QString::fromUtf8(f.readAll()).split(QLatin1Char('\n')))
            if (const qsizetype eq = line.indexOf(QLatin1Char('=')); eq > 0 && !keys.contains(line.left(eq)))
                keys.insert(line.left(eq), line.mid(eq + 1));
    return keys;
}

} // namespace

// ───────────────────────────────────────────────────────────────────────── kinds and tabs

QVariantList AppCenterEngine::purposes() const
{
    return {
        item({{"id", "all"}, {"title", tr("Discover")}, {"icon", "starred"}}),
        item({{"id", "games"}, {"title", tr("Games")}, {"icon", "applications-games-symbolic"}}),
        item({{"id", "graphics"}, {"title", tr("Graphics & photos")}, {"icon", "applications-graphics-symbolic"}}),
        item({{"id", "media"}, {"title", tr("Audio & video")}, {"icon", "applications-multimedia-symbolic"}}),
        item({{"id", "office"}, {"title", tr("Office")}, {"icon", "applications-office-symbolic"}}),
        item({{"id", "internet"}, {"title", tr("Internet & chat")}, {"icon", "applications-internet-symbolic"}}),
        item({{"id", "development"}, {"title", tr("Development")}, {"icon", "applications-development-symbolic"}}),
        item({{"id", "education"}, {"title", tr("Education & science")}, {"icon", "applications-education-symbolic"}}),
        item({{"id", "system"}, {"title", tr("System & tools")}, {"icon", "applications-utilities-symbolic"}}),
    };
}

static const QStringList kAllKinds{QStringLiteral("flatpak"), QStringLiteral("snap"), QStringLiteral("appimage"),
                                   QStringLiteral("android"), QStringLiteral("windows"), QStringLiteral("containers"),
                                   QStringLiteral("nix"), QStringLiteral("homebrew"), QStringLiteral("python"),
                                   QStringLiteral("node"), QStringLiteral("rust"), QStringLiteral("go"), QStringLiteral("conda")};

QVariantMap AppCenterEngine::rowFor(const QString &kind) const
{
    static const QHash<QString, QPair<const char *, const char *>> info{
        {QStringLiteral("flatpak"), {QT_TR_NOOP("Flatpak"), QT_TR_NOOP("Sandboxed apps from Flathub, for everyone on this computer")}},
        {QStringLiteral("snap"), {QT_TR_NOOP("Snap"), QT_TR_NOOP("Apps from the Snap Store")}},
        {QStringLiteral("appimage"), {QT_TR_NOOP("AppImage"), QT_TR_NOOP("One file per app, kept in your Applications folder")}},
        {QStringLiteral("android"), {QT_TR_NOOP("Android"), QT_TR_NOOP("Free Android apps from F-Droid, in Waydroid")}},
        {QStringLiteral("windows"), {QT_TR_NOOP("Windows"), QT_TR_NOOP("Windows programs and games in Bottles (Wine)")}},
        {QStringLiteral("containers"), {QT_TR_NOOP("Containers"), QT_TR_NOOP("Other Linux systems' programs, in Distrobox, Toolbx or JuNest")}},
        {QStringLiteral("nix"), {QT_TR_NOOP("Nix"), QT_TR_NOOP("Programs from Nixpkgs, in your own Nix profile")}},
        {QStringLiteral("homebrew"), {QT_TR_NOOP("Homebrew"), QT_TR_NOOP("Command-line programs from Homebrew")}},
        {QStringLiteral("python"), {QT_TR_NOOP("Python"), QT_TR_NOOP("Python tools from PyPI, each in its own environment (pipx)")}},
        {QStringLiteral("node"), {QT_TR_NOOP("Node.js"), QT_TR_NOOP("Command-line tools from npm")}},
        {QStringLiteral("rust"), {QT_TR_NOOP("Rust"), QT_TR_NOOP("Programs from crates.io, built with cargo")}},
        {QStringLiteral("go"), {QT_TR_NOOP("Go"), QT_TR_NOOP("Go programs, installed with go install")}},
        {QStringLiteral("conda"), {QT_TR_NOOP("Conda"), QT_TR_NOOP("Environments from conda-forge, for data science")}},
    };
    static const QHash<QString, QString> icons{
        {QStringLiteral("flatpak"), QStringLiteral("application-vnd.flatpak.ref")}, {QStringLiteral("snap"), QStringLiteral("application-vnd.snap")},
        {QStringLiteral("appimage"), QStringLiteral("application-x-iso9660-appimage")}, {QStringLiteral("android"), QStringLiteral("smartphone")},
        {QStringLiteral("windows"), QStringLiteral("wine-symbolic")}, {QStringLiteral("containers"), QStringLiteral("virtual-desktops")},
        {QStringLiteral("nix"), QStringLiteral("package")}, {QStringLiteral("homebrew"), QStringLiteral("package-x-generic")},
        {QStringLiteral("python"), QStringLiteral("text-x-python")}, {QStringLiteral("node"), QStringLiteral("application-javascript")},
        {QStringLiteral("rust"), QStringLiteral("text-x-rust")}, {QStringLiteral("go"), QStringLiteral("text-x-go")},
        {QStringLiteral("conda"), QStringLiteral("office-chart-line")},
    };
    const auto i = info.value(kind);
    return item({{"kind", kind}, {"title", tr(i.first)}, {"subtitle", tr(i.second)}, {"icon", icons.value(kind)},
                 {"state", "loading"}, {"message", QString()}, {"items", QVariantList{}}});
}

QVariantList AppCenterEngine::kinds() const
{
    QVariantList out;
    for (const QString &k : kAllKinds) {
        const QVariantMap r = rowFor(k);
        out << item({{"id", k}, {"title", r.value("title")}, {"icon", r.value("icon")}, {"ready", ready(k)}});
    }
    return out;
}

QStringList AppCenterEngine::kindsFor(const QString &purpose) const
{
    const QStringList stores{QStringLiteral("flatpak"), QStringLiteral("snap"), QStringLiteral("appimage")};
    if (purpose == QLatin1String("all"))
        return kAllKinds;
    if (purpose == QLatin1String("games"))
        return stores + QStringList{QStringLiteral("windows"), QStringLiteral("android")};
    if (purpose == QLatin1String("development"))
        return stores + QStringList{QStringLiteral("containers"), QStringLiteral("nix"), QStringLiteral("homebrew"), QStringLiteral("python"),
                                    QStringLiteral("node"), QStringLiteral("rust"), QStringLiteral("go"), QStringLiteral("conda")};
    if (purpose == QLatin1String("system"))
        return stores + QStringList{QStringLiteral("containers")};
    return stores;
}

bool AppCenterEngine::ready(const QString &kind) const
{
    if (m_demo)
        return kind != QLatin1String("conda") && kind != QLatin1String("homebrew");
    if (kind == QLatin1String("flatpak"))
        return Util::has(QStringLiteral("flatpak"));
    if (kind == QLatin1String("snap"))
        return SnapdClient::available();
    if (kind == QLatin1String("appimage"))
        return true;
    if (kind == QLatin1String("android"))
        return QFileInfo::exists(kWaydroidConfig);
    if (kind == QLatin1String("windows"))
        return QDir(QStringLiteral("/var/lib/flatpak/app/") + kBottles).exists()
               || QDir(share() + QStringLiteral("/flatpak/app/") + kBottles).exists();
    return !program(kind).isEmpty();
}

AppCenterEngine::AppCenterEngine(bool demo, bool window, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
{
    m_updates = item({{"checking", false}, {"total", 0}, {"kinds", QVariantList{}}});
    if (m_demo) {
        loadRows();   // the open tab's sample rows; loadDemo() fills the rest with them
        return;
    }
    m_snapd = new SnapdClient(this);
    refreshInstalled();
    if (!window)
        return;   // the background check counts the updates itself
    loadRows();
    loadGroups();
    checkUpdates();
}

void AppCenterEngine::setPurpose(const QString &purpose)
{
    m_purpose = purpose;
    m_query.clear();
    loadRows();
}

void AppCenterEngine::search(const QString &text)
{
    m_query = text.trimmed();
    loadRows();
}

void AppCenterEngine::loadRows()
{
    ++m_generation;
    m_rows.clear();
    const QStringList kinds = m_query.isEmpty() ? kindsFor(m_purpose) : kAllKinds;
    for (const QString &k : kinds)
        m_rows << rowFor(k);
    emit rowsChanged();
    if (m_demo) {
        loadDemo();
        return;
    }
    for (const QString &k : kinds) {
        if (!ready(k)) {
            setRow(k, {}, QStringLiteral("setup"));
            continue;
        }
        if (m_query.isEmpty())
            browse(k, m_purpose);
        else
            searchKind(k, m_query);
    }
}

void AppCenterEngine::setRow(const QString &kind, const QVariantList &items, const QString &state, const QString &message)
{
    for (QVariant &r : m_rows) {
        QVariantMap m = r.toMap();
        if (m.value("kind").toString() != kind)
            continue;
        QVariantList list = items;
        markInstalled(list);
        m.insert("items", list);
        m.insert("state", state == QLatin1String("ready") && list.isEmpty() ? QStringLiteral("empty") : state);
        m.insert("message", message);
        r = m;
    }
    emit rowsChanged();
}

void AppCenterEngine::markInstalled(QVariantList &items) const
{
    for (QVariant &v : items) {
        QVariantMap m = v.toMap();
        m.insert("installed", m_installedIds.value(m.value("kind").toString()).contains(m.value("id").toString()));
        v = m;
    }
}

// ───────────────────────────────────────────────────────────────────────── the catalogues

void AppCenterEngine::browse(const QString &kind, const QString &purpose)
{
    const int gen = m_generation;
    auto landed = [this, gen, kind](const QVariantList &items, const QString &state = QStringLiteral("ready"), const QString &msg = QString()) {
        if (gen == m_generation)
            setRow(kind, items, state, msg);
    };
    static const QHash<QString, QString> flathub{
        {QStringLiteral("games"), QStringLiteral("Game")},           {QStringLiteral("graphics"), QStringLiteral("Graphics")},
        {QStringLiteral("media"), QStringLiteral("AudioVideo")},     {QStringLiteral("office"), QStringLiteral("Office")},
        {QStringLiteral("internet"), QStringLiteral("Network")},     {QStringLiteral("development"), QStringLiteral("Development")},
        {QStringLiteral("education"), QStringLiteral("Education")},  {QStringLiteral("system"), QStringLiteral("Utility")},
    };
    static const QHash<QString, QString> snapSections{
        {QStringLiteral("all"), QStringLiteral("featured")},         {QStringLiteral("games"), QStringLiteral("games")},
        {QStringLiteral("graphics"), QStringLiteral("art-and-design")}, {QStringLiteral("media"), QStringLiteral("photo-and-video")},
        {QStringLiteral("office"), QStringLiteral("productivity")},  {QStringLiteral("internet"), QStringLiteral("social")},
        {QStringLiteral("development"), QStringLiteral("development")}, {QStringLiteral("education"), QStringLiteral("education")},
        {QStringLiteral("system"), QStringLiteral("utilities")},
    };
    if (kind == QLatin1String("flatpak")) {
        const QString url = purpose == QLatin1String("all")
            ? kFlathubApi + QStringLiteral("/collection/popular?page=1&per_page=30")
            : kFlathubApi + QStringLiteral("/collection/category/") + flathub.value(purpose) + QStringLiteral("?page=1&per_page=30");
        getJson(url, 12, [landed](const QJsonDocument &d, bool ok) {
            QVariantList items;
            for (const QJsonValue &h : d.object().value(QStringLiteral("hits")).toArray()) {
                const QJsonObject o = h.toObject();
                items << appItem(QStringLiteral("flatpak"), o.value(QStringLiteral("app_id")).toString(), o.value(QStringLiteral("name")).toString(),
                                 o.value(QStringLiteral("summary")).toString(), o.value(QStringLiteral("icon")).toString());
            }
            ok || !items.isEmpty() ? landed(items) : landed({}, QStringLiteral("error"), QObject::tr("Flathub can't be reached right now."));
        });
    } else if (kind == QLatin1String("snap")) {
        m_snapd->request(QStringLiteral("GET"), QStringLiteral("/v2/find?section=") + snapSections.value(purpose), {}, [landed](const QJsonObject &r) {
            QVariantList items;
            for (const QJsonValue &v : r.value(QStringLiteral("result")).toArray()) {
                const QJsonObject o = v.toObject();
                QString icon;
                for (const QJsonValue &m : o.value(QStringLiteral("media")).toArray())
                    if (m.toObject().value(QStringLiteral("type")).toString() == QLatin1String("icon"))
                        icon = m.toObject().value(QStringLiteral("url")).toString();
                QVariantMap it = appItem(QStringLiteral("snap"), o.value(QStringLiteral("name")).toString(), o.value(QStringLiteral("title")).toString(),
                                         o.value(QStringLiteral("summary")).toString(), icon, o.value(QStringLiteral("version")).toString());
                it.insert("classic", o.value(QStringLiteral("confinement")).toString() == QLatin1String("classic"));
                items << it;
            }
            r.isEmpty() ? landed({}, QStringLiteral("error"), QObject::tr("The Snap Store can't be reached right now.")) : landed(items.mid(0, 30));
        });
    } else if (kind == QLatin1String("appimage")) {
        static const QHash<QString, QString> cats{
            {QStringLiteral("games"), QStringLiteral("Game")},          {QStringLiteral("graphics"), QStringLiteral("Graphics")},
            {QStringLiteral("media"), QStringLiteral("AudioVideo")},    {QStringLiteral("office"), QStringLiteral("Office")},
            {QStringLiteral("internet"), QStringLiteral("Network")},    {QStringLiteral("development"), QStringLiteral("Development")},
            {QStringLiteral("education"), QStringLiteral("Education")}, {QStringLiteral("system"), QStringLiteral("Utility")},
        };
        const QString cat = cats.value(purpose);
        getJson(kAppImageFeed, 24, [landed, cat](const QJsonDocument &d, bool) {
            QVariantList items;
            for (const QJsonValue &v : d.object().value(QStringLiteral("items")).toArray()) {
                const QJsonObject o = v.toObject();
                QString repo;
                for (const QJsonValue &l : o.value(QStringLiteral("links")).toArray())
                    if (l.toObject().value(QStringLiteral("type")).toString() == QLatin1String("GitHub"))
                        repo = l.toObject().value(QStringLiteral("url")).toString();
                // Only those whose file Asima App Center can fetch (from GitHub Releases).
                if (repo.isEmpty() || (!cat.isEmpty() && !o.value(QStringLiteral("categories")).toVariant().toStringList().contains(cat)))
                    continue;
                const QJsonArray icons = o.value(QStringLiteral("icons")).toArray();
                const QString icon = icons.isEmpty() ? QString() : icons.first().toString();
                QVariantMap it = appItem(QStringLiteral("appimage"), o.value(QStringLiteral("name")).toString(), o.value(QStringLiteral("name")).toString(),
                                         o.value(QStringLiteral("description")).toString().section(QLatin1Char('.'), 0, 0),
                                         icon.isEmpty() ? QString() : kAppImageMedia + icon);
                it.insert("repo", repo);
                it.insert("categories", o.value(QStringLiteral("categories")).toVariant());
                items << it;
                if (items.size() >= 30)
                    break;
            }
            items.isEmpty() ? landed({}, QStringLiteral("error"), QObject::tr("AppImageHub can't be reached right now.")) : landed(items);
        });
    } else if (kind == QLatin1String("windows") && purpose == QLatin1String("games")) {
        // Game stores that run in Bottles or are on Flathub themselves.
        landed(installedItems(kind));
    } else {
        // Kinds without a catalogue to browse: what you have, and the search above for more.
        landed(installedItems(kind));
    }
}

void AppCenterEngine::searchKind(const QString &kind, const QString &text)
{
    const int gen = m_generation;
    auto landed = [this, gen, kind](const QVariantList &items, const QString &state = QStringLiteral("ready"), const QString &msg = QString()) {
        if (gen == m_generation)
            setRow(kind, items, state, msg);
    };
    const QString q = QString::fromUtf8(QUrl::toPercentEncoding(text));
    // Kinds that install by exact name: one card to install what was typed.
    auto byName = [&](const QString &summary) {
        return validName(text) ? QVariantList{appItem(kind, text, text, summary)} : QVariantList{};
    };
    if (kind == QLatin1String("flatpak")) {
        getJson(kFlathubApi + QStringLiteral("/search"), 1, [landed](const QJsonDocument &d, bool) {
            QVariantList items;
            for (const QJsonValue &h : d.object().value(QStringLiteral("hits")).toArray()) {
                const QJsonObject o = h.toObject();
                items << appItem(QStringLiteral("flatpak"), o.value(QStringLiteral("app_id")).toString(), o.value(QStringLiteral("name")).toString(),
                                 o.value(QStringLiteral("summary")).toString(), o.value(QStringLiteral("icon")).toString());
            }
            landed(items.mid(0, 30));
        }, QJsonDocument(QJsonObject{{QStringLiteral("query"), text}, {QStringLiteral("filters"), QJsonArray()}}).toJson(QJsonDocument::Compact));
    } else if (kind == QLatin1String("snap")) {
        m_snapd->request(QStringLiteral("GET"), QStringLiteral("/v2/find?q=") + q, {}, [landed](const QJsonObject &r) {
            QVariantList items;
            for (const QJsonValue &v : r.value(QStringLiteral("result")).toArray()) {
                const QJsonObject o = v.toObject();
                QVariantMap it = appItem(QStringLiteral("snap"), o.value(QStringLiteral("name")).toString(), o.value(QStringLiteral("title")).toString(),
                                         o.value(QStringLiteral("summary")).toString(), QString(), o.value(QStringLiteral("version")).toString());
                for (const QJsonValue &m : o.value(QStringLiteral("media")).toArray())
                    if (m.toObject().value(QStringLiteral("type")).toString() == QLatin1String("icon"))
                        it.insert("icon", m.toObject().value(QStringLiteral("url")).toString());
                it.insert("classic", o.value(QStringLiteral("confinement")).toString() == QLatin1String("classic"));
                items << it;
            }
            landed(items.mid(0, 30));
        });
    } else if (kind == QLatin1String("appimage")) {
        getJson(kAppImageFeed, 24, [landed, text](const QJsonDocument &d, bool) {
            QVariantList items;
            for (const QJsonValue &v : d.object().value(QStringLiteral("items")).toArray()) {
                const QJsonObject o = v.toObject();
                const QString name = o.value(QStringLiteral("name")).toString();
                const QString desc = o.value(QStringLiteral("description")).toString();
                QString repo;
                for (const QJsonValue &l : o.value(QStringLiteral("links")).toArray())
                    if (l.toObject().value(QStringLiteral("type")).toString() == QLatin1String("GitHub"))
                        repo = l.toObject().value(QStringLiteral("url")).toString();
                if (repo.isEmpty() || (!name.contains(text, Qt::CaseInsensitive) && !desc.contains(text, Qt::CaseInsensitive)))
                    continue;
                const QJsonArray icons = o.value(QStringLiteral("icons")).toArray();
                const QString icon = icons.isEmpty() ? QString() : icons.first().toString();
                QVariantMap it = appItem(QStringLiteral("appimage"), name, name, desc.section(QLatin1Char('.'), 0, 0),
                                         icon.isEmpty() ? QString() : kAppImageMedia + icon);
                it.insert("repo", repo);
                items << it;
                if (items.size() >= 30)
                    break;
            }
            landed(items);
        });
    } else if (kind == QLatin1String("android")) {
        getJson(QStringLiteral("https://search.f-droid.org/api/search_apps?lang=en&q=") + q, 6, [landed](const QJsonDocument &d, bool) {
            QVariantList items;
            for (const QJsonValue &v : d.object().value(QStringLiteral("apps")).toArray()) {
                const QJsonObject o = v.toObject();
                const QString pkg = o.value(QStringLiteral("url")).toString().section(QLatin1Char('/'), -1);
                items << appItem(QStringLiteral("android"), pkg, o.value(QStringLiteral("name")).toString(),
                                 o.value(QStringLiteral("summary")).toString(), o.value(QStringLiteral("icon")).toString());
            }
            landed(items.mid(0, 30));
        });
    } else if (kind == QLatin1String("node")) {
        getJson(QStringLiteral("https://registry.npmjs.org/-/v1/search?size=20&text=") + q, 6, [landed](const QJsonDocument &d, bool) {
            QVariantList items;
            for (const QJsonValue &v : d.object().value(QStringLiteral("objects")).toArray()) {
                const QJsonObject p = v.toObject().value(QStringLiteral("package")).toObject();
                items << appItem(QStringLiteral("node"), p.value(QStringLiteral("name")).toString(), p.value(QStringLiteral("name")).toString(),
                                 p.value(QStringLiteral("description")).toString(), QString(), p.value(QStringLiteral("version")).toString());
            }
            landed(items);
        });
    } else if (kind == QLatin1String("rust")) {
        getJson(QStringLiteral("https://crates.io/api/v1/crates?per_page=20&q=") + q, 6, [landed](const QJsonDocument &d, bool) {
            QVariantList items;
            for (const QJsonValue &v : d.object().value(QStringLiteral("crates")).toArray()) {
                const QJsonObject c = v.toObject();
                items << appItem(QStringLiteral("rust"), c.value(QStringLiteral("name")).toString(), c.value(QStringLiteral("name")).toString(),
                                 c.value(QStringLiteral("description")).toString(), QString(), c.value(QStringLiteral("max_version")).toString());
            }
            landed(items);
        });
    } else if (kind == QLatin1String("python")) {
        // PyPI has no search to ask: the tool of that exact name, when PyPI knows it.
        getJson(QStringLiteral("https://pypi.org/pypi/") + q + QStringLiteral("/json"), 6, [landed](const QJsonDocument &d, bool ok) {
            const QJsonObject i = d.object().value(QStringLiteral("info")).toObject();
            ok && !i.isEmpty() ? landed({appItem(QStringLiteral("python"), i.value(QStringLiteral("name")).toString(), i.value(QStringLiteral("name")).toString(),
                                                 i.value(QStringLiteral("summary")).toString(), QString(), i.value(QStringLiteral("version")).toString())})
                               : landed({});
        });
    } else if (kind == QLatin1String("conda")) {
        getJson(QStringLiteral("https://api.anaconda.org/search?type=conda&name=") + q, 6, [landed](const QJsonDocument &d, bool) {
            QVariantList items;
            QSet<QString> seen;
            for (const QJsonValue &v : d.array()) {
                const QJsonObject p = v.toObject();
                const QString name = p.value(QStringLiteral("name")).toString();
                if (p.value(QStringLiteral("owner")).toString() != QLatin1String("conda-forge") || seen.contains(name))
                    continue;
                seen.insert(name);
                items << appItem(QStringLiteral("conda"), name, name, p.value(QStringLiteral("summary")).toString());
            }
            landed(items.mid(0, 20));
        });
    } else if (kind == QLatin1String("homebrew")) {
        getJson(QStringLiteral("https://formulae.brew.sh/api/formula/") + q + QStringLiteral(".json"), 6, [landed](const QJsonDocument &d, bool ok) {
            const QJsonObject f = d.object();
            ok && !f.isEmpty() ? landed({appItem(QStringLiteral("homebrew"), f.value(QStringLiteral("name")).toString(), f.value(QStringLiteral("name")).toString(),
                                                 f.value(QStringLiteral("desc")).toString(), QString(),
                                                 f.value(QStringLiteral("versions")).toObject().value(QStringLiteral("stable")).toString())})
                               : landed({});
        });
    } else if (kind == QLatin1String("nix")) {
        runAsync(this, program(kind), {QStringLiteral("search"), QStringLiteral("nixpkgs"), text, QStringLiteral("--json")},
                 [landed](const QString &out, bool) {
            QVariantList items;
            const QJsonObject all = QJsonDocument::fromJson(out.toUtf8()).object();
            for (auto it = all.begin(); it != all.end() && items.size() < 30; ++it) {
                const QJsonObject o = it.value().toObject();
                const QString attr = it.key().section(QLatin1Char('.'), 2);   // legacyPackages.x86_64-linux.NAME
                items << appItem(QStringLiteral("nix"), attr, o.value(QStringLiteral("pname")).toString(),
                                 o.value(QStringLiteral("description")).toString(), QString(), o.value(QStringLiteral("version")).toString());
            }
            landed(items);
        });
        setRow(kind, {}, QStringLiteral("loading"), tr("The first search downloads the list of Nixpkgs."));
    } else if (kind == QLatin1String("go")) {
        landed(text.contains(QLatin1Char('/')) ? byName(tr("Go module: installed with go install …@latest")) : QVariantList{}, QStringLiteral("ready"),
               tr("Type a module path, for example golang.org/x/tools/gopls."));
    } else if (kind == QLatin1String("containers")) {
        landed(byName(tr("Installed from the container's own packages, and added to your menu or terminal")));
    } else {
        landed({});
    }
}

// ───────────────────────────────────────────────────────────────────────── what is installed

QVariantList AppCenterEngine::installedItems(const QString &kind) const
{
    for (const QVariant &r : m_installed)
        if (r.toMap().value("kind").toString() == kind)
            return r.toMap().value("items").toList();
    return {};
}

void AppCenterEngine::refreshInstalled()
{
    if (m_demo)
        return;
    // Each kind answers on its own; the page fills in as they do.
    auto put = [this](const QString &kind, const QVariantList &items) {
        QSet<QString> ids;
        for (const QVariant &v : items)
            ids.insert(v.toMap().value("id").toString());
        m_installedIds.insert(kind, ids);
        QVariantMap row = rowFor(kind);
        row.insert("items", items);
        row.insert("ready", ready(kind));
        QVariantList tools;
        if (kind == QLatin1String("flatpak"))
            tools << item({{"id", "unused"}, {"title", tr("Remove unused runtimes")}}) << item({{"id", "repair"}, {"title", tr("Repair")}});
        else if (kind == QLatin1String("appimage"))
            tools << item({{"id", "cleanup"}, {"title", tr("Clean up the menu")}});
        else if (kind == QLatin1String("nix"))
            tools << item({{"id", "garbage"}, {"title", tr("Free up space")}});
        else if (kind == QLatin1String("android") && ready(kind))
            tools << item({{"id", "launch"}, {"title", tr("Open Android")}}) << item({{"id", "stop"}, {"title", tr("Stop Android")}})
                  << item({{"id", "fdroid"}, {"title", tr("Get the F-Droid app")}}) << item({{"id", "removeandroid"}, {"title", tr("Remove Android")}});
        else if (kind == QLatin1String("windows") && ready(kind))
            tools << item({{"id", "bottles"}, {"title", tr("Open Bottles")}});
        row.insert("tools", tools);
        bool found = false;
        for (QVariant &r : m_installed)
            if (r.toMap().value("kind").toString() == kind) {
                r = row;
                found = true;
            }
        if (!found)
            m_installed << row;
        std::sort(m_installed.begin(), m_installed.end(), [](const QVariant &a, const QVariant &b) {
            return kAllKinds.indexOf(a.toMap().value("kind").toString()) < kAllKinds.indexOf(b.toMap().value("kind").toString());
        });
        // Rows already shown learn what is installed now.
        for (QVariant &r : m_rows) {
            QVariantMap m = r.toMap();
            if (m.value("kind").toString() != kind)
                continue;
            QVariantList list = m.value("items").toList();
            markInstalled(list);
            m.insert("items", list);
            r = m;
        }
        emit installedChanged();
        emit rowsChanged();
    };

    if (ready(QStringLiteral("flatpak")))
        runAsync(this, QStringLiteral("flatpak"), {QStringLiteral("list"), QStringLiteral("--app"), QStringLiteral("--columns=application,name,version,installation")},
                 [this, put](const QString &out, bool) {
            QVariantList items;
            for (const QString &l : out.split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
                const QStringList f = l.split(QLatin1Char('\t'));
                QVariantMap it = appItem(QStringLiteral("flatpak"), f.value(0), f.value(1), QString(), QString(), f.value(2));
                it.insert("installation", f.value(3));
                it.insert("installed", true);
                items << it;
            }
            put(QStringLiteral("flatpak"), items);
            // Apps installed since (here or with Discover) get their cache in memory too.
            if (cachesInMemory())
                applyCaches(items, true);
        });
    if (ready(QStringLiteral("snap")))
        m_snapd->request(QStringLiteral("GET"), QStringLiteral("/v2/snaps"), {}, [put](const QJsonObject &r) {
            QVariantList items;
            for (const QJsonValue &v : r.value(QStringLiteral("result")).toArray()) {
                const QJsonObject o = v.toObject();
                if (o.value(QStringLiteral("type")).toString() != QLatin1String("app"))
                    continue;   // bases, snapd itself, kernels
                QVariantMap it = appItem(QStringLiteral("snap"), o.value(QStringLiteral("name")).toString(), o.value(QStringLiteral("title")).toString(),
                                         o.value(QStringLiteral("summary")).toString(), QString(), o.value(QStringLiteral("version")).toString());
                it.insert("installed", true);
                items << it;
            }
            put(QStringLiteral("snap"), items);
        });
    {   // AppImages: the menu entries Asima App Center wrote, and files in ~/Applications without one.
        QVariantList items;
        QSet<QString> known;
        for (const QFileInfo &f : QDir(menuDir()).entryInfoList({QStringLiteral("asimaos-appimage-*.desktop")}, QDir::Files)) {
            const auto k = desktopKeys(f.absoluteFilePath());
            QVariantMap it = appItem(QStringLiteral("appimage"), k.value(QStringLiteral("X-AsimaOS-Name"), k.value(QStringLiteral("Name"))),
                                     k.value(QStringLiteral("Name")), k.value(QStringLiteral("Comment")),
                                     QUrl::fromLocalFile(k.value(QStringLiteral("Icon"))).toString(), k.value(QStringLiteral("X-AsimaOS-Version")));
            it.insert("file", k.value(QStringLiteral("X-AsimaOS-AppImage")));
            it.insert("entry", f.absoluteFilePath());
            it.insert("repo", k.value(QStringLiteral("X-AsimaOS-Source")));
            it.insert("missing", !QFileInfo::exists(k.value(QStringLiteral("X-AsimaOS-AppImage"))));
            it.insert("installed", true);
            known.insert(k.value(QStringLiteral("X-AsimaOS-AppImage")));
            items << it;
        }
        for (const QFileInfo &f : QDir(appsDir()).entryInfoList({QStringLiteral("*.AppImage"), QStringLiteral("*.appimage")}, QDir::Files)) {
            if (known.contains(f.absoluteFilePath()))
                continue;
            QVariantMap it = appItem(QStringLiteral("appimage"), f.completeBaseName(), f.completeBaseName(), tr("Not in the app menu yet"));
            it.insert("file", f.absoluteFilePath());
            it.insert("installed", true);
            items << it;
        }
        put(QStringLiteral("appimage"), items);
    }
    if (ready(QStringLiteral("android")))
        runAsync(this, QStringLiteral("waydroid"), {QStringLiteral("app"), QStringLiteral("list")}, [put](const QString &out, bool) {
            QVariantList items;
            QString name;
            for (const QString &l : out.split(QLatin1Char('\n'))) {
                if (l.startsWith(QLatin1String("Name:")))
                    name = l.mid(5).trimmed();
                else if (l.startsWith(QLatin1String("packageName:"))) {
                    QVariantMap it = appItem(QStringLiteral("android"), l.mid(12).trimmed(), name, QString());
                    it.insert("installed", true);
                    items << it;
                }
            }
            put(QStringLiteral("android"), items);
        });
    else
        put(QStringLiteral("android"), {});
    if (ready(QStringLiteral("windows")))
        runAsync(this, QStringLiteral("flatpak"), {QStringLiteral("run"), QStringLiteral("--command=bottles-cli"), kBottles, QStringLiteral("--json"),
                                                   QStringLiteral("programs"), QStringLiteral("-b"), kBottle},
                 [put](const QString &out, bool) {
            QVariantList items;
            for (const QJsonValue &v : QJsonDocument::fromJson(out.toUtf8()).array()) {
                const QJsonObject p = v.toObject();
                QVariantMap it = appItem(QStringLiteral("windows"), p.value(QStringLiteral("name")).toString(), p.value(QStringLiteral("name")).toString(),
                                         p.value(QStringLiteral("path")).toString());
                it.insert("installed", true);
                items << it;
            }
            put(QStringLiteral("windows"), items);
        });
    else
        put(QStringLiteral("windows"), {});
    if (ready(QStringLiteral("containers")))
        runAsync(this, program(QStringLiteral("containers")), {QStringLiteral("list"), QStringLiteral("--no-color")}, [this, put](const QString &out, bool) {
            QVariantList items;
            for (const QString &l : out.split(QLatin1Char('\n'), Qt::SkipEmptyParts).mid(1)) {
                const QStringList f = l.split(QLatin1Char('|'));
                if (f.size() < 4)
                    continue;
                QVariantMap it = appItem(QStringLiteral("containers"), f.value(1).trimmed(), f.value(1).trimmed(),
                                         tr("Distrobox · %1").arg(f.value(3).trimmed()), QStringLiteral("virtual-desktops"), f.value(2).trimmed());
                it.insert("installed", true);
                it.insert("container", true);
                items << it;
            }
            if (QDir(home() + QStringLiteral("/.junest")).exists()) {
                QVariantMap it = appItem(QStringLiteral("containers"), QStringLiteral("junest"), QStringLiteral("JuNest"),
                                         tr("Arch Linux in your home folder, without root"), QStringLiteral("virtual-desktops"));
                it.insert("installed", true);
                it.insert("container", true);
                items << it;
            }
            put(QStringLiteral("containers"), items);
        });
    if (ready(QStringLiteral("nix")))
        runAsync(this, program(QStringLiteral("nix")), {QStringLiteral("profile"), QStringLiteral("list"), QStringLiteral("--json")}, [put](const QString &out, bool) {
            QVariantList items;
            const QJsonObject e = QJsonDocument::fromJson(out.toUtf8()).object().value(QStringLiteral("elements")).toObject();
            for (auto it = e.begin(); it != e.end(); ++it) {
                QVariantMap m = appItem(QStringLiteral("nix"), it.key(), it.key(), it.value().toObject().value(QStringLiteral("attrPath")).toString());
                m.insert("installed", true);
                items << m;
            }
            put(QStringLiteral("nix"), items);
        });
    auto lines = [this, put](const QString &kind, const QStringList &args, std::function<QVariantMap(const QString &)> parse) {
        if (!ready(kind))
            return put(kind, {});
        runAsync(this, program(kind), args, [put, kind, parse](const QString &out, bool) {
            QVariantList items;
            for (const QString &l : out.split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
                QVariantMap it = parse(l);
                if (it.isEmpty())
                    continue;
                it.insert("installed", true);
                items << it;
            }
            put(kind, items);
        });
    };
    lines(QStringLiteral("homebrew"), {QStringLiteral("list"), QStringLiteral("--formula"), QStringLiteral("--versions")}, [](const QString &l) {
        return appItem(QStringLiteral("homebrew"), l.section(QLatin1Char(' '), 0, 0), l.section(QLatin1Char(' '), 0, 0), QString(), QString(),
                       l.section(QLatin1Char(' '), 1, 1));
    });
    lines(QStringLiteral("rust"), {QStringLiteral("install"), QStringLiteral("--list")}, [](const QString &l) {
        if (l.startsWith(QLatin1Char(' ')))
            return QVariantMap();
        return appItem(QStringLiteral("rust"), l.section(QLatin1Char(' '), 0, 0), l.section(QLatin1Char(' '), 0, 0), QString(), QString(),
                       l.section(QLatin1Char(' '), 1, 1).remove(QLatin1Char('v')).remove(QLatin1Char(':')));
    });
    if (ready(QStringLiteral("python")))
        runAsync(this, program(QStringLiteral("python")), {QStringLiteral("list"), QStringLiteral("--json")}, [put](const QString &out, bool) {
            QVariantList items;
            const QJsonObject venvs = QJsonDocument::fromJson(out.toUtf8()).object().value(QStringLiteral("venvs")).toObject();
            for (auto it = venvs.begin(); it != venvs.end(); ++it) {
                const QJsonObject main = it.value().toObject().value(QStringLiteral("metadata")).toObject().value(QStringLiteral("main_package")).toObject();
                QVariantMap m = appItem(QStringLiteral("python"), it.key(), it.key(), QString(), QString(), main.value(QStringLiteral("package_version")).toString());
                m.insert("installed", true);
                items << m;
            }
            put(QStringLiteral("python"), items);
        });
    else
        put(QStringLiteral("python"), {});
    if (ready(QStringLiteral("node")))
        runAsync(this, program(QStringLiteral("node")), {QStringLiteral("ls"), QStringLiteral("-g"), QStringLiteral("--depth=0"), QStringLiteral("--json")},
                 [put](const QString &out, bool) {
            QVariantList items;
            const QJsonObject deps = QJsonDocument::fromJson(out.toUtf8()).object().value(QStringLiteral("dependencies")).toObject();
            for (auto it = deps.begin(); it != deps.end(); ++it) {
                if (it.key() == QLatin1String("npm") || it.key() == QLatin1String("corepack"))
                    continue;
                QVariantMap m = appItem(QStringLiteral("node"), it.key(), it.key(), QString(), QString(),
                                        it.value().toObject().value(QStringLiteral("version")).toString());
                m.insert("installed", true);
                items << m;
            }
            put(QStringLiteral("node"), items);
        });
    else
        put(QStringLiteral("node"), {});
    {   // Go: the programs in ~/go/bin.
        QVariantList items;
        if (ready(QStringLiteral("go")))
            for (const QFileInfo &f : QDir(home() + QStringLiteral("/go/bin")).entryInfoList(QDir::Files | QDir::Executable)) {
                QVariantMap m = appItem(QStringLiteral("go"), f.fileName(), f.fileName(), QString());
                m.insert("installed", true);
                items << m;
            }
        put(QStringLiteral("go"), items);
    }
    if (ready(QStringLiteral("conda")))
        runAsync(this, program(QStringLiteral("conda")), {QStringLiteral("env"), QStringLiteral("list"), QStringLiteral("--json")}, [put](const QString &out, bool) {
            QVariantList items;
            const QJsonArray envs = QJsonDocument::fromJson(out.toUtf8()).object().value(QStringLiteral("envs")).toArray();
            for (const QJsonValue &v : envs) {
                const QString name = QFileInfo(v.toString()).fileName();
                if (name == QLatin1String("miniforge3"))
                    continue;   // the base environment
                QVariantMap m = appItem(QStringLiteral("conda"), name, name, v.toString());
                m.insert("installed", true);
                items << m;
            }
            put(QStringLiteral("conda"), items);
        });
    else
        put(QStringLiteral("conda"), {});
}

// ───────────────────────────────────────────────────────────────────────── changes

QList<AppCenterEngine::Step> AppCenterEngine::installSteps(const QVariantMap &it) const
{
    const QString kind = it.value("kind").toString();
    const QString id = it.value("id").toString();
    const QString title = tr("Installing %1").arg(it.value("name").toString());
    auto run = [&](const QString &prog, const QStringList &args) { Step s; s.title = title; s.program = prog; s.args = args; return QList<Step>{s}; };
    if (kind == QLatin1String("flatpak"))
        return run(QStringLiteral("flatpak"), {QStringLiteral("install"), QStringLiteral("--system"), QStringLiteral("-y"), QStringLiteral("--noninteractive"),
                                              QStringLiteral("flathub"), id});
    if (kind == QLatin1String("snap")) {
        Step s;
        s.title = title;
        s.snapMethod = QStringLiteral("POST");
        s.snapPath = QStringLiteral("/v2/snaps/") + id;
        QJsonObject body{{QStringLiteral("action"), QStringLiteral("install")}};
        if (it.value("classic").toBool())
            body.insert(QStringLiteral("classic"), true);
        s.snapBody = QJsonDocument(body).toJson(QJsonDocument::Compact);
        return {s};
    }
    if (kind == QLatin1String("nix"))
        return run(program(kind), {QStringLiteral("profile"), QStringLiteral("install"), QStringLiteral("nixpkgs#") + id});
    if (kind == QLatin1String("homebrew"))
        return run(program(kind), {QStringLiteral("install"), id});
    if (kind == QLatin1String("python"))
        return run(program(kind), {QStringLiteral("install"), id});
    if (kind == QLatin1String("node"))
        return run(program(kind), {QStringLiteral("install"), QStringLiteral("-g"), id});
    if (kind == QLatin1String("rust"))
        return run(program(kind), {QStringLiteral("install"), id});
    if (kind == QLatin1String("go"))
        return run(program(kind), {QStringLiteral("install"), id + QStringLiteral("@latest")});
    if (kind == QLatin1String("conda"))
        return run(program(kind), {QStringLiteral("create"), QStringLiteral("-y"), QStringLiteral("-n"), id, QStringLiteral("-c"), QStringLiteral("conda-forge"), id});
    if (kind == QLatin1String("containers")) {
        // Into the first Distrobox container (one from Arch Linux is made when there is none),
        // with whichever package manager it has, then onto the menu or into ~/.local/bin.
        const QVariantList boxes = installedItems(kind);
        QString box;
        for (const QVariant &b : boxes)
            if (b.toMap().value("id").toString() != QLatin1String("junest")) {
                box = b.toMap().value("id").toString();
                break;
            }
        QList<Step> steps;
        if (box.isEmpty()) {
            box = QStringLiteral("arch");
            steps << run(program(kind), {QStringLiteral("create"), QStringLiteral("--yes"), QStringLiteral("--name"), box,
                                         QStringLiteral("--image"), QStringLiteral("docker.io/library/archlinux:latest")});
            steps.last().title = tr("Making a container from Arch Linux");
        }
        const QString pm = QStringLiteral("sudo pacman -S --noconfirm \"$1\" || sudo apt-get install -y \"$1\" || sudo dnf install -y \"$1\" "
                                          "|| sudo zypper -n install \"$1\" || sudo apk add \"$1\"");
        steps << run(program(kind), {QStringLiteral("enter"), box, QStringLiteral("--"), QStringLiteral("sh"), QStringLiteral("-c"), pm, QStringLiteral("sh"), id});
        const QString exp = QStringLiteral("distrobox-export --app \"$1\" || distrobox-export --bin \"$(command -v \"$1\")\" --export-path \"$HOME/.local/bin\"");
        steps << run(program(kind), {QStringLiteral("enter"), box, QStringLiteral("--"), QStringLiteral("sh"), QStringLiteral("-c"), exp, QStringLiteral("sh"), id});
        steps.last().title = tr("Adding %1 to your menu").arg(id);
        return steps;
    }
    return {};
}

QList<AppCenterEngine::Step> AppCenterEngine::removeSteps(const QVariantMap &it) const
{
    const QString kind = it.value("kind").toString();
    const QString id = it.value("id").toString();
    const QString title = tr("Removing %1").arg(it.value("name").toString());
    auto run = [&](const QString &prog, const QStringList &args) { Step s; s.title = title; s.program = prog; s.args = args; return QList<Step>{s}; };
    if (kind == QLatin1String("flatpak"))
        return run(QStringLiteral("flatpak"), {QStringLiteral("uninstall"), it.value("installation").toString() == QLatin1String("user") ? QStringLiteral("--user") : QStringLiteral("--system"),
                                               QStringLiteral("-y"), QStringLiteral("--noninteractive"), id});
    if (kind == QLatin1String("snap")) {
        Step s;
        s.title = title;
        s.snapMethod = QStringLiteral("POST");
        s.snapPath = QStringLiteral("/v2/snaps/") + id;
        s.snapBody = R"({"action":"remove"})";
        return {s};
    }
    if (kind == QLatin1String("appimage")) {
        Step s;
        s.title = title;
        const QString file = it.value("file").toString(), entry = it.value("entry").toString();
        const QString icon = QUrl(it.value("icon").toString()).toLocalFile();
        s.action = [file, entry, icon] {
            bool ok = file.isEmpty() || !QFileInfo::exists(file) || QFile::remove(file);
            if (!entry.isEmpty())
                QFile::remove(entry);
            if (icon.startsWith(share()))
                QFile::remove(icon);
            return ok;
        };
        return {s};
    }
    if (kind == QLatin1String("android"))
        return run(QStringLiteral("waydroid"), {QStringLiteral("app"), QStringLiteral("remove"), id});
    if (kind == QLatin1String("containers"))
        return id == QLatin1String("junest") ? run(QStringLiteral("rm"), {QStringLiteral("-rf"), home() + QStringLiteral("/.junest")})
                                              : run(program(kind), {QStringLiteral("rm"), QStringLiteral("--force"), id});
    if (kind == QLatin1String("nix"))
        return run(program(kind), {QStringLiteral("profile"), QStringLiteral("remove"), id});
    if (kind == QLatin1String("homebrew"))
        return run(program(kind), {QStringLiteral("uninstall"), id});
    if (kind == QLatin1String("python"))
        return run(program(kind), {QStringLiteral("uninstall"), id});
    if (kind == QLatin1String("node"))
        return run(program(kind), {QStringLiteral("uninstall"), QStringLiteral("-g"), id});
    if (kind == QLatin1String("rust"))
        return run(program(kind), {QStringLiteral("uninstall"), id});
    if (kind == QLatin1String("go")) {
        Step s;
        s.title = title;
        const QString file = home() + QStringLiteral("/go/bin/") + id;
        s.action = [file] { return QFile::remove(file); };
        return {s};
    }
    if (kind == QLatin1String("conda"))
        return run(program(kind), {QStringLiteral("env"), QStringLiteral("remove"), QStringLiteral("-y"), QStringLiteral("-n"), id});
    return {};
}

QList<AppCenterEngine::Step> AppCenterEngine::updateSteps(const QString &kind) const
{
    const QString title = tr("Updating %1").arg(rowFor(kind).value("title").toString());
    auto run = [&](const QString &prog, const QStringList &args) { Step s; s.title = title; s.program = prog; s.args = args; return QList<Step>{s}; };
    if (!ready(kind))
        return {};
    if (kind == QLatin1String("flatpak"))
        return run(QStringLiteral("flatpak"), {QStringLiteral("update"), QStringLiteral("-y"), QStringLiteral("--noninteractive")});
    if (kind == QLatin1String("snap")) {
        Step s;
        s.title = title;
        s.snapMethod = QStringLiteral("POST");
        s.snapPath = QStringLiteral("/v2/snaps");
        s.snapBody = R"({"action":"refresh"})";
        return {s};
    }
    if (kind == QLatin1String("containers"))
        return run(program(kind), {QStringLiteral("upgrade"), QStringLiteral("--all")});
    if (kind == QLatin1String("nix"))
        return run(program(kind), {QStringLiteral("profile"), QStringLiteral("upgrade"), QStringLiteral("--all")});
    if (kind == QLatin1String("homebrew"))
        return run(program(kind), {QStringLiteral("update")}) + run(program(kind), {QStringLiteral("upgrade")});
    if (kind == QLatin1String("python"))
        return run(program(kind), {QStringLiteral("upgrade-all")});
    if (kind == QLatin1String("node"))
        return run(program(kind), {QStringLiteral("update"), QStringLiteral("-g")});
    if (kind == QLatin1String("rust")) {
        // cargo install replaces a program when crates.io has a newer version.
        QList<Step> steps;
        for (const QVariant &v : installedItems(kind))
            steps += run(program(kind), {QStringLiteral("install"), v.toMap().value("id").toString()});
        return steps;
    }
    if (kind == QLatin1String("go")) {
        // Each program again from its module's newest version (`go version -m` names the module).
        QList<Step> steps;
        for (const QVariant &v : installedItems(kind)) {
            const QString bin = home() + QStringLiteral("/go/bin/") + v.toMap().value("id").toString();
            steps += run(QStringLiteral("sh"), {QStringLiteral("-c"), QStringLiteral("p=$(\"$0\" version -m \"$1\" | awk '$1 == \"path\" { print $2 }') && [ -n \"$p\" ] && \"$0\" install \"$p@latest\""),
                                                program(kind), bin});
        }
        return steps;
    }
    if (kind == QLatin1String("conda")) {
        QList<Step> steps;
        for (const QVariant &v : installedItems(kind))
            steps += run(program(kind), {QStringLiteral("update"), QStringLiteral("-y"), QStringLiteral("--all"), QStringLiteral("-n"), v.toMap().value("id").toString()});
        return steps;
    }
    return {};
}

void AppCenterEngine::install(const QVariantMap &it)
{
    const QString kind = it.value("kind").toString();
    const QString id = it.value("id").toString();
    const QString name = it.value("name").toString();
    if (!validName(id) && kind != QLatin1String("appimage"))
        return emit toast(tr("That name can't be installed."), QStringLiteral("warn"));
    if (m_demo) {
        m_installedIds[kind].insert(id);
        for (QVariant &r : m_rows) {
            QVariantMap m = r.toMap();
            QVariantList list = m.value("items").toList();
            markInstalled(list);
            m.insert("items", list);
            r = m;
        }
        emit rowsChanged();
        return emit toast(tr("%1 is installed.").arg(name), QStringLiteral("ok"));
    }
    const QString done = tr("%1 is installed.").arg(name), failed = tr("%1 could not be installed.").arg(name);
    if (kind == QLatin1String("appimage")) {
        // The newest release's AppImage on GitHub, then a menu entry with AppImageHub's icon.
        const QString repo = it.value("repo").toString();
        getJson(QStringLiteral("https://api.github.com/repos/") + repo + QStringLiteral("/releases/latest"), 1,
                [this, it, name, repo, done, failed](const QJsonDocument &d, bool ok) {
            QString url, asset;
            for (const QJsonValue &a : d.object().value(QStringLiteral("assets")).toArray()) {
                const QString n = a.toObject().value(QStringLiteral("name")).toString();
                const bool image = n.endsWith(QLatin1String(".AppImage"), Qt::CaseInsensitive);
                const bool otherArch = n.contains(QRegularExpression(QStringLiteral("(arm|aarch64|i386|i686)"), QRegularExpression::CaseInsensitiveOption));
                if (image && !otherArch && (url.isEmpty() || n.contains(QLatin1String("x86_64")) || n.contains(QLatin1String("amd64")))) {
                    url = a.toObject().value(QStringLiteral("browser_download_url")).toString();
                    asset = n;
                }
            }
            if (!ok || url.isEmpty())
                return emit toast(tr("%1's release on GitHub has no AppImage for this computer.").arg(name), QStringLiteral("warn"));
            const QString tag = d.object().value(QStringLiteral("tag_name")).toString();
            const QString s = slug(name);
            const QString file = appsDir() + QLatin1Char('/') + s + QStringLiteral(".AppImage");
            const QString icon = share() + QStringLiteral("/icons/hicolor/128x128/apps/asimaos-appimage-") + s + QStringLiteral(".png");
            Job job{QStringLiteral("appimage"), tr("Installing %1").arg(name), done, failed, {}};
            Step get;
            get.title = tr("Downloading %1").arg(asset);
            get.url = url;
            get.path = file;
            job.steps << get;
            if (const QString iconUrl = it.value("icon").toString(); !iconUrl.isEmpty()) {
                Step ic;
                ic.title = tr("Its icon");
                ic.url = iconUrl;
                ic.path = icon;
                job.steps << ic;
            }
            Step menu;
            menu.title = tr("Adding %1 to the app menu").arg(name);
            const QString comment = it.value("summary").toString();
            const QString cats = it.value("categories").toStringList().join(QLatin1Char(';'));
            menu.action = [file, icon, name, s, repo, tag, comment, cats] {
                QFile::setPermissions(file, QFile::permissions(file) | QFileDevice::ExeOwner | QFileDevice::ExeUser);
                QDir().mkpath(menuDir());
                QFile f(menuDir() + QStringLiteral("/asimaos-appimage-") + s + QStringLiteral(".desktop"));
                if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate))
                    return false;
                f.write(QStringLiteral("[Desktop Entry]\nType=Application\nName=%1\nComment=%2\nExec=\"%3\" %U\nIcon=%4\nCategories=%5;\nTerminal=false\n"
                                       "X-AsimaOS-AppImage=%3\nX-AsimaOS-Name=%1\nX-AsimaOS-Source=%6\nX-AsimaOS-Version=%7\n")
                            .arg(name, comment, file, QFileInfo::exists(icon) ? icon : QStringLiteral("application-x-executable"),
                                 cats.isEmpty() ? QStringLiteral("Utility") : cats, repo, tag)
                            .toUtf8());
                return true;
            };
            job.steps << menu;
            enqueue(job);
        });
        return;
    }
    if (kind == QLatin1String("android")) {
        // F-Droid's suggested version of the app, then Waydroid installs the file.
        getJson(QStringLiteral("https://f-droid.org/api/v1/packages/") + id, 1, [this, id, name, done, failed](const QJsonDocument &d, bool ok) {
            const qint64 code = d.object().value(QStringLiteral("suggestedVersionCode")).toVariant().toLongLong();
            if (!ok || code <= 0)
                return emit toast(tr("F-Droid doesn't know %1.").arg(name), QStringLiteral("warn"));
            const QString apk = QStandardPaths::writableLocation(QStandardPaths::CacheLocation) + QLatin1Char('/') + id + QStringLiteral(".apk");
            Job job{QStringLiteral("android"), tr("Installing %1").arg(name), done, failed, {}};
            Step get;
            get.title = tr("Downloading %1 from F-Droid").arg(name);
            get.url = QStringLiteral("https://f-droid.org/repo/%1_%2.apk").arg(id).arg(code);
            get.path = apk;
            Step add;
            add.title = tr("Installing %1 in Android").arg(name);
            add.program = QStringLiteral("waydroid");
            add.args = {QStringLiteral("app"), QStringLiteral("install"), apk};
            job.steps << get << add;
            enqueue(job);
        });
        return;
    }
    const QList<Step> steps = installSteps(it);
    if (steps.isEmpty())
        return emit toast(tr("%1 can't be installed from here.").arg(name), QStringLiteral("warn"));
    enqueue(Job{kind, tr("Installing %1").arg(name), done, failed, steps});
}

void AppCenterEngine::remove(const QVariantMap &it)
{
    const QString kind = it.value("kind").toString();
    const QString name = it.value("name").toString();
    if (m_demo) {
        m_installedIds[kind].remove(it.value("id").toString());
        return emit toast(tr("%1 is removed.").arg(name), QStringLiteral("ok"));
    }
    if (kind == QLatin1String("windows"))
        return tool(kind, QStringLiteral("bottles"));
    const QList<Step> steps = removeSteps(it);
    if (steps.isEmpty())
        return emit toast(tr("%1 can't be removed from here.").arg(name), QStringLiteral("warn"));
    enqueue(Job{kind, tr("Removing %1").arg(name), tr("%1 is removed.").arg(name), tr("%1 could not be removed.").arg(name), steps});
}

void AppCenterEngine::open(const QVariantMap &it)
{
    const QString kind = it.value("kind").toString();
    const QString id = it.value("id").toString();
    bool started = false;
    if (m_demo)
        started = false;
    else if (kind == QLatin1String("flatpak"))
        started = QProcess::startDetached(QStringLiteral("flatpak"), {QStringLiteral("run"), id});
    else if (kind == QLatin1String("snap"))
        started = QProcess::startDetached(QStringLiteral("snap"), {QStringLiteral("run"), id});
    else if (kind == QLatin1String("appimage"))
        started = QProcess::startDetached(it.value("file").toString(), {});
    else if (kind == QLatin1String("android"))
        started = QProcess::startDetached(QStringLiteral("waydroid"), {QStringLiteral("app"), QStringLiteral("launch"), id});
    else if (kind == QLatin1String("windows"))
        started = QProcess::startDetached(QStringLiteral("flatpak"), {QStringLiteral("run"), QStringLiteral("--command=bottles-cli"), kBottles, QStringLiteral("run"),
                                                                      QStringLiteral("-b"), kBottle, QStringLiteral("-p"), id});
    else if (kind == QLatin1String("containers"))
        started = id == QLatin1String("junest")
            ? QProcess::startDetached(QStringLiteral("konsole"), {QStringLiteral("-e"), home() + QStringLiteral("/.local/share/junest/bin/junest")})
            : QProcess::startDetached(QStringLiteral("konsole"), {QStringLiteral("-e"), program(kind), QStringLiteral("enter"), id});
    else
        // Command-line programs open in a terminal.
        started = QProcess::startDetached(QStringLiteral("konsole"), {QStringLiteral("--noclose"), QStringLiteral("-e"), QStringLiteral("sh"), QStringLiteral("-c"),
                                                                       QStringLiteral("echo \"$0 --help\"; \"$0\" --help"), id});
    if (!started)
        emit toast(tr("Opens %1.").arg(it.value("name").toString()), QStringLiteral("info"));
}

// ───────────────────────────────────────────────────────────────────────── setting a kind up

QList<AppCenterEngine::Step> AppCenterEngine::setUpSteps(const QString &kind) const
{
    auto run = [](const QString &title, const QString &prog, const QStringList &args) { Step s; s.title = title; s.program = prog; s.args = args; return s; };
    auto get = [](const QString &title, const QString &url, const QString &path, const QString &sumUrl) {
        Step s; s.title = title; s.url = url; s.path = path; s.sha256Url = sumUrl; return s;
    };
    const QString cache = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
    if (kind == QLatin1String("android")) {
        Step s;
        s.title = tr("Downloading Android (about 1 GB)");
        s.unit = QStringLiteral("asimaos-android-setup.service");
        return {s};
    }
    if (kind == QLatin1String("homebrew")) {
        Step s;
        s.title = tr("Making Homebrew's folder (/home/linuxbrew)");
        s.unit = QStringLiteral("asimaos-homebrew-setup@%1.service").arg(qEnvironmentVariable("USER"));
        return {s, run(tr("Getting Homebrew"), kBrew, {QStringLiteral("update"), QStringLiteral("--force"), QStringLiteral("--quiet")})};
    }
    if (kind == QLatin1String("windows"))
        return {run(tr("Installing Bottles from Flathub"), QStringLiteral("flatpak"),
                    {QStringLiteral("install"), QStringLiteral("--system"), QStringLiteral("-y"), QStringLiteral("--noninteractive"), QStringLiteral("flathub"), kBottles}),
                run(tr("Making a bottle for Windows apps"), QStringLiteral("flatpak"),
                    {QStringLiteral("run"), QStringLiteral("--command=bottles-cli"), kBottles, QStringLiteral("new"), QStringLiteral("--bottle-name"), kBottle,
                     QStringLiteral("--environment"), QStringLiteral("application")})};
    if (kind == QLatin1String("python"))
        // pipx in a Python environment of its own (the system's Python stays as it is, PEP 668).
        return {run(tr("Making pipx's own Python environment"), QStringLiteral("python3"), {QStringLiteral("-m"), QStringLiteral("venv"), tools() + QStringLiteral("/pipx-venv")}),
                run(tr("Installing pipx"), tools() + QStringLiteral("/pipx-venv/bin/pip"), {QStringLiteral("install"), QStringLiteral("--quiet"), QStringLiteral("pipx")}),
                run(tr("Adding pipx to your programs"), QStringLiteral("ln"), {QStringLiteral("-sf"), tools() + QStringLiteral("/pipx-venv/bin/pipx"), home() + QStringLiteral("/.local/bin/pipx")})};
    if (kind == QLatin1String("rust"))
        return {get(tr("Downloading rustup"), QStringLiteral("https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init"),
                    cache + QStringLiteral("/rustup-init"), QStringLiteral("https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init.sha256")),
                run(tr("Installing Rust (rustup, cargo)"), QStringLiteral("sh"), {QStringLiteral("-c"), QStringLiteral("chmod +x \"$0\" && \"$0\" -y --no-modify-path"), cache + QStringLiteral("/rustup-init")})};
    if (kind == QLatin1String("conda"))
        return {get(tr("Downloading Miniforge"), QStringLiteral("https://github.com/conda-forge/miniforge/releases/latest/download/Miniforge3-Linux-x86_64.sh"),
                    cache + QStringLiteral("/Miniforge3.sh"), QStringLiteral("https://github.com/conda-forge/miniforge/releases/latest/download/Miniforge3-Linux-x86_64.sh.sha256")),
                run(tr("Installing Conda (Miniforge)"), QStringLiteral("bash"), {cache + QStringLiteral("/Miniforge3.sh"), QStringLiteral("-b"), QStringLiteral("-p"), home() + QStringLiteral("/miniforge3")})};
    return {};
}

void AppCenterEngine::setUp(const QString &kind)
{
    const QString title = rowFor(kind).value("title").toString();
    if (m_demo)
        return emit toast(tr("Setting up %1…").arg(title), QStringLiteral("info"));
    const QString done = tr("%1 is set up.").arg(title), failed = tr("%1 could not be set up.").arg(title);
    if (kind == QLatin1String("node") || kind == QLatin1String("go")) {
        // The newest Node.js LTS or Go release, from its own site, checked against its checksum.
        const bool node = kind == QLatin1String("node");
        getJson(node ? QStringLiteral("https://nodejs.org/dist/index.json") : QStringLiteral("https://go.dev/dl/?mode=json"), 1,
                [this, node, title, done, failed](const QJsonDocument &d, bool ok) {
            QString url, sum, sumUrl;
            if (node) {
                for (const QJsonValue &v : d.array())
                    if (v.toObject().value(QStringLiteral("lts")).isString()) {
                        const QString ver = v.toObject().value(QStringLiteral("version")).toString();
                        url = QStringLiteral("https://nodejs.org/dist/%1/node-%1-linux-x64.tar.xz").arg(ver);
                        sumUrl = QStringLiteral("https://nodejs.org/dist/%1/SHASUMS256.txt").arg(ver);
                        break;
                    }
            } else {
                const QJsonArray releases = d.array();
                for (const QJsonValue &f : releases.isEmpty() ? QJsonArray() : releases.first().toObject().value(QStringLiteral("files")).toArray()) {
                    const QJsonObject o = f.toObject();
                    if (o.value(QStringLiteral("os")).toString() == QLatin1String("linux") && o.value(QStringLiteral("arch")).toString() == QLatin1String("amd64")
                        && o.value(QStringLiteral("kind")).toString() == QLatin1String("archive")) {
                        url = QStringLiteral("https://go.dev/dl/") + o.value(QStringLiteral("filename")).toString();
                        sum = o.value(QStringLiteral("sha256")).toString();
                    }
                }
            }
            if (!ok || url.isEmpty())
                return emit toast(tr("%1's download can't be found right now.").arg(title), QStringLiteral("warn"));
            const QString archive = QStandardPaths::writableLocation(QStandardPaths::CacheLocation) + QLatin1Char('/') + url.section(QLatin1Char('/'), -1);
            const QString dest = tools() + (node ? QStringLiteral("/node") : QStringLiteral("/go"));
            Step get;
            get.title = tr("Downloading %1").arg(url.section(QLatin1Char('/'), -1));
            get.url = url;
            get.path = archive;
            get.sha256 = sum;
            get.sha256Url = sumUrl;
            Step unpack;
            unpack.title = tr("Unpacking %1").arg(title);
            unpack.program = QStringLiteral("sh");
            unpack.args = {QStringLiteral("-c"), QStringLiteral("rm -rf \"$1\" && mkdir -p \"$1\" && tar -xf \"$0\" -C \"$1\" --strip-components=1 && rm -f \"$0\""), archive, dest};
            enqueue(Job{node ? QStringLiteral("node") : QStringLiteral("go"), tr("Setting up %1").arg(title), done, failed, {get, unpack}});
        });
        return;
    }
    if (kind == QLatin1String("containers")) {
        // JuNest, next to Distrobox and Toolbx: Arch Linux in the home folder, no root needed.
        Step clone;
        clone.title = tr("Getting JuNest");
        clone.program = QStringLiteral("git");
        clone.args = {QStringLiteral("clone"), QStringLiteral("--depth=1"), QStringLiteral("https://github.com/fsquillace/junest.git"), home() + QStringLiteral("/.local/share/junest")};
        Step setup;
        setup.title = tr("Setting JuNest up (downloads Arch Linux)");
        setup.program = home() + QStringLiteral("/.local/share/junest/bin/junest");
        setup.args = {QStringLiteral("setup")};
        enqueue(Job{kind, tr("Setting up JuNest"), tr("JuNest is set up."), tr("JuNest could not be set up."), {clone, setup}});
        return;
    }
    const QList<Step> steps = setUpSteps(kind);
    if (steps.isEmpty())
        return emit toast(tr("%1 can't be set up here.").arg(title), QStringLiteral("warn"));
    enqueue(Job{kind, tr("Setting up %1").arg(title), done, failed, steps});
}

void AppCenterEngine::tool(const QString &kind, const QString &t)
{
    if (m_demo)
        return emit toast(tr("Done."), QStringLiteral("ok"));
    auto run = [&](const QString &title, const QString &prog, const QStringList &args) { Step s; s.title = title; s.program = prog; s.args = args; return s; };
    if (kind == QLatin1String("flatpak") && t == QLatin1String("unused")) {
        // Moved from Asima Maintenance (D56): runtimes no app needs any more.
        enqueue(Job{kind, tr("Removing unused runtimes"), tr("Unused Flatpak runtimes removed."), tr("Cleaning up Flatpak failed."),
                    {run(tr("Removing unused runtimes"), QStringLiteral("flatpak"), {QStringLiteral("uninstall"), QStringLiteral("--unused"), QStringLiteral("-y"),
                                                                                    QStringLiteral("--noninteractive")})}});
    } else if (kind == QLatin1String("flatpak") && t == QLatin1String("repair")) {
        // Your own installation as you; the one for everyone through Asima Maintenance's service.
        enqueue(Job{kind, tr("Repairing your Flatpak apps"), tr("Your Flatpak apps are checked and repaired."), tr("Repairing failed."),
                    {run(tr("Checking your own apps"), QStringLiteral("flatpak"), {QStringLiteral("repair"), QStringLiteral("--user")})}});
        if (!m_maintenance) {
            m_maintenance = new ServiceClient(QStringLiteral("Maintenance1"), this);
            connect(m_maintenance, &ServiceClient::failed, this, [this](const QString &m) { emit toast(m, QStringLiteral("error")); });
        }
        if (m_maintenance->available()) {
            emit toast(tr("Repairing the apps installed for everyone… this can take a few minutes."), QStringLiteral("info"));
            m_maintenance->call(QStringLiteral("RepairFlatpak"), {}, [this](const QDBusMessage &) {
                emit toast(tr("The apps installed for everyone are checked and repaired."), QStringLiteral("ok"));
                refreshInstalled();
            });
        }
    } else if (kind == QLatin1String("appimage") && t == QLatin1String("cleanup")) {
        // Menu entries and icons whose AppImage is gone.
        int n = 0;
        for (const QVariant &v : installedItems(kind)) {
            const QVariantMap m = v.toMap();
            if (!m.value("missing").toBool())
                continue;
            QFile::remove(m.value("entry").toString());
            QFile::remove(QUrl(m.value("icon").toString()).toLocalFile());
            ++n;
        }
        emit toast(n ? tr("%n menu entries without their AppImage removed.", "", n) : tr("Nothing to clean up."), QStringLiteral("ok"));
        refreshInstalled();
    } else if (kind == QLatin1String("nix") && t == QLatin1String("garbage")) {
        enqueue(Job{kind, tr("Freeing up space"), tr("Old program versions are removed."), tr("Cleaning up failed."),
                    {run(tr("Removing program versions older than 30 days"), QStringLiteral("nix-collect-garbage"),
                         {QStringLiteral("--delete-older-than"), QStringLiteral("30d")})}});
    } else if (kind == QLatin1String("android") && t == QLatin1String("launch")) {
        QProcess::startDetached(QStringLiteral("waydroid"), {QStringLiteral("show-full-ui")});
    } else if (kind == QLatin1String("android") && t == QLatin1String("stop")) {
        QProcess::startDetached(QStringLiteral("waydroid"), {QStringLiteral("session"), QStringLiteral("stop")});
        emit toast(tr("Android is stopped. It starts again when you open an Android app."), QStringLiteral("ok"));
    } else if (kind == QLatin1String("android") && t == QLatin1String("fdroid")) {
        install(appItem(QStringLiteral("android"), QStringLiteral("org.fdroid.fdroid"), QStringLiteral("F-Droid"), QString()));
    } else if (kind == QLatin1String("android") && t == QLatin1String("removeandroid")) {
        QProcess::execute(QStringLiteral("waydroid"), {QStringLiteral("session"), QStringLiteral("stop")});
        Step s;
        s.title = tr("Removing Android and its apps");
        s.unit = QStringLiteral("asimaos-android-remove.service");
        enqueue(Job{kind, s.title, tr("Android and its apps are removed."), tr("Android could not be removed."), {s}});
    } else if (kind == QLatin1String("windows") && t == QLatin1String("bottles")) {
        QProcess::startDetached(QStringLiteral("flatpak"), {QStringLiteral("run"), kBottles});
    }
}

// ───────────────────────────────────────────────────────────────────────── updates

void AppCenterEngine::countUpdates(const QString &kind, std::function<void(int, QStringList)> done)
{
    if (!ready(kind))
        return done(0, {});
    if (kind == QLatin1String("flatpak")) {
        runAsync(this, QStringLiteral("flatpak"), {QStringLiteral("remote-ls"), QStringLiteral("--updates"), QStringLiteral("--app"), QStringLiteral("--columns=name")},
                 [done](const QString &out, bool) { const QStringList n = out.split(QLatin1Char('\n'), Qt::SkipEmptyParts); done(int(n.size()), n); });
    } else if (kind == QLatin1String("snap")) {
        m_snapd->request(QStringLiteral("GET"), QStringLiteral("/v2/find?select=refresh"), {}, [done](const QJsonObject &r) {
            QStringList n;
            for (const QJsonValue &v : r.value(QStringLiteral("result")).toArray())
                n << v.toObject().value(QStringLiteral("title")).toString();
            done(int(n.size()), n);
        });
    } else if (kind == QLatin1String("homebrew")) {
        runAsync(this, program(kind), {QStringLiteral("outdated"), QStringLiteral("--quiet")},
                 [done](const QString &out, bool) { const QStringList n = out.split(QLatin1Char('\n'), Qt::SkipEmptyParts); done(int(n.size()), n); });
    } else if (kind == QLatin1String("node")) {
        runAsync(this, program(kind), {QStringLiteral("outdated"), QStringLiteral("-g"), QStringLiteral("--json")}, [done](const QString &out, bool) {
            const QStringList n = QJsonDocument::fromJson(out.toUtf8()).object().keys();
            done(int(n.size()), n);
        });
    } else if (kind == QLatin1String("appimage")) {
        // Each AppImage from GitHub against its newest release.
        QVariantList tracked;
        for (const QVariant &v : installedItems(kind))
            if (!v.toMap().value("repo").toString().isEmpty())
                tracked << v;
        if (tracked.isEmpty())
            return done(0, {});
        auto names = std::make_shared<QStringList>();
        auto left = std::make_shared<int>(int(tracked.size()));
        for (const QVariant &v : tracked) {
            const QVariantMap m = v.toMap();
            getJson(QStringLiteral("https://api.github.com/repos/") + m.value("repo").toString() + QStringLiteral("/releases/latest"), 6,
                    [m, names, left, done](const QJsonDocument &d, bool ok) {
                const QString tag = d.object().value(QStringLiteral("tag_name")).toString();
                if (ok && !tag.isEmpty() && tag != m.value("version").toString())
                    *names << m.value("name").toString();
                if (--*left == 0)
                    done(int(names->size()), *names);
            });
        }
    } else {
        done(0, {});
    }
}

void AppCenterEngine::checkUpdates()
{
    if (m_demo)
        return;
    m_updates = item({{"checking", true}, {"total", 0}, {"kinds", QVariantList{}}});
    emit updatesChanged();
    const QStringList counted{QStringLiteral("flatpak"), QStringLiteral("snap"), QStringLiteral("appimage"), QStringLiteral("homebrew"), QStringLiteral("node")};
    auto left = std::make_shared<int>(int(counted.size()));
    for (const QString &k : counted) {
        countUpdates(k, [this, k, left](int n, const QStringList &names) {
            QVariantList kinds = m_updates.value("kinds").toList();
            if (n > 0)
                kinds << item({{"kind", k}, {"title", rowFor(k).value("title")}, {"count", n}, {"names", names}});
            m_updates.insert("kinds", kinds);
            m_updates.insert("total", m_updates.value("total").toInt() + n);
            if (--*left == 0)
                m_updates.insert("checking", false);
            emit updatesChanged();
        });
    }
}

void AppCenterEngine::updateKind(const QString &kind)
{
    if (m_demo)
        return emit toast(tr("Up to date."), QStringLiteral("ok"));
    const QString title = rowFor(kind).value("title").toString();
    if (kind == QLatin1String("appimage")) {
        // An AppImage is updated by installing its newest release again.
        for (const QVariant &v : installedItems(kind))
            if (!v.toMap().value("repo").toString().isEmpty())
                install(v.toMap());
        return;
    }
    const QList<Step> steps = updateSteps(kind);
    if (!steps.isEmpty())
        enqueue(Job{kind, tr("Updating %1").arg(title), tr("%1 is up to date.").arg(title), tr("Updating %1 failed.").arg(title), steps});
}

void AppCenterEngine::updateAll()
{
    for (const QString &k : kAllKinds)
        if (ready(k) && k != QLatin1String("android") && k != QLatin1String("windows"))
            updateKind(k);
}

void AppCenterEngine::checkUpdatesQuietly(std::function<void(int)> done)
{
    checkUpdates();
    auto conn = std::make_shared<QMetaObject::Connection>();
    *conn = connect(this, &AppCenterEngine::updatesChanged, this, [this, conn, done] {
        if (m_updates.value("checking").toBool())
            return;
        disconnect(*conn);
        done(m_updates.value("total").toInt());
    });
}

// ───────────────────────────────────────────────────────────────────────── app groups (the signed feed)

void AppCenterEngine::loadGroups()
{
    const QByteArray key = feedKey();
    if (key.isEmpty())
        return;
    // Fetched fresh, with its signature right after, so the two always belong together.
    getJson(kFeed, 0, [this, key](const QJsonDocument &, bool) {
        // The feed is used only with a valid signature by the feed key (D39): its bytes as fetched.
        QFile feed(cacheFile(kFeed));
        if (!feed.open(QIODevice::ReadOnly))
            return;
        const QByteArray data = feed.readAll();
        auto *reply = m_net.get(QNetworkRequest(QUrl(kFeed + QStringLiteral(".sig"))));
        connect(reply, &QNetworkReply::finished, this, [this, reply, data, key] {
            reply->deleteLater();
            const QByteArray sig = reply->readAll();
            if (!verifyEd25519(data, sig, key))
                return;
            m_groups.clear();
            for (const QJsonValue &g : QJsonDocument::fromJson(data).object().value(QStringLiteral("groups")).toArray())
                m_groups << g.toObject().toVariantMap();
            emit groupsChanged();
        });
    });
}

void AppCenterEngine::installGroup(const QString &id)
{
    for (const QVariant &g : m_groups) {
        const QVariantMap group = g.toMap();
        if (group.value("id").toString() != id)
            continue;
        for (const QVariant &a : group.value("apps").toList()) {
            const QVariantMap app = a.toMap();
            if (!m_installedIds.value(app.value("kind").toString()).contains(app.value("id").toString()))
                install(appItem(app.value("kind").toString(), app.value("id").toString(), app.value("name").toString(), QString()));
        }
    }
}

// ───────────────────────────────────────────────────────────────────────── app files

void AppCenterEngine::openFile(const QString &path)
{
    const QFileInfo f(QUrl(path).isLocalFile() ? QUrl(path).toLocalFile() : path);
    const QString s = f.suffix().toLower();
    QString kind, what, detail;
    if (s == QLatin1String("flatpakref")) {
        kind = QStringLiteral("flatpak"), what = tr("A Flatpak app from its own site");
        detail = tr("Installs the app this file points to, sandboxed, for everyone on this computer.");
    } else if (s == QLatin1String("flatpak")) {
        kind = QStringLiteral("flatpak"), what = tr("A Flatpak app in one file");
        detail = tr("Installs the app from this file, sandboxed. It updates from its own site, if it names one.");
    } else if (s == QLatin1String("snap")) {
        kind = QStringLiteral("snap"), what = tr("A snap");
        detail = tr("Installed from this file, it isn't checked by the Snap Store and doesn't update by itself. Install only snaps you trust.");
    } else if (s == QLatin1String("appimage")) {
        kind = QStringLiteral("appimage"), what = tr("An AppImage");
        detail = tr("Moves to your Applications folder and appears in the app menu. It runs without a sandbox: only open AppImages you trust.");
    } else if (s == QLatin1String("apk")) {
        kind = QStringLiteral("android"), what = tr("An Android app");
        detail = ready(QStringLiteral("android")) ? tr("Installs it in Android (Waydroid); it then appears in your app menu.")
                                                 : tr("Android needs to be set up first (Installed → Android).");
    } else if (s == QLatin1String("exe") || s == QLatin1String("msi")) {
        kind = QStringLiteral("windows"), what = tr("A Windows program");
        detail = ready(QStringLiteral("windows")) ? tr("Runs it in the Windows apps bottle (Bottles, with Wine). Not every Windows program works.")
                                                 : tr("Bottles needs to be set up first: it runs Windows programs with Wine.");
    }
    m_file = item({{"path", f.absoluteFilePath()}, {"name", f.fileName()}, {"kind", kind}, {"title", what}, {"detail", detail},
                   {"size", double(f.size())}, {"ok", f.isFile() && !kind.isEmpty()},
                   {"problem", !f.isFile() ? tr("The file can't be found.") : kind.isEmpty() ? tr("This kind of file isn't an app Asima App Center knows.") : QString()}});
    emit fileChanged();
}

void AppCenterEngine::installFile()
{
    const QString path = m_file.value("path").toString(), kind = m_file.value("kind").toString(), name = m_file.value("name").toString();
    if (!m_file.value("ok").toBool())
        return;
    if (m_demo)
        return emit toast(tr("%1 is installed.").arg(name), QStringLiteral("ok"));
    const QString suffix = QFileInfo(path).suffix().toLower();
    auto run = [&](const QString &prog, const QStringList &args) { Step s; s.title = tr("Installing %1").arg(name); s.program = prog; s.args = args; return s; };
    QList<Step> steps;
    if (suffix == QLatin1String("flatpakref"))
        steps << run(QStringLiteral("flatpak"), {QStringLiteral("install"), QStringLiteral("--system"), QStringLiteral("-y"), QStringLiteral("--noninteractive"), QStringLiteral("--from"), path});
    else if (suffix == QLatin1String("flatpak"))
        steps << run(QStringLiteral("flatpak"), {QStringLiteral("install"), QStringLiteral("--system"), QStringLiteral("-y"), QStringLiteral("--noninteractive"), QStringLiteral("--bundle"), path});
    else if (suffix == QLatin1String("snap"))
        steps << run(QStringLiteral("pkexec"), {QStringLiteral("snap"), QStringLiteral("install"), QStringLiteral("--dangerous"), path});
    else if (suffix == QLatin1String("apk"))
        steps << run(QStringLiteral("waydroid"), {QStringLiteral("app"), QStringLiteral("install"), path});
    else if (kind == QLatin1String("windows"))
        steps << run(QStringLiteral("flatpak"), {QStringLiteral("run"), QStringLiteral("--file-forwarding"), QStringLiteral("--command=bottles-cli"), kBottles,
                                                 QStringLiteral("run"), QStringLiteral("-b"), kBottle, QStringLiteral("-e"), QStringLiteral("@@"), path, QStringLiteral("@@")});
    else if (kind == QLatin1String("appimage")) {
        const QString s = slug(QFileInfo(path).completeBaseName());
        const QString dest = appsDir() + QLatin1Char('/') + s + QStringLiteral(".AppImage");
        Step move;
        move.title = tr("Adding %1 to your apps").arg(name);
        move.action = [path, dest, s, name] {
            QDir().mkpath(appsDir());
            if (path != dest && !QFile::rename(path, dest) && !QFile::copy(path, dest))
                return false;
            QFile::setPermissions(dest, QFile::permissions(dest) | QFileDevice::ExeOwner | QFileDevice::ExeUser);
            QDir().mkpath(menuDir());
            QFile f(menuDir() + QStringLiteral("/asimaos-appimage-") + s + QStringLiteral(".desktop"));
            if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate))
                return false;
            const QString title = QFileInfo(name).completeBaseName();
            f.write(QStringLiteral("[Desktop Entry]\nType=Application\nName=%1\nExec=\"%2\" %U\nIcon=application-x-executable\nCategories=Utility;\nTerminal=false\n"
                                   "X-AsimaOS-AppImage=%2\nX-AsimaOS-Name=%1\n").arg(title, dest).toUtf8());
            return true;
        };
        steps << move;
    }
    enqueue(Job{kind, tr("Installing %1").arg(name), tr("%1 is installed.").arg(name), tr("%1 could not be installed.").arg(name), steps});
}

void AppCenterEngine::openUrl(const QString &url)
{
    QDesktopServices::openUrl(QUrl(url));
}

void AppCenterEngine::openUpdater()
{
    if (m_demo || !QProcess::startDetached(QStringLiteral("asimaos-updater"), {}))
        emit toast(tr("Opens Asima Updater."), QStringLiteral("info"));
}

// ───────────────────────────────────────────────────────────────────────── caches in memory

namespace {
// Game launchers and Wine keep large shader and download caches: they stay on the disk.
const QStringList kCachesOnDisk{QStringLiteral("com.valvesoftware.Steam"), QStringLiteral("com.heroicgameslauncher.hgl"),
                                QStringLiteral("net.lutris.Lutris"), QStringLiteral("com.usebottles.bottles"),
                                QStringLiteral("org.prismlauncher.PrismLauncher")};

QString settingsFile()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation) + QStringLiteral("/asimaos/appcenter.conf");
}

QString appCache(const QString &id)
{
    return QStringLiteral("/run/asimaos/appcache/%1/%2").arg(::getuid()).arg(id);
}

// Whether this person's Flatpak override for the app already points its cache to memory.
bool cacheOverridden(const QString &id)
{
    QFile f(share() + QStringLiteral("/flatpak/overrides/") + id);
    return f.open(QIODevice::ReadOnly) && f.readAll().contains(("XDG_CACHE_HOME=" + appCache(id)).toUtf8());
}
} // namespace

bool AppCenterEngine::cachesInMemory() const
{
    if (m_demo)
        return m_demoCaches;
    return QSettings(settingsFile(), QSettings::IniFormat).value(QStringLiteral("Flatpak/CachesInMemory"), false).toBool();
}

void AppCenterEngine::setCachesInMemory(bool on)
{
    if (m_demo) {
        m_demoCaches = on;
        emit installedChanged();
        return;
    }
    QSettings(settingsFile(), QSettings::IniFormat).setValue(QStringLiteral("Flatpak/CachesInMemory"), on);
    applyCaches(installedItems(QStringLiteral("flatpak")), on);
    emit installedChanged();
    emit toast(on ? tr("Apps keep their caches in memory from their next start.") : tr("Apps keep their caches on the disk again from their next start."),
               QStringLiteral("ok"));
}

// A Flatpak override per app (this person's own): its XDG_CACHE_HOME in the memory place, which
// the sandbox may use (created when the app starts). Turned off: the override's two entries go.
void AppCenterEngine::applyCaches(const QVariantList &apps, bool on)
{
    if (!QFileInfo(QStringLiteral("/run/asimaos/appcache/%1").arg(::getuid())).isDir() && on)
        return;   // not signed in through systemd: no place in memory
    for (const QVariant &v : apps) {
        const QString id = v.toMap().value("id").toString();
        if (kCachesOnDisk.contains(id) || !validName(id) || (on && cacheOverridden(id)))
            continue;
        const QStringList args = on ? QStringList{QStringLiteral("override"), QStringLiteral("--user"),
                                                  QStringLiteral("--env=XDG_CACHE_HOME=") + appCache(id),
                                                  QStringLiteral("--filesystem=") + appCache(id) + QStringLiteral(":create"), id}
                                    : QStringList{QStringLiteral("override"), QStringLiteral("--user"), QStringLiteral("--unset-env=XDG_CACHE_HOME"),
                                                  QStringLiteral("--nofilesystem=") + appCache(id), id};
        runAsync(this, QStringLiteral("flatpak"), args, [](const QString &, bool) {});
    }
}

// ───────────────────────────────────────────────────────────────────────── the job runner

QVariantMap AppCenterEngine::job() const
{
    const bool busy = !m_jobs.isEmpty();
    return item({{"busy", busy}, {"title", busy ? m_jobs.first().title : QString()}, {"detail", m_detail},
                 {"waiting", busy ? int(m_jobs.size()) - 1 : 0}});
}

void AppCenterEngine::enqueue(const Job &job)
{
    m_jobs << job;
    emit jobChanged();
    if (m_jobs.size() == 1)
        runNext();
}

void AppCenterEngine::runNext()
{
    if (m_jobs.isEmpty())
        return;
    m_step = 0;
    runStep();
}

void AppCenterEngine::runStep()
{
    Job &job = m_jobs.first();
    if (m_step >= job.steps.size())
        return stepDone(true);
    const Step &s = job.steps.at(m_step);
    m_detail = s.title;
    emit jobChanged();
    if (s.action) {
        const bool ok = s.action();
        QTimer::singleShot(0, this, [this, ok] { ok ? (++m_step, runStep()) : stepDone(false); });
        return;
    }
    if (!s.snapMethod.isEmpty()) {
        m_snapd->change(s.snapMethod, s.snapPath, s.snapBody, [this](bool ok, const QString &message) {
            ok ? (++m_step, runStep()) : stepDone(false, message);
        });
        return;
    }
    if (!s.unit.isEmpty()) {
        // A system unit: polkit asks for the administrator's password, then it runs to its end.
        QDBusMessage m = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.systemd1"), QStringLiteral("/org/freedesktop/systemd1"),
                                                        QStringLiteral("org.freedesktop.systemd1.Manager"), QStringLiteral("StartUnit"));
        m << s.unit << QStringLiteral("replace");
        m.setInteractiveAuthorizationAllowed(true);
        auto *w = new QDBusPendingCallWatcher(QDBusConnection::systemBus().asyncCall(m, 10 * 60 * 1000), this);
        const QString unit = s.unit;
        connect(w, &QDBusPendingCallWatcher::finished, this, [this, w, unit] {
            const QDBusPendingReply<QDBusObjectPath> r = *w;
            w->deleteLater();
            if (r.isError())
                return stepDone(false, r.error().message());
            // Waits for the unit to finish (it is a oneshot).
            auto *poll = new QTimer(this);
            poll->setInterval(2000);
            connect(poll, &QTimer::timeout, this, [this, poll, unit] {
                const QString state = Util::run(QStringLiteral("systemctl"), {QStringLiteral("show"), QStringLiteral("-P"), QStringLiteral("ActiveState"), unit}).trimmed();
                if (state == QLatin1String("activating") || state == QLatin1String("deactivating"))
                    return;
                poll->deleteLater();
                state == QLatin1String("failed") ? stepDone(false, tr("%1 failed.").arg(unit)) : (++m_step, runStep());
            });
            poll->start();
        });
        return;
    }
    if (!s.url.isEmpty()) {
        QDir().mkpath(QFileInfo(s.path).absolutePath());
        QNetworkRequest req{QUrl(s.url)};
        req.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
        req.setHeader(QNetworkRequest::UserAgentHeader, QStringLiteral("AsimaAppCenter/0.1"));
        auto *reply = m_net.get(req);
        auto file = std::make_shared<QFile>(s.path + QStringLiteral(".part"));
        if (!file->open(QIODevice::WriteOnly | QIODevice::Truncate))
            return stepDone(false, tr("%1 can't be written.").arg(s.path));
        connect(reply, &QNetworkReply::readyRead, this, [reply, file] { file->write(reply->readAll()); });
        connect(reply, &QNetworkReply::downloadProgress, this, [this, title = s.title](qint64 got, qint64 total) {
            m_detail = total > 0 ? QStringLiteral("%1 · %2 %").arg(title).arg(100 * got / total) : title;
            emit jobChanged();
        });
        const QString path = s.path, sum = s.sha256, sumUrl = s.sha256Url;
        connect(reply, &QNetworkReply::finished, this, [this, reply, file, path, sum, sumUrl] {
            file->write(reply->readAll());
            file->close();
            reply->deleteLater();
            if (reply->error() != QNetworkReply::NoError) {
                QFile::remove(file->fileName());
                return stepDone(false, reply->errorString());
            }
            // The checksum, when the download has one (given, or in a checksum file).
            auto finish = [this, file, path](const QString &expected) {
                if (!expected.isEmpty()) {
                    QFile f(file->fileName());
                    QCryptographicHash h(QCryptographicHash::Sha256);
                    if (!f.open(QIODevice::ReadOnly) || !h.addData(&f) || h.result().toHex() != expected.toLatin1().toLower()) {
                        QFile::remove(file->fileName());
                        return stepDone(false, tr("The download doesn't match its checksum."));
                    }
                }
                QFile::remove(path);
                QFile::rename(file->fileName(), path) ? (++m_step, runStep()) : stepDone(false, tr("%1 can't be written.").arg(path));
            };
            if (sumUrl.isEmpty())
                return finish(sum);
            auto *sums = m_net.get(QNetworkRequest(QUrl(sumUrl)));
            connect(sums, &QNetworkReply::finished, this, [sums, finish, path] {
                sums->deleteLater();
                // "<hash>  <name>" lines: the one for this file, or the only one.
                const QString name = QFileInfo(path).fileName();
                QString hash;
                for (const QString &l : QString::fromUtf8(sums->readAll()).split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
                    const QString h = l.section(QLatin1Char(' '), 0, 0).trimmed();
                    if (hash.isEmpty() || l.contains(name))
                        hash = h;
                }
                hash.isEmpty() ? finish(QStringLiteral("missing")) : finish(hash);
            });
        });
        return;
    }
    m_process = new QProcess(this);
    m_process->setProcessEnvironment(toolEnv());
    m_process->setProcessChannelMode(QProcess::MergedChannels);
    QProcess *p = m_process;
    connect(p, &QProcess::readyRead, this, [this, p] {
        const QStringList lines = QString::fromUtf8(p->readAll()).split(QLatin1Char('\n'), Qt::SkipEmptyParts);
        const QString line = lines.isEmpty() ? QString() : lines.last().trimmed();
        if (!line.isEmpty()) {
            m_detail = line.left(160);
            emit jobChanged();
        }
    });
    connect(p, &QProcess::finished, this, [this, p](int code, QProcess::ExitStatus status) {
        p->deleteLater();
        status == QProcess::NormalExit && code == 0 ? (++m_step, runStep()) : stepDone(false, m_detail);
    });
    connect(p, &QProcess::errorOccurred, this, [this, p](QProcess::ProcessError e) {
        if (e == QProcess::FailedToStart) {
            p->deleteLater();
            stepDone(false, tr("%1 could not be started.").arg(p->program()));
        }
    });
    p->start(s.program, s.args);
}

void AppCenterEngine::stepDone(bool ok, const QString &detail)
{
    if (m_jobs.isEmpty())
        return;
    const Job job = m_jobs.takeFirst();
    m_detail.clear();
    emit toast(ok ? job.done : (detail.isEmpty() ? job.failed : job.failed + QLatin1Char(' ') + detail), ok ? QStringLiteral("ok") : QStringLiteral("error"));
    emit jobChanged();
    refreshInstalled();
    if (ok && job.title.startsWith(tr("Updating")))
        checkUpdates();
    if (!m_jobs.isEmpty())
        runNext();
}

// ───────────────────────────────────────────────────────────────────────── HTTPS with a cache

QString AppCenterEngine::cacheFile(const QString &url) const
{
    return QStandardPaths::writableLocation(QStandardPaths::CacheLocation) + QStringLiteral("/http/")
           + QString::fromLatin1(QCryptographicHash::hash(url.toUtf8(), QCryptographicHash::Sha1).toHex());
}

void AppCenterEngine::getJson(const QString &url, int maxAgeHours, std::function<void(const QJsonDocument &, bool)> done, const QByteArray &postJson)
{
    const QString file = cacheFile(url + QString::fromUtf8(postJson));
    const QFileInfo cached(file);
    auto fromCache = [file]() {
        QFile f(file);
        return f.open(QIODevice::ReadOnly) ? QJsonDocument::fromJson(f.readAll()) : QJsonDocument();
    };
    if (cached.exists() && cached.lastModified().secsTo(QDateTime::currentDateTime()) < qint64(maxAgeHours) * 3600)
        return done(fromCache(), true);
    QNetworkRequest req{QUrl(url)};
    req.setHeader(QNetworkRequest::UserAgentHeader, QStringLiteral("AsimaAppCenter/0.1 (+https://github.com/asimaos/asimaos)"));
    req.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
    req.setTransferTimeout(30000);
    QNetworkReply *reply;
    if (postJson.isEmpty()) {
        reply = m_net.get(req);
    } else {
        req.setHeader(QNetworkRequest::ContentTypeHeader, QStringLiteral("application/json"));
        reply = m_net.post(req, postJson);
    }
    connect(reply, &QNetworkReply::finished, this, [reply, file, done, fromCache] {
        reply->deleteLater();
        const QByteArray body = reply->readAll();
        const QJsonDocument d = QJsonDocument::fromJson(body);
        if (reply->error() == QNetworkReply::NoError && !d.isNull()) {
            QDir().mkpath(QFileInfo(file).absolutePath());
            QFile f(file);
            if (f.open(QIODevice::WriteOnly | QIODevice::Truncate))
                f.write(body);
            return done(d, true);
        }
        // Offline or refused: the last answer, if there is one.
        const QJsonDocument old = fromCache();
        done(old, false);
    });
}

// ───────────────────────────────────────────────────────────────────────── demo

void AppCenterEngine::loadDemo()
{
    auto demo = [](const QString &kind, const QString &id, const QString &name, const QString &summary, const QString &icon) {
        return appItem(kind, id, name, summary, icon);
    };
    const QHash<QString, QVariantList> sample{
        {QStringLiteral("flatpak"), {demo("flatpak", "org.mozilla.firefox", "Firefox", "Fast, private and safe web browser", "internet-web-browser"),
                                     demo("flatpak", "com.valvesoftware.Steam", "Steam", "Launcher for the Steam software distribution service", "applications-games"),
                                     demo("flatpak", "org.gimp.GIMP", "GIMP", "Create images and edit photographs", "applications-graphics"),
                                     demo("flatpak", "org.libreoffice.LibreOffice", "LibreOffice", "The LibreOffice productivity suite", "applications-office"),
                                     demo("flatpak", "org.kde.kdenlive", "Kdenlive", "Video editing for everyone", "kdenlive"),
                                     demo("flatpak", "org.kde.digikam", "digiKam", "Manage, edit and share your photos", "digikam")}},
        {QStringLiteral("snap"), {demo("snap", "spotify", "Spotify", "Music for everyone", "applications-multimedia"),
                                  demo("snap", "code", "Visual Studio Code", "Code editing. Redefined.", "applications-development"),
                                  demo("snap", "postman", "Postman", "API development environment", "applications-internet")}},
        {QStringLiteral("appimage"), {demo("appimage", "Krita", "Krita", "Digital painting", "applications-graphics"),
                                      demo("appimage", "Kdenlive", "Kdenlive", "Video editor", "kdenlive"),
                                      demo("appimage", "Joplin", "Joplin", "Notes and to-dos", "accessories-text-editor")}},
        {QStringLiteral("android"), {demo("android", "org.fdroid.fdroid", "F-Droid", "The free Android app store", "smartphone"),
                                     demo("android", "org.mozilla.fennec_fdroid", "Fennec", "Browse the web", "internet-web-browser")}},
        {QStringLiteral("windows"), {demo("windows", "Battle.net", "Battle.net", "Blizzard's games, in Bottles", "applications-games")}},
        {QStringLiteral("containers"), {demo("containers", "ubuntu-dev", "ubuntu-dev", "Distrobox · Ubuntu 24.04", "virtual-desktops")}},
        {QStringLiteral("nix"), {demo("nix", "neovim", "neovim", "Vim-fork focused on extensibility", "utilities-terminal"),
                                 demo("nix", "ripgrep", "ripgrep", "Fast line-oriented search", "utilities-terminal")}},
        {QStringLiteral("python"), {demo("python", "httpie", "httpie", "Command-line HTTP client", "utilities-terminal")}},
        {QStringLiteral("node"), {demo("node", "typescript", "typescript", "TypeScript language", "utilities-terminal")}},
        {QStringLiteral("rust"), {demo("rust", "bat", "bat", "A cat clone with wings", "utilities-terminal")}},
        {QStringLiteral("go"), {demo("go", "gopls", "gopls", "The Go language server", "utilities-terminal")}},
    };
    if (m_installedIds.isEmpty()) {
        m_installedIds.insert(QStringLiteral("flatpak"), {QStringLiteral("org.mozilla.firefox"), QStringLiteral("com.valvesoftware.Steam")});
        m_installedIds.insert(QStringLiteral("android"), {QStringLiteral("org.fdroid.fdroid")});
        m_installedIds.insert(QStringLiteral("containers"), {QStringLiteral("ubuntu-dev")});
        m_installedIds.insert(QStringLiteral("nix"), {QStringLiteral("neovim"), QStringLiteral("ripgrep")});
        m_installedIds.insert(QStringLiteral("python"), {QStringLiteral("httpie")});
        m_installedIds.insert(QStringLiteral("windows"), {QStringLiteral("Battle.net")});
        m_installed.clear();
        for (const QString &k : kAllKinds) {
            QVariantList items;
            for (const QVariant &v : sample.value(k))
                if (m_installedIds.value(k).contains(v.toMap().value("id").toString()))
                    items << v;
            QVariantMap row = rowFor(k);
            QVariantList list = items;
            markInstalled(list);
            row.insert("items", list);
            row.insert("ready", ready(k));
            row.insert("tools", k == QLatin1String("flatpak") ? QVariantList{item({{"id", "unused"}, {"title", tr("Remove unused runtimes")}}),
                                                                           item({{"id", "repair"}, {"title", tr("Repair")}})}
                                                              : QVariantList{});
            m_installed << row;
        }
        m_updates = item({{"checking", false}, {"total", 5},
                          {"kinds", QVariantList{item({{"kind", "flatpak"}, {"title", tr("Flatpak")}, {"count", 4},
                                                       {"names", QStringList{"Firefox", "Steam", "Mesa", "KDE Frameworks"}}}),
                                                 item({{"kind", "snap"}, {"title", tr("Snap")}, {"count", 1}, {"names", QStringList{"Spotify"}}})}}});
        m_groups = {item({{"id", "gaming"}, {"title", tr("Gaming")}, {"summary", tr("Steam, Heroic, Lutris and the tools around them")},
                          {"apps", QVariantList{item({{"kind", "flatpak"}, {"id", "com.valvesoftware.Steam"}, {"name", "Steam"}}),
                                                item({{"kind", "flatpak"}, {"id", "com.heroicgameslauncher.hgl"}, {"name", "Heroic"}}),
                                                item({{"kind", "flatpak"}, {"id", "net.lutris.Lutris"}, {"name", "Lutris"}}),
                                                item({{"kind", "flatpak"}, {"id", "org.freedesktop.Platform.VulkanLayer.MangoHud"}, {"name", "MangoHud"}})}}}),
                    item({{"id", "office"}, {"title", tr("Office")}, {"summary", tr("Documents, mail and notes")},
                          {"apps", QVariantList{item({{"kind", "flatpak"}, {"id", "org.libreoffice.LibreOffice"}, {"name", "LibreOffice"}}),
                                                item({{"kind", "flatpak"}, {"id", "org.mozilla.Thunderbird"}, {"name", "Thunderbird"}}),
                                                item({{"kind", "flatpak"}, {"id", "net.cozic.joplin_desktop"}, {"name", "Joplin"}})}}})};
    }
    for (QVariant &r : m_rows) {
        QVariantMap m = r.toMap();
        const QString k = m.value("kind").toString();
        QVariantList list;
        for (const QVariant &v : sample.value(k)) {
            const QVariantMap a = v.toMap();
            if (m_query.isEmpty() || a.value("name").toString().contains(m_query, Qt::CaseInsensitive))
                list << a;
        }
        markInstalled(list);
        m.insert("items", list);
        m.insert("state", !ready(k) ? QStringLiteral("setup") : list.isEmpty() ? QStringLiteral("empty") : QStringLiteral("ready"));
        r = m;
    }
    emit rowsChanged();
    emit installedChanged();
    emit updatesChanged();
    emit groupsChanged();
}

void AppCenterEngine::setDemoState(const QString &state)
{
    if (state == QLatin1String("file")) {
        openFile(QStringLiteral("/home/asha/Downloads/Obsidian-1.9.12.AppImage"));
        m_file.insert(QStringLiteral("size"), 128.4e6);   // a sample: the file isn't really there
        m_file.insert(QStringLiteral("ok"), true);
        m_file.insert(QStringLiteral("problem"), QString());
        emit fileChanged();
    }
    if (state == QLatin1String("search"))
        search(QStringLiteral("o"));
    if (state == QLatin1String("busy")) {
        m_jobs << Job{QStringLiteral("flatpak"), tr("Installing GIMP"), QString(), QString(), {}};
        m_detail = tr("Downloading org.gimp.GIMP · 42 %");
        emit jobChanged();
    }
}
