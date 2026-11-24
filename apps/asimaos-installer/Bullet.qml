import QtQuick
import QtQuick.Layouts
import Asima.Ui

// One line of a feature list.
RowLayout {
    property string text
    Layout.fillWidth: true
    spacing: 10
    Rectangle {
        implicitWidth: 6
        implicitHeight: 6
        radius: 3
        color: Theme.accent
        Layout.alignment: Qt.AlignTop
        Layout.topMargin: 7
    }
    UiText {
        text: parent.text
        variant: "small"
        tone: "muted"
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
    }
}
