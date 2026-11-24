pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// One reset (D7, D12, D13): the computer returns to a fresh installation of the current version,
// exactly as if it had just been installed. There is no first-start setup (D10), so the fresh
// system's account (and, when encrypted, its new disk passphrase) is set here first, as the
// installer would ask.
DPage {
    id: page
    title: qsTr("Reset")
    subtitle: qsTr("Return this computer to a fresh installation of Asima OS %1, as if it had just been installed.").arg(page.version)

    readonly property var win: B.ApplicationWindow.window
    readonly property string version: Backend.system.version || ""
    readonly property bool encrypted: !!Backend.protection.encrypted
    readonly property bool prepared: !!Backend.reset.prepared
    readonly property string recoveryKey: Backend.reset.recoveryKey || ""

    readonly property var deleted: [
        { title: qsTr("All users and their files"), detail: qsTr("Documents, pictures, downloads: everything in /home") },
        { title: qsTr("All snapshots"), detail: qsTr("Including the daily snapshots of your files") },
        { title: qsTr("Apps, containers and virtual machines"), detail: qsTr("Flatpak apps, Podman and Distrobox containers, VMs") },
        { title: qsTr("All settings"), detail: qsTr("System and desktop settings, Wi-Fi passwords, accounts") },
        { title: qsTr("Language, keyboard, time zone and computer name"), detail: qsTr("Back to what Asima OS ships: English, the US keyboard, UTC") },
        { title: qsTr("Old passphrases and keys"), detail: qsTr("The old passphrase, recovery key and automatic unlock stop working") },
        { title: qsTr("The previous Asima OS version"), detail: qsTr("Only %1 remains").arg(page.version) }
    ]

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: heroRow.implicitHeight + 48
        radius: Theme.radiusXl
        color: Theme.errorSoft
        border.width: 1
        border.color: Qt.alpha(Theme.error, 0.25)

        RowLayout {
            id: heroRow
            anchors.fill: parent
            anchors.margins: 24
            spacing: 20
            IconTile { name: "edit-reset"; tone: "error"; size: 56; color: Theme.surface; Layout.alignment: Qt.AlignTop }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6
                UiText {
                    text: qsTr("Everything is deleted. Nothing is kept.")
                    variant: "subtitle"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
                UiText {
                    text: qsTr("Afterwards the computer is exactly like a new installation of Asima OS %1, with the account you choose below, just like resetting a phone. This cannot be undone.").arg(page.version)
                    tone: "muted"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
            }
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        iconName: "edit-undo"
        title: qsTr("Only the settings misbehave?")
        text: qsTr("Reset settings puts them back to the defaults and keeps your files, accounts and apps.")
        DButton { text: qsTr("Reset settings"); variant: "ghost"; onClicked: page.win.navigate("changes") }
    }

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 820 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            title: qsTr("Deleted")
            icon: "trash-empty"
            tone: "error"
            spacing: 0
            Repeater {
                model: page.deleted
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    title: modelData.title
                    subtitle: modelData.detail
                    separator: index < page.deleted.length - 1
                }
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            title: qsTr("Kept")
            icon: "dialog-ok-apply"
            tone: "ok"
            spacing: 0
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Asima OS %1").arg(page.version)
                subtitle: qsTr("Freshly installed, with the account you choose below")
            }
            ListRow {
                Layout.fillWidth: true
                iconName: "backup"
                title: qsTr("Back up first?")
                subtitle: qsTr("Backups on another drive are never touched")
                separator: false
                DButton { text: qsTr("Backup"); variant: "ghost"; onClicked: page.win.navigate("backups") }
            }
        }
    }

    // ── Step 1: the fresh system's account ───────────────────────────────────
    Card {
        id: fresh
        visible: !page.prepared
        Layout.fillWidth: true
        title: qsTr("Who will use the fresh system?")
        subtitle: page.encrypted ? qsTr("Asima OS has no setup screen after a reset, so the new account and disk passphrase are set now, as the installer would ask.")
                                 : qsTr("Asima OS has no setup screen after a reset, so the new account is set now, as the installer would ask.")
        icon: "user-identity"
        tone: "accent"
        spacing: 14

        readonly property bool nameOk: /^[a-z_][a-z0-9_-]{0,31}$/.test(user.text)
        readonly property bool passOk: pass.text.length >= 4 && pass.text === repeat.text
        readonly property bool diskOk: !page.encrypted || (sameAsAccount.checked ? pass.text.length >= 8
                                       : (disk.text.length >= 8 && disk.text === diskRepeat.text))
        readonly property bool ready: full.text.trim().length > 0 && nameOk && passOk && diskOk
                                      && (!page.encrypted || current.text.length > 0)

        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 640 ? 2 : 1
            columnSpacing: 16
            rowSpacing: 10
            DTextField { id: full; Layout.fillWidth: true; placeholderText: qsTr("Name"); text: Backend.account.fullName || "" }
            DTextField {
                id: user
                Layout.fillWidth: true
                placeholderText: qsTr("Username")
                text: Backend.account.username || ""
                font.family: "Noto Sans Mono"
                tone: user.text.length > 0 && !fresh.nameOk ? "error" : ""
                validator: RegularExpressionValidator { regularExpression: /[a-z0-9_-]*/ }
            }
            PasswordField { id: pass; Layout.fillWidth: true; placeholderText: qsTr("New password (at least 4 characters)") }
            PasswordField {
                id: repeat
                Layout.fillWidth: true
                placeholderText: qsTr("Repeat the password")
                tone: repeat.text.length > 0 && repeat.text !== pass.text ? "error" : ""
            }
        }

        ColumnLayout {
            visible: page.encrypted
            Layout.fillWidth: true
            spacing: 10
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }
            DCheckBox {
                id: sameAsAccount
                text: qsTr("Unlock the disk with this password")
                checked: true
            }
            GridLayout {
                visible: !sameAsAccount.checked
                Layout.fillWidth: true
                columns: page.contentWidth > 640 ? 2 : 1
                columnSpacing: 16
                rowSpacing: 10
                PasswordField { id: disk; Layout.fillWidth: true; placeholderText: qsTr("New disk passphrase (at least 8 characters)") }
                PasswordField {
                    id: diskRepeat
                    Layout.fillWidth: true
                    placeholderText: qsTr("Repeat the passphrase")
                    tone: diskRepeat.text.length > 0 && diskRepeat.text !== disk.text ? "error" : ""
                }
            }
            UiText {
                text: qsTr("After the reset the keyboard is the US layout, also when this passphrase is typed at start-up. If yours isn't a US keyboard, choose a passphrase that types the same on both, or keep the recovery key at hand.")
                variant: "small"
                tone: "muted"
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
            UiText {
                visible: sameAsAccount.checked && pass.text.length > 0 && pass.text.length < 8
                text: qsTr("A disk passphrase needs at least 8 characters: choose a longer password or a separate passphrase.")
                variant: "small"
                tone: "error"
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
            UiText { text: qsTr("To allow this, type the current disk passphrase or recovery key"); variant: "small"; font.weight: Font.DemiBold }
            PasswordField { id: current; Layout.fillWidth: true; Layout.maximumWidth: 420 }
        }

        DButton {
            Layout.topMargin: 4
            text: page.encrypted ? qsTr("Continue: make the new recovery key") : qsTr("Continue")
            variant: "primary"
            iconName: "go-next"
            iconRight: true
            enabled: fresh.ready
            onClicked: {
                Backend.prepareFactoryReset({
                    fullName: full.text.trim(), username: user.text, password: pass.text,
                    passphrase: page.encrypted ? (sameAsAccount.checked ? pass.text : disk.text) : "",
                    current: current.text
                })
                pass.text = repeat.text = disk.text = diskRepeat.text = current.text = ""
            }
        }
    }

    // ── Step 2: the new recovery key, then confirm ───────────────────────────
    Card {
        visible: page.prepared
        Layout.fillWidth: true
        title: page.recoveryKey.length > 0 ? qsTr("The fresh system's recovery key") : qsTr("Ready to reset")
        subtitle: page.recoveryKey.length > 0
                  ? qsTr("It unlocks the disk if the new passphrase is forgotten. It is shown only now: write it down, or save it away from this computer.")
                  : qsTr("Afterwards, sign in as %1 with the password you just chose.").arg(Backend.reset.username || "")
        icon: page.recoveryKey.length > 0 ? "password-show-on" : "edit-reset"
        tone: page.recoveryKey.length > 0 ? "accent" : "error"
        spacing: 16

        Rectangle {
            visible: page.recoveryKey.length > 0
            Layout.fillWidth: true
            implicitHeight: resetKeyGrid.implicitHeight + 36
            radius: Theme.radiusL - 2
            color: Theme.surfaceAlt
            border.width: 1
            border.color: Theme.border
            GridLayout {
                id: resetKeyGrid
                anchors.centerIn: parent
                columns: 4
                columnSpacing: 24
                rowSpacing: 12
                Repeater {
                    model: page.recoveryKey.length ? page.recoveryKey.split("-") : []
                    delegate: UiText {
                        required property string modelData
                        text: modelData
                        font.family: "Noto Sans Mono"
                        font.pixelSize: 20
                        font.weight: Font.DemiBold
                        font.letterSpacing: 1.2
                    }
                }
            }
        }
        DCheckBox {
            id: keySaved
            visible: page.recoveryKey.length > 0
            text: qsTr("I have saved the new recovery key somewhere safe")
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            DTextField {
                id: confirmField
                Layout.preferredWidth: 260
                placeholderText: qsTr("Type RESET to confirm")
            }
            DButton {
                text: qsTr("Reset and restart")
                variant: "danger"
                iconName: "edit-reset"
                enabled: confirmField.text.trim().toUpperCase() === "RESET" && (page.recoveryKey.length === 0 || keySaved.checked)
                onClicked: page.win.confirm({
                    title: qsTr("Reset this computer now?"),
                    text: qsTr("Everything on this computer will be deleted and it will restart as a fresh installation. This cannot be undone."),
                    tone: "error", confirmText: qsTr("Reset and restart"), icon: "edit-reset"
                }, () => Backend.factoryReset())
            }
            Item { Layout.fillWidth: true }
            DButton { text: qsTr("Start over"); variant: "ghost"; onClicked: Backend.cancelFactoryReset() }
        }
    }
}
