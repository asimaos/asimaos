pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

// The screen settings Plasma keeps, one click away, and the laptop keyboard light. RGB lighting
// lives in Asima Gaming (D34); this page only points there on devices that have such lights.
DPage {
    id: page
    title: qsTr("Display")
    readonly property var dsp: Backend.display
    readonly property var monitors: page.dsp.monitors || []
    subtitle: page.dsp.kbdAvailable ? qsTr("Every screen's brightness, the screen settings one click away, and the keyboard light.")
                                    : qsTr("Every screen's brightness, and the screen settings one click away.")

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 860 && !!page.dsp.kbdAvailable ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Screen")
            subtitle: qsTr("Plasma keeps these; Asima Manager opens the right page.")
            icon: "monitor"
            tone: "info"
            spacing: 0
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Resolution, scaling and monitors")
                DButton { text: qsTr("Open"); variant: "ghost"; onClicked: Backend.openSettings("kcm_kscreen") }
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Night light")
                separator: false
                DButton { text: qsTr("Open"); variant: "ghost"; onClicked: Backend.openSettings("kcm_nightlight") }
            }
        }

        Card {
            visible: !!page.dsp.kbdAvailable
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Keyboard light")
            subtitle: (page.dsp.kbdLevel || 0) > 0 ? qsTr("Level %1 of %2").arg(page.dsp.kbdLevel).arg(page.dsp.kbdMax) : qsTr("Off")
            icon: "input-keyboard"
            tone: "info"
            RowLayout {
                Layout.fillWidth: true
                UiText { text: qsTr("Off"); tone: "muted"; variant: "small" }
                DSlider {
                    Layout.fillWidth: true
                    from: 0
                    to: page.dsp.kbdMax || 1
                    stepSize: 1
                    value: page.dsp.kbdLevel || 0
                    onMoved: Backend.setKeyboardLight(Math.round(value))
                }
                UiText { text: qsTr("Bright"); tone: "muted"; variant: "small" }
            }
        }
    }

    // Every screen's brightness, the external monitors' over the cable (DDC/CI, D55), through
    // Plasma's own brightness control.
    Card {
        Layout.fillWidth: true
        title: qsTr("Brightness")
        subtitle: page.monitors.some(m => !m.internal)
                  ? qsTr("External monitors change over the cable (DDC/CI).")
                  : qsTr("External monitors that allow it (DDC/CI, on in most monitors' own menus) appear here once connected.")
        icon: "brightness-high"
        tone: "info"
        spacing: 0
        Repeater {
            model: page.monitors
            delegate: ListRow {
                id: monitorRow
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: modelData.internal ? "computer-laptop" : "monitor"
                title: modelData.internal ? qsTr("Built-in screen") : (modelData.label || qsTr("Monitor"))
                subtitle: Math.round(100 * modelData.brightness / Math.max(1, modelData.max)) + " %"
                separator: index < page.monitors.length - 1
                DSlider {
                    implicitWidth: 260
                    from: 0
                    to: monitorRow.modelData.max
                    stepSize: 1
                    value: monitorRow.modelData.brightness
                    onMoved: Backend.setMonitorBrightness(monitorRow.modelData.name, Math.round(value))
                }
            }
        }
        UiText {
            visible: page.monitors.length === 0
            text: qsTr("Plasma isn't showing any screen whose brightness can change.")
            tone: "muted"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }
    }

    InfoBanner {
        visible: (page.dsp.rgbLights || 0) > 0
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("RGB lighting is in Asima Gaming")
        text: qsTr("The colour of the lights on this device and its controllers is set there.")
        DButton { text: qsTr("Open Asima Gaming"); variant: "secondary"; onClicked: Backend.openApp("asimaos-gaming --page lighting") }
    }
}
