// Asima Maintenance: health, security, snapshots & backup, system changes and reset for Asima OS.

#include "asimaosui.h"
#include "maintenanceengine.h"

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
    app.setApplicationName(QStringLiteral("asimaos-maintenance"));
    app.setApplicationDisplayName(QStringLiteral("Asima Maintenance"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setOrganizationDomain(QStringLiteral("asimaos.github.io"));
    app.setDesktopFileName(QStringLiteral("io.github.asimaos.Maintenance"));
    AsimaUi::applicationSetup();

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Asima Maintenance"));
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption demoOpt(QStringLiteral("demo"), QStringLiteral("Show sample data instead of this computer."));
    const QCommandLineOption themeOpt(QStringLiteral("theme"), QStringLiteral("system, light or dark."), QStringLiteral("mode"), QStringLiteral("system"));
    const QCommandLineOption pageOpt(QStringLiteral("page"), QStringLiteral("Page to open."), QStringLiteral("page"), QStringLiteral("overview"));
    const QCommandLineOption stateOpt(QStringLiteral("state"), QStringLiteral("Demo state (for screenshots)."), QStringLiteral("state"));
    const QCommandLineOption sizeOpt(QStringLiteral("size"), QStringLiteral("Window size WxH."), QStringLiteral("size"));
    const QCommandLineOption delayOpt(QStringLiteral("delay"), QStringLiteral("Screenshot delay in ms."), QStringLiteral("ms"), QStringLiteral("1800"));
    const QCommandLineOption shotOpt(QStringLiteral("screenshot"), QStringLiteral("Render the window to a PNG and quit."), QStringLiteral("file"));
    parser.addOptions({demoOpt, themeOpt, pageOpt, stateOpt, sizeOpt, shotOpt, delayOpt});
    parser.process(app);

    // Outside Asima OS there is nothing real to show, so fall back to the demo. Tests that run the
    // service on a private bus (ASIMAOS_SERVICES_BUS=session) count as a real system.
    const bool onAsima = QFileInfo::exists(QStringLiteral("/run/initramfs/slots"))
                          || qEnvironmentVariableIsSet("ASIMAOS_SERVICES_BUS");
    MaintenanceEngine backend(parser.isSet(demoOpt) || !onAsima);
    if (parser.isSet(stateOpt))
        backend.setDemoState(parser.value(stateOpt));

    QQmlApplicationEngine engine;
    AsimaUi::install(&engine);
    MaintenanceEngine::instance = &backend;
    engine.setInitialProperties({
        {QStringLiteral("startPage"), parser.value(pageOpt)},
        {QStringLiteral("themeMode"), parser.value(themeOpt)},
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("AsimaMaintenance", "Main");
    if (engine.rootObjects().isEmpty())
        return 1;
    AsimaUi::finishWindow(&engine, parser.value(sizeOpt), parser.value(shotOpt), parser.value(delayOpt).toInt());
    return app.exec();
}
