import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// D14: erase the whole disk, Asima OS included, before selling or giving the computer away.
DPage {
    id: page
    title: qsTr("Erase the whole disk")
    subtitle: qsTr("For selling or giving away this computer. Afterwards nothing is left on the disk, not even Asima OS.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var disk: Backend.status.disk || ({})
    readonly property string serialTail: (page.disk.serial || "").length >= 4 ? page.disk.serial.slice(-4) : "ERASE"

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: heroRow.implicitHeight + 44
        radius: Theme.radiusXl
        color: Theme.errorSoft
        border.width: 1
        border.color: Qt.alpha(Theme.error, 0.3)
        RowLayout {
            id: heroRow
            anchors.fill: parent
            anchors.margins: 22
            spacing: 18
            IconTile { name: "edit-delete"; tone: "error"; size: 54; color: Theme.surface }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                UiText { text: page.disk.model || ""; variant: "subtitle"; Layout.fillWidth: true }
                UiText {
                    text: qsTr("%1  ·  serial …%2  ·  everything on it is erased").arg(Theme.bytes(page.disk.size)).arg(page.serialTail)
                    tone: "muted"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
            }
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("How it's erased")
        icon: "help-about"
        tone: "info"
        spacing: 0
        ListRow {
            Layout.fillWidth: true
            visible: !!Backend.status.encrypted
            title: qsTr("1. The encryption keys are destroyed")
            subtitle: qsTr("From that moment the data can't be read by anyone, even with the old passphrase or recovery key.")
        }
        ListRow {
            Layout.fillWidth: true
            title: Backend.status.encrypted ? qsTr("2. Every block of the disk is erased") : qsTr("Every block of the disk is erased")
            subtitle: qsTr("The SSD's own erase command where it has one, so even spare areas are cleared.")
            separator: false
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("The computer won't start until an operating system is installed again")
        text: qsTr("To use it yourself afterwards, install Asima OS from a USB stick.")
    }

    DButton {
        text: qsTr("Erase the whole disk")
        variant: "danger"
        iconName: "edit-delete"
        onClicked: page.win.confirm({
            title: qsTr("Erase %1?").arg(page.disk.model),
            text: qsTr("Everything on this disk is erased, Asima OS included. To confirm it's the right disk, type the last four characters of its serial number: %1").arg(page.serialTail),
            tone: "error", icon: "edit-delete", confirmText: qsTr("Erase everything"), requiredWord: page.serialTail
        }, () => page.win.run("wipe", { serialTail: page.serialTail }))
    }
}
