#include "installerengine.h"
#include "sysinfo.h"

#include <QClipboard>
#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QSet>
#include <QThread>
#include <QTimeZone>

#include <algorithm>
#include <unistd.h>

namespace {

constexpr qint64 kMinDiskBytes = 48LL * 1024 * 1024 * 1024;   // two versions side by side plus an update

QVariantMap item(std::initializer_list<std::pair<QString, QVariant>> values)
{
    QVariantMap m;
    for (const auto &[k, v] : values)
        m.insert(k, v);
    return m;
}

QString readTrimmed(const QString &path)
{
    QFile f(path);
    return f.open(QIODevice::ReadOnly) ? QString::fromUtf8(f.readAll()).trimmed() : QString();
}

QString runCommand(const QString &program, const QStringList &args)
{
    QProcess p;
    p.start(program, args);
    if (!p.waitForFinished(5000))
        return {};
    return QString::fromUtf8(p.readAllStandardOutput()).trimmed();
}

// Keyboard and time zone where a language's country alone doesn't say (several layouts or time
// zones in the country, or a layout people there prefer); every other language takes its country's.
struct LanguageDefaults { const char *id, *keyboard, *timezone; };
const LanguageDefaults kLanguageDefaults[] = {
    {"en_US.UTF-8", "us", "America/New_York"},  {"en_GB.UTF-8", "gb", "Europe/London"},
    {"en_IN.UTF-8", "in:eng", "Asia/Kolkata"},  {"en_AU.UTF-8", "us", "Australia/Sydney"},
    {"en_CA.UTF-8", "us", "America/Toronto"},   {"fr_CA.UTF-8", "ca", "America/Toronto"},
    {"bn_BD.UTF-8", "bd", "Asia/Dhaka"},        {"bn_IN.UTF-8", "in:eng", "Asia/Kolkata"},
    {"hi_IN.UTF-8", "in:eng", "Asia/Kolkata"},  {"mr_IN.UTF-8", "in:eng", "Asia/Kolkata"},
    {"ta_IN.UTF-8", "in:eng", "Asia/Kolkata"},  {"te_IN.UTF-8", "in:eng", "Asia/Kolkata"},
    {"gu_IN.UTF-8", "in:eng", "Asia/Kolkata"},  {"kn_IN.UTF-8", "in:eng", "Asia/Kolkata"},
    {"ml_IN.UTF-8", "in:eng", "Asia/Kolkata"},  {"pa_IN.UTF-8", "in:eng", "Asia/Kolkata"},
    {"ur_PK.UTF-8", "us", "Asia/Karachi"},      {"ne_NP.UTF-8", "us", "Asia/Kathmandu"},
    {"ar_SA.UTF-8", "ara", "Asia/Riyadh"},      {"es_MX.UTF-8", "latam", "America/Mexico_City"},
    {"nl_NL.UTF-8", "us", "Europe/Amsterdam"},  {"pt_BR.UTF-8", "br", "America/Sao_Paulo"},
    {"ru_RU.UTF-8", "ru", "Europe/Moscow"},     {"uk_UA.UTF-8", "ua", "Europe/Kyiv"},
    {"kk_KZ.UTF-8", "kz", "Asia/Almaty"},       {"id_ID.UTF-8", "us", "Asia/Jakarta"},
    {"vi_VN.UTF-8", "us", "Asia/Ho_Chi_Minh"},  {"th_TH.UTF-8", "us", "Asia/Bangkok"},
    {"zh_CN.UTF-8", "us", "Asia/Shanghai"},     {"zh_TW.UTF-8", "us", "Asia/Taipei"},
    {"zh_HK.UTF-8", "us", "Asia/Hong_Kong"},    {"es_US.UTF-8", "us", "America/New_York"},
};

// Layouts with a variant people often want; every other layout comes from XKB's list.
struct Keyboard { const char *id, *title; };
const Keyboard kKeyboards[] = {
    {"us", "English (US)"}, {"gb", "English (UK)"}, {"in:eng", "English (India, with rupee)"},
    {"bd", "Bangla"}, {"bd:probhat", "Bangla (Probhat)"}, {"in:ben", "Bangla (India)"},
    {"in:bolnagri", "Hindi (Bolnagri)"}, {"in:hin-wx", "Hindi (Wx)"}, {"in:tam", "Tamil (InScript)"},
    {"in:tel", "Telugu"}, {"ara", "Arabic"}, {"de", "German"}, {"ch", "German (Switzerland)"},
    {"es", "Spanish"}, {"latam", "Spanish (Latin America)"}, {"fr", "French"}, {"it", "Italian"},
    {"nl", "Dutch"}, {"pl", "Polish"}, {"br", "Portuguese (Brazil)"}, {"pt", "Portuguese"},
    {"ru", "Russian"}, {"ua", "Ukrainian"}, {"se", "Swedish"}, {"no", "Norwegian"}, {"dk", "Danish"},
    {"fi", "Finnish"}, {"tr", "Turkish"}, {"cz", "Czech"}, {"hu", "Hungarian"}, {"jp", "Japanese"},
    {"kr", "Korean"},
};

QString memoryText(double bytes)
{
    return QStringLiteral("%1 GB").arg(qRound(bytes / (1024.0 * 1024 * 1024)));
}

} // namespace

