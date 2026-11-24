// Asima Updater: installs Asima OS update files and chooses which version starts.

#include "asimaosui.h"
#include "updaterengine.h"

#include <QCommandLineParser>
#include <QDir>
#include <QFileInfo>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QTimer>
#include <QUrl>
#include <QtQml/qqmlextensionplugin.h>

Q_IMPORT_QML_PLUGIN(Asima_UiPlugin)

int main(int argc, char *argv[])
{
    AsimaUi::preApplicationSetup();
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("asimaos-updater"));
    app.setApplicationDisplayName(QStringLiteral("Asima Updater"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setOrganizationDomain(QStringLiteral("asimaos.github.io"));
    app.setDesktopFileName(QStringLiteral("io.github.asimaos.Updater"));
    AsimaUi::applicationSetup();

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Asima Updater"));
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption demoOpt(QStringLiteral("demo"), QStringLiteral("Show sample data instead of this computer."));
    const QCommandLineOption themeOpt(QStringLiteral("theme"), QStringLiteral("system, light or dark."), QStringLiteral("mode"), QStringLiteral("system"));
    const QCommandLineOption pageOpt(QStringLiteral("page"), QStringLiteral("Page to open."), QStringLiteral("page"), QStringLiteral("updates"));
    const QCommandLineOption stateOpt(QStringLiteral("state"), QStringLiteral("Demo state (for screenshots)."), QStringLiteral("state"));
    const QCommandLineOption sizeOpt(QStringLiteral("size"), QStringLiteral("Window size WxH."), QStringLiteral("size"));
    const QCommandLineOption delayOpt(QStringLiteral("delay"), QStringLiteral("Screenshot delay in ms."), QStringLiteral("ms"), QStringLiteral("1800"));
    const QCommandLineOption autoUpdateOpt(QStringLiteral("autotest-update"),
        QStringLiteral("Developer test: verify and install FILE through the service at once."), QStringLiteral("file"));
    const QCommandLineOption shotOpt(QStringLiteral("screenshot"), QStringLiteral("Render the window to a PNG and quit."), QStringLiteral("file"));
    const QCommandLineOption troubleOpt(QStringLiteral("trouble"),
        QStringLiteral("The desktop kept failing on this version since its update (the sign-in safety net): offer going back."),
        QStringLiteral("version"));
    parser.addOptions({demoOpt, themeOpt, pageOpt, stateOpt, sizeOpt, shotOpt, autoUpdateOpt, delayOpt, troubleOpt});
    parser.addPositionalArgument(QStringLiteral("file"), QStringLiteral("A .aosupd update file to check and install."));
    parser.process(app);

    // Outside Asima OS there is nothing real to show, so fall back to the demo. Tests that run the
    // service on a private bus (ASIMAOS_SERVICES_BUS=session) count as a real system.
    const bool onAsima = QFileInfo::exists(QStringLiteral("/run/initramfs/slots"))
                          || qEnvironmentVariableIsSet("ASIMAOS_SERVICES_BUS");
    UpdaterEngine backend(parser.isSet(demoOpt) || !onAsima);
    if (parser.isSet(stateOpt))
        backend.setDemoState(parser.value(stateOpt));
    if (parser.isSet(autoUpdateOpt)) {
        // Choose the file, and once the service has verified it, install it.
        const QUrl file = QUrl::fromLocalFile(parser.value(autoUpdateOpt));
        QTimer::singleShot(300, &backend, [&backend, file] { backend.selectUpdateFile(file); });
        QObject::connect(&backend, &UpdaterEngine::pendingUpdateChanged, &backend, [&backend] {
            static bool started = false;
            if (!started && backend.pendingUpdate().value("state").toString() == QLatin1String("ready")) {
                started = true;
                backend.installUpdate();
            }
        });
    }

    QQmlApplicationEngine engine;
    AsimaUi::install(&engine);
    UpdaterEngine::instance = &backend;
    // Opened with an update file (double-click in Dolphin): straight to Updates, checking it.
    const QStringList files = parser.positionalArguments();
    if (!files.isEmpty()) {
        const QUrl file = QUrl::fromUserInput(files.first(), QDir::currentPath(), QUrl::AssumeLocalFile);
        QTimer::singleShot(300, &backend, [&backend, file] { backend.selectUpdateFile(file); });
    }
    engine.setInitialProperties({
        {QStringLiteral("startPage"), parser.isSet(troubleOpt) ? QStringLiteral("versions")
                                      : files.isEmpty() ? parser.value(pageOpt) : QStringLiteral("updates")},
        {QStringLiteral("themeMode"), parser.value(themeOpt)},
        {QStringLiteral("trouble"), parser.value(troubleOpt)},
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("AsimaUpdater", "Main");
    if (engine.rootObjects().isEmpty())
        return 1;
    AsimaUi::finishWindow(&engine, parser.value(sizeOpt), parser.value(shotOpt), parser.value(delayOpt).toInt());
    return app.exec();
}
