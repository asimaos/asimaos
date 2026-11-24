pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

// RGB lighting (D34): only on devices with lights the kernel drives, such as a DualSense light bar
// or a handheld's joystick rings. One colour for all of them, kept for lights that connect later.
DPage {
    id: page
    title: qsTr("Lighting")
    subtitle: qsTr("One colour for every light on this device and its controllers.")

    readonly property var lit: Backend.lighting
    readonly property var lights: lit.lights || []
    readonly property bool on: (lit.brightness || 0) > 0
    property int lastBrightness: 70
    readonly property var swatches: [
        { color: "#2F80ED", name: qsTr("Asima blue") }, { color: "#38BDF8", name: qsTr("Sky") },
        { color: "#6366F1", name: qsTr("Indigo") }, { color: "#A855F7", name: qsTr("Violet") },
        { color: "#F43F5E", name: qsTr("Rose") }, { color: "#F59E0B", name: qsTr("Amber") },
        { color: "#14B8A6", name: qsTr("Teal") }, { color: "#22C55E", name: qsTr("Forest") },
        { color: "#FFFFFF", name: qsTr("White") }
    ]

    Card {
        Layout.fillWidth: true
        title: qsTr("Colour")
        subtitle: page.on ? qsTr("The lights stay this colour after a restart, and controllers take it when they connect.")
                          : qsTr("The lights are off.")
        icon: "brightness-high"
        actions: [ DSwitch {
            checked: page.on
            Accessible.name: qsTr("Lights on")
            onToggled: {
                if (!checked)
                    page.lastBrightness = page.lit.brightness || 70
                Backend.setLighting(page.lit.color || "#2F80ED", checked ? page.lastBrightness : 0)
            }
        } ]

        RowLayout {
            Layout.fillWidth: true
            spacing: 24

            // What the lights show now.
            Rectangle {
                implicitWidth: 140
                implicitHeight: 140
                radius: 70
                color: Theme.surfaceAlt
                border.width: 1
                border.color: Theme.border
                Rectangle {
                    anchors.centerIn: parent
                    width: 104
                    height: 104
                    radius: 52
                    color: "transparent"
                    border.width: 10
                    border.color: page.on ? page.lit.color : Theme.track
                    opacity: page.on ? 0.35 + 0.65 * (page.lit.brightness / 100) : 1
                }
                AsimaMark { anchors.centerIn: parent; size: 44 }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 12
                Flow {
                    Layout.fillWidth: true
                    spacing: 10
                    Repeater {
                        model: page.swatches
                        delegate: Rectangle {
                            id: swatch
                            required property var modelData
                            readonly property bool chosen: String(page.lit.color).toUpperCase() === modelData.color
                            width: 36
                            height: 36
                            radius: 18
                            color: modelData.color
                            border.width: chosen ? 3 : 1
                            border.color: chosen ? Theme.text : Theme.borderStrong
                            Icon {
                                anchors.centerIn: parent
                                visible: swatch.chosen
                                name: "checkmark"
                                size: 16
                                color: swatch.modelData.color === "#FFFFFF" ? "#12151B" : "#FFFFFF"
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Backend.setLighting(swatch.modelData.color, page.on ? page.lit.brightness : page.lastBrightness)
                            }
                            Accessible.role: Accessible.RadioButton
                            Accessible.name: modelData.name
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 4
                    UiText { text: qsTr("Brightness"); font.weight: Font.Medium }
                    DSlider {
                        Layout.fillWidth: true
                        from: 5
                        to: 100
                        stepSize: 5
                        enabled: page.on
                        value: page.on ? page.lit.brightness : page.lastBrightness
                        onMoved: Backend.setLighting(page.lit.color || "#2F80ED", Math.round(value))
                    }
                    UiText { text: (page.on ? page.lit.brightness : 0) + " %"; tone: "muted"; Layout.preferredWidth: 44 }
                }
            }
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Lights on this device")
        icon: "view-grid"
        tone: "info"
        spacing: 0
        Repeater {
            model: page.lights
            delegate: ListRow {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: /controller|dualsense|gamepad|joy-con/i.test(modelData.device) ? "input-gamepad" : "brightness-high"
                title: modelData.name
                subtitle: modelData.device
                value: page.on ? qsTr("On") : qsTr("Off")
                tone: page.on ? "ok" : "neutral"
                separator: index < page.lights.length - 1
            }
        }
    }
}