InstallerEngine::InstallerEngine(bool demo, bool dryRun, QObject *parent)
    : QObject(parent)
    , m_demo(demo)
    , m_dryRun(dryRun)
{
    m_demoTimer.setInterval(110);
    connect(&m_demoTimer, &QTimer::timeout, this, &InstallerEngine::demoTick);
    loadLists();
    if (m_demo)
        loadDemoSystem();
    else
        loadSystem();
    resetInstall();

    // Names the new system already uses (from its factory /etc; the live system has the same one).
    for (const QString &db : {QStringLiteral("passwd"), QStringLiteral("group")}) {
        QString path = QStringLiteral("/usr/share/factory/etc/") + db;
        if (!QFileInfo::exists(path))
            path = QStringLiteral("/etc/") + db;
        for (const QString &line : readTrimmed(path).split(QLatin1Char('\n'), Qt::SkipEmptyParts))
            m_systemNames.insert(line.section(QLatin1Char(':'), 0, 0));
    }
    m_systemNames.insert(QStringLiteral("root"));
}

// ───────────────────────────────────────────────────────────────────────── account

QString InstallerEngine::usernameProblem(const QString &name) const
{
    static const QRegularExpression valid(QStringLiteral("^[a-z_][a-z0-9_-]{0,31}$"));
    if (name.isEmpty())
        return tr("Choose a username.");
    if (!valid.match(name).hasMatch())
        return name.front().isDigit() || name.front() == QLatin1Char('-')
                   ? tr("A username starts with a letter.")
                   : tr("Use lowercase letters, digits, - and _ (at most 32).");
    if (m_systemNames.contains(name))
        return tr("“%1” is used by the system. Choose another username.").arg(name);
    return {};
}

QString InstallerEngine::hostnameProblem(const QString &name) const
{
    static const QRegularExpression valid(QStringLiteral("^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$"));
    if (name.isEmpty())
        return tr("Choose a computer name.");
    if (!valid.match(name).hasMatch())
        return tr("Use lowercase letters, digits and -, not at the start or end.");
    return {};
}

// "Asha Roy" → "asha", "José Núñez" → "jose". Scripts without a Latin spelling give "user".
QString InstallerEngine::suggestUsername(const QString &fullName) const
{
    const QString first = fullName.trimmed().section(QLatin1Char(' '), 0, 0, QString::SectionSkipEmpty);
    QString name;
    for (const QChar c : first.normalized(QString::NormalizationForm_KD).toLower()) {
        if ((c >= QLatin1Char('a') && c <= QLatin1Char('z')) || (c >= QLatin1Char('0') && c <= QLatin1Char('9'))
            || c == QLatin1Char('_') || c == QLatin1Char('-'))
            name.append(c);
    }
    while (!name.isEmpty() && !(name.front() >= QLatin1Char('a') && name.front() <= QLatin1Char('z')))
        name.remove(0, 1);
    name.truncate(24);
    if (name.isEmpty() || m_systemNames.contains(name))
        name = QStringLiteral("user");
    return name;
}

// "asha" on a laptop → "asha-laptop".
QString InstallerEngine::suggestHostname(const QString &username) const
{
    const QString kind = m_system.value("deviceType").toString();
    const QString base = username.isEmpty() ? QStringLiteral("asimaos") : username;
    return QStringLiteral("%1-%2").arg(base, kind.isEmpty() ? QStringLiteral("pc") : kind).left(63);
}

// ───────────────────────────────────────────────────────────────────────── lists

// glibc names a locale whose only character set is UTF-8 without ".UTF-8" ("en_IN", "sr_RS@latin")
// and others with it ("de_DE.UTF-8"): the list keeps glibc's own names, which are those of the
// compiled locales, and compares them without it.
static bool sameLocale(const QString &a, const QString &b)
{
    return QString(a).remove(QStringLiteral(".UTF-8")) == QString(b).remove(QStringLiteral(".UTF-8"));
}

// "de_AT.UTF-8" → its QLocale ("sr_RS@latin" → Serbian in Latin script).
static QLocale localeOf(const QString &id)
{
    const QString base = id.section(QLatin1Char('.'), 0, 0).section(QLatin1Char('@'), 0, 0);
    const QString modifier = id.section(QLatin1Char('@'), 1);
    QLocale l(base);
    if (modifier == QLatin1String("latin"))
        l = QLocale(l.language(), QLocale::LatinScript, l.territory());
    else if (modifier == QLatin1String("cyrillic"))
        l = QLocale(l.language(), QLocale::CyrillicScript, l.territory());
    return l;
}

static QString capitalized(QString s)
{
    if (!s.isEmpty())
        s[0] = s.at(0).toUpper();
    return s;
}

// A glibc locale's own English names (LC_IDENTIFICATION), for languages Qt doesn't know.
static QPair<QString, QString> glibcNames(const QString &id)
{
    const QString text = readTrimmed(QStringLiteral("/usr/share/i18n/locales/") + QString(id).remove(QStringLiteral(".UTF-8")));
    static const QRegularExpression language(QStringLiteral("^language\\s+\"([^\"]+)\""), QRegularExpression::MultilineOption);
    static const QRegularExpression territory(QStringLiteral("^territory\\s+\"([^\"]+)\""), QRegularExpression::MultilineOption);
    return {language.match(text).captured(1), territory.match(text).captured(1)};
}

