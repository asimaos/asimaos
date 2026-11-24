import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// Factory reset from Rescue (D7, D12): same result as in Asima Maintenance. There is no first-start
// setup (D10), so the fresh system's account is chosen here.
DPage {
    id: page
    title: qsTr("Factory reset")
    subtitle: qsTr("Asima OS %1 starts fresh. Every file, app, setting and snapshot on this computer is deleted.").arg(Backend.status.version || "")

    readonly property var win: B.ApplicationWindow.window
    readonly property bool encrypted: !!Backend.status.encrypted
    readonly property bool nameOk: /^[a-z_][a-z0-9_-]{0,31}$/.test(user.text)
    readonly property bool passOk: pass.text.length >= (page.encrypted ? 8 : 4) && pass.text === repeat.text
    readonly property bool ready: full.text.trim().length > 0 && page.nameOk && page.passOk

    Card {
        Layout.fillWidth: true
        title: qsTr("Who will use the fresh system?")
        subtitle: page.encrypted ? qsTr("This password signs in and unlocks the disk. A new recovery key is shown at the end.")
                                 : qsTr("This password signs in to the fresh system.")
        icon: "user-identity"
        tone: "accent"
        spacing: 12

        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 640 ? 2 : 1
            columnSpacing: 16
            rowSpacing: 10
            DTextField { id: full; Layout.fillWidth: true; placeholderText: qsTr("Name"); focus: true; Component.onCompleted: forceActiveFocus() }
            DTextField {
                id: user
                Layout.fillWidth: true
                placeholderText: qsTr("Username")
                font.family: "Noto Sans Mono"
                tone: user.text.length > 0 && !page.nameOk ? "error" : ""
                validator: RegularExpressionValidator { regularExpression: /[a-z0-9_-]*/ }
            }
            PasswordField {
                id: pass
                Layout.fillWidth: true
                placeholderText: page.encrypted ? qsTr("Password (at least 8 characters)") : qsTr("Password (at least 4 characters)")
            }
            PasswordField {
                id: repeat
                Layout.fillWidth: true
                placeholderText: qsTr("Repeat the password")
                tone: repeat.text.length > 0 && repeat.text !== pass.text ? "error" : ""
            }
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "error"
        title: qsTr("This can't be undone")
        text: qsTr("Backups on other drives are not touched. Everything on this disk except Asima OS %1 is deleted.").arg(Backend.status.version || "")
    }

    DButton {
        text: qsTr("Reset this computer")
        variant: "danger"
        iconName: "edit-reset"
        enabled: page.ready
        onClicked: page.win.confirm({
            title: qsTr("Reset this computer?"),
            text: qsTr("Everything on this computer is deleted, except a fresh Asima OS with the account %1.").arg(user.text),
            tone: "error", icon: "edit-reset", confirmText: qsTr("Reset"), requiredWord: "RESET"
        }, () => {
            page.win.run("factory-reset", { fullName: full.text.trim(), username: user.text,
                                            password: pass.text, passphrase: page.encrypted ? pass.text : "" })
            pass.text = repeat.text = ""
        })
    }
}
