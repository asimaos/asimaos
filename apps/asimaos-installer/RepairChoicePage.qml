pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// "Repair this computer" (D55), step 2: what is on the disk, and what the repair does. The start-up
// is always put back; installing this USB stick's version and bringing back the settings and
// accounts from the newest snapshot are choices.
WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property var probe: Backend.repair.probe || ({})
    readonly property var versions: probe.versions || []
    readonly property string newest: versions.length > 0 ? versions[0] : ""
    readonly property string stick: probe.stick || Backend.repair.stick || ""
    // No version on the disk can start: the stick's version is needed.
    readonly property bool mustInstall: versions.length === 0

    function compare(a, b) {
        const x = a.split(".").map(Number), y = b.split(".").map(Number)
        for (let i = 0; i < Math.max(x.length, y.length); ++i)
            if ((x[i] || 0) !== (y[i] || 0))
                return (x[i] || 0) < (y[i] || 0) ? -1 : 1
        return 0
    }
    readonly property int stickVsDisk: newest ? compare(stick, newest) : 1

    title: qsTr("What should the repair do?")
    subtitle: qsTr("Asima OS on %1. Your files stay as they are, whatever you choose.").arg(page.win.repairDisk)
    nextText: qsTr("Repair")
    nextIcon: "tools-wizard"
    onBack: page.win.go("repair-disk")
    onNext: page.win.confirm({
        title: qsTr("Repair Asima OS on %1?").arg(page.win.repairDisk),
        text: page.win.repairInstallStick
              ? qsTr("The start-up is put back and Asima OS %1 from this USB stick is installed. Your files stay.").arg(page.stick)
              : qsTr("The start-up is put back. Your files stay."),
        icon: "tools-wizard", tone: "accent", confirmText: qsTr("Repair")
    }, () => page.win.startRepair())

    Component.onCompleted: if (page.mustInstall) page.win.repairInstallStick = true

    Card {
        Layout.fillWidth: true
        title: qsTr("On this disk")
        icon: "drive-harddisk"
        spacing: 0
        ListRow {
            Layout.fillWidth: true
            title: qsTr("Versions of Asima OS")
            value: page.versions.length > 0 ? page.versions.join(" · ") : qsTr("None that can start")
            tone: page.versions.length > 0 ? "ok" : "warn"
        }
        ListRow {
            Layout.fillWidth: true
            title: qsTr("Settings come from")
            value: page.probe.settings ? qsTr("Asima OS %1").arg(page.probe.settings) : qsTr("Not known")
        }
        ListRow {
            Layout.fillWidth: true
            title: qsTr("Newest snapshot of the settings and accounts")
            value: page.probe.snapshot || qsTr("None")
            separator: false
        }
    }

    SelectCard {
        Layout.fillWidth: true
        iconName: "system-reboot"
        title: qsTr("Put back the start-up")
        detail: qsTr("The boot partition is checked; the boot loader, each version's boot image, Asima Rescue and the start menu are written again.")
        locked: true
        selected: true
    }

    SelectCard {
        Layout.fillWidth: true
        multi: true
        iconName: "run-install"
        title: qsTr("Install Asima OS %1 from this USB stick").arg(page.stick)
        detail: page.mustInstall
                ? qsTr("No version on this disk can start, so this one is installed. Files and settings stay.")
                : page.stickVsDisk > 0
                  ? qsTr("Newer than %1 on the disk: it becomes the version that starts, with %1 as its backup. Files and settings stay, and its first start brings the settings up to date.").arg(page.newest)
                  : page.stickVsDisk === 0
                    ? qsTr("The same version as on the disk: a fresh copy of it. Files and settings stay.")
                    : qsTr("Older than %1 on the disk: going back. Files stay; settings a newer version changed may need Reset settings afterwards (Asima Maintenance → System changes).").arg(page.newest)
        locked: page.mustInstall
        lockedText: qsTr("Needed")
        selected: page.win.repairInstallStick
        onToggled: page.win.repairInstallStick = !page.win.repairInstallStick
    }

    SelectCard {
        Layout.fillWidth: true
        multi: true
        iconName: "edit-undo-history"
        title: qsTr("Bring back the settings and accounts")
        detail: qsTr("As they were in the newest snapshot (%1, UTC). The ones there now are kept aside on the disk, not deleted.").arg(page.probe.snapshot || "")
        disabledReason: page.probe.snapshot ? "" : qsTr("There is no snapshot of the settings and accounts on this disk.")
        selected: page.win.repairRestoreAccount
        onToggled: page.win.repairRestoreAccount = !page.win.repairRestoreAccount
    }
}
