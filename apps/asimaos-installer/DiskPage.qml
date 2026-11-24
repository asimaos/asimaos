pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property var chosen: page.win.diskInfo()

    title: qsTr("Where should Asima OS go?")
    subtitle: qsTr("Asima OS uses a whole disk. Everything on the disk you choose will be erased.")
    canContinue: page.chosen.usable === true
    onNext: win.next()
    onBack: win.back()
    footerStart: [
        DButton { text: qsTr("Refresh"); variant: "ghost"; large: true; iconName: "view-refresh"; onClicked: Backend.refreshDisks() }
    ]

    Repeater {
        model: Backend.disks
        delegate: SelectCard {
            id: diskCard
            required property var modelData
            Layout.fillWidth: true
            iconName: modelData.transport === "usb" ? "drive-removable-media-usb" : "drive-harddisk"
            iconTone: selected ? "accent" : "info"
            title: modelData.model
            subtitle: [Theme.bytes(modelData.size), modelData.transportLabel,
                       modelData.serial ? qsTr("serial …%1").arg(modelData.serial.slice(-4)) : ""]
                      .filter(s => !!s).join("  ·  ")
            detail: qsTr("Contains: %1").arg(modelData.contents)
            disabledLabel: modelData.usable ? "" : (modelData.label || "")
            disabledReason: modelData.usable ? "" : (modelData.reason || "")
            selected: page.win.disk === modelData.path
            onToggled: page.win.disk = modelData.path
        }
    }

    InfoBanner {
        visible: Backend.disks.length === 0
        Layout.fillWidth: true
        tone: "error"
        title: qsTr("No disks found")
        text: qsTr("Connect a disk with at least 16 GB, then press Refresh.")
    }

    InfoBanner {
        visible: page.canContinue
        Layout.fillWidth: true
        tone: "error"
        title: qsTr("Everything on %1 will be erased").arg(page.chosen.model || "")
        text: page.chosen.contents === qsTr("Empty")
              ? qsTr("This disk looks empty.")
              : qsTr("It contains: %1. Copy anything you need to another disk first.").arg(page.chosen.contents || "")
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Want to keep Windows or another system?")
        text: qsTr("Asima OS doesn't share a disk with other systems. Install it on a second disk and choose which one to start in your firmware's boot menu.")
    }
}
