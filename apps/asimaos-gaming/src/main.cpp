// Asima Gaming: power and scheduling for games, in-game helpers, controllers and the handheld's
// own controls.

#include "asimaosui.h"
#include "controllertester.h"
#include "gamingengine.h"

#include <QCommandLineParser>
#include <QFileInfo>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QtQml/qqmlextensionplugin.h>

Q_IMPORT_QML_PLUGIN(Asima_UiPlugin)

int main(int argc, char *argv[])
{
    AsimaUi::preApplicationSetup();
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("asimaos-gaming"));
    app.setApplicationDisplayName(QStringLiteral("Asima Gaming"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setOrganizationDomain(QStringLiteral("asimaos.github.io"));
    app.setDesktopFileName(QStringLiteral("io.github.asimaos.Gaming"));
    AsimaUi::applicationSetup();

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Asima Gaming"));
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption demoOpt(QStringLiteral("demo"), QStringLiteral("Show a sample device instead of this computer."));
    const QCommandLineOption themeOpt(QStringLiteral("theme"), QStringLiteral("system, light or dark."), QStringLiteral("mode"), QStringLiteral("system"));
    const QCommandLineOption pageOpt(QStringLiteral("page"), QStringLiteral("Page to open."), QStringLiteral("page"), QStringLiteral("gaming"));
    const QCommandLineOption stateOpt(QStringLiteral("state"), QStringLiteral("Demo state for screenshots: laptop."), QStringLiteral("state"));
    const QCommandLineOption sizeOpt(QStringLiteral("size"), QStringLiteral("Window size WxH."), QStringLiteral("size"));
    const QCommandLineOption delayOpt(QStringLiteral("delay"), QStringLiteral("Screenshot delay in ms."), QStringLiteral("ms"), QStringLiteral("1800"));
    const QCommandLineOption shotOpt(QStringLiteral("screenshot"), QStringLiteral("Render the window to a PNG and quit."), QStringLiteral("file"));
    parser.addOptions({demoOpt, themeOpt, pageOpt, stateOpt, sizeOpt, shotOpt, delayOpt});
    parser.process(app);

    // Outside Asima OS the sample device makes more sense than a half-empty view.
    // Tests that run the services on a private bus (ASIMAOS_SERVICES_BUS=session) count as a real system.
    const bool demo = parser.isSet(demoOpt) || (!QFileInfo::exists(QStringLiteral("/run/initramfs/slots"))
                                                && !qEnvironmentVariableIsSet("ASIMAOS_SERVICES_BUS"));
    GamingEngine backend(demo);
    ControllerTester tester(demo);
    if (demo && parser.value(stateOpt).split(QLatin1Char('-')).contains(QLatin1String("laptop")))
        backend.setDemoDevice(QStringLiteral("laptop"));

    QQmlApplicationEngine engine;
    AsimaUi::install(&engine);
    GamingEngine::instance = &backend;
    ControllerTester::instance = &tester;
    engine.setInitialProperties({
        {QStringLiteral("startPage"), parser.value(pageOpt)},
        {QStringLiteral("themeMode"), parser.value(themeOpt)},
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("AsimaGaming", "Main");
    if (engine.rootObjects().isEmpty())
        return 1;
    AsimaUi::finishWindow(&engine, parser.value(sizeOpt), parser.value(shotOpt), parser.value(delayOpt).toInt());
    return app.exec();
}
