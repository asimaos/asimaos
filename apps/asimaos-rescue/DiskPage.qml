import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Check and repair the disk")
    subtitle: qsTr("Checking only reads. Repairs are offered when the check finds something.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var st: Backend.status
    readonly property var disk: st.disk || ({})
    readonly property real used: st.totalBytes > 0 ? 1 - st.freeBytes / st.totalBytes : 0
    readonly property bool tight: st.freeBytes < 3e9

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 820 ? 3 : 1
        columnSpacing: 16
        rowSpacing: 16
        StatTile {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            iconName: "drive-harddisk-symbolic"
            label: qsTr("Disk")
            value: page.disk.model || "–"
            caption: Theme.bytes(page.disk.size) + "  ·  " + (page.st.encrypted ? qsTr("encrypted") : qsTr("not encrypted"))
        }
        StatTile {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            iconName: "drive-harddisk-symbolic"
            iconTone: page.tight ? "warn" : "ok"
            label: qsTr("Free space")
            value: Theme.bytes(page.st.freeBytes)
            progress: page.used
            caption: page.tight ? qsTr("Nearly full: Asima OS may not start") : qsTr("Enough to start")
        }
        StatTile {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            iconName: "edit-undo-history"
            iconTone: "info"
            label: qsTr("Asima OS versions")
            value: page.st.version || "–"
            caption: page.st.previousVersion ? qsTr("Previous: %1").arg(page.st.previousVersion) : ""
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Check everything")
        subtitle: qsTr("The file system, both Asima OS versions (signatures and integrity), the boot files and the free space. Takes a few minutes; nothing is changed.")
        icon: "dialog-ok-apply"
        tone: "info"
        DButton {
            text: qsTr("Check the disk")
            variant: "primary"
            iconName: "system-search"
            focus: true
            onClicked: page.win.run("check")
            Component.onCompleted: forceActiveFocus()
        }
    }

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 820 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Free emergency space")
            subtitle: page.st.reserveBytes > 0
                      ? qsTr("Asima OS keeps %1 aside for exactly this moment. Releasing it lets the system start; delete some files afterwards.").arg(Theme.bytes(page.st.reserveBytes))
                      : qsTr("The emergency space is already in use. Delete files, or restore an older snapshot.")
            icon: "edit-clear"
            tone: page.tight ? "warn" : "info"
            Item { Layout.fillHeight: true }
            DButton {
                text: qsTr("Release %1").arg(Theme.bytes(page.st.reserveBytes))
                variant: page.tight ? "primary" : "secondary"
                enabled: page.st.reserveBytes > 0
                onClicked: page.win.run("free-space")
            }
        }
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Repair the file system")
            subtitle: qsTr("Only if the check reports file system errors. A snapshot of your files is taken first when possible.")
            icon: "tools-wizard"
            tone: "warn"
            Item { Layout.fillHeight: true }
            DButton {
                text: qsTr("Repair")
                variant: "secondary"
                onClicked: page.win.confirm({
                    title: qsTr("Repair the file system?"),
                    text: qsTr("Use this when the check found errors. Repairs can't always save every file."),
                    tone: "warn", icon: "tools-wizard", confirmText: qsTr("Repair")
                }, () => page.win.run("repair"))
            }
        }
    }
}