void InstallerEngine::loadLists()
{
    // Keyboards: the common variants above, then every XKB layout.
    QSet<QString> keyboardIds;
    QVariantList keyboards;
    for (const Keyboard &k : kKeyboards) {
        keyboards.append(item({{"id", k.id}, {"title", k.title}}));
        keyboardIds.insert(QString::fromLatin1(k.id));
    }
    bool inLayouts = false;
    for (const QString &line : readTrimmed(QStringLiteral("/usr/share/X11/xkb/rules/base.lst")).split(QLatin1Char('\n'))) {
        if (line.startsWith(QLatin1Char('!'))) {
            inLayouts = line.trimmed() == QLatin1String("! layout");
            continue;
        }
        const QString id = line.simplified().section(QLatin1Char(' '), 0, 0);
        const QString title = line.simplified().section(QLatin1Char(' '), 1);
        if (inLayouts && !id.isEmpty() && !title.isEmpty() && !keyboardIds.contains(id)) {
            keyboards.append(item({{"id", id}, {"title", title}}));
            keyboardIds.insert(id);
        }
    }
    std::sort(keyboards.begin(), keyboards.end(), [](const QVariant &a, const QVariant &b) {
        return a.toMap().value("title").toString().localeAwareCompare(b.toMap().value("title").toString()) < 0;
    });
    m_keyboards = keyboards;

    // Time zones: tzdata's list of places (zone.tab), each with its country, searchable by city,
    // country (in English and in its own language) and the area it covers.
    const QDateTime now = QDateTime::currentDateTimeUtc();
    const auto utcText = [&now](const QByteArray &id) {
        const int offset = QTimeZone(id).offsetFromUtc(now);
        const int minutes = qAbs(offset) / 60;
        return QStringLiteral("UTC%1%2:%3").arg(offset < 0 ? QStringLiteral("−") : QStringLiteral("+"))
            .arg(minutes / 60, 2, 10, QLatin1Char('0')).arg(minutes % 60, 2, 10, QLatin1Char('0'));
    };
    QHash<QString, QString> firstZone;   // country code → its main zone (zone.tab lists it first)
    QHash<QString, int> zonesIn;
    QList<QStringList> rows;
    for (const QString &line : readTrimmed(QStringLiteral("/usr/share/zoneinfo/zone.tab")).split(QLatin1Char('\n'))) {
        const QStringList f = line.split(QLatin1Char('\t'));
        if (line.startsWith(QLatin1Char('#')) || f.size() < 3 || !QTimeZone(f.at(2).toLatin1()).isValid())
            continue;
        rows.append(f);
        if (!firstZone.contains(f.at(0)))
            firstZone.insert(f.at(0), f.at(2));
        zonesIn[f.at(0)]++;
    }
    QVariantList zones;
    for (const QStringList &f : rows) {
        const QString id = f.at(2);
        QString city = id.section(QLatin1Char('/'), -1);
        city.replace(QLatin1Char('_'), QLatin1Char(' '));
        const QLocale::Territory territory = QLocale::codeToTerritory(f.at(0));
        const QString country = territory == QLocale::AnyTerritory ? f.at(0) : QLocale::territoryToString(territory);
        const QString native = QLocale(QLocale::AnyLanguage, territory).nativeTerritoryName();
        const QString area = f.value(3);
        const QString where = zonesIn.value(f.at(0)) > 1 && !area.isEmpty() ? country + QStringLiteral(" · ") + area : country;
        zones.append(item({{"id", id}, {"title", city}, {"subtitle", where + QStringLiteral(" · ") + utcText(id.toLatin1())},
                           {"keywords", QStringList{native, f.at(0), id.section(QLatin1Char('/'), 0, 0)}.join(QLatin1Char(' '))}}));
    }
    if (zones.isEmpty()) {   // no zone.tab: every zone Qt knows
        for (const QByteArray &rawId : QTimeZone::availableTimeZoneIds()) {
            const QString id = QString::fromLatin1(rawId);
            if (!id.contains(QLatin1Char('/')) || id.startsWith(QLatin1String("Etc/")))
                continue;
            QString city = id.section(QLatin1Char('/'), -1);
            city.replace(QLatin1Char('_'), QLatin1Char(' '));
            zones.append(item({{"id", id}, {"title", city}, {"subtitle", id.section(QLatin1Char('/'), 0, -2) + QStringLiteral(" · ") + utcText(rawId)}}));
        }
    }
    std::sort(zones.begin(), zones.end(), [](const QVariant &a, const QVariant &b) {
        return a.toMap().value("title").toString().localeAwareCompare(b.toMap().value("title").toString()) < 0;
    });
    m_timezones = {item({{"id", "UTC"}, {"title", "UTC"}, {"subtitle", tr("Coordinated Universal Time")}})};
    m_timezones.append(zones);

    // Languages: every language Linux has (glibc's UTF-8 locales; the system carries them all),
    // named in its own language. A note says where the desktop itself isn't translated yet.
    QStringList ids;
    for (const QString &line : readTrimmed(QStringLiteral("/usr/share/i18n/SUPPORTED")).split(QLatin1Char('\n'))) {
        const QString id = line.section(QLatin1Char(' '), 0, 0);
        if (line.section(QLatin1Char(' '), 1) == QLatin1String("UTF-8") && id.contains(QLatin1Char('_')))
            ids.append(id);
    }
    if (ids.isEmpty())
        for (const LanguageDefaults &d : kLanguageDefaults)
            ids.append(QString::fromLatin1(d.id));
    const bool kdeTranslations = QFileInfo::exists(QStringLiteral("/usr/share/locale/de/LC_MESSAGES/plasmashell.mo"));
    const auto translated = [kdeTranslations](const QString &id) {
        if (!kdeTranslations || id.startsWith(QLatin1String("en_")))
            return true;
        const QString base = id.section(QLatin1Char('.'), 0, 0).section(QLatin1Char('@'), 0, 0);
        const QString mod = id.contains(QLatin1Char('@')) ? QLatin1Char('@') + id.section(QLatin1Char('@'), 1) : QString();
        for (const QString &dir : {base + mod, base.section(QLatin1Char('_'), 0, 0) + mod, base, base.section(QLatin1Char('_'), 0, 0)})
            if (QFileInfo::exists(QStringLiteral("/usr/share/locale/%1/LC_MESSAGES/plasmashell.mo").arg(dir)))
                return true;
        return false;
    };
    QHash<QLocale::Language, int> territoriesOf;
    for (const QString &id : ids)
        territoriesOf[localeOf(id).language()]++;
    QVariantList languages;
    for (const QString &id : ids) {
        const QLocale l = localeOf(id);
        const QString code = id.section(QLatin1Char('_'), 1).section(QLatin1Char('.'), 0, 0).section(QLatin1Char('@'), 0, 0);
        const QLocale::Territory territory = QLocale::codeToTerritory(code);
        QString title, english;
        if (l.language() == QLocale::C || territory == QLocale::AnyTerritory) {
            const auto [language, country] = glibcNames(id);
            english = language.isEmpty() ? id.section(QLatin1Char('.'), 0, 0) : language + QStringLiteral(" (") + country + QLatin1Char(')');
            title = english;
        } else {
            // The language's own name without a country's flavour ("Deutsch", not "Schweizer
            // Hochdeutsch"; CLDR calls even plain English "American English"), then the country.
            const QString name = l.language() == QLocale::English
                ? QStringLiteral("English")
                : capitalized(QLocale(l.language(), l.script(), QLocale::AnyTerritory).nativeLanguageName());
            const QString nativeCountry = l.territory() == territory ? l.nativeTerritoryName() : QLocale::territoryToString(territory);
            const bool several = territoriesOf.value(l.language()) > 1;
            const QString script = id.contains(QLatin1Char('@')) ? QStringLiteral(", ") + QLocale::scriptToString(l.script()) : QString();
            title = name + (several ? QStringLiteral(" (") + nativeCountry + QLatin1Char(')') : QString());
            english = QLocale::languageToString(l.language()) + QStringLiteral(" (") + QLocale::territoryToString(territory) + script + QLatin1Char(')');
        }
        QString keyboard, timezone;
        for (const LanguageDefaults &d : kLanguageDefaults)
            if (sameLocale(id, QString::fromLatin1(d.id))) {
                keyboard = QString::fromLatin1(d.keyboard);
                timezone = QString::fromLatin1(d.timezone);
            }
        const QString country = code.toLower(), language = id.section(QLatin1Char('_'), 0, 0);
        // Otherwise the country's layout (XKB names most after the country), or the language's.
        static const QHash<QString, QString> layoutOfLanguage{{"ar", "ara"}, {"fa", "ir"}, {"he", "il"}, {"el", "gr"}, {"eo", "epo"}};
        if (keyboard.isEmpty())
            keyboard = language == QLatin1String("es") && country != QLatin1String("es") ? QStringLiteral("latam")
                     : keyboardIds.contains(country) && country != QLatin1String("us") ? country
                     : layoutOfLanguage.contains(language) ? layoutOfLanguage.value(language)
                     : keyboardIds.contains(language) ? language : QStringLiteral("us");
        if (timezone.isEmpty())
            timezone = firstZone.value(code, QStringLiteral("UTC"));
        languages.append(item({{"id", id}, {"title", title},
                               {"subtitle", !translated(id) ? tr("%1 · the desktop is in English for now").arg(english)
                                            : english == title ? QString() : english},
                               {"english", english}, {"keyboard", keyboard}, {"timezone", timezone}}));
    }
    std::sort(languages.begin(), languages.end(), [](const QVariant &a, const QVariant &b) {
        return a.toMap().value("english").toString().localeAwareCompare(b.toMap().value("english").toString()) < 0;
    });
    m_languages = languages;
}

