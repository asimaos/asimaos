pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

DPage {
    id: page
    title: qsTr("Power & Battery")
    subtitle: qsTr("Battery health, charging care and how your device saves power.")

    readonly property var b: Backend.battery

    InfoBanner {
        visible: !page.b.present
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("No battery")
        text: qsTr("This computer runs from the wall. Power modes are below.")
    }

    // ── Battery ──────────────────────────────────────────────────────────────
    Card {
        visible: !!page.b.present
        Layout.fillWidth: true
        padding: 24

        RowLayout {
            Layout.fillWidth: true
            spacing: 28
            StatusRing {
                size: 132
                thickness: 11
                value: (page.b.percent || 0) / 100
                color: page.b.percent < 15 ? Theme.error : page.b.percent < 30 ? Theme.warn : Theme.ok
                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: 0
                    UiText { text: (page.b.percent || 0) + "%"; font.pixelSize: Theme.text2xl; font.weight: Font.Bold; Layout.alignment: Qt.AlignHCenter }
                    UiText {
                        text: page.b.status === "Charging" ? qsTr("Charging") : page.b.status === "Full" ? qsTr("Full") : qsTr("On battery")
                        variant: "caption"
                        tone: "muted"
                        Layout.alignment: Qt.AlignHCenter
                    }
                }
            }
            GridLayout {
                Layout.fillWidth: true
                columns: page.contentWidth > 760 ? 4 : 2
                columnSpacing: 24
                rowSpacing: 16
                Repeater {
                    model: [
                        { label: qsTr("Time"), value: page.b.timeText || "–" },
                        { label: qsTr("Power draw"), value: page.b.powerW > 0 ? page.b.powerW.toFixed(1) + " W" : "–" },
                        { label: qsTr("Health"), value: page.b.health > 0 ? page.b.health + " %" : "–" },
                        { label: qsTr("Charge cycles"), value: page.b.cycles > 0 ? String(page.b.cycles) : "–" }
                    ]
                    delegate: ColumnLayout {
                        id: entry
                        required property var modelData
                        spacing: 2
                        UiText { text: entry.modelData.label; variant: "small"; tone: "muted" }
                        UiText { text: entry.modelData.value; variant: "subtitle" }
                    }
                }
                UiText {
                    Layout.columnSpan: page.contentWidth > 760 ? 4 : 2
                    Layout.fillWidth: true
                    visible: page.b.designWh > 0
                    text: qsTr("Holds %1 Wh of its original %2 Wh.").arg(Number(page.b.fullWh).toFixed(1)).arg(Number(page.b.designWh).toFixed(0))
                    variant: "small"
                    tone: "muted"
                }
            }
        }
    }

    // ── Battery care ─────────────────────────────────────────────────────────
    Card {
        visible: !!page.b.present
        Layout.fillWidth: true
        title: qsTr("Battery care")
        subtitle: qsTr("Batteries last years longer when they don't sit at 100 %. Turn this off before a long trip.")
        icon: "battery-080"
        tone: "ok"
        actions: [
            DSwitch {
                enabled: !!page.b.limitSupported
                checked: !!page.b.limitEnabled
                onToggled: Backend.set("power", "limitEnabled", checked)
                Accessible.name: qsTr("Limit charging")
            }
        ]
        ColumnLayout {
            visible: !!page.b.limitSupported && !!page.b.limitEnabled && !page.b.limitFixed
            Layout.fillWidth: true
            spacing: 2
            RowLayout {
                UiText { text: qsTr("Stop charging at"); font.weight: Font.Medium; Layout.fillWidth: true }
                UiText { text: (page.b.chargeLimit || 80) + " %"; variant: "subtitle"; tone: "accent"; font.weight: Font.Bold }
            }
            DSlider {
                Layout.fillWidth: true
                from: 60
                to: 100
                stepSize: 5
                value: page.b.chargeLimit || 80
                onMoved: Backend.set("power", "chargeLimit", value)
            }
        }
        UiText {
            visible: !!page.b.limitSupported && !!page.b.limitFixed
            text: !!page.b.limitEnabled ? qsTr("On: this laptop stops charging early by itself, at a level it sets (often 60 to 80 %).")
                                        : qsTr("This laptop sets the level itself (often 60 to 80 %): switch it on to use it.")
            variant: "small"
            tone: "muted"
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }
        UiText {
            visible: !page.b.limitSupported
            text: qsTr("This battery doesn't support a charge limit.")
            variant: "small"
            tone: "muted"
        }
    }

    // ── Power mode ───────────────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("Power mode")
        icon: "speedometer"
        actions: [ StatusPill { visible: !Backend.power.available; text: qsTr("Not available"); tone: "neutral" } ]
        SegmentedTabs {
            Layout.fillWidth: true
            segmentWidth: Math.max(120, (page.contentWidth - 48) / 3)
            options: [qsTr("Power saver"), qsTr("Balanced"), qsTr("Performance")]
            currentIndex: ["power-saver", "balanced", "performance"].indexOf(Backend.power.profile)
            onActivated: index => Backend.setPowerProfile(["power-saver", "balanced", "performance"][index])
        }
    }

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 860 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Sleep and screen")
            subtitle: qsTr("Plasma keeps these; Asima Manager opens the right page.")
            icon: "system-shutdown"
            tone: "info"
            spacing: 0
            ListRow {
                Layout.fillWidth: true
                title: qsTr("When to dim, turn off the screen and sleep")
                DButton { text: qsTr("Open"); variant: "ghost"; onClicked: Backend.openSettings("kcm_powerdevilprofilesconfig") }
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Power button and lid")
                separator: false
                DButton { text: qsTr("Open"); variant: "ghost"; onClicked: Backend.openSettings("kcm_powerdevilprofilesconfig") }
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("What woke your device")
            subtitle: qsTr("Helps find why the battery drains while asleep")
            icon: "chronometer"
            tone: "info"
            spacing: 0
            Repeater {
                model: Backend.power.wake || []
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    title: modelData.title
                    value: modelData.detail
                    separator: index < (Backend.power.wake || []).length - 1
                }
            }
            UiText {
                visible: (Backend.power.wake || []).length === 0
                text: qsTr("Nothing recorded yet.")
                tone: "muted"
            }
        }
    }
}
