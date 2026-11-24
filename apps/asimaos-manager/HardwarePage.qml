pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Hardware")
    subtitle: qsTr("What's inside, and how warm it is right now.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var d: Backend.device
    readonly property var t: Backend.thermals
    function tone(c) { return c >= 90 ? "error" : c >= 80 ? "warn" : "ok" }

    headerActions: [
        DButton { text: qsTr("Copy system report"); iconName: "edit-copy"; onClicked: Backend.copyReport() }
    ]

    // ── Specs ────────────────────────────────────────────────────────────────
    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 900 ? 3 : 2
        columnSpacing: 16
        rowSpacing: 16

        Repeater {
            model: [
                { icon: "view-process-system", label: qsTr("Processor"), value: page.d.cpu || "–",
                  detail: page.d.cores ? qsTr("%1 cores · %2 threads").arg(page.d.cores).arg(page.d.threads) : "" },
                { icon: "monitor", label: qsTr("Graphics"), value: (page.d.gpus || []).map(g => g.name).join(" + ") || "–",
                  detail: (page.d.gpus || []).map(g => g.driver).filter(s => !!s).join(" · ") },
                { icon: "speedometer", label: qsTr("Memory"), value: page.d.memory > 0 ? Math.round(page.d.memory / 1073741824) + " GB" : "–", detail: "" },
                { icon: "drive-harddisk-symbolic", label: qsTr("Storage"), value: page.d.storage || "–", detail: "" },
                { icon: "view-fullscreen", label: qsTr("Screen"), value: page.d.screen || "–", detail: "" },
                { icon: "network-wireless", label: qsTr("Wireless"), value: page.d.wireless || "–", detail: "" },
                { icon: page.win ? page.win.deviceIcon() : "computer", label: qsTr("Model"), value: page.d.model || "–", detail: page.d.typeLabel || "" },
                { icon: "view-certificate", label: qsTr("Firmware"), value: page.d.bios || "–", detail: "" },
                { icon: "dialog-ok-apply", label: qsTr("System"), value: page.d.os || "–", detail: page.d.kernel || "" }
            ].filter(s => s.value !== "–")
            delegate: StatTile {
                required property var modelData
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1
                iconName: modelData.icon
                label: modelData.label
                value: modelData.value
                caption: modelData.detail
            }
        }
    }

    // ── Sensors ──────────────────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("Temperatures")
        subtitle: Backend.demo ? qsTr("Sample values") : qsTr("Updated every few seconds")
        icon: "temperature-normal"
        tone: "info"

        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 760 ? 4 : 2
            columnSpacing: 16
            rowSpacing: 16
            Repeater {
                model: [
                    { key: "cpu", label: qsTr("Processor") }, { key: "gpu", label: qsTr("Graphics") },
                    { key: "ssd", label: qsTr("SSD") }, { key: "battery", label: qsTr("Battery") }
                ].filter(s => page.t[s.key] !== undefined)
                delegate: ColumnLayout {
                    id: sensor
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 8
                    StatusRing {
                        Layout.alignment: Qt.AlignHCenter
                        size: 104
                        thickness: 9
                        value: Math.min(1, (page.t[sensor.modelData.key] || 0) / 100)
                        color: Theme.statusColor(page.tone(page.t[sensor.modelData.key]))
                        UiText {
                            anchors.centerIn: parent
                            text: (page.t[sensor.modelData.key] || 0) + "°"
                            variant: "subtitle"
                            font.weight: Font.Bold
                        }
                    }
                    // Filling the cell lets the grid spread the rings across the card.
                    UiText {
                        text: sensor.modelData.label
                        font.weight: Font.Medium
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                    }
                }
            }
        }
        Repeater {
            model: page.t.fans || []
            delegate: ListRow {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: "speedometer"
                title: modelData.label
                value: modelData.rpm + " rpm"
                separator: index < (page.t.fans || []).length - 1
            }
        }
    }
}
