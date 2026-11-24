import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// The person who will use this computer. Their account is the administrator; more people can be
// added later in System Settings.
WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property int minLength: 4
    readonly property string nameProblem: Backend.usernameProblem(user.text)
    readonly property string hostProblem: Backend.hostnameProblem(host.text)
    readonly property bool matches: pass.text === repeat.text

    title: qsTr("Who will use this computer?")
    subtitle: qsTr("Your account is the computer's administrator. You can add more people later in System Settings.")
    canContinue: full.text.trim().length > 0 && page.nameProblem === "" && page.hostProblem === ""
                 && pass.text.length >= page.minLength && page.matches
    hint: full.text.trim().length === 0 ? qsTr("Enter your name")
          : pass.text.length < page.minLength ? qsTr("Choose a password") : ""
    onBack: page.win.back()
    onNext: {
        page.win.fullName = full.text.trim()
        page.win.username = user.text
        page.win.hostname = host.text
        page.win.password = pass.text
        page.win.next()
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Your account")
        icon: "user-identity"
        tone: "accent"
        spacing: 14

        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 640 ? 2 : 1
            columnSpacing: 16
            rowSpacing: 6

            UiText { text: qsTr("Your name"); variant: "small"; font.weight: Font.DemiBold }
            UiText { text: qsTr("Username"); variant: "small"; font.weight: Font.DemiBold; visible: parent.columns === 2 }
            DTextField {
                id: full
                Layout.fillWidth: true
                placeholderText: qsTr("e.g. Asha Roy")
                text: page.win.fullName
                maximumLength: 64
                // The username and computer name follow the name until they are edited by hand.
                onTextEdited: {
                    if (!page.win.usernameTouched)
                        user.text = Backend.suggestUsername(text)
                    if (!page.win.hostnameTouched)
                        host.text = Backend.suggestHostname(user.text)
                }
                Accessible.name: qsTr("Your name")
            }
            UiText { text: qsTr("Username"); variant: "small"; font.weight: Font.DemiBold; visible: parent.columns === 1 }
            DTextField {
                id: user
                Layout.fillWidth: true
                placeholderText: qsTr("e.g. asha")
                text: page.win.username
                maximumLength: 32
                font.family: "Noto Sans Mono"
                tone: user.text.length > 0 && page.nameProblem !== "" ? "error" : ""
                validator: RegularExpressionValidator { regularExpression: /[a-z0-9_-]*/ }
                onTextEdited: {
                    page.win.usernameTouched = true
                    if (!page.win.hostnameTouched)
                        host.text = Backend.suggestHostname(text)
                }
                Accessible.name: qsTr("Username")
            }
        }
        UiText {
            text: user.text.length > 0 && page.nameProblem !== "" ? page.nameProblem
                  : qsTr("Used to sign in and for your home folder (/home/%1).").arg(user.text || "…")
            variant: "small"
            tone: user.text.length > 0 && page.nameProblem !== "" ? "error" : "muted"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }

        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border; Layout.topMargin: 4 }

        UiText { text: qsTr("Password"); variant: "small"; font.weight: Font.DemiBold }
        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 640 ? 2 : 1
            columnSpacing: 16
            rowSpacing: 10
            PasswordField {
                id: pass
                Layout.fillWidth: true
                placeholderText: qsTr("Password (at least %1 characters)").arg(page.minLength)
                text: page.win.password
            }
            PasswordField {
                id: repeat
                Layout.fillWidth: true
                placeholderText: qsTr("Repeat the password")
                text: page.win.password
                tone: repeat.text.length > 0 && !page.matches ? "error" : ""
            }
        }
        UiText {
            text: repeat.text.length > 0 && !page.matches ? qsTr("The passwords don't match yet.")
                  : qsTr("You'll use it to sign in and to approve changes to the system.")
            variant: "small"
            tone: repeat.text.length > 0 && !page.matches ? "error" : "muted"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Computer name")
        subtitle: qsTr("How this computer appears to your phone (KDE Connect), on your network and in Bluetooth.")
        icon: "computer"
        tone: "info"
        DTextField {
            id: host
            Layout.fillWidth: true
            Layout.maximumWidth: 420
            text: page.win.hostname
            maximumLength: 63
            font.family: "Noto Sans Mono"
            tone: page.hostProblem !== "" ? "error" : ""
            validator: RegularExpressionValidator { regularExpression: /[a-z0-9-]*/ }
            onTextEdited: page.win.hostnameTouched = true
            Accessible.name: qsTr("Computer name")
        }
        UiText {
            visible: page.hostProblem !== ""
            text: page.hostProblem
            variant: "small"
            tone: "error"
        }
    }
}
