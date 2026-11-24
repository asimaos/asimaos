import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Unlock your disk")
    subtitle: qsTr("This tool needs to read your encrypted disk. Nothing changes by unlocking it.")

    readonly property var win: B.ApplicationWindow.window
    property string problem: ""
    property bool busy: false

    function tryUnlock() {
        if (secret.text.length === 0 || page.busy)
            return
        page.busy = true
        page.problem = ""
        Backend.unlock(secret.text)
    }

    Connections {
        target: Backend
        function onUnlocked(ok, message) {
            page.busy = false
            secret.text = ""
            if (ok)
                page.win.page = page.win.afterUnlock || "home"
            else
                page.problem = message
        }
    }

    Card {
        Layout.fillWidth: true
        Layout.maximumWidth: 640
        title: qsTr("Passphrase or recovery key")
        subtitle: qsTr("The recovery key is the 64 letters you saved when Asima OS was installed; dashes are optional.")
        icon: "object-unlocked"
        tone: "accent"
        spacing: 14

        PasswordField {
            id: secret
            Layout.fillWidth: true
            focus: true
            tone: page.problem.length > 0 ? "error" : ""
            onAccepted: page.tryUnlock()
            Component.onCompleted: forceActiveFocus()
        }
        UiText {
            text: page.problem.length > 0 ? page.problem
                  : qsTr("Rescue uses the US keyboard layout: letters and digits work the same on most keyboards, some symbols may not.")
            variant: "small"
            tone: page.problem.length > 0 ? "error" : "muted"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }
        DButton {
            text: qsTr("Unlock")
            variant: "primary"
            iconName: "object-unlocked"
            busy: page.busy
            enabled: secret.text.length > 0
            onClicked: page.tryUnlock()
        }
    }
}
