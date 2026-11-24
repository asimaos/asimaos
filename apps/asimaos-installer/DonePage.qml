import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import QtQuick.Dialogs
import Asima.Ui

WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property string key: Backend.install.recoveryKey || ""
    readonly property bool fakeKey: Backend.demo || !!Backend.install.dryRun

    readonly property bool encrypted: page.win.encrypt

    title: qsTr("Asima OS is installed")
    subtitle: page.encrypted ? qsTr("One last, important step: keep your recovery key safe.")
                             : qsTr("Restart, and your new system is ready.")
    showBack: false
    nextText: qsTr("Restart now")
    nextIcon: "system-reboot"
    canContinue: !page.encrypted || saved.checked
    hint: !page.encrypted || saved.checked ? "" : qsTr("Save your recovery key first")
    onNext: Backend.restart()

    Card {
        visible: page.encrypted
        Layout.fillWidth: true
        title: qsTr("Your recovery key")
        subtitle: qsTr("If you ever forget your passphrase, this key unlocks your disk. It is shown only now: write it down, or save it somewhere other than this computer.")
        icon: "view-certificate"
        tone: "accent"
        spacing: 16
        actions: [ StatusPill { visible: page.fakeKey; text: qsTr("Demo key, not real"); tone: "warn" } ]

        Rectangle {
            Layout.fillWidth: true
            implicitHeight: keyGrid.implicitHeight + 40
            radius: Theme.radiusL - 2
            color: Theme.surfaceAlt
            border.width: 1
            border.color: Theme.border

            GridLayout {
                id: keyGrid
                anchors.centerIn: parent
                columns: 4
                columnSpacing: 26
                rowSpacing: 14
                Repeater {
                    model: page.key.length ? page.key.split("-") : []
                    delegate: UiText {
                        required property string modelData
                        text: modelData
                        font.family: "Noto Sans Mono"
                        font.pixelSize: 22
                        font.weight: Font.DemiBold
                        font.letterSpacing: 1.5
                    }
                }
            }
        }

        RowLayout {
            spacing: 10
            DButton { text: qsTr("Copy"); iconName: "edit-copy"; onClicked: Backend.copyText(page.key) }
            DButton { text: qsTr("Save to a file…"); iconName: "document-save-as"; onClicked: saveDialog.open() }
        }

        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.border }

        DCheckBox {
            id: saved
            Layout.fillWidth: true
            text: qsTr("I have saved my recovery key somewhere safe")
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Remove the USB stick")
        text: page.encrypted
              ? qsTr("Take out the USB stick when the screen goes dark. Asima OS then starts from your disk and asks for your passphrase.")
              : qsTr("Take out the USB stick when the screen goes dark. Asima OS then starts from your disk; sign in as %1.").arg(page.win.username || qsTr("yourself"))
    }

    FileDialog {
        id: saveDialog
        title: qsTr("Save the recovery key")
        fileMode: FileDialog.SaveFile
        nameFilters: [qsTr("Text files (*.txt)")]
        defaultSuffix: "txt"
        selectedFile: "file:///asimaos-recovery-key.txt"
        onAccepted: Backend.saveText(selectedFile,
            qsTr("Asima OS recovery key\n\n%1\n\nIt unlocks your disk if you forget the passphrase. Keep it away from the computer.\n").arg(page.key))
    }
}
