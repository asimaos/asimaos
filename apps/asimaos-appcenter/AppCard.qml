pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// One app: its icon (from its catalogue, or the icon theme), name, summary, and what can be done.
Rectangle {
    id: card
    property var app: ({})
    readonly property bool remoteIcon: /^(https?|file):/.test(app.icon || "")
    readonly property var win: B.ApplicationWindow.window
    width: 220
    height: 190
    radius: Theme.radiusL
    color: Theme.surface
    border.width: 1
    border.color: Theme.border

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 14
        spacing: 6
        Item {
            implicitWidth: 48
            implicitHeight: 48
            Image {
                id: remote
                visible: card.remoteIcon && status !== Image.Error
                anchors.fill: parent
                source: card.remoteIcon ? card.app.icon : ""
                sourceSize.width: 96
                sourceSize.height: 96
                fillMode: Image.PreserveAspectFit
                asynchronous: true
            }
            // The app's own icon in its colours (a generic one when the theme hasn't got it, or the
            // catalogue's picture can't be fetched).
            Image {
                visible: !remote.visible
                anchors.centerIn: parent
                width: 44
                height: 44
                sourceSize: Qt.size(88, 88)
                source: "image://dicon/" + (card.remoteIcon || !card.app.icon ? "application-x-executable" : card.app.icon)
                fillMode: Image.PreserveAspectFit
            }
        }
        UiText { text: card.app.name || card.app.id || ""; font.weight: Font.DemiBold; Layout.fillWidth: true; elide: Text.ElideRight }
        UiText {
            text: card.app.summary || (card.app.version ? qsTr("Version %1").arg(card.app.version) : "")
            variant: "small"
            tone: "muted"
            Layout.fillWidth: true
            Layout.fillHeight: true
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            DButton {
                visible: !card.app.installed
                text: qsTr("Install")
                variant: "primary"
                enabled: !card.app.missing
                onClicked: Backend.install(card.app)
            }
            DButton {
                visible: !!card.app.installed
                text: qsTr("Open")
                variant: "secondary"
                onClicked: Backend.open(card.app)
            }
            DButton {
                visible: !!card.app.installed
                text: qsTr("Remove")
                variant: "ghost"
                onClicked: card.win.confirm({
                    title: qsTr("Remove %1?").arg(card.app.name),
                    text: card.app.container ? qsTr("The container and everything installed in it are deleted.")
                                             : qsTr("It is uninstalled; your own files stay."),
                    icon: "edit-delete", tone: "warn", confirmText: qsTr("Remove")
                }, () => Backend.remove(card.app))
            }
        }
    }
}
