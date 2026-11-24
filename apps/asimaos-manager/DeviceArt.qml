pragma ComponentBehavior: Bound
import QtQuick
import Asima.Ui

// A small drawn silhouette of this device for the Home hero: handheld, laptop, tablet or desktop.
// Light shapes with the hero colour as "ink" for screens and controls.
Item {
    id: art
    property string type: "desktop"
    property color body: "#FFFFFF"
    property color ink: Theme.heroMid
    implicitWidth: 76
    implicitHeight: 56

    // Handheld: wide body, screen in the middle, stick + d-pad left, buttons + stick right.
    Item {
        visible: art.type === "handheld"
        anchors.centerIn: parent
        width: 76
        height: 36
        Rectangle { anchors.fill: parent; radius: 15; color: art.body }
        Rectangle { anchors.centerIn: parent; width: 36; height: 25; radius: 3; color: art.ink }
        Rectangle { x: 6; y: 5; width: 10; height: 10; radius: 5; color: "transparent"; border.width: 2; border.color: art.ink }
        Rectangle { x: 7; y: 24; width: 9; height: 3; radius: 1; color: art.ink }
        Rectangle { x: 10; y: 21; width: 3; height: 9; radius: 1; color: art.ink }
        Repeater {
            model: [[64, 5], [60, 9], [68, 9], [64, 13]]
            delegate: Rectangle {
                required property var modelData
                x: modelData[0] - 1.5
                y: modelData[1] - 1.5
                width: 3.5
                height: 3.5
                radius: 1.75
                color: art.ink
            }
        }
        Rectangle { x: 59; y: 20; width: 10; height: 10; radius: 5; color: "transparent"; border.width: 2; border.color: art.ink }
    }

    // Laptop: open lid with a screen, thin base with a notch.
    Item {
        visible: art.type === "laptop"
        anchors.centerIn: parent
        width: 74
        height: 46
        Rectangle {
            id: lid
            anchors.horizontalCenter: parent.horizontalCenter
            width: 56
            height: 38
            radius: 4
            color: art.body
            Rectangle { anchors.fill: parent; anchors.margins: 4; anchors.bottomMargin: 5; radius: 2; color: art.ink }
        }
        Rectangle {
            anchors.top: lid.bottom
            anchors.topMargin: 1
            width: parent.width
            height: 6
            radius: 3
            color: art.body
            Rectangle { anchors.horizontalCenter: parent.horizontalCenter; width: 14; height: 2; radius: 1; color: art.ink }
        }
    }

    // Tablet: portrait slab.
    Rectangle {
        visible: art.type === "tablet"
        anchors.centerIn: parent
        width: 40
        height: 54
        radius: 7
        color: art.body
        Rectangle { anchors.fill: parent; anchors.margins: 4; radius: 3; color: art.ink }
    }

    // Desktop: monitor on a stand.
    Item {
        visible: art.type !== "handheld" && art.type !== "laptop" && art.type !== "tablet"
        anchors.centerIn: parent
        width: 62
        height: 52
        Rectangle {
            id: monitor
            width: parent.width
            height: 40
            radius: 4
            color: art.body
            Rectangle { anchors.fill: parent; anchors.margins: 4; radius: 2; color: art.ink }
        }
        Rectangle { anchors.horizontalCenter: parent.horizontalCenter; anchors.top: monitor.bottom; width: 8; height: 8; color: art.body }
        Rectangle { anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; width: 28; height: 4; radius: 2; color: art.body }
    }
}
