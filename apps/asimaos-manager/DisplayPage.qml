import QtQuick
import QtQuick.Layouts
import Asima.Ui

// The screen settings Plasma keeps, one click away, and the laptop keyboard light. RGB lighting
// lives in Asima Gaming (D34); this page only points there on devices that have such lights.
DPage {
    id: page
    title: qsTr("Display")
    readonly property var dsp: Backend.display
    subtitle: page.dsp.kbdAvailable ? qsTr("Reach the screen settings fast, and set the keyboard light.")
                                    : qsTr("Reach the screen settings fast.")

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
                DButton { text: qsTr("Open"); variant: "ghost"; onClicked: Backend.openSettings("kcm_nightlight") }
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Brightness of external monitors")
                subtitle: qsTr("Works over the cable on most monitors (DDC/CI)")
                separator: false
                DButton { text: qsTr("Open"); variant: "ghost"; onClicked: Backend.openSettings("kcm_powerdevilprofilesconfig") }
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

    InfoBanner {
        visible: (page.dsp.rgbLights || 0) > 0
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("RGB lighting is in Asima Gaming")
        text: qsTr("The colour of the lights on this device and its controllers is set there.")
        DButton { text: qsTr("Open Asima Gaming"); variant: "secondary"; onClicked: Backend.openApp("asimaos-gaming --page lighting") }
    }
}
