#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <PolkitQt1/Agent/Listener>
#include <PolkitQt1/Agent/Session>
#include <PolkitQt1/Identity>

#include <QPointer>
#include <QStringList>
#include <QVariantList>

// The Asima OS password dialog (D37): the session's polkit agent. It asks for the password as KDE's
// agent does, and offers a fingerprint and a face button when the account has them. A button asks
// asimaos-faced to check the finger or face (as root); its one-time token then goes to polkit's
// helper in place of the password, where pam_asimaos_face.so (use=admin) hands it back to
// asimaos-faced. Nothing biometric happens unless the person taps a button.
class Agent : public PolkitQt1::Agent::Listener
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Agent)
    QML_SINGLETON
    Q_PROPERTY(bool demo READ demo CONSTANT)
    Q_PROPERTY(bool active READ active NOTIFY changed)
    Q_PROPERTY(QString message READ message NOTIFY changed)
    Q_PROPERTY(QString actionId READ actionId NOTIFY changed)
    Q_PROPERTY(QVariantList identities READ identities NOTIFY changed)   // [{ name, user }]
    Q_PROPERTY(int identity READ identity NOTIFY changed)
    Q_PROPERTY(QString prompt READ prompt NOTIFY changed)
    Q_PROPERTY(bool waiting READ waiting NOTIFY changed)      // polkit waits for an answer
    Q_PROPERTY(QString error READ error NOTIFY changed)
    Q_PROPERTY(QStringList methods READ methods NOTIFY changed)    // "fingerprint", "face"
    Q_PROPERTY(QString checking READ checking NOTIFY changed)      // the button being checked, or ""

public:
    // The one instance main() makes; QML gets it as the Agent singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline Agent *instance = nullptr;
    static Agent *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit Agent(bool demo, QObject *parent = nullptr);

    bool demo() const { return m_demo; }
    bool active() const { return m_active; }
    QString message() const { return m_message; }
    QString actionId() const { return m_actionId; }
    QVariantList identities() const;
    int identity() const { return m_identity; }
    QString prompt() const { return m_prompt; }
    bool waiting() const { return m_waiting; }
    QString error() const { return m_error; }
    QStringList methods() const { return m_methods; }
    QString checking() const { return m_checking; }

    Q_INVOKABLE void submit(const QString &password);
    Q_INVOKABLE void cancel();
    Q_INVOKABLE void useBiometric(const QString &kind);
    Q_INVOKABLE void setIdentity(int index);

    // Screenshots: "" (asking), "fingerprint" (waiting for a finger), "error".
    void setDemoState(const QString &state);

    void initiateAuthentication(const QString &actionId, const QString &message, const QString &iconName,
                                const PolkitQt1::Details &details, const QString &cookie,
                                const PolkitQt1::Identity::List &identities,
                                PolkitQt1::Agent::AsyncResult *result) override;
    bool initiateAuthenticationFinish() override;
    void cancelAuthentication() override;

signals:
    void changed();

private:
    void startSession();
    void finish();
    void respond(const QString &answer);
    void readMethods();
    QString user() const;   // the account of the chosen identity

    bool m_demo;
    bool m_active = false;
    QString m_message, m_actionId, m_cookie, m_prompt, m_error, m_checking, m_pendingToken;
    PolkitQt1::Identity::List m_identityList;
    int m_identity = 0;
    bool m_waiting = false;
    QStringList m_methods;
    QPointer<PolkitQt1::Agent::Session> m_session;
    PolkitQt1::Agent::AsyncResult *m_result = nullptr;
};
