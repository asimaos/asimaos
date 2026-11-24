pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Reinstall Asima OS")
    subtitle: qsTr("Writes a fresh, verified copy of Asima OS from an update file. Your files, apps and settings stay.")

    readonly property var win: B.ApplicationWindow.window
    property string chosen: Backend.updateFiles.length > 0 ? Backend.updateFiles[0].path : ""
    readonly property var current: Backend.updateFiles.find(f => f.path === page.chosen) || null

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 10
        Repeater {
            model: Backend.updateFiles
            delegate: SelectCard {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: "drive-removable-media"
                title: qsTr("Asima OS %1").arg(modelData.version)
                subtitle: (modelData.older ? qsTr("Older than the installed version  ·  ") : "")
                          + qsTr("On %1  ·  %2").arg(modelData.device).arg(Theme.bytes(modelData.size))
                selected: page.chosen === modelData.path
                focus: index === 0
                activeFocusOnTab: true
                onToggled: page.chosen = modelData.path
            }
        }
    }

    Card {
        visible: Backend.updateFiles.length === 0
        Layout.fillWidth: true
        title: qsTr("No update file found")
        subtitle: qsTr("Copy a .aosupd update file onto a USB stick on another computer (from the Asima OS website), plug it in here, then look again.")
        icon: "drive-removable-media"
        tone: "info"
        DButton { text: qsTr("Look again"); iconName: "view-refresh"; onClicked: Backend.refresh() }
    }

    // Older versions are allowed (D35): Rescue is for getting the computer working again.
    InfoBanner {
        visible: !!page.current && !!page.current.older
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("This is an older version than the one installed (%1)").arg(Backend.status.version || "")
        text: qsTr("Your files stay. Settings that a newer version changed may not all work in the older one. Pick the newest file you have if you can.")
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Only files signed by Asima OS are accepted")
        text: qsTr("The signature is checked before anything is written. A file that isn't genuine is refused and nothing changes.")
    }

    DButton {
        visible: Backend.updateFiles.length > 0
        text: page.current ? qsTr("Reinstall Asima OS %1").arg(page.current.version) : qsTr("Reinstall")
        variant: "primary"
        iconName: "download"
        enabled: page.current !== null
        onClicked: page.win.confirm({
            title: page.current.older ? qsTr("Go back to Asima OS %1?").arg(page.current.version)
                                      : qsTr("Reinstall Asima OS %1?").arg(page.current.version),
            text: page.current.older
                  ? qsTr("The system files are replaced with this older, verified copy. Your files and apps are not touched; settings made by the newer version may not all work.")
                  : qsTr("The system files are replaced with this verified copy. Your files, apps and settings are not touched."),
            icon: "download", tone: page.current.older ? "warn" : "accent", confirmText: page.current.older ? qsTr("Go back") : qsTr("Reinstall")
        }, () => page.win.run("reinstall", { file: page.chosen }))
    }
}
