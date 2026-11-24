#include "agent.h"

#include <PolkitQt1/Details>

#include <QDBusConnection>
#include <utility>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QVariantMap>

#include <pwd.h>
#include <unistd.h>

namespace {
const QString kFace = QStringLiteral("io.github.asimaos.Face1");
const QString kFacePath = QStringLiteral("/io/github/asimaos/Face1");

// "unix-user:alice" (or a uid) → the account name.
QString accountOf(const PolkitQt1::Identity &identity)
{
    const QString id = identity.toString();
    if (!id.startsWith(QLatin1String("unix-user:")))
        return {};
    const QString value = id.mid(10);
    bool numeric = false;
    const uint uid = value.toUInt(&numeric);
    if (!numeric)
        return value;
    const passwd *pw = getpwuid(uid);
    return pw ? QString::fromLocal8Bit(pw->pw_name) : value;
}

// The person's full name from the account, else the account name.
QString fullName(const QString &account)
{
    const passwd *pw = getpwnam(account.toLocal8Bit().constData());
    const QString gecos = pw && pw->pw_gecos ? QString::fromLocal8Bit(pw->pw_gecos).section(QLatin1Char(','), 0, 0) : QString();
    return gecos.isEmpty() ? account : gecos;
}
} // namespace

Agent::Agent(bool demo, QObject *parent)
    : PolkitQt1::Agent::Listener(parent)
    , m_demo(demo)
{
    if (m_demo)
        setDemoState(QString());
}

QVariantList Agent::identities() const
{
    QVariantList out;
    if (m_demo)
        return {QVariantMap{{"name", QStringLiteral("Debu")}, {"user", QStringLiteral("debu")}}};
    for (const PolkitQt1::Identity &i : m_identityList) {
        const QString account = accountOf(i);
        if (!account.isEmpty())
            out.append(QVariantMap{{"name", fullName(account)}, {"user", account}});
    }
    return out;
}

QString Agent::user() const
{
    return m_identity < m_identityList.size() ? accountOf(m_identityList.at(m_identity)) : QString();
}

void Agent::setDemoState(const QString &state)
{
    m_active = true;
    m_message = tr("Changing how visible this computer is on the network needs your password");
    m_actionId = QStringLiteral("io.github.asimaos.manager.shield");
    m_prompt = QStringLiteral("Password:");
    m_waiting = true;
    m_methods = {QStringLiteral("fingerprint"), QStringLiteral("face")};
    m_checking = state == QLatin1String("fingerprint") ? state : QString();
    m_error = state == QLatin1String("error") ? tr("That password wasn't right. Try again.") : QString();
    emit changed();
}

// ───────────────────────────────────────────────────────────────────────── polkit

void Agent::initiateAuthentication(const QString &actionId, const QString &message, const QString &,
                                   const PolkitQt1::Details &, const QString &cookie,
                                   const PolkitQt1::Identity::List &identities, PolkitQt1::Agent::AsyncResult *result)
{
    if (m_active) {
        result->setError(tr("Another request is waiting for a password"));
        result->setCompleted();
        return;
    }
    m_active = true;
    m_actionId = actionId;
    m_message = message;
    m_cookie = cookie;
    m_identityList = identities;
    m_result = result;
    m_error.clear();
    // The person signed in, when they may approve; else the first administrator.
    m_identity = 0;
    const passwd *me = getpwuid(getuid());
    for (int i = 0; me && i < identities.size(); ++i)
        if (accountOf(identities.at(i)) == QString::fromLocal8Bit(me->pw_name))
            m_identity = i;
    startSession();
}

bool Agent::initiateAuthenticationFinish()
{
    return true;
}

void Agent::cancelAuthentication()
{
    // polkit gave up (the app went away or a time limit passed).
    if (m_session)
        m_session->cancel();
    finish();
}

