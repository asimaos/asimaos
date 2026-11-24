import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// Encryption is on by default and never mandatory (decision D9).
WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property int minLength: 8

    // 0–4: length plus variety. Long passphrases of plain words score well, as they should.
    function strength(p) {
        if (p.length < page.minLength) return 0
        let s = 1
        if (p.length >= 12) s++
        if (p.length >= 16) s++
        let classes = 0
        if (/[a-z]/.test(p)) classes++
        if (/[A-Z]/.test(p)) classes++
        if (/[0-9]/.test(p)) classes++
        if (/[^A-Za-z0-9]/.test(p)) classes++
        if (classes >= 3) s++
        return Math.min(4, s)
    }
    property bool encrypt: page.win.encrypt
    // The account password can unlock the disk only if it is long enough for a disk passphrase.
    readonly property bool accountFits: page.win.password.length >= page.minLength
    property bool useAccount: page.win.useAccountPassword && page.accountFits
    readonly property int score: strength(pass.text)
    readonly property bool matches: pass.text === repeat.text
    readonly property bool usKeyboard: page.win.keyboard === "us"

    title: qsTr("Protect your data")
    subtitle: qsTr("Encryption keeps your files private if this computer is lost or stolen. It is on unless you switch it off.")
    canContinue: !page.encrypt || page.useAccount || (pass.text.length >= page.minLength && page.matches)
    onNext: {
        page.win.encrypt = page.encrypt
        page.win.useAccountPassword = page.useAccount
        page.win.passphrase = page.encrypt && !page.useAccount ? pass.text : ""
        page.win.next()
    }
    onBack: page.win.back()

    Card {
        Layout.fillWidth: true
        title: qsTr("Encrypt this disk")
        subtitle: page.encrypt ? qsTr("Recommended. Without the passphrase, nobody can read anything on this disk.")
                               : qsTr("Off: anyone who has this disk can read your files.")
        icon: "object-locked"
        tone: page.encrypt ? "accent" : "warn"
        actions: [
            DSwitch {
                checked: page.encrypt
                onToggled: page.encrypt = checked
                Accessible.name: qsTr("Encrypt this disk")
            }
        ]

        // Shown instead of the passphrase choices, so the card never ends up with nothing visible
        // inside (a layout whose children are all hidden keeps its old height).
        InfoBanner {
            visible: !page.encrypt
            Layout.fillWidth: true
            tone: "warn"
            title: qsTr("Encryption can't be added later")
            text: qsTr("Turning it on afterwards means installing Asima OS again. Everything else works the same without it.")
        }

        ColumnLayout {
            visible: page.encrypt
            Layout.fillWidth: true
            spacing: 12

            DCheckBox {
                Layout.fillWidth: true
                text: qsTr("Unlock it with my account password")
                checked: page.useAccount
                enabled: page.accountFits
                onToggled: page.useAccount = checked
            }
            UiText {
                text: page.accountFits
                      ? (page.useAccount ? qsTr("One password to remember. If you change your account password later, change the disk passphrase too in Asima Maintenance → Security.")
                                         : qsTr("Choose a separate disk passphrase below."))
                      : qsTr("Your account password is shorter than %1 characters, so the disk needs its own passphrase.").arg(page.minLength)
                variant: "small"
                tone: "muted"
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }

            ColumnLayout {
                visible: !page.useAccount
                Layout.fillWidth: true
                spacing: 10
                PasswordField {
                    id: pass
                    Layout.fillWidth: true
                    placeholderText: qsTr("Disk passphrase (at least %1 characters)").arg(page.minLength)
                    text: page.win.passphrase
                }
                RowLayout {
                    visible: pass.text.length > 0
                    Layout.fillWidth: true
                    spacing: 12
                    DProgressBar {
                        Layout.fillWidth: true
                        value: Math.max(0.08, page.score / 4)
                        tone: page.score <= 1 ? "error" : page.score === 2 ? "warn" : "ok"
                    }
                    UiText {
                        text: [qsTr("Too short"), qsTr("Weak"), qsTr("Okay"), qsTr("Strong"), qsTr("Very strong")][page.score]
                        variant: "small"
                        font.weight: Font.DemiBold
                        tone: page.score <= 1 ? "error" : page.score === 2 ? "warn" : "ok"
                    }
                }
                PasswordField {
                    id: repeat
                    Layout.fillWidth: true
                    placeholderText: qsTr("Repeat the passphrase")
                    text: page.win.passphrase
                    tone: repeat.text.length > 0 && !page.matches ? "error" : ""
                }
                UiText {
                    text: repeat.text.length > 0 && !page.matches ? qsTr("The passphrases don't match yet.")
                        : qsTr("Tip: a few unrelated words make a passphrase that is long and easy to remember.")
                    variant: "small"
                    tone: repeat.text.length > 0 && !page.matches ? "error" : "muted"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
            }
        }
    }

    // The installer stores the layout for the start-up screen too (asimaos-keyboard), in
    // a small firmware variable. Only firmware that refuses to keep it falls back to US.
    InfoBanner {
        visible: page.encrypt && !page.usKeyboard
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("At startup, the passphrase is typed with your layout (%1)")
              .arg(page.win.keyboardInfo().title)
        text: qsTr("If this computer's firmware doesn't keep the layout, the US layout is used: letters and digits work the same everywhere; some symbols may not.")
    }

    InfoBanner {
        visible: page.encrypt
        Layout.fillWidth: true
        tone: "accent"
        title: qsTr("A recovery key is created too")
        text: qsTr("If you ever forget the passphrase, the recovery key unlocks the disk. You'll write it down at the end.")
    }

    InfoBanner {
        visible: page.encrypt && !!Backend.system.tpm
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("No typing at every start, if you like")
        text: qsTr("After installing, Asima Maintenance → Security can let this computer's security chip (TPM 2.0) unlock the disk by itself.")
    }
}
