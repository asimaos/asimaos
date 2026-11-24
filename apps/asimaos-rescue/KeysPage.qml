import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Secure Boot and unlock")
    subtitle: qsTr("For when the computer refuses to start Asima OS, or the disk no longer unlocks by itself.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var st: Backend.status

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 820 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Approve the Asima OS key again")
            subtitle: qsTr("After a firmware reset the computer can forget the Asima OS key, and Secure Boot then refuses Asima OS. You get a code to type on the blue screen at the next start.")
            icon: "view-certificate"
            tone: "accent"
            actions: [ StatusPill { text: page.st.secureBoot === "on" ? qsTr("Secure Boot on") : qsTr("Secure Boot off"); tone: page.st.secureBoot === "on" ? "ok" : "neutral" } ]
            Item { Layout.fillHeight: true }
            DButton {
                text: qsTr("Approve the Asima OS key")
                variant: "primary"
                iconName: "view-certificate"
                focus: true
                onClicked: page.win.run("approve-key")
                Component.onCompleted: forceActiveFocus()
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            visible: !!page.st.encrypted
            title: qsTr("Turn off automatic unlock")
            subtitle: qsTr("If the disk keeps asking for the recovery key, the security chip's unlock may be stale. Turning it off means typing your passphrase at every start, until you set it up again in Asima Maintenance → Security.")
            icon: "computer"
            tone: "warn"
            actions: [ StatusPill { text: page.st.tpmEnrolled ? qsTr("On") : qsTr("Off"); tone: page.st.tpmEnrolled ? "ok" : "neutral" } ]
            Item { Layout.fillHeight: true }
            DButton {
                text: page.win.locked ? qsTr("Unlock the disk first") : qsTr("Turn off automatic unlock")
                variant: "secondary"
                enabled: !!page.st.tpmEnrolled
                onClicked: {
                    if (page.win.locked) {
                        page.win.afterUnlock = "keys"
                        page.win.page = "unlock"
                        return
                    }
                    page.win.confirm({
                        title: qsTr("Turn off automatic unlock?"),
                        text: qsTr("You'll type your passphrase at every start until you set it up again in Asima Maintenance → Security."),
                        icon: "computer", confirmText: qsTr("Turn off")
                    }, () => page.win.run("remove-tpm"))
                }
            }
        }
    }
}