void Agent::startSession()
{
    if (m_session)
        m_session->deleteLater();
    m_waiting = false;
    m_prompt.clear();
    m_pendingToken.clear();
    m_checking.clear();
    if (m_identityList.isEmpty()) {
        finish();
        return;
    }
    auto *session = new PolkitQt1::Agent::Session(m_identityList.at(m_identity), m_cookie, m_result, this);
    m_session = session;
    connect(session, &PolkitQt1::Agent::Session::request, this, [this, session](const QString &request, bool) {
        if (session != m_session)
            return;
        m_prompt = request;
        m_waiting = true;
        emit changed();
        if (!m_pendingToken.isEmpty())
            respond(std::exchange(m_pendingToken, QString()));
    });
    connect(session, &PolkitQt1::Agent::Session::completed, this, [this, session](bool gained) {
        if (session != m_session)
            return;
        if (gained) {
            finish();
            return;
        }
        // Wrong password (or a token that failed): ask again with a fresh session.
        m_error = m_error.isEmpty() ? tr("That wasn't right. Try again.") : m_error;
        startSession();
    });
    connect(session, &PolkitQt1::Agent::Session::showError, this, [this](const QString &text) {
        m_error = text;
        emit changed();
    });
    readMethods();
    emit changed();
    session->initiate();
}

void Agent::finish()
{
    if (m_result) {
        m_result->setCompleted();
        m_result = nullptr;
    }
    if (m_session)
        m_session->deleteLater();
    m_active = false;
    m_waiting = false;
    m_checking.clear();
    m_pendingToken.clear();
    m_identityList.clear();
    emit changed();
}

void Agent::respond(const QString &answer)
{
    if (!m_session || !m_waiting)
        return;
    m_waiting = false;
    m_session->setResponse(answer);
    emit changed();
}

// ───────────────────────────────────────────────────────────────────────── actions

void Agent::submit(const QString &password)
{
    if (m_demo)
        return;
    m_error.clear();
    respond(password);
}

void Agent::cancel()
{
    if (m_demo)
        return;
    if (m_session)
        m_session->cancel();
    finish();
}

void Agent::setIdentity(int index)
{
    if (m_demo || index == m_identity || index < 0 || index >= m_identityList.size())
        return;
    m_identity = index;
    m_error.clear();
    startSession();
}

// Which buttons this account gets: asimaos-faced knows (a reader with a finger, a face and a camera).
void Agent::readMethods()
{
    m_methods.clear();
    QDBusMessage m = QDBusMessage::createMethodCall(kFace, kFacePath, kFace, QStringLiteral("AdminMethods"));
    m << user();
    auto *w = new QDBusPendingCallWatcher(QDBusConnection::systemBus().asyncCall(m, 10000), this);
    const QString account = user();
    connect(w, &QDBusPendingCallWatcher::finished, this, [this, w, account] {
        const QDBusPendingReply<QStringList> r = *w;
        w->deleteLater();
        if (m_active && account == user() && r.isValid()) {
            m_methods = r.value();
            emit changed();
        }
    });
}

void Agent::useBiometric(const QString &kind)
{
    if (m_demo || !m_active || !m_checking.isEmpty())
        return;
    m_checking = kind;
    m_error.clear();
    emit changed();
    QDBusMessage m = QDBusMessage::createMethodCall(kFace, kFacePath, kFace, QStringLiteral("AdminToken"));
    m << kind << user();
    auto *w = new QDBusPendingCallWatcher(QDBusConnection::systemBus().asyncCall(m, 60000), this);
    const QString cookie = m_cookie;
    connect(w, &QDBusPendingCallWatcher::finished, this, [this, w, cookie] {
        const QDBusPendingReply<QString> r = *w;
        w->deleteLater();
        if (!m_active || cookie != m_cookie)
            return;
        m_checking.clear();
        if (r.isError()) {
            m_error = r.error().message();
            emit changed();
            return;
        }
        if (m_waiting)
            respond(r.value());
        else
            m_pendingToken = r.value();
        emit changed();
    });
}
