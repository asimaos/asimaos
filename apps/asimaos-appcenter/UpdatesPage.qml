pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

// Updates of every kind: counted in the background (asimaos-appcenter-updates.timer), installed
// with one click. Asima OS's own updates are Asima Updater's.
DPage {
    id: page
    title: qsTr("Updates")
    subtitle: (Backend.updates.total || 0) > 0 ? qsTr("%n update(s) ready to install.", "", Backend.updates.total)
              : Backend.updates.checking ? qsTr("Looking for updates…") : qsTr("Your apps are up to date.")

    headerActions: [
        DButton { text: qsTr("Check again"); variant: "ghost"; iconName: "view-refresh"; enabled: !Backend.updates.checking; onClicked: Backend.checkUpdates() },
        DButton { text: qsTr("Update all"); variant: "primary"; iconName: "update-none"; onClicked: Backend.updateAll() }
    ]

    Repeater {
        model: Backend.updates.kinds || []
        delegate: ListRow {
            id: upRow
            required property var modelData
            Layout.fillWidth: true
            title: modelData.title
            subtitle: (modelData.names || []).slice(0, 8).join(" · ") + ((modelData.names || []).length > 8 ? " …" : "")
            value: qsTr("%n update(s)", "", modelData.count)
            DButton { text: qsTr("Update"); variant: "ghost"; onClicked: Backend.updateKind(upRow.modelData.kind) }
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Not counted, but updated with “Update all”")
        text: qsTr("Nix programs, Python, Rust, Go and Conda tools, and the containers' own packages. Android apps update in F-Droid, Windows programs in Bottles.")
    }
    InfoBanner {
        Layout.fillWidth: true
        tone: "ok"
        title: qsTr("Asima OS itself")
        text: qsTr("Updates of the system come as one file, installed with Asima Updater.")
        DButton { text: qsTr("Open Asima Updater"); variant: "ghost"; onClicked: Backend.openUpdater() }
    }
}
