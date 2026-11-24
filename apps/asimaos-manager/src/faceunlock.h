#pragma once

#include <QQmlEngine>
#include <QtQml/qqmlregistration.h>

#include <QObject>
#include <QStringList>
#include <QTimer>

// Face sign-in through the Asima OS face service (system D-Bus, io.github.asimaos.Face1, asimaos-faced).
// The service owns the camera and the face models; this app only starts enrolment and shows its
// progress. No camera frame or face data ever passes through the app.
//
// Contract (see apps/DESIGN.md):
//   property  Camera            s        name of the camera face sign-in uses, empty when none
//   property  CameraKind        s        infrared | webcam (switched on in this app, D37) | ""
//   property  Webcam            s        a normal webcam's name, used or not
//   method    ListFaces()       → as     the caller's face models (the caller is the bus peer)
//   method    Enroll(s label)            polkit: io.github.asimaos.face.enroll (active local user)
//   method    Cancel()
//   method    DeleteAll()
//   signal    EnrollProgress(d progress, s hint)   hint: look | turn | closer | farther | hold
//   signal    EnrollDone(b ok, s message)
class FaceUnlock : public QObject
{
    Q_OBJECT
    QML_NAMED_ELEMENT(Face)
    QML_SINGLETON
    Q_PROPERTY(bool camera READ hasCamera NOTIFY changed)
    Q_PROPERTY(QString cameraName READ cameraName NOTIFY changed)
    Q_PROPERTY(QString cameraKind READ cameraKind NOTIFY changed)   // infrared | webcam | ""
    Q_PROPERTY(QString webcam READ webcam NOTIFY changed)           // a normal webcam, used or not
    Q_PROPERTY(bool service READ service NOTIFY changed)
    Q_PROPERTY(QStringList faces READ faces NOTIFY changed)
    Q_PROPERTY(bool enrolling READ enrolling NOTIFY changed)
    Q_PROPERTY(double progress READ progress NOTIFY changed)
    Q_PROPERTY(QString hint READ hint NOTIFY changed)
    Q_PROPERTY(QString message READ message NOTIFY changed)

public:
    // The one instance main() makes; QML gets it as the Face singleton (known at build time,
    // so qmllint and the QML compiler see its properties).
    static inline FaceUnlock *instance = nullptr;
    static FaceUnlock *create(QQmlEngine *, QJSEngine *)
    {
        QJSEngine::setObjectOwnership(instance, QJSEngine::CppOwnership);
        return instance;
    }

    explicit FaceUnlock(bool demo, QObject *parent = nullptr);

    bool hasCamera() const { return !m_camera.isEmpty(); }
    QString cameraName() const { return m_camera; }
    QString cameraKind() const { return m_kind; }
    QString webcam() const { return m_webcam; }
    bool service() const { return m_service; }
    QStringList faces() const { return m_faces; }
    bool enrolling() const { return m_enrolling; }
    double progress() const { return m_progress; }
    QString hint() const { return m_hint; }
    QString message() const;

    Q_INVOKABLE void refresh();
    Q_INVOKABLE void enroll();
    Q_INVOKABLE void cancel();
    Q_INVOKABLE void removeAll();

    // Demo: the sample device's camera ("infrared", "webcam" not switched on yet, or ""); state
    // "enrolling" starts mid-scan.
    void setDemo(const QString &camera, const QString &state);

signals:
    void changed();
    void finished(bool ok, const QString &message);

private slots:
    void onProgress(double progress, const QString &hint);
    void onDone(bool ok, const QString &message);

private:
    static QString findInfraredCamera();

    bool m_demo;
    bool m_service = false;
    bool m_enrolling = false;
    QString m_camera;
    QString m_kind;
    QString m_webcam;
    QStringList m_faces;
    double m_progress = 0.0;
    QString m_hint;
    QTimer m_demoTimer;
};