QString InstallerEngine::localTime(const QString &timezone) const
{
    const QTimeZone tz(timezone.toLatin1());
    return QDateTime::currentDateTime().toTimeZone(tz.isValid() ? tz : QTimeZone::utc()).toString(QStringLiteral("HH:mm"));
}

// ───────────────────────────────────────────────────────────────────────── this computer

void InstallerEngine::loadDemoSystem()
{
    m_system = item({{"version", "2026.11.15.1"}, {"uefi", true}, {"secureBoot", true}, {"tpm", true},
                     {"memory", 34359738368.0}, {"hasBattery", true}, {"onBattery", false},
                     {"gpu", "NVIDIA GeForce RTX 4060"}, {"threads", 16}, {"deviceType", "laptop"}});
    m_disks = {
        item({{"path", "/dev/nvme0n1"}, {"model", "Samsung SSD 990 PRO 1TB"}, {"size", 1000204886016.0},
              {"transport", "nvme"}, {"transportLabel", tr("NVMe SSD")}, {"serial", "S6Z2NJ0W817K2Q"},
              {"contents", tr("Windows · 4 partitions")}, {"usable", true}}),
        item({{"path", "/dev/sda"}, {"model", "WD Blue SA510 2TB"}, {"size", 2000398934016.0},
              {"transport", "sata"}, {"transportLabel", tr("SATA SSD")}, {"serial", "23045L801234"},
              {"contents", tr("Empty")}, {"usable", true}}),
        item({{"path", "/dev/sdb"}, {"model", "SanDisk Ultra Fit"}, {"size", 32015679488.0},
              {"transport", "usb"}, {"transportLabel", tr("USB")}, {"serial", "4C530001190805"},
              {"contents", tr("Asima OS installer")}, {"usable", false}, {"label", tr("Live USB")},
              {"reason", tr("Asima OS is running from this USB stick.")}}),
    };
    QString language = QStringLiteral("en_IN.UTF-8");
    for (const QVariant &l : m_languages)
        if (sameLocale(l.toMap().value("id").toString(), language))
            language = l.toMap().value("id").toString();
    m_defaults = item({{"language", language}, {"keyboard", "in:eng"}, {"timezone", "Asia/Kolkata"},
                       {"disk", "/dev/nvme0n1"}});
}

