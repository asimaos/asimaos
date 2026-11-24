import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// Progress and result of any rescue job.
DPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property var job: Backend.job
    readonly property bool running: job.state === "running"
    readonly property bool failed: job.state === "error"
    readonly property bool done: job.state === "done"

    title: job.title || ""
    subtitle: running ? qsTr("Please keep the computer switched on.") : failed ? qsTr("It stopped before finishing.") : qsTr("Finished.")

    Card {
        Layout.fillWidth: true
        padding: 24
        spacing: 16
        RowLayout {
            Layout.fillWidth: true
            UiText {
                text: page.failed ? qsTr("Stopped") : page.done ? qsTr("Done") : (page.job.detail || qsTr("Working…"))
                variant: "heading"
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            UiText {
                text: Math.round((page.job.progress || 0) * 100) + " %"
                font.pixelSize: Theme.text2xl
                font.weight: Font.Bold
                tone: page.failed ? "error" : page.done ? "ok" : "accent"
            }
        }
        DProgressBar {
            Layout.fillWidth: true
            implicitHeight: 10
            value: page.job.progress || 0
            tone: page.failed ? "error" : page.done ? "ok" : "accent"
        }
        StepList { Layout.fillWidth: true; steps: page.job.steps || [] }
    }

    InfoBanner {
        visible: !page.running && (page.job.message || "").length > 0
        Layout.fillWidth: true
        tone: page.failed ? "error" : "ok"
        title: page.failed ? qsTr("What happened") : qsTr("Result")
        text: page.job.message || ""
    }

    // Approving the Asima OS key: the code for the blue screen
    Card {
        visible: page.done && (page.job.code || "").length > 0
        Layout.fillWidth: true
        title: qsTr("Your one-time code")
        subtitle: qsTr("At the next start a blue screen appears: choose Enroll MOK → Continue → Yes, type this code, then Reboot.")
        icon: "view-certificate"
        tone: "accent"
        UiText {
            text: page.job.code || ""
            font.family: "Noto Sans Mono"
            font.pixelSize: 36
            font.weight: Font.Bold
            font.letterSpacing: 3
        }
    }

    // Factory reset: the fresh system's recovery key, shown once
    Card {
        visible: page.done && (page.job.recoveryKey || "").length > 0
        Layout.fillWidth: true
        title: qsTr("The new recovery key")
        subtitle: qsTr("It unlocks the disk if the password is forgotten. Write it down now: it is shown only this once.")
        icon: "password-show-on"
        tone: "accent"
        GridLayout {
            columns: 4
            columnSpacing: 22
            rowSpacing: 10
            Repeater {
                model: (page.job.recoveryKey || "").length ? page.job.recoveryKey.split("-") : []
                delegate: UiText {
                    required property string modelData
                    text: modelData
                    font.family: "Noto Sans Mono"
                    font.pixelSize: 20
                    font.weight: Font.DemiBold
                }
            }
        }
    }

    RowLayout {
        visible: !page.running
        spacing: 12
        DButton {
            text: page.job.action === "wipe" ? qsTr("Power off") : qsTr("Restart now")
            variant: "primary"
            iconName: page.job.action === "wipe" ? "system-shutdown" : "system-reboot"
            focus: true
            onClicked: page.job.action === "wipe" ? Backend.powerOff() : Backend.restart()
        }
        DButton {
            visible: page.job.action !== "wipe" || page.failed
            text: qsTr("Back to the tools")
            variant: "ghost"
            onClicked: page.win.back()
        }
    }
}
