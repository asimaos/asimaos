pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// Apps: purposes as tabs along the top (like KDE's categories), then one row per kind of software,
// each scrolling sideways. A search looks through every kind at once.
DPage {
    id: page
    title: Backend.query ? qsTr("Results for “%1”").arg(Backend.query) : qsTr("Apps")
    subtitle: qsTr("Every kind of software for Linux in one place: sandboxed apps, Android and Windows apps, and developer tools.")
    readonly property var win: B.ApplicationWindow.window

    headerActions: [
        DTextField {
            id: searchField
            implicitWidth: 300
            placeholderText: qsTr("Search every kind of app")
            text: Backend.query
            onAccepted: Backend.search(text)
            inputMethodHints: Qt.ImhNoPredictiveText
        }
    ]

    // ── The purposes, left to right ─────────────────────────────────────────
    Flickable {
        Layout.fillWidth: true
        implicitHeight: tabs.implicitHeight
        contentWidth: tabs.implicitWidth
        clip: true
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        Row {
            id: tabs
            spacing: 8
            Repeater {
                model: Backend.purposes
                delegate: Rectangle {
                    id: tab
                    required property var modelData
                    readonly property bool active: !Backend.query && Backend.purpose === modelData.id
                    implicitWidth: tabRow.implicitWidth + 28
                    implicitHeight: 38
                    radius: height / 2
                    color: active ? Theme.accent : tabMouse.containsMouse ? Theme.surfaceHover : Theme.surface
                    border.width: active ? 0 : 1
                    border.color: Theme.border
                    Row {
                        id: tabRow
                        anchors.centerIn: parent
                        spacing: 8
                        Icon { name: tab.modelData.icon; size: 16; color: tab.active ? Theme.textOnAccent : Theme.textMuted; anchors.verticalCenter: parent.verticalCenter }
                        UiText { text: tab.modelData.title; font.weight: Font.Medium; color: tab.active ? Theme.textOnAccent : Theme.text; anchors.verticalCenter: parent.verticalCenter }
                    }
                    MouseArea {
                        id: tabMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { searchField.text = ""; Backend.setPurpose(tab.modelData.id) }
                    }
                    Accessible.role: Accessible.PageTab
                    Accessible.name: tab.modelData.title
                }
            }
        }
    }

    // ── App groups (the signed feed, D56): a set of apps in one go ──────────
    GridLayout {
        visible: !Backend.query && Backend.purpose === "all" && Backend.groups.length > 0
        Layout.fillWidth: true
        columns: page.contentWidth > 900 ? 3 : 1
        columnSpacing: 14
        rowSpacing: 14
        Repeater {
            model: Backend.groups
            delegate: Card {
                id: groupCard
                required property var modelData
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                title: modelData.title
                subtitle: modelData.summary || ""
                icon: "folder-favorites"
                tone: "info"
                UiText {
                    text: (groupCard.modelData.apps || []).map(a => a.name).join(" · ")
                    variant: "small"
                    tone: "muted"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
                actions: [ DButton {
                    text: qsTr("Install all")
                    variant: "secondary"
                    onClicked: page.win.confirm({
                        title: qsTr("Install %1?").arg(groupCard.modelData.title),
                        text: qsTr("%n app(s) not installed yet are installed, one after another.", "", (groupCard.modelData.apps || []).length),
                        icon: "folder-favorites", tone: "accent", confirmText: qsTr("Install all")
                    }, () => Backend.installGroup(groupCard.modelData.id))
                } ]
            }
        }
    }

    // ── One row per kind of software ─────────────────────────────────────────
    Repeater {
        model: Backend.rows
        delegate: KindRow {
            required property var modelData
            Layout.fillWidth: true
            row: modelData
        }
    }
}
