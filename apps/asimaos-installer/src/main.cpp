// Asima OS Installer: installs Asima OS from the live USB stick onto a whole disk.

#include "installerengine.h"
#include "asimaosui.h"

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
    app.setApplicationName(QStringLiteral("asimaos-installer"));
    app.setApplicationDisplayName(QStringLiteral("Install Asima OS"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setOrganizationDomain(QStringLiteral("asimaos.github.io"));
    app.setDesktopFileName(QStringLiteral("io.github.asimaos.Installer"));
    AsimaUi::applicationSetup();

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Asima OS Installer"));
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption demoOpt(QStringLiteral("demo"), QStringLiteral("Show sample data; nothing is installed."));
    const QCommandLineOption dryOpt(QStringLiteral("dry-run"), QStringLiteral("Use this computer, but the engine only simulates writing."));
    const QCommandLineOption fullOpt(QStringLiteral("fullscreen"), QStringLiteral("Fill the screen (live session)."));
    const QCommandLineOption themeOpt(QStringLiteral("theme"), QStringLiteral("system, light or dark."), QStringLiteral("mode"), QStringLiteral("system"));
    const QCommandLineOption pageOpt(QStringLiteral("page"), QStringLiteral("Step to open."), QStringLiteral("page"), QStringLiteral("welcome"));
    const QCommandLineOption stateOpt(QStringLiteral("state"), QStringLiteral("Demo state (for screenshots)."), QStringLiteral("state"));
    const QCommandLineOption sizeOpt(QStringLiteral("size"), QStringLiteral("Window size WxH."), QStringLiteral("size"));
    const QCommandLineOption shotOpt(QStringLiteral("screenshot"), QStringLiteral("Render the window to a PNG and quit."), QStringLiteral("file"));
    const QCommandLineOption delayOpt(QStringLiteral("delay"), QStringLiteral("Screenshot delay in ms."), QStringLiteral("ms"), QStringLiteral("1800"));
    const QCommandLineOption autoOpt(QStringLiteral("autotest-target"),
                                     QStringLiteral("Developer test: install at once onto TARGET (use with --dry-run)."), QStringLiteral("target"));
    parser.addOptions({demoOpt, dryOpt, fullOpt, themeOpt, pageOpt, stateOpt, sizeOpt, shotOpt, delayOpt, autoOpt});
    parser.process(app);

    // Away from the live USB stick there is nothing to install from, so fall back to the demo.
    const bool onLive = QFileInfo::exists(QStringLiteral("/run/initramfs/live/asimaos"))
                        || qEnvironmentVariableIsSet("ASIMAOS_INSTALL_ENGINE");
    const bool demo = parser.isSet(demoOpt) || (!onLive && !parser.isSet(dryOpt));
    InstallerEngine backend(demo, parser.isSet(dryOpt));
    const QString state = parser.value(stateOpt);
    if (demo && !state.isEmpty())
        backend.setDemoState(state);

    QQmlApplicationEngine engine;
    AsimaUi::install(&engine);
    InstallerEngine::instance = &backend;
    engine.setInitialProperties({
        {QStringLiteral("startPage"), parser.value(pageOpt)},
        {QStringLiteral("themeMode"), parser.value(themeOpt)},
        {QStringLiteral("demoState"), state},
        {QStringLiteral("autotestTarget"), parser.isSet(dryOpt) ? parser.value(autoOpt) : QString()},
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("AsimaInstaller", "Main");
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
