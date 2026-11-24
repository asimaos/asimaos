#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QObject>
#include <QTimer>
#include <QVariantMap>

class QSocketNotifier;

// Live view of one game controller: buttons, sticks and triggers, read from its evdev node
// (/dev/input/eventN — logind gives the person at the seat access to game controllers).
// Can also make the controller rumble (force feedback).
class ControllerTester : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Tester)
    QML_SINGLETON
    Q_PROPERTY(bool active READ active NOTIFY activeChanged)
    Q_PROPERTY(QString device READ device NOTIFY activeChanged)
    Q_PROPERTY(QVariantMap state READ state NOTIFY stateChanged)

public:
    // The one instance main() makes; QML gets it as the Tester singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline ControllerTester *instance = nullptr;
    static ControllerTester *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit ControllerTester(bool demo, QObject *parent = nullptr);
    ~ControllerTester() override;

    bool active() const;
    QString device() const { return m_path; }
    QVariantMap state() const { return m_state; }

    // kind: xbox | playstation | nintendo | … (Xbox pads report the left/top buttons swapped)
    Q_INVOKABLE bool start(const QString &path, const QString &kind = QString());
    Q_INVOKABLE void stop();
    Q_INVOKABLE bool rumble(const QString &path, int milliseconds = 400);

signals:
    void activeChanged();
    void stateChanged();
    void error(const QString &message);

private:
    void readEvents();
    void demoStep();
    void resetState();
    double normalized(int code, int value, bool centred) const;

    bool m_demo;
    int m_fd = -1;
    bool m_swapXY = false;
    QString m_path;
    QSocketNotifier *m_notifier = nullptr;
    QVariantMap m_state;
    QTimer m_emitTimer;
    QTimer m_demoTimer;
    int m_demoTick = 0;
    struct Range { int min = -32768; int max = 32767; };
    Range m_ranges[0x40];
};