void InstallerEngine::loadSystem()
{
    QString version = readTrimmed(QStringLiteral("/run/initramfs/live/asimaos/VERSION"));
    if (version.isEmpty() && qEnvironmentVariableIsSet("ASIMAOS_INSTALL_SOURCE"))
        version = readTrimmed(qEnvironmentVariable("ASIMAOS_INSTALL_SOURCE") + QStringLiteral("/VERSION"));
    if (version.isEmpty())
        version = tr("unknown version");

    QFile sb(QStringLiteral("/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c"));
    const bool secureBoot = sb.open(QIODevice::ReadOnly) && sb.readAll().right(1) == QByteArray(1, '\x01');
    const bool tpm = readTrimmed(QStringLiteral("/sys/class/tpm/tpm0/tpm_version_major")) == QLatin1String("2");

    double memory = 0;
    for (const QString &line : readTrimmed(QStringLiteral("/proc/meminfo")).split(QLatin1Char('\n')))
        if (line.startsWith(QLatin1String("MemTotal:")))
            memory = line.section(QLatin1Char(' '), -2, -2).toDouble() * 1024.0;

    bool hasBattery = false, onBattery = false;
    const QDir power(QStringLiteral("/sys/class/power_supply"));
    for (const QString &supply : power.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
        if (readTrimmed(power.filePath(supply + QStringLiteral("/type"))) != QLatin1String("Battery"))
            continue;
        hasBattery = true;
        onBattery |= readTrimmed(power.filePath(supply + QStringLiteral("/status"))) == QLatin1String("Discharging");
    }

    QStringList gpus;
    const QDir drm(QStringLiteral("/sys/class/drm"));
    for (const QString &card : drm.entryList({QStringLiteral("card?"), QStringLiteral("card??")}, QDir::Dirs | QDir::System)) {
        const QString vendor = readTrimmed(drm.filePath(card + QStringLiteral("/device/vendor")));
        const QString device = readTrimmed(drm.filePath(card + QStringLiteral("/device/device")));
        if (vendor.isEmpty())
            continue;
        const QString name = SysInfo::gpuName(vendor, device);
        if (!name.isEmpty() && !gpus.contains(name))
            gpus.append(name);
    }
    const int threads = QThread::idealThreadCount();

    m_system = item({{"version", version}, {"uefi", QFileInfo::exists(QStringLiteral("/sys/firmware/efi"))},
                     {"secureBoot", secureBoot}, {"tpm", tpm}, {"memory", memory}, {"hasBattery", hasBattery},
                     {"onBattery", onBattery}, {"gpu", gpus.value(0)}, {"threads", threads},
                     {"deviceType", SysInfo::deviceType().value("type")}});

    // Defaults follow the live session's language when it is one we offer.
    // English (United States) otherwise.
    const QString lang = qEnvironmentVariable("LANG").section(QLatin1Char('.'), 0, 0);
    QVariantMap language, english;
    for (const QVariant &l : m_languages) {
        const QString id = l.toMap().value("id").toString().section(QLatin1Char('.'), 0, 0);
        if (id == lang)
            language = l.toMap();
        if (id == QLatin1String("en_US"))
            english = l.toMap();
    }
    if (language.isEmpty())
        language = english.isEmpty() ? m_languages.first().toMap() : english;
    m_defaults = item({{"language", language.value("id")}, {"keyboard", language.value("keyboard")},
                       {"timezone", language.value("timezone")}});
    refreshDisks();
}

