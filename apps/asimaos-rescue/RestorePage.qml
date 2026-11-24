pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Restore my files")
    subtitle: qsTr("Your home folder goes back to how it was at the snapshot. Today's files are kept as a snapshot first, so nothing is lost.")

    readonly property var win: B.ApplicationWindow.window
    property string chosen: Backend.snapshots.length > 0 ? Backend.snapshots[0].id : ""
    readonly property var current: Backend.snapshots.find(s => s.id === page.chosen) || null

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 10
        Repeater {
            model: Backend.snapshots
            delegate: SelectCard {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: modelData.kind === "update" ? "download" : "view-history"
                title: modelData.title
                subtitle: (modelData.kind === "update" ? qsTr("Taken before an update") : qsTr("Daily snapshot"))
                          + "  ·  " + qsTr("%1 changed since").arg(Theme.bytes(modelData.size))
                selected: page.chosen === modelData.id
                focus: index === 0
                activeFocusOnTab: true
                onToggled: page.chosen = modelData.id
            }
        }
        UiText {
            visible: Backend.snapshots.length === 0
            text: qsTr("There are no snapshots on this disk.")
            tone: "muted"
        }
    }

    RowLayout {
        spacing: 12
        DButton {
            text: qsTr("Restore from this snapshot")
            variant: "primary"
            iconName: "view-history"
            enabled: page.current !== null
            onClicked: page.win.confirm({
                title: qsTr("Restore your files from %1?").arg(page.current.title),
                text: qsTr("Your home folder goes back to that moment. Your files as they are now are saved as a snapshot first."),
                icon: "view-history", confirmText: qsTr("Restore")
            }, () => page.win.run("restore", { snapshot: page.chosen }))
        }
    }
}
