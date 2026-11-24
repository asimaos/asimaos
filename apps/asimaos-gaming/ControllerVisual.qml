pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Effects
import Asima.Ui

// A drawn game controller that lights up pressed buttons and moves its sticks.
// buttons: { a, b, x, y, lb, rb, ls, rs, start, select, home, up, down, left, right, lx, ly, rx, ry, lt, rt }
Item {
    id: pad
    property var buttons: ({})
    property string kind: "xbox"        // xbox | playstation | nintendo | builtin | generic

    readonly property color body: Theme.dark ? "#262C37" : "#E9ECF1"
    readonly property color part: Theme.dark ? "#323A47" : "#FFFFFF"
    readonly property color partLine: Theme.dark ? "#424B5A" : "#D3D8E0"
    readonly property var faceLabels: kind === "playstation" ? { a: "✕", b: "○", x: "□", y: "△" }
                                    : kind === "nintendo" ? { a: "B", b: "A", x: "Y", y: "X" }
                                    : { a: "A", b: "B", x: "X", y: "Y" }
    function on(key) { return !!pad.buttons[key] }
    function axis(key) { return pad.buttons[key] || 0 }

    implicitWidth: 440
    implicitHeight: 270

    // Triggers: analogue gauges above the bumpers
    Repeater {
        model: [{ key: "lt", x: 70, label: "LT" }, { key: "rt", x: 270, label: "RT" }]
        delegate: Item {
            required property var modelData
            x: modelData.x
            y: 0
            width: 100
            height: 14
            Rectangle { anchors.fill: parent; radius: 7; color: pad.partLine }
            Rectangle {
                width: Math.max(14, parent.width * pad.axis(parent.modelData.key))
                height: parent.height
                radius: 7
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: Theme.brandFrom }
                    GradientStop { position: 1; color: Theme.brandMid }
                }
                opacity: pad.axis(parent.modelData.key) > 0.02 ? 1 : 0.0
            }
            UiText {
                anchors.centerIn: parent
                text: parent.modelData.label + "  " + Math.round(pad.axis(parent.modelData.key) * 100) + "%"
                variant: "caption"
                font.weight: Font.Bold
                color: pad.axis(parent.modelData.key) > 0.45 ? "#FFFFFF" : Theme.textMuted
            }
        }
    }

    // Bumpers
    Repeater {
        model: [{ key: "lb", x: 66 }, { key: "rb", x: 266 }]
        delegate: Rectangle {
            required property var modelData
            x: modelData.x
            y: 24
            width: 108
            height: 26
            radius: 13
            color: pad.on(modelData.key) ? Theme.accent : pad.part
            border.width: 1
            border.color: pad.on(modelData.key) ? Theme.accent : pad.partLine
            UiText {
                anchors.centerIn: parent
                text: parent.modelData.key.toUpperCase()
                variant: "caption"
                font.weight: Font.Bold
                color: pad.on(parent.modelData.key) ? "#FFFFFF" : Theme.textMuted
            }
        }
    }

    // Body + grips, merged into one shape with a soft shadow
    Item {
        id: shell
        anchors.fill: parent
        layer.enabled: true
        layer.effect: MultiEffect {
            shadowEnabled: true
            shadowOpacity: Theme.dark ? 0.5 : 0.18
            shadowBlur: 0.7
            shadowVerticalOffset: 4
            autoPaddingEnabled: true
        }
        Rectangle { x: 20; y: 44; width: 400; height: 150; radius: 75; color: pad.body }
        Rectangle { x: 34; y: 120; width: 132; height: 140; radius: 66; color: pad.body }
        Rectangle { x: 274; y: 120; width: 132; height: 140; radius: 66; color: pad.body }
    }

    // Stick (inline components can't see this file's ids, so everything comes in as properties)
    component Stick: Item {
        id: stick
        property bool pressed: false
        property real ax: 0
        property real ay: 0
        property color partColor
        property color lineColor
        width: 64
        height: 64
        Rectangle { anchors.fill: parent; radius: 32; color: stick.lineColor; opacity: 0.6 }
        Rectangle {
            width: 44
            height: 44
            radius: 22
            x: (stick.width - width) / 2 + stick.ax * 11
            y: (stick.height - height) / 2 + stick.ay * 11
            color: stick.pressed ? Theme.accent : stick.partColor
            border.width: 1
            border.color: stick.lineColor
            Rectangle {
                anchors.centerIn: parent
                width: 18
                height: 18
                radius: 9
                color: "transparent"
                border.width: 2
                border.color: stick.pressed ? "#FFFFFF" : stick.lineColor
            }
        }
    }

    Stick { x: 92; y: 78; pressed: pad.on("ls"); ax: pad.axis("lx"); ay: pad.axis("ly"); partColor: pad.part; lineColor: pad.partLine }
    Stick { x: 248; y: 150; pressed: pad.on("rs"); ax: pad.axis("rx"); ay: pad.axis("ry"); partColor: pad.part; lineColor: pad.partLine }

    // D-pad
    Repeater {
        model: [{ key: "up", x: 0, y: -1 }, { key: "down", x: 0, y: 1 }, { key: "left", x: -1, y: 0 }, { key: "right", x: 1, y: 0 }]
        delegate: Rectangle {
            required property var modelData
            x: 148 + modelData.x * 22
            y: 168 + modelData.y * 22
            width: 22
            height: 22
            radius: 5
            color: pad.on(modelData.key) ? Theme.accent : pad.part
            border.width: 1
            border.color: pad.on(modelData.key) ? Theme.accent : pad.partLine
        }
    }

    // Face buttons
    Repeater {
        model: [{ key: "y", x: 0, y: -1 }, { key: "a", x: 0, y: 1 }, { key: "x", x: -1, y: 0 }, { key: "b", x: 1, y: 0 }]
        delegate: Rectangle {
            required property var modelData
            x: 312 + modelData.x * 28
            y: 96 + modelData.y * 28
            width: 30
            height: 30
            radius: 15
            color: pad.on(modelData.key) ? Theme.accent : pad.part
            border.width: 1
            border.color: pad.on(modelData.key) ? Theme.accent : pad.partLine
            scale: pad.on(modelData.key) ? 0.92 : 1.0
            Behavior on scale { NumberAnimation { duration: 60 } }
            UiText {
                anchors.centerIn: parent
                text: pad.faceLabels[parent.modelData.key]
                font.weight: Font.Bold
                color: pad.on(parent.modelData.key) ? "#FFFFFF" : Theme.textMuted
            }
        }
    }

    // View / Menu / Home
    Rectangle { x: 186; y: 100; width: 26; height: 14; radius: 7; color: pad.on("select") ? Theme.accent : pad.part; border.width: 1; border.color: pad.partLine }
    Rectangle { x: 228; y: 100; width: 26; height: 14; radius: 7; color: pad.on("start") ? Theme.accent : pad.part; border.width: 1; border.color: pad.partLine }
    Rectangle {
        x: 206
        y: 126
        width: 28
        height: 28
        radius: 14
        color: pad.on("home") ? Theme.accent : pad.part
        border.width: 1
        border.color: pad.partLine
        AsimaMark { anchors.centerIn: parent; size: 18; opacity: pad.on("home") ? 1 : 0.85 }
    }
}
