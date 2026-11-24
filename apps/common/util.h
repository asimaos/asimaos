#pragma once

// Small helpers the Asima OS apps' engines share.

#include <QFile>
#include <QProcess>
#include <QStandardPaths>
#include <QString>
#include <QStringList>
#include <QVariantMap>

#include <initializer_list>
#include <utility>

namespace Util {

// A map from { key, value } pairs: the shape every engine hands to QML.
inline QVariantMap item(std::initializer_list<std::pair<QString, QVariant>> values)
{
    QVariantMap m;
    for (const auto &[k, v] : values)
        m.insert(k, v);
    return m;
}

// Runs a program and returns what it printed; empty when it can't start or takes too long.
inline QString run(const QString &program, const QStringList &args, int timeoutMs = 4000)
{
    QProcess p;
    p.start(program, args);
    if (!p.waitForFinished(timeoutMs))
        return {};
    return QString::fromUtf8(p.readAllStandardOutput());
}

// Whether a program is installed.
inline bool has(const QString &program)
{
    return !QStandardPaths::findExecutable(program).isEmpty();
}

// os-release as a map: IMAGE_VERSION, IMAGE_ID, PRETTY_NAME, …
inline QVariantMap osRelease()
{
    QVariantMap out;
    for (const QString &path : {QStringLiteral("/usr/lib/os-release"), QStringLiteral("/etc/os-release")}) {
        QFile f(path);
        if (!f.open(QIODevice::ReadOnly))
            continue;
        while (!f.atEnd()) {
            const QString line = QString::fromUtf8(f.readLine()).trimmed();
            const int eq = line.indexOf(QLatin1Char('='));
            if (eq <= 0 || line.startsWith(QLatin1Char('#')))
                continue;
            QString value = line.mid(eq + 1);
            if (value.size() >= 2 && (value.startsWith(QLatin1Char('"')) || value.startsWith(QLatin1Char('\''))))
                value = value.mid(1, value.size() - 2);
            out.insert(line.left(eq), value);
        }
        break;
    }
    return out;
}

// Like Theme.bytes in Asima UI: 1000-based, one decimal below 100.
inline QString bytesText(double n)
{
    static const char *const units[] = {"B", "KB", "MB", "GB", "TB"};
    int i = 0;
    while (n >= 1000 && i < 4) {
        n /= 1000;
        ++i;
    }
    return (n >= 100 || i == 0 ? QString::number(qRound(n)) : QString::number(n, 'f', 1)) + QLatin1Char(' ') + QLatin1String(units[i]);
}

// The services' sentences can start in lower case (they come from tools); people see a sentence.
inline QString sentence(QString s)
{
    if (!s.isEmpty())
        s[0] = s.at(0).toUpper();
    return s;
}

} // namespace Util
