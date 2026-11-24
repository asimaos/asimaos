pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window

    title: qsTr("Welcome to Asima OS")
    subtitle: qsTr("A secure system that keeps itself working, for work and for play. Installing takes about ten minutes.")
    showBack: false
    nextText: qsTr("Install Asima OS")
    nextIcon: "go-next"
    canContinue: !Backend.requirements.some(r => r.tone === "error")
    hint: canContinue ? "" : qsTr("This computer can't run Asima OS yet; see below.")
    onNext: win.next()
    footerStart: [
        DButton { text: qsTr("Try Asima OS first"); variant: "ghost"; large: true; onClicked: Backend.quit() },
        DButton { text: qsTr("Repair an installed Asima OS"); variant: "ghost"; large: true; iconName: "tools-wizard"; onClicked: Backend.openRepair() }
    ]

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 700 ? 3 : 1
        columnSpacing: 16
        rowSpacing: 16

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            icon: "edit-undo-history"
            title: qsTr("Never breaks")
            subtitle: qsTr("Two versions are kept. If an update fails, Asima OS goes back by itself.")
        }
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            icon: "object-locked"
            tone: "ok"
            title: qsTr("Private by default")
            subtitle: qsTr("The whole disk is encrypted. System files can't be changed behind your back.")
        }
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            icon: "input-gamepad"
            tone: "info"
            title: qsTr("Work and play")
            subtitle: qsTr("Developer tools, containers and gaming, ready when you are.")
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("This computer")
        subtitle: qsTr("Checked automatically")
        icon: "computer"
        tone: "info"
        spacing: 0

        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 700 ? 2 : 1
            columnSpacing: 28
            rowSpacing: 0
            Repeater {
                model: Backend.requirements
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    iconName: modelData.icon
                    title: modelData.title
                    subtitle: modelData.detail
                    value: modelData.value
                    tone: modelData.tone
                    separator: index < Backend.requirements.length - (page.contentWidth > 700 ? 2 : 1)
                }
            }
        }
    }
}
