// Asima Rescue: repairs Asima OS when it can't start. Runs from its own signed UKI on the ESP,
// entirely in RAM, full screen and keyboard-first.

#include "asimaosui.h"
#include "rescueengine.h"

#include <QCommandLineParser>
#include <QFileInfo>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QTimer>
#include <QtQml/qqmlextensionplugin.h>

Q_IMPORT_QML_PLUGIN(Asima_UiPlugin)

int main(int argc, char *argv[])
{
    AsimaUi::preApplicationSetup();
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("asimaos-rescue"));
    app.setApplicationDisplayName(QStringLiteral("Asima Rescue"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setOrganizationDomain(QStringLiteral("asimaos.github.io"));
    AsimaUi::applicationSetup();

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Asima Rescue"));
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption demoOpt(QStringLiteral("demo"), QStringLiteral("Show a sample computer; nothing is changed."));
    const QCommandLineOption fullOpt(QStringLiteral("fullscreen"), QStringLiteral("Fill the screen (rescue system)."));
    const QCommandLineOption themeOpt(QStringLiteral("theme"), QStringLiteral("system, light or dark."), QStringLiteral("mode"), QStringLiteral("dark"));
    const QCommandLineOption pageOpt(QStringLiteral("page"), QStringLiteral("Page to open."), QStringLiteral("page"), QStringLiteral("home"));
    const QCommandLineOption stateOpt(QStringLiteral("state"), QStringLiteral("Demo state (for screenshots)."), QStringLiteral("state"));
    const QCommandLineOption sizeOpt(QStringLiteral("size"), QStringLiteral("Window size WxH."), QStringLiteral("size"));
    const QCommandLineOption shotOpt(QStringLiteral("screenshot"), QStringLiteral("Render the window to a PNG and quit."), QStringLiteral("file"));
    const QCommandLineOption delayOpt(QStringLiteral("delay"), QStringLiteral("Screenshot delay in ms."), QStringLiteral("ms"), QStringLiteral("1800"));
    parser.addOptions({demoOpt, fullOpt, themeOpt, pageOpt, stateOpt, sizeOpt, shotOpt, delayOpt});
    parser.process(app);

    // Only the rescue system has the engine; anywhere else the sample computer makes more sense.
    const bool inRescue = QFileInfo::exists(QStringLiteral("/usr/lib/asimaos/asimaos-rescue-engine"))
                          || qEnvironmentVariableIsSet("ASIMAOS_RESCUE_ENGINE");
    const bool demo = parser.isSet(demoOpt) || !inRescue;
    RescueEngine backend(demo);
    const QString state = parser.value(stateOpt);
    if (demo && !state.isEmpty())
        backend.setDemoState(state);

    QQmlApplicationEngine engine;
    AsimaUi::install(&engine);
    RescueEngine::instance = &backend;
    engine.setInitialProperties({
        {QStringLiteral("startPage"), parser.value(pageOpt)},
        {QStringLiteral("themeMode"), parser.value(themeOpt)},
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("AsimaRescue", "Main");
    if (engine.rootObjects().isEmpty())
        return 1;

    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().constFirst());
    if (window && parser.isSet(fullOpt))
        window->showFullScreen();
    if (window && parser.isSet(sizeOpt)) {
        const QStringList wh = parser.value(sizeOpt).split(QLatin1Char('x'));
        if (wh.size() == 2)
            window->resize(wh[0].toInt(), wh[1].toInt());
    }
    if (window && parser.isSet(shotOpt)) {
        const QString file = parser.value(shotOpt);
        QTimer::singleShot(parser.value(delayOpt).toInt(), &app, [window, file] {
            window->grabWindow().save(file);
            QCoreApplication::quit();
        });
    }
    return app.exec();
}
