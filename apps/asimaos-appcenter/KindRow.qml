pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// One kind of software: its name, then its apps in a row that scrolls sideways.
ColumnLayout {
    id: kindRow
    property var row: ({})
    readonly property var items: row.items || []
    spacing: 10

    RowLayout {
        Layout.fillWidth: true
        spacing: 12
        IconTile { name: kindRow.row.icon || "package"; size: 36; tone: "info" }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            UiText { text: kindRow.row.title || ""; variant: "heading" }
            UiText { text: kindRow.row.subtitle || ""; variant: "small"; tone: "muted"; Layout.fillWidth: true; elide: Text.ElideRight }
        }
        Spinner { visible: kindRow.row.state === "loading"; size: 18 }
    }

    // Not set up yet: one card that does it.
    InfoBanner {
        visible: kindRow.row.state === "setup"
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("%1 isn't set up yet").arg(kindRow.row.title || "")
        text: kindRow.row.kind === "android" ? qsTr("Setting up downloads Android once (about 1 GB). Then F-Droid's apps install here.")
            : kindRow.row.kind === "windows" ? qsTr("Bottles comes from Flathub and runs Windows programs with Wine.")
            : kindRow.row.kind === "snap" ? qsTr("The Snap service starts by itself when it's first used; try again in a moment.")
            : qsTr("It's set up in your home folder, without changing the system.")
        DButton {
            visible: kindRow.row.kind !== "snap"
            text: qsTr("Set up")
            variant: "secondary"
            onClicked: Backend.setUp(kindRow.row.kind)
        }
    }

    UiText {
        visible: kindRow.row.state === "empty" || kindRow.row.state === "error" || (!!kindRow.row.message && kindRow.row.state !== "setup")
        text: kindRow.row.message || (kindRow.row.state === "empty"
              ? (Backend.query ? qsTr("Nothing found here.") : qsTr("Nothing installed yet. Search above to find something."))
              : "")
        tone: kindRow.row.state === "error" ? "warn" : "muted"
        variant: "small"
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
    }

    ListView {
        visible: kindRow.items.length > 0
        Layout.fillWidth: true
        implicitHeight: 198
        orientation: ListView.Horizontal
        spacing: 12
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: kindRow.items
        delegate: AppCard {
            required property var modelData
            app: modelData
        }
        B.ScrollBar.horizontal: DScrollBar {}
    }
}
