// Asima Manager: the settings of this computer. Power and battery, display and lighting,
// sign-in with face and fingerprint, the hardware, and Asima Shield.

#include "asimaosui.h"
#include "faceunlock.h"
#include "fingerprint.h"
#include "managerengine.h"

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
    app.setApplicationName(QStringLiteral("asimaos-manager"));
    app.setApplicationDisplayName(QStringLiteral("Asima Manager"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setOrganizationDomain(QStringLiteral("asimaos.github.io"));
    app.setDesktopFileName(QStringLiteral("io.github.asimaos.Manager"));
    AsimaUi::applicationSetup();

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Asima Manager"));
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption demoOpt(QStringLiteral("demo"), QStringLiteral("Show a sample device instead of this computer."));
    const QCommandLineOption themeOpt(QStringLiteral("theme"), QStringLiteral("system, light or dark."), QStringLiteral("mode"), QStringLiteral("system"));
    const QCommandLineOption pageOpt(QStringLiteral("page"), QStringLiteral("Page to open."), QStringLiteral("page"), QStringLiteral("home"));
    const QCommandLineOption stateOpt(QStringLiteral("state"),
        QStringLiteral("Demo state for screenshots, dash-separated: laptop, ideapad (battery care at a level the laptop sets), webcam (a normal webcam not switched on), enrolling (fingerprint), face (face scan)."), QStringLiteral("state"));
    const QCommandLineOption sizeOpt(QStringLiteral("size"), QStringLiteral("Window size WxH."), QStringLiteral("size"));
    const QCommandLineOption delayOpt(QStringLiteral("delay"), QStringLiteral("Screenshot delay in ms."), QStringLiteral("ms"), QStringLiteral("1800"));
    const QCommandLineOption shotOpt(QStringLiteral("screenshot"), QStringLiteral("Render the window to a PNG and quit."), QStringLiteral("file"));
    parser.addOptions({demoOpt, themeOpt, pageOpt, stateOpt, sizeOpt, shotOpt, delayOpt});
    parser.process(app);

    // Outside Asima OS the sample device makes more sense than a half-empty view.
    // Tests that run the services on a private bus (ASIMAOS_SERVICES_BUS=session) count as a real system.
    const bool demo = parser.isSet(demoOpt) || (!QFileInfo::exists(QStringLiteral("/run/initramfs/slots"))
                                                && !qEnvironmentVariableIsSet("ASIMAOS_SERVICES_BUS"));
    ManagerEngine backend(demo);
    Fingerprint fingerprint(demo);
    FaceUnlock face(demo);
    if (demo) {
        const QStringList state = parser.value(stateOpt).split(QLatin1Char('-'), Qt::SkipEmptyParts);
        if (state.contains(QLatin1String("laptop")))
            backend.setDemoDevice(QStringLiteral("laptop"));
        if (state.contains(QLatin1String("ideapad")))
            backend.setDemoDevice(QStringLiteral("ideapad"));
        if (state.contains(QLatin1String("enrolling")))
            fingerprint.setDemoState(QStringLiteral("enrolling"));
        // The sample handheld has no camera; the sample laptop has an infrared one.
        face.setDemo(state.contains(QLatin1String("laptop")) ? QStringLiteral("infrared")
                     : state.contains(QLatin1String("webcam")) ? QStringLiteral("webcam") : QString(),
                     state.contains(QLatin1String("face")) ? QStringLiteral("enrolling") : QString());
    }

    QQmlApplicationEngine engine;
    AsimaUi::install(&engine);
    ManagerEngine::instance = &backend;
    Fingerprint::instance = &fingerprint;
    FaceUnlock::instance = &face;
    engine.setInitialProperties({
        {QStringLiteral("startPage"), parser.value(pageOpt)},
        {QStringLiteral("themeMode"), parser.value(themeOpt)},
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("AsimaManager", "Main");
    if (engine.rootObjects().isEmpty())
        return 1;
    AsimaUi::finishWindow(&engine, parser.value(sizeOpt), parser.value(shotOpt), parser.value(delayOpt).toInt());
    return app.exec();
}
