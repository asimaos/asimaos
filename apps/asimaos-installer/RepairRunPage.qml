pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// "Repair this computer" (D55), step 3: the repair's steps, then the restart.
WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property var r: Backend.repair
    readonly property bool failed: r.state === "error"
    readonly property bool done: r.state === "ok"

    title: failed ? qsTr("The repair stopped") : done ? qsTr("Asima OS is repaired") : qsTr("Repairing Asima OS")
    subtitle: failed ? qsTr("Your files were not touched.")
              : done ? qsTr("Asima OS %1 starts again. Take out the USB stick and restart.").arg(r.version || "")
              : qsTr("This takes a minute, or a few when this USB stick's version is installed.")
    showFooter: failed || done
    showBack: failed
    backText: qsTr("Back to the choices")
    onBack: page.win.go("repair-choice")
    nextText: done ? qsTr("Restart") : qsTr("Try again")
    nextIcon: done ? "system-reboot" : "view-refresh"
    onNext: done ? Backend.restart() : page.win.go("repair-disk")   // the passphrase is asked again

    Card {
        Layout.fillWidth: true
        padding: 24
        StepList {
            Layout.fillWidth: true
            steps: page.r.steps || []
        }
    }

    InfoBanner {
        visible: page.failed
        Layout.fillWidth: true
        tone: "error"
        title: qsTr("What happened")
        text: page.r.message || ""
    }

    Card {
        visible: (page.r.log || []).length > 0
        Layout.fillWidth: true
        title: qsTr("Details")
        icon: "documentation"
        UiText {
            text: (page.r.log || []).join("\n")
            variant: "small"
            tone: "muted"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }
    }
}
