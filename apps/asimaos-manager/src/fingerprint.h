#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QObject>
#include <QStringList>
#include <QTimer>
#include <QVariantList>

// Fingerprint sign-in through fprintd (system D-Bus, net.reactivated.Fprint). Enrolling is the
// user's own action; fprintd asks polkit, so no root helper is involved. A reader plugged in or
// removed while the app is open shows up by itself (the USB device list is watched).
class Fingerprint : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Fingerprint)
    QML_SINGLETON
    Q_PROPERTY(bool available READ available NOTIFY changed)
    Q_PROPERTY(QString reader READ reader NOTIFY changed)
    Q_PROPERTY(QStringList enrolled READ enrolled NOTIFY changed)
    Q_PROPERTY(bool enrolling READ enrolling NOTIFY changed)
    Q_PROPERTY(QString finger READ finger NOTIFY changed)
    Q_PROPERTY(int stage READ stage NOTIFY changed)
    Q_PROPERTY(int stages READ stages NOTIFY changed)
    Q_PROPERTY(QString message READ message NOTIFY changed)
    Q_PROPERTY(QVariantList fingers READ fingers CONSTANT)

public:
    // The one instance main() makes; QML gets it as the Fingerprint singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline Fingerprint *instance = nullptr;
    static Fingerprint *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit Fingerprint(bool demo, QObject *parent = nullptr);

    bool available() const { return m_available; }
    QString reader() const { return m_reader; }
    QStringList enrolled() const { return m_enrolled; }
    bool enrolling() const { return m_enrolling; }
    QString finger() const { return m_finger; }
    int stage() const { return m_stage; }
    int stages() const { return m_stages; }
    QString message() const { return m_message; }
    QVariantList fingers() const;          // [{ id, title }] in fprintd naming
    Q_INVOKABLE QString fingerTitle(const QString &id) const;

    Q_INVOKABLE void refresh();
    Q_INVOKABLE void enroll(const QString &finger);
    Q_INVOKABLE void cancel();
    Q_INVOKABLE void removeAll();

    void setDemoState(const QString &state);

signals:
    void changed();
    void finished(bool ok, const QString &message);

private slots:
    void onEnrollStatus(const QString &result, bool done);

private:
    void endEnroll();
    QString user() const;

    bool m_demo;
    bool m_available = false;
    bool m_enrolling = false;
    QString m_device;
    QString m_reader;
    QStringList m_enrolled;
    QString m_finger;
    int m_stage = 0;
    int m_stages = 8;
    QString m_message;
    QTimer m_demoTimer;
    QTimer m_watch;          // USB devices changed → ask fprintd again
    QStringList m_usb;
};