void InstallerEngine::refreshDisks()
{
    if (m_demo) {
        emit toast(tr("Disks refreshed."), QStringLiteral("ok"));
        return;
    }
    const QString liveSource = runCommand(QStringLiteral("findmnt"), {QStringLiteral("-no"), QStringLiteral("SOURCE"),
                                                                       QStringLiteral("/run/initramfs/live")});
    const QJsonObject root = QJsonDocument::fromJson(runCommand(QStringLiteral("lsblk"),
        {QStringLiteral("-J"), QStringLiteral("-b"), QStringLiteral("-o"),
         QStringLiteral("NAME,PATH,TYPE,SIZE,MODEL,SERIAL,TRAN,RO,MOUNTPOINTS,FSTYPE,LABEL,PARTLABEL")}).toUtf8()).object();

    QVariantList disks;
    for (const QJsonValue &value : root.value(QStringLiteral("blockdevices")).toArray()) {
        const QJsonObject d = value.toObject();
        if (d.value(QStringLiteral("type")).toString() != QLatin1String("disk"))
            continue;
        const QString path = d.value(QStringLiteral("path")).toString();
        const QString name = d.value(QStringLiteral("name")).toString();
        const QString tran = d.value(QStringLiteral("tran")).toString();
        const double size = d.value(QStringLiteral("size")).toDouble();

        bool mounted = false, live = path == liveSource;
        QStringList kinds;
        const auto inspect = [&](const QJsonObject &o) {
            for (const QJsonValue &mp : o.value(QStringLiteral("mountpoints")).toArray())
                mounted |= !mp.isNull();
            const QString fs = o.value(QStringLiteral("fstype")).toString();
            const QString label = o.value(QStringLiteral("label")).toString();
            const QString partlabel = o.value(QStringLiteral("partlabel")).toString();
            live |= o.value(QStringLiteral("path")).toString() == liveSource || label == QLatin1String("ASIMAOS_LIVE");
            QString kind;
            if (partlabel == QLatin1String("crypt_pool")) kind = tr("Asima OS");
            else if (fs == QLatin1String("ntfs") || partlabel.contains(QLatin1String("Microsoft"))) kind = tr("Windows");
            else if (fs == QLatin1String("crypto_LUKS")) kind = tr("Encrypted data");
            else if (fs == QLatin1String("iso9660")) kind = tr("Installer or disc image");
            else if (QStringList{"ext4", "btrfs", "xfs", "f2fs"}.contains(fs)) kind = tr("Linux");
            if (!kind.isEmpty() && !kinds.contains(kind))
                kinds.append(kind);
        };
        inspect(d);
        const QJsonArray children = d.value(QStringLiteral("children")).toArray();
        for (const QJsonValue &c : children)
            inspect(c.toObject());

        QString contents = kinds.join(QStringLiteral(" · "));
        if (!children.isEmpty())
            contents += (contents.isEmpty() ? QString() : QStringLiteral(" · ")) + tr("%n partition(s)", nullptr, int(children.size()));
        if (contents.isEmpty())
            contents = tr("Empty");

        QString transportLabel = tran == QLatin1String("nvme") ? tr("NVMe SSD")
                               : tran == QLatin1String("sata") ? tr("SATA")
                               : tran == QLatin1String("usb") ? tr("USB")
                               : tran == QLatin1String("mmc") ? tr("SD / eMMC")
                               : name.startsWith(QLatin1String("vd")) ? tr("Virtual disk") : QString();
        QString model = d.value(QStringLiteral("model")).toString().trimmed();
        if (model.isEmpty())
            model = name.startsWith(QLatin1String("vd")) ? tr("Virtual disk") : tr("Disk %1").arg(name);

        QString label, reason;
        if (live) { label = tr("Live USB"); reason = tr("Asima OS is running from this USB stick."); }
        else if (d.value(QStringLiteral("ro")).toBool()) { label = tr("Read-only"); reason = tr("This disk can't be written to."); }
        else if (mounted) { label = tr("In use"); reason = tr("Something on this disk is open. Close it, then refresh."); }
        else if (size < kMinDiskBytes) { label = tr("Too small"); reason = tr("Asima OS needs at least 48 GB."); }

        disks.append(item({{"path", path}, {"model", model}, {"size", size}, {"transport", tran},
                           {"transportLabel", transportLabel}, {"serial", d.value(QStringLiteral("serial")).toString().trimmed()},
                           {"contents", contents}, {"usable", label.isEmpty()}, {"label", label}, {"reason", reason}}));
    }
    m_disks = disks;
    if (!m_defaults.contains("disk"))
        for (const QVariant &disk : m_disks)
            if (disk.toMap().value("usable").toBool()) { m_defaults.insert("disk", disk.toMap().value("path")); break; }
    emit disksChanged();
}

QVariantList InstallerEngine::requirements() const
{
    const bool uefi = m_system.value("uefi").toBool();
    const bool sb = m_system.value("secureBoot").toBool();
    const bool tpm = m_system.value("tpm").toBool();
    const double memory = m_system.value("memory").toDouble();
    int usable = 0;
    for (const QVariant &d : m_disks)
        usable += d.toMap().value("usable").toBool() ? 1 : 0;

    QVariantList list;
    list << item({{"icon", "computer"}, {"title", tr("Starts in UEFI mode")}, {"value", uefi ? tr("UEFI") : tr("Legacy BIOS")},
                  {"tone", uefi ? "ok" : "error"},
                  {"detail", uefi ? tr("Required by Asima OS") : tr("Turn off legacy (CSM) mode in the firmware settings, then start again.")}});
    list << item({{"icon", "view-certificate"}, {"title", tr("Secure Boot")}, {"value", sb ? tr("On") : tr("Off")},
                  {"tone", sb ? "ok" : "info"},
                  {"detail", sb ? tr("Only signed boot files can start this computer") : tr("Works without it; you can turn it on later")}});
    list << item({{"icon", "object-locked"}, {"title", tr("Security chip (TPM 2.0)")}, {"value", tpm ? tr("Found") : tr("Not found")},
                  {"tone", tpm ? "ok" : "info"},
                  {"detail", tpm ? tr("Can unlock your disk automatically") : tr("You'll type your disk passphrase at every start")}});
    list << item({{"icon", "speedometer"}, {"title", tr("Memory")}, {"value", memoryText(memory)},
                  {"tone", memory >= 3.8 * 1024 * 1024 * 1024 ? "ok" : "warn"}, {"detail", tr("4 GB or more recommended")}});
    list << item({{"icon", "drive-harddisk"}, {"title", tr("A disk for Asima OS")},
                  {"value", usable > 0 ? tr("%n suitable", nullptr, usable) : tr("None")}, {"tone", usable > 0 ? "ok" : "error"},
                  {"detail", usable > 0 ? tr("48 GB or more, not in use") : tr("Connect a disk with at least 48 GB")}});
    if (m_system.value("hasBattery").toBool()) {
        const bool battery = m_system.value("onBattery").toBool();
        list << item({{"icon", "battery-100"}, {"title", tr("Power")}, {"value", battery ? tr("On battery") : tr("Plugged in")},
                      {"tone", battery ? "warn" : "ok"},
                      {"detail", battery ? tr("Plug in the charger before you start") : tr("Keep it plugged in until the end")}});
    }
    return list;
}

// ───────────────────────────────────────────────────────────────────────── installing

