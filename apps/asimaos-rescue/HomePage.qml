pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("What would you like to do?")
    subtitle: qsTr("Start with the first tool; the ones further down change more.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var st: Backend.status
    readonly property int columns: page.contentWidth > 820 ? 2 : 1
    readonly property var tools: [
        { key: "restart", icon: "system-reboot", tone: "ok", title: qsTr("Start Asima OS again"),
          text: qsTr("Sometimes a second try is all it needs.") },
        { key: "previous", icon: "edit-undo-history", tone: "ok", title: qsTr("Start the previous version"),
          text: qsTr("Asima OS %1, once. Your files and settings are the same.").arg(page.st.previousVersion || "") },
        { key: "disk", icon: "drive-harddisk-symbolic", tone: "info", title: qsTr("Check and repair the disk"),
          text: qsTr("File system, both Asima OS versions, boot files and free space.") },
        { key: "restore", icon: "view-history", tone: "info", title: qsTr("Restore my files"),
          text: qsTr("Put your home folder back to a snapshot. Today's files are kept too.") },
        { key: "reinstall", icon: "drive-removable-media", tone: "accent", title: qsTr("Reinstall Asima OS"),
          text: qsTr("From an update file on a USB stick. Your files and settings stay.") },
        { key: "keys", icon: "view-certificate", tone: "accent", title: qsTr("Secure Boot and unlock"),
          text: qsTr("Approve the Asima OS key again, or turn off automatic unlock.") },
        { key: "reset", icon: "edit-reset", tone: "error", title: qsTr("Factory reset"),
          text: qsTr("A fresh Asima OS %1. Everything else is deleted.").arg(page.st.version || "") },
        { key: "wipe", icon: "edit-delete", tone: "error", title: qsTr("Erase the whole disk"),
          text: qsTr("Everything, Asima OS included: before selling or giving away this computer.") }
    ].filter(t => t.key !== "previous" || !!page.st.previousVersion)

    function activate(key) {
        if (key === "restart")
            Backend.restart()
        else if (key === "previous")
            page.win.confirm({
                title: qsTr("Start Asima OS %1?").arg(page.st.previousVersion),
                text: qsTr("The computer restarts into the previous version, this time only. Asima Updater can make it the default."),
                icon: "edit-undo-history", confirmText: qsTr("Restart")
            }, () => page.win.run("start-previous"))
        else
            page.win.open(key)
    }

    InfoBanner {
        visible: !!page.st.reason
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("Why you're here")
        text: page.st.reason || ""
    }

    GridLayout {
        id: grid
        Layout.fillWidth: true
        columns: page.columns
        columnSpacing: 16
        rowSpacing: 16
        Repeater {
            id: repeater
            model: page.tools
            delegate: ToolTile {
                id: tile
                required property var modelData
                required property int index
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                iconName: modelData.icon
                tone: modelData.tone
                title: modelData.title
                text: modelData.text
                focus: index === 0
                onActivated: page.activate(modelData.key)
                // Arrow keys move around the grid.
                function go(delta) {
                    const target = repeater.itemAt(index + delta)
                    if (target) target.forceActiveFocus()
                }
                Keys.onLeftPressed: go(-1)
                Keys.onRightPressed: go(1)
                Keys.onUpPressed: go(-page.columns)
                Keys.onDownPressed: go(page.columns)
            }
        }
    }

    UiText {
        text: qsTr("Tab or the arrow keys move between tools; Enter opens one; Esc goes back.")
        variant: "small"
        tone: "subtle"
    }

    Component.onCompleted: {
        const first = repeater.itemAt(0)
        if (first) first.forceActiveFocus()
    }
}
