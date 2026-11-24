pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Effects
import Asima.Ui

// Face enrolment visual: an infrared-style viewport with a sweeping scan line, ringed by ticks that
// light up as the face service reports progress. Purely illustrative: no camera frames are shown.
Item {
    id: scan
    property real progress: 0          // 0..1
    property bool active: false        // scanning right now
    property int ticks: 60
    readonly property real view: width - 44

    implicitWidth: 220
    implicitHeight: 220

    // ── Progress ring ────────────────────────────────────────────────────────
    Repeater {
        model: scan.ticks
        delegate: Item {
            id: tick
            required property int index
            anchors.fill: parent
            rotation: index * 360 / scan.ticks
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                y: 2
                width: 3
                height: 12
                radius: 1.5
                color: tick.index < Math.round(scan.progress * scan.ticks) ? Theme.accent : Theme.track
                Behavior on color { ColorAnimation { duration: 160 } }
            }
        }
    }

    // ── Viewport (masked to a circle) ────────────────────────────────────────
    Item {
        id: viewport
        anchors.centerIn: parent
        width: scan.view
        height: scan.view
        visible: false
        layer.enabled: true

        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.0; color: "#1B2029" }
                GradientStop { position: 1.0; color: "#0C0F14" }
            }
        }
        // Head and shoulders in the grey tones of an infrared picture
        Rectangle {
            id: head
            anchors.horizontalCenter: parent.horizontalCenter
            y: parent.height * 0.2
            width: parent.width * 0.34
            height: width * 1.16
            radius: width / 2
            gradient: Gradient {
                GradientStop { position: 0.0; color: "#A9B1BE" }
                GradientStop { position: 1.0; color: "#6E7685" }
            }
        }
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: parent.height * 0.68
            width: parent.width * 0.76
            height: parent.height * 0.5
            radius: width * 0.38
            gradient: Gradient {
                GradientStop { position: 0.0; color: "#8D95A3" }
                GradientStop { position: 1.0; color: "#4F5664" }
            }
        }
        // Tracking points the scanner "locks on" to
        Repeater {
            model: [[0.36, 0.42], [0.64, 0.42], [0.5, 0.56], [0.38, 0.7], [0.62, 0.7]]
            delegate: Rectangle {
                required property var modelData
                x: head.x + head.width * modelData[0] - 3
                y: head.y + head.height * modelData[1] - 3
                width: 6
                height: 6
                radius: 3
                color: Theme.accent
                visible: scan.active
                SequentialAnimation on opacity {
                    running: scan.active
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.35; duration: 700; easing.type: Easing.InOutSine }
                    NumberAnimation { to: 1.0; duration: 700; easing.type: Easing.InOutSine }
                }
            }
        }
        // Scan line with a soft trail
        Item {
            id: sweep
            visible: scan.active
            width: parent.width
            height: 48
            y: parent.height * 0.5 - height
            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop { position: 0.0; color: "transparent" }
                    GradientStop { position: 1.0; color: Qt.alpha(Theme.accent, 0.32) }
                }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 3
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: "transparent" }
                    GradientStop { position: 0.5; color: Theme.accent }
                    GradientStop { position: 1.0; color: "transparent" }
                }
            }
            SequentialAnimation on y {
                running: scan.active
                loops: Animation.Infinite
                NumberAnimation { from: -sweep.height; to: viewport.height; duration: 1800; easing.type: Easing.InOutQuad }
                PauseAnimation { duration: 200 }
            }
        }
    }
    Rectangle {
        id: circleMask
        anchors.fill: viewport
        radius: width / 2
        visible: false
        layer.enabled: true
    }
    MultiEffect {
        anchors.fill: viewport
        source: viewport
        maskEnabled: true
        maskSource: circleMask
    }
    Rectangle {
        anchors.fill: viewport
        radius: width / 2
        color: "transparent"
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, 0.08)
    }

    // Done: a check badge
    Rectangle {
        visible: !scan.active && scan.progress >= 1
        anchors.right: viewport.right
        anchors.bottom: viewport.bottom
        width: 38
        height: 38
        radius: 19
        color: Theme.ok
        border.width: 3
        border.color: Theme.surface
        Icon { anchors.centerIn: parent; name: "checkmark"; size: 18; color: "#FFFFFF" }
    }
}
