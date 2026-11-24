pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property var job: Backend.install
    readonly property bool failed: job.state === "error"
    readonly property real progress: job.progress || 0

    readonly property var tips: [
        { icon: "edit-undo-history", tone: "accent", title: qsTr("Two versions, always"),
          text: qsTr("Every update keeps the previous version. If one fails to start, Asima OS goes back by itself.") },
        { icon: "view-history", tone: "info", title: qsTr("Snapshots every night"),
          text: qsTr("Deleted a file by mistake? Yesterday's copy is in Asima Maintenance under Snapshots & Backup.") },
        { icon: "object-locked", tone: "ok", title: qsTr("Locked down, not locked in"),
          text: qsTr("System files are verified on every read, while apps, containers and VMs stay yours to install.") },
        { icon: "input-gamepad", tone: "info", title: qsTr("Ready to play"),
          text: qsTr("Open Discover to install Steam, Heroic or Lutris. Your graphics drivers are already set up.") }
    ]
    property int tip: 0

    title: failed ? qsTr("The installation stopped") : qsTr("Installing Asima OS")
    subtitle: failed ? qsTr("Your other disks were not touched.")
                     : qsTr("This takes five to ten minutes. You can watch, or take a short break.")
    showFooter: failed
    showBack: failed
    backText: qsTr("Back to summary")
    showNext: failed
    nextText: qsTr("Try again")
    nextIcon: "view-refresh"
    onBack: page.win.go("summary")
    onNext: page.win.go("account")   // passwords are not kept, so they are asked again

    Timer {
        interval: 6000
        repeat: true
        running: !page.failed
        onTriggered: page.tip = (page.tip + 1) % page.tips.length
    }

    Card {
        Layout.fillWidth: true
        padding: 24
        spacing: 16

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                UiText {
                    text: page.failed ? qsTr("Stopped") : page.progress >= 1 ? qsTr("Done") : qsTr("Installing…")
                    variant: "heading"
                }
                UiText {
                    text: page.job.detail || ""
                    variant: "small"
                    tone: "muted"
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                }
            }
            UiText {
                text: Math.round(page.progress * 100) + " %"
                font.pixelSize: Theme.text2xl
                font.weight: Font.Bold
                tone: page.failed ? "error" : "accent"
            }
        }
        DProgressBar {
            Layout.fillWidth: true
            implicitHeight: 10
            value: page.progress
            tone: page.failed ? "error" : "accent"
        }
        StepList {
            Layout.fillWidth: true
            Layout.topMargin: 6
            steps: page.job.steps || []
        }
    }

    InfoBanner {
        visible: page.failed
        Layout.fillWidth: true
        tone: "error"
        title: qsTr("What happened")
        text: page.job.error || ""
    }

    // Rotating tips while waiting
    Card {
        visible: !page.failed
        Layout.fillWidth: true
        padding: 22

        RowLayout {
            Layout.fillWidth: true
            spacing: 16
            IconTile {
                name: page.tips[page.tip].icon
                tone: page.tips[page.tip].tone
                size: 48
                Layout.alignment: Qt.AlignTop
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                UiText { text: qsTr("Did you know?"); variant: "overline"; tone: "subtle" }
                UiText { text: page.tips[page.tip].title; variant: "heading"; Layout.fillWidth: true }
                UiText { text: page.tips[page.tip].text; tone: "muted"; Layout.fillWidth: true; wrapMode: Text.WordWrap }
            }
            Row {
                spacing: 6
                Layout.alignment: Qt.AlignBottom
                Repeater {
                    model: page.tips.length
                    delegate: Rectangle {
                        required property int index
                        width: index === page.tip ? 18 : 6
                        height: 6
                        radius: 3
                        color: index === page.tip ? Theme.accent : Theme.track
                        Behavior on width { NumberAnimation { duration: Theme.normal } }
                    }
                }
            }
        }
    }
}
