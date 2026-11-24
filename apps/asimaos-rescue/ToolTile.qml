import QtQuick
import QtQuick.Layouts
import Asima.Ui

// A large, keyboard-focusable tool: Tab or the arrow keys move between tiles, Enter opens one.
Rectangle {
    id: tile
    property string iconName
    property string title
    property string text
    property string tone: "accent"
    signal activated()

    activeFocusOnTab: true
    implicitHeight: Math.max(104, row.implicitHeight + 40)
    radius: Theme.radiusL
    color: hover.containsMouse || activeFocus ? Theme.surfaceHover : Theme.surface
    border.width: activeFocus ? 2 : 1
    border.color: activeFocus ? Theme.accent : Theme.border
    Behavior on color { ColorAnimation { duration: Theme.fast } }

    Keys.onReturnPressed: tile.activated()
    Keys.onEnterPressed: tile.activated()
    Keys.onSpacePressed: tile.activated()

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.margins: 20
        spacing: 16
        IconTile { name: tile.iconName; tone: tile.tone; size: 48; Layout.alignment: Qt.AlignVCenter }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 4
            UiText { text: tile.title; variant: "heading"; Layout.fillWidth: true; wrapMode: Text.WordWrap }
            UiText { text: tile.text; variant: "small"; tone: "muted"; Layout.fillWidth: true; wrapMode: Text.WordWrap }
        }
        Icon { name: "go-next"; size: 18; color: Theme.textSubtle }
    }

    MouseArea {
        id: hover
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            tile.forceActiveFocus()
            tile.activated()
        }
    }

    Accessible.role: Accessible.Button
    Accessible.name: tile.title
    Accessible.description: tile.text
}
