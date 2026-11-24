// Asima OS password dialog (D37): the session's polkit agent, with fingerprint and face buttons.
// Started in place of KDE's agent (plasma-polkit-agent.service, drop-in). If it can't register,
// KDE's own agent takes over, so password dialogs always work.

#include "agent.h"
#include "asimaosui.h"

#include <PolkitQt1/Subject>

#include <QCommandLineParser>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QtQml/qqmlextensionplugin.h>

#include <sys/prctl.h>
#include <unistd.h>

Q_IMPORT_QML_PLUGIN(Asima_UiPlugin)

int main(int argc, char *argv[])
{
    // No other program of the session may attach to the dialog (a debugger, or one reading its
    // memory): it handles passwords and the fingerprint/face tokens. asimaos-faced checks this
    // before it gives a token (its /proc files belong to root then).
    prctl(PR_SET_DUMPABLE, 0, 0, 0, 0);
    AsimaUi::preApplicationSetup();
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("asimaos-polkit-agent"));
    app.setApplicationDisplayName(QStringLiteral("Asima OS"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setOrganizationDomain(QStringLiteral("asimaos.github.io"));
    app.setQuitOnLastWindowClosed(false);
    AsimaUi::applicationSetup();

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Asima OS password dialog (polkit agent)"));
    parser.addHelpOption();
    parser.addVersionOption();
    const QCommandLineOption demoOpt(QStringLiteral("demo"), QStringLiteral("Show a sample request instead of registering with polkit."));
    const QCommandLineOption themeOpt(QStringLiteral("theme"), QStringLiteral("system, light or dark."), QStringLiteral("mode"), QStringLiteral("system"));
    const QCommandLineOption pageOpt(QStringLiteral("page"), QStringLiteral("Ignored (screenshots)."), QStringLiteral("page"));
    const QCommandLineOption stateOpt(QStringLiteral("state"), QStringLiteral("Demo state: fingerprint, error."), QStringLiteral("state"));
    const QCommandLineOption sizeOpt(QStringLiteral("size"), QStringLiteral("Ignored: the dialog has its own size."), QStringLiteral("size"));
    const QCommandLineOption delayOpt(QStringLiteral("delay"), QStringLiteral("Screenshot delay in ms."), QStringLiteral("ms"), QStringLiteral("1800"));
    const QCommandLineOption shotOpt(QStringLiteral("screenshot"), QStringLiteral("Render the window to a PNG and quit."), QStringLiteral("file"));
    parser.addOptions({demoOpt, themeOpt, pageOpt, stateOpt, sizeOpt, shotOpt, delayOpt});
    parser.process(app);

    const bool demo = parser.isSet(demoOpt) || parser.isSet(shotOpt);
    Agent agent(demo);
    if (demo) {
        agent.setDemoState(parser.value(stateOpt));
    } else {
        const PolkitQt1::UnixSessionSubject subject(QCoreApplication::applicationPid());
        if (!agent.registerListener(subject, QStringLiteral("/io/github/asimaos/PolicyKit1/AuthenticationAgent"))) {
            qWarning("asimaos-polkit-agent: polkit refused the registration; starting KDE's agent instead");
            execl("/usr/lib/polkit-kde-authentication-agent-1", "polkit-kde-authentication-agent-1", static_cast<char *>(nullptr));
            return 1;
        }
    }

    QQmlApplicationEngine engine;
    AsimaUi::install(&engine);
    Agent::instance = &agent;
    engine.setInitialProperties({{QStringLiteral("themeMode"), parser.value(themeOpt)}});
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("AsimaPolkitAgent", "Main");
    if (engine.rootObjects().isEmpty())
        return 1;
    AsimaUi::finishWindow(&engine, QString(), parser.value(shotOpt), parser.value(delayOpt).toInt());
    return app.exec();
}