void InstallerEngine::resetInstall(bool encrypt)
{
    m_encrypt = encrypt;
    QVariantList steps;
    const std::pair<const char *, QString> ids[] = {
        {"check", tr("Check the disk")},
        {"prepare", tr("Prepare Asima OS and your account")},
        {"write", encrypt ? tr("Partition, encrypt and copy Asima OS") : tr("Partition and copy Asima OS")},
        {"recovery", tr("Create your recovery key")},
        {"finish", tr("Finish")},
    };
    for (const auto &[id, title] : ids) {
        if (!encrypt && qstrcmp(id, "recovery") == 0)
            continue;   // no recovery key without encryption
        steps.append(item({{"id", id}, {"title", title}, {"detail", QString()}, {"state", "pending"}}));
    }
    m_install = item({{"state", "idle"}, {"progress", 0.0}, {"detail", QString()}, {"steps", steps},
                      {"recoveryKey", QString()}, {"error", QString()}, {"dryRun", m_dryRun}});
    m_log.clear();
}

void InstallerEngine::setStep(const QString &id, const QString &state)
{
    QVariantList steps = m_install.value("steps").toList();
    for (QVariant &s : steps) {
        QVariantMap m = s.toMap();
        if (m.value("id").toString() == id) {
            m.insert("state", state);
            if (state == QLatin1String("done") && id != QLatin1String("write"))
                m.insert("detail", QString());
            s = m;
        }
    }
    m_install.insert("steps", steps);
}

void InstallerEngine::fail(const QString &message)
{
    QVariantList steps = m_install.value("steps").toList();
    for (QVariant &s : steps) {
        QVariantMap m = s.toMap();
        if (m.value("state").toString() == QLatin1String("active"))
            m.insert("state", "failed");
        s = m;
    }
    m_install.insert("steps", steps);
    m_install.insert("state", "error");
    m_install.insert("error", message);
    emit installChanged();
}

void InstallerEngine::handleEvent(const QByteArray &line)
{
    const QJsonObject e = QJsonDocument::fromJson(line).object();
    const QString event = e.value(QStringLiteral("event")).toString();
    if (event == QLatin1String("step")) {
        setStep(e.value(QStringLiteral("id")).toString(), e.value(QStringLiteral("state")).toString());
    } else if (event == QLatin1String("progress")) {
        m_install.insert("progress", e.value(QStringLiteral("value")).toDouble());
        const QString detail = e.value(QStringLiteral("detail")).toString();
        if (!detail.isEmpty()) {
            m_install.insert("detail", detail);
            QVariantList steps = m_install.value("steps").toList();
            for (QVariant &s : steps) {
                QVariantMap m = s.toMap();
                if (m.value("state").toString() == QLatin1String("active"))
                    m.insert("detail", detail);
                s = m;
            }
            m_install.insert("steps", steps);
        }
    } else if (event == QLatin1String("recovery-key")) {
        m_install.insert("recoveryKey", e.value(QStringLiteral("key")).toString());
    } else if (event == QLatin1String("log")) {
        m_log << e.value(QStringLiteral("message")).toString();
        return;
    } else if (event == QLatin1String("error")) {
        fail(e.value(QStringLiteral("message")).toString());
        return;
    } else if (event == QLatin1String("done")) {
        m_install.insert("state", "done");
        m_install.insert("progress", 1.0);
    } else {
        return;
    }
    emit installChanged();
}

QString InstallerEngine::findEngine() const
{
    const QStringList candidates{
        qEnvironmentVariable("ASIMAOS_INSTALL_ENGINE"),
        QStringLiteral("/run/initramfs/live/asimaos/asimaos-install"),
        QCoreApplication::applicationDirPath() + QStringLiteral("/asimaos-install"),
    };
    for (const QString &c : candidates)
        if (!c.isEmpty() && QFileInfo(c).isExecutable())
            return c;
    return {};
}

void InstallerEngine::startInstall(const QVariantMap &options)
{
    const bool encrypt = options.value("encrypt", true).toBool();
    resetInstall(encrypt);
    m_install.insert("state", "running");
    emit installChanged();

    if (m_demo) {
        m_demoProgress = 0.0;
        m_demoTimer.start();
        return;
    }

    const QString engine = findEngine();
    if (engine.isEmpty()) {
        fail(tr("The installer engine (asimaos-install) was not found. Start the installer from the Asima OS USB stick."));
        return;
    }
    const QString user = options.value("user").toString();
    QStringList args{options.value("disk").toString(), QStringLiteral("--yes"), QStringLiteral("--json"),
                     QStringLiteral("--encrypt"), encrypt ? QStringLiteral("yes") : QStringLiteral("no"),
                     QStringLiteral("--locale"), options.value("language").toString(),
                     QStringLiteral("--keymap"), options.value("keyboard").toString(),
                     QStringLiteral("--timezone"), options.value("timezone").toString()};
    if (encrypt)
        args << QStringLiteral("--passphrase-stdin");
    if (!user.isEmpty())
        args << QStringLiteral("--user") << user << QStringLiteral("--fullname") << options.value("fullName").toString()
             << QStringLiteral("--password-stdin");
    if (const QString host = options.value("hostname").toString(); !host.isEmpty())
        args << QStringLiteral("--hostname") << host;
    if (const QString source = qEnvironmentVariable("ASIMAOS_INSTALL_SOURCE"); !source.isEmpty())
        args << QStringLiteral("--source") << source;
    if (m_dryRun)
        args << QStringLiteral("--dry-run");

    QString program = engine;
    if (::geteuid() != 0 && !m_dryRun) {
        args.prepend(engine);
        program = QStringLiteral("pkexec");
    }

    m_buffer.clear();
    m_process = new QProcess(this);
    connect(m_process, &QProcess::readyReadStandardOutput, this, [this] {
        m_buffer += m_process->readAllStandardOutput();
        for (int nl = m_buffer.indexOf('\n'); nl >= 0; nl = m_buffer.indexOf('\n')) {
            const QByteArray line = m_buffer.left(nl).trimmed();
            m_buffer.remove(0, nl + 1);
            if (!line.isEmpty())
                handleEvent(line);
        }
    });
    connect(m_process, &QProcess::readyReadStandardError, this, [this] {
        m_log << QString::fromUtf8(m_process->readAllStandardError()).trimmed();
    });
    connect(m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart)
            fail(tr("The installer engine could not be started: %1").arg(m_process->errorString()));
    });
    connect(m_process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        if (m_install.value("state") == QLatin1String("running"))
            fail(status == QProcess::NormalExit && code == 126
                     ? tr("Permission to install was not given.")
                     : tr("The installer stopped unexpectedly (code %1).").arg(code));
        m_process->deleteLater();
    });
    m_process->start(program, args);
    // Secrets go through a pipe, never on the command line or in the environment: first the disk
    // passphrase (when encrypting), then the account password.
    if (encrypt)
        m_process->write(options.value("passphrase").toString().toUtf8() + '\n');
    if (!user.isEmpty())
        m_process->write(options.value("password").toString().toUtf8() + '\n');
    m_process->closeWriteChannel();
}

