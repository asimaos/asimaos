pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import QtQuick.Dialogs
import Asima.Ui

DPage {
    id: page
    title: qsTr("Health")
    subtitle: qsTr("The checks that keep Asima OS reliable, and when they last ran.")

    startAt: Backend.uiHint === "health-report" ? crashCard : null
    readonly property int total: Backend.health.length
    readonly property int passed: Backend.health.filter(h => h.tone === "ok").length
    readonly property var maintenance: Backend.system.maintenance || []

    headerActions: [
        DButton {
            text: Backend.checking ? qsTr("Checking…") : qsTr("Run full check")
            variant: "primary"
            iconName: "kt-check-data"
            busy: Backend.checking
            enabled: !Backend.checking
            onClicked: Backend.runHealthCheck()
        }
    ]

    Card {
        Layout.fillWidth: true
        padding: 24

        RowLayout {
            Layout.fillWidth: true
            spacing: 24
            StatusRing {
                size: 96
                thickness: 9
                value: page.total > 0 ? page.passed / page.total : 0
                color: page.passed === page.total ? Theme.ok : Theme.warn
                UiText {
                    anchors.centerIn: parent
                    text: page.passed + "/" + page.total
                    variant: "subtitle"
                    font.weight: Font.Bold
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                UiText {
                    text: page.passed === page.total ? qsTr("All checks passed")
                                                     : qsTr("%1 checks need attention").arg(page.total - page.passed)
                    variant: "subtitle"
                }
                UiText {
                    text: Backend.checking ? qsTr("Checking system files, boot, disk and security…")
                                           : qsTr("Last full check: %1").arg(Backend.system.lastCheck || qsTr("never"))
                    tone: "muted"
                }
                DProgressBar {
                    visible: Backend.checking
                    indeterminate: true
                    Layout.fillWidth: true
                    Layout.topMargin: 8
                }
            }
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Checks")
        icon: "kt-check-data"
        tone: "ok"
        spacing: 0

        Repeater {
            model: Backend.health
            delegate: ListRow {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: modelData.icon
                title: modelData.title
                subtitle: modelData.detail
                value: modelData.value
                tone: modelData.tone
                separator: index < page.total - 1
            }
        }
    }

    // A version that failed to start (and was left by itself), and kernel crashes.
    Card {
        id: crashCard
        Layout.fillWidth: true
        title: qsTr("Failed starts and crashes")
        subtitle: Backend.crashes.length === 0 ? qsTr("None. Asima OS started properly every time.")
                                               : qsTr("Asima OS went back to a working version by itself. This is what happened.")
        icon: "dialog-warning"
        tone: Backend.crashes.length === 0 ? "ok" : "warn"
        spacing: 0

        Repeater {
            model: Backend.crashes
            delegate: ColumnLayout {
                id: crash
                required property var modelData
                required property int index
                Layout.fillWidth: true
                spacing: 8
                ListRow {
                    Layout.fillWidth: true
                    iconName: crash.modelData.kind === "crash" ? "dialog-error" : "system-reboot"
                    iconTone: crash.modelData.kind === "crash" ? "error" : "warn"
                    title: crash.modelData.title
                    subtitle: crash.modelData.when + " · " + crash.modelData.detail
                    separator: crash.index < Backend.crashes.length - 1 && !crash.modelData.log
                }
                Rectangle {
                    visible: !!crash.modelData.log
                    Layout.fillWidth: true
                    Layout.bottomMargin: 8
                    implicitHeight: logText.implicitHeight + 20
                    radius: Theme.radiusS
                    color: Theme.surfaceAlt
                    border.color: Theme.border
                    UiText {
                        id: logText
                        anchors.fill: parent
                        anchors.margins: 10
                        text: crash.modelData.log || ""
                        font.family: "Noto Sans Mono"
                        font.pixelSize: Theme.textXs
                        wrapMode: Text.WrapAnywhere
                    }
                }
            }
        }
    }

    // Made on this computer, read and edited by the person, saved or copied by them.
    Card {
        Layout.fillWidth: true
        title: qsTr("Problem report")
        subtitle: qsTr("Collects the versions, the health check, failed starts and recent errors. You read and edit it before saving it; nothing is sent.")
        icon: "document-edit"
        tone: "info"
        actions: [
            DButton {
                text: Backend.reporting ? qsTr("Collecting…") : Backend.report.length > 0 ? qsTr("Make it again") : qsTr("Make a report")
                variant: Backend.report.length > 0 ? "ghost" : "primary"
                iconName: "document-new"
                busy: Backend.reporting
                enabled: !Backend.reporting
                onClicked: Backend.makeReport()
            }
        ]

        Connections {
            target: Backend
            function onReportChanged() { reportText.text = Backend.report }
        }
        B.ScrollView {
            visible: Backend.report.length > 0
            Layout.fillWidth: true
            Layout.preferredHeight: 320
            B.TextArea {
                id: reportText
                text: Backend.report
                font.family: "Noto Sans Mono"
                font.pixelSize: Theme.textXs
                wrapMode: TextEdit.Wrap
                color: Theme.text
                selectByMouse: true
                background: Rectangle { color: Theme.surfaceAlt; radius: Theme.radiusS; border.color: Theme.border }
            }
        }
        RowLayout {
            visible: Backend.report.length > 0
            spacing: 10
            DButton { text: qsTr("Save…"); variant: "primary"; iconName: "document-save"; onClicked: saveDialog.open() }
            DButton { text: qsTr("Copy"); variant: "secondary"; iconName: "edit-copy"; onClicked: Backend.copyText(reportText.text) }
            UiText { text: qsTr("Asima Hub can send it for you."); tone: "muted" }
        }
        FileDialog {
            id: saveDialog
            title: qsTr("Save the problem report")
            fileMode: FileDialog.SaveFile
            defaultSuffix: "txt"
            nameFilters: [qsTr("Text files (*.txt)")]
            currentFile: "asimaos-problem-report.txt"
            onAccepted: Backend.saveReport(reportText.text, selectedFile)
        }
    }

    Card {
        visible: page.maintenance.length > 0
        Layout.fillWidth: true
        title: qsTr("Automatic maintenance")
        subtitle: qsTr("Runs in the background. You don't need to do anything.")
        icon: "clock"
        tone: "info"
        spacing: 0

        Repeater {
            model: page.maintenance
            delegate: ListRow {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: modelData.icon
                title: modelData.title
                subtitle: modelData.when
                value: modelData.last
                separator: index < page.maintenance.length - 1
            }
        }
    }
}
