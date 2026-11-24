import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property var d: page.win.diskInfo()
    // The user types the end of the disk's serial number: proves they looked at which disk it is.
    readonly property string confirmWord: (d.serial || "").length >= 4 ? d.serial.slice(-4) : "ERASE"

    title: qsTr("Ready to install")
    subtitle: qsTr("Check your choices. Nothing on your disks changes until you press Erase and install.")
    nextText: qsTr("Erase and install")
    nextVariant: "danger"
    canContinue: page.d.usable === true && page.win.accountReady && page.win.encryptionReady
    hint: !page.win.accountReady ? qsTr("Set up your account first")
          : !page.win.encryptionReady ? qsTr("Set a disk passphrase first") : ""
    onBack: win.back()
    onNext: page.win.confirm({
        title: qsTr("Erase %1?").arg(page.d.model),
        text: page.confirmWord === "ERASE"
              ? qsTr("Everything on this %1 disk will be erased.").arg(Theme.bytes(page.d.size))
              : qsTr("Everything on this %1 disk will be erased. To confirm it's the right disk, type the last four characters of its serial number: %2")
                .arg(Theme.bytes(page.d.size)).arg(page.confirmWord),
        tone: "error",
        icon: "drive-harddisk",
        confirmText: qsTr("Erase and install"),
        requiredWord: page.confirmWord
    }, () => page.win.startInstall())

    Card {
        Layout.fillWidth: true
        spacing: 0

        ListRow {
            Layout.fillWidth: true
            iconName: "globe"
            title: qsTr("Language")
            subtitle: page.win.languageInfo().title
            DButton { text: qsTr("Change"); variant: "ghost"; onClicked: page.win.go("language") }
        }
        ListRow {
            Layout.fillWidth: true
            iconName: "input-keyboard"
            title: qsTr("Keyboard")
            subtitle: page.win.keyboardInfo().title
            DButton { text: qsTr("Change"); variant: "ghost"; onClicked: page.win.go("language") }
        }
        ListRow {
            Layout.fillWidth: true
            iconName: "clock"
            title: qsTr("Time zone")
            subtitle: page.win.timezoneInfo().title + (page.win.timezoneInfo().subtitle ? "  ·  " + page.win.timezoneInfo().subtitle : "")
            DButton { text: qsTr("Change"); variant: "ghost"; onClicked: page.win.go("language") }
        }
        ListRow {
            Layout.fillWidth: true
            iconName: "drive-harddisk"
            title: qsTr("Disk")
            subtitle: (page.d.model || "") + "  ·  " + Theme.bytes(page.d.size) + "  ·  " + qsTr("contains %1").arg(page.d.contents || "")
            value: qsTr("Will be erased")
            tone: "error"
            DButton { text: qsTr("Change"); variant: "ghost"; onClicked: page.win.go("disk") }
        }
        ListRow {
            Layout.fillWidth: true
            iconName: "user-identity"
            title: qsTr("Account")
            subtitle: page.win.fullName.length > 0
                      ? qsTr("%1 · %2 · administrator").arg(page.win.fullName).arg(page.win.username)
                      : qsTr("Not set up yet")
            tone: page.win.accountReady ? "neutral" : "error"
            DButton { text: qsTr("Change"); variant: "ghost"; onClicked: page.win.go("account") }
        }
        ListRow {
            Layout.fillWidth: true
            iconName: "computer"
            title: qsTr("Computer name")
            subtitle: page.win.hostname
            DButton { text: qsTr("Change"); variant: "ghost"; onClicked: page.win.go("account") }
        }
        ListRow {
            Layout.fillWidth: true
            iconName: "object-locked"
            title: qsTr("Encryption")
            subtitle: !page.win.encrypt ? qsTr("Off: the disk is not encrypted")
                      : page.win.useAccountPassword ? qsTr("On · unlocks with your account password · recovery key")
                      : qsTr("On · its own passphrase · recovery key")
            value: page.win.encrypt ? "" : qsTr("Not encrypted")
            tone: page.win.encrypt ? "neutral" : "warn"
            separator: false
            DButton { text: qsTr("Change"); variant: "ghost"; onClicked: page.win.go("encryption") }
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("After the first start")
        text: page.win.encrypt
              ? qsTr("Asima Maintenance → Security sets up Secure Boot and, if you like, lets this computer unlock the disk by itself.")
              : qsTr("Asima Maintenance → Security sets up Secure Boot.")
    }
}