void InstallerEngine::demoTick()
{
    m_demoProgress = qMin(1.0, m_demoProgress + 0.0085);
    const double p = m_demoProgress;
    const auto st = [p](double from, double to) {
        return p >= to ? QStringLiteral("done") : p >= from ? QStringLiteral("active") : QStringLiteral("pending");
    };
    setStep(QStringLiteral("check"), st(0.0, 0.03));
    setStep(QStringLiteral("prepare"), st(0.03, 0.08));
    setStep(QStringLiteral("write"), st(0.08, 0.90));
    if (m_encrypt)
        setStep(QStringLiteral("recovery"), st(0.90, 0.97));
    setStep(QStringLiteral("finish"), st(m_encrypt ? 0.97 : 0.90, 1.0));
    const double written = qBound(0.0, (p - 0.08) / 0.82, 1.0) * 5.0;
    if (p >= 0.08 && p < 0.90) {
        const QString detail = tr("Copying Asima OS · %1 of 5.0 GB").arg(written, 0, 'f', 1);
        m_install.insert("detail", detail);
        QVariantList steps = m_install.value("steps").toList();
        for (QVariant &s : steps) {
            QVariantMap m = s.toMap();
            if (m.value("id").toString() == QLatin1String("write"))
                m.insert("detail", detail);
            s = m;
        }
        m_install.insert("steps", steps);
    }
    if (p >= 0.95 && m_encrypt)
        m_install.insert("recoveryKey", "fkcrtjbg-ndhvhjrl-bucdbhit-tirlukce-hkjgrdlg-kbtnbilh-djvbrkfu-ihfcehcn");
    m_install.insert("progress", p);
    if (p >= 1.0) {
        m_demoTimer.stop();
        m_install.insert("state", "done");
    }
    emit installChanged();
}

void InstallerEngine::setDemoState(const QString &fullState)
{
    const bool plain = fullState.contains(QLatin1String("plain"));
    const QString state = fullState.section(QLatin1Char('-'), 0, 0);
    if (state == QLatin1String("installing") || state == QLatin1String("done")) {
        startInstall({{QStringLiteral("encrypt"), !plain}});
        m_demoTimer.stop();
        m_demoProgress = state == QLatin1String("done") ? 0.999 : 0.57;
        demoTick();
    } else if (state == QLatin1String("error")) {
        resetInstall();
        m_install.insert("state", "running");
        setStep(QStringLiteral("check"), QStringLiteral("done"));
        setStep(QStringLiteral("prepare"), QStringLiteral("done"));
        setStep(QStringLiteral("write"), QStringLiteral("active"));
        m_install.insert("progress", 0.41);
        fail(tr("Writing the disk failed: the disk reported a read/write error. Your other disks were not touched."));
    }
}

// ───────────────────────────────────────────────────────────────────────── finishing

void InstallerEngine::copyText(const QString &text)
{
    QGuiApplication::clipboard()->setText(text);
    emit toast(tr("Copied. Paste it somewhere safe, away from this computer."), QStringLiteral("ok"));
}

bool InstallerEngine::saveText(const QUrl &file, const QString &text)
{
    QFile f(file.toLocalFile());
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        emit toast(tr("Could not save: %1").arg(f.errorString()), QStringLiteral("error"));
        return false;
    }
    f.write(text.toUtf8());
    emit toast(tr("Saved to %1").arg(f.fileName()), QStringLiteral("ok"));
    return true;
}

void InstallerEngine::restart()
{
    if (m_demo || m_dryRun) {
        emit toast(tr("Demo: the computer would restart now."), QStringLiteral("info"));
        return;
    }
    QProcess::startDetached(QStringLiteral("systemctl"), {QStringLiteral("reboot")});
}

void InstallerEngine::quit()
{
    QCoreApplication::quit();
}

void InstallerEngine::openRepair()
{
    const QString tool = QStringLiteral("/run/initramfs/live/asimaos/asimaos-repair");
    if (m_demo || !QFileInfo::exists(tool)
        || !QProcess::startDetached(QStringLiteral("konsole"), {QStringLiteral("--hide-menubar"), QStringLiteral("-e"),
                                                               QStringLiteral("sudo"), tool, QStringLiteral("--interactive")}))
        emit toast(tr("Repairing works from the Asima OS USB stick."), QStringLiteral("info"));
}
