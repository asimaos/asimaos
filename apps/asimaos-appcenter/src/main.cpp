// Asima App Center (D56): software of every platform that runs on Linux, in one place.
//
//   asimaos-appcenter [FILE]          the window; with an app file (.flatpakref, .snap, .AppImage,
//                                     .apk, .exe …): what it is, and installing it
//   asimaos-appcenter --check-updates no window (the user timer asimaos-appcenter-updates): counts
//                                     the updates and, when there are some, says so in a notification

#include "appcenterengine.h"
#include "asimaosui.h"

#include <QCommandLineParser>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusVariant>
#include <QFileInfo>
#include <QGuiApplication>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QTimer>
#include <QtQml/qqmlextensionplugin.h>

Q_IMPORT_QML_PLUGIN(Asima_UiPlugin)

namespace {

// The notification for the updates: "Open" shows them, "Update all" installs them.
class UpdateNotice : public QObject
{
    Q_OBJECT
public:
    void show(int count)
    {
        QDBusMessage m = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.Notifications"), QStringLiteral("/org/freedesktop/Notifications"),
                                                        QStringLiteral("org.freedesktop.Notifications"), QStringLiteral("Notify"));
        m << QStringLiteral("Asima App Center") << uint(0) << QStringLiteral("io.github.asimaos.AppCenter")
          << QCoreApplication::translate("UpdateNotice", "%n app update(s)", "", count)
          << QCoreApplication::translate("UpdateNotice", "Updates for your apps are ready to install.")
          << QStringList{QStringLiteral("default"), QCoreApplication::translate("UpdateNotice", "Open"), QStringLiteral("update"),
                         QCoreApplication::translate("UpdateNotice", "Update all")}
          << QVariantMap{{QStringLiteral("desktop-entry"), QStringLiteral("io.github.asimaos.AppCenter")}} << int(-1);
        const QDBusMessage r = QDBusConnection::sessionBus().call(m);
        m_id = r.arguments().value(0).toUInt();
        QDBusConnection::sessionBus().connect(QString(), QStringLiteral("/org/freedesktop/Notifications"), QStringLiteral("org.freedesktop.Notifications"),
                                              QStringLiteral("ActionInvoked"), this, SLOT(action(uint, QString)));
        QDBusConnection::sessionBus().connect(QString(), QStringLiteral("/org/freedesktop/Notifications"), QStringLiteral("org.freedesktop.Notifications"),
                                              QStringLiteral("NotificationClosed"), this, SLOT(closed(uint, uint)));
        // Not waited for longer than an hour.
        QTimer::singleShot(3600 * 1000, qApp, &QCoreApplication::quit);
    }

public slots:
    void action(uint id, const QString &key)
    {
        if (id != m_id)
            return;
        QStringList args{QStringLiteral("--page"), QStringLiteral("updates")};
        if (key == QLatin1String("update"))
            args << QStringLiteral("--update-all");
        QProcess::startDetached(QCoreApplication::applicationFilePath(), args);
        QCoreApplication::quit();
    }
    void closed(uint id, uint)
    {
        if (id == m_id)
            QTimer::singleShot(2000, qApp, &QCoreApplication::quit);   // an action may still arrive
    }

private:
    uint m_id = 0;
};

} // namespace

int main(int argc, char *argv[])
{
    // Without a window: the background check.
    for (int i = 1; i < argc; ++i)
        if (qstrcmp(argv[i], "--check-updates") == 0) {
            QGuiApplication app(argc, argv);
            app.setApplicationName(QStringLiteral("asimaos-appcenter"));
            AppCenterEngine backend(false, false);
            UpdateNotice notice;
            backend.checkUpdatesQuietly([&notice](int count) {
                if (count > 0)
                    notice.show(count);
                else
                    QCoreApplication::quit();
            });
            return app.exec();
        }

    AsimaUi::preApplicationSetup();
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("asimaos-appcenter"));
    app.setApplicationDisplayName(QStringLiteral("Asima App Center"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setOrganizationDomain(QStringLiteral("asimaos.github.io"));
    app.setDesktopFileName(QStringLiteral("io.github.asimaos.AppCenter"));
    AsimaUi::applicationSetup();

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Asima App Center"));
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption demoOpt(QStringLiteral("demo"), QStringLiteral("Show sample apps instead of this computer's."));
    const QCommandLineOption themeOpt(QStringLiteral("theme"), QStringLiteral("system, light or dark."), QStringLiteral("mode"), QStringLiteral("system"));
    const QCommandLineOption pageOpt(QStringLiteral("page"), QStringLiteral("Page to open: store, installed, updates or file."), QStringLiteral("page"),
                                     QStringLiteral("store"));
    const QCommandLineOption stateOpt(QStringLiteral("state"), QStringLiteral("Demo state for screenshots: search, busy, file."), QStringLiteral("state"));
    const QCommandLineOption updateOpt(QStringLiteral("update-all"), QStringLiteral("Install every update."));
    const QCommandLineOption sizeOpt(QStringLiteral("size"), QStringLiteral("Window size WxH."), QStringLiteral("size"));
    const QCommandLineOption delayOpt(QStringLiteral("delay"), QStringLiteral("Screenshot delay in ms."), QStringLiteral("ms"), QStringLiteral("1800"));
    const QCommandLineOption shotOpt(QStringLiteral("screenshot"), QStringLiteral("Render the window to a PNG and quit."), QStringLiteral("file"));
    parser.addOptions({demoOpt, themeOpt, pageOpt, stateOpt, updateOpt, sizeOpt, shotOpt, delayOpt});
    parser.addPositionalArgument(QStringLiteral("file"), QStringLiteral("An app file to install."));
    parser.process(app);

    const bool demo = parser.isSet(demoOpt) || !QFileInfo::exists(QStringLiteral("/run/initramfs/slots"));
    AppCenterEngine backend(demo);
    QString page = parser.value(pageOpt);
    if (demo && parser.isSet(stateOpt))
        backend.setDemoState(parser.value(stateOpt));
    if (!parser.positionalArguments().isEmpty()) {
        backend.openFile(parser.positionalArguments().first());
        page = QStringLiteral("file");
    }
    if (parser.isSet(updateOpt))
        backend.updateAll();

    QQmlApplicationEngine engine;
    AsimaUi::install(&engine);
    AppCenterEngine::instance = &backend;
    engine.setInitialProperties({
        {QStringLiteral("startPage"), page},
        {QStringLiteral("themeMode"), parser.value(themeOpt)},
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app, [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("AsimaAppCenter", "Main");
    if (engine.rootObjects().isEmpty())
        return 1;
    AsimaUi::finishWindow(&engine, parser.value(sizeOpt), parser.value(shotOpt), parser.value(delayOpt).toInt());
    return app.exec();
}

#include "main.moc"
