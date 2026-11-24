import QtQuick
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQuick.Controls.Basic as B
import Asima.Ui

// Android apps with Waydroid (D28; in Asima Dev since D34): set up once, then Android
// apps sit in the app menu next to the others.
DPage {
    id: page
    title: qsTr("Android apps")
    subtitle: qsTr("Run Android apps next to your other apps. Android itself is downloaded once, when you set it up.")

    readonly property var a: Backend.android
    readonly property var win: B.ApplicationWindow.window

    // Not set up yet.
    Card {
        visible: !page.a.ready
        Layout.fillWidth: true
        title: page.a.settingUp ? qsTr("Setting up Android…") : qsTr("Set up Android apps")
        subtitle: page.a.settingUp ? qsTr("Downloading Android from Waydroid's servers (about 1 GB). You can keep working; this page shows when it's ready.")
                                   : qsTr("Downloads Android (about 1 GB) and prepares it. Android runs in its own container: its apps can't reach your files unless you share them.")
        icon: "smartphone"
        tone: page.a.setupFailed ? "error" : "info"

        DProgressBar {
            visible: !!page.a.settingUp
            Layout.fillWidth: true
            indeterminate: true
        }
        UiText {
            visible: !!page.a.setupFailed && !page.a.settingUp
            Layout.fillWidth: true
            text: qsTr("The last set-up didn't finish. Check the internet connection and try again.")
            color: Theme.error
            wrapMode: Text.WordWrap
        }
        DButton {
            visible: !page.a.settingUp
            text: qsTr("Set up Android")
            variant: "primary"
            iconName: "download"
            enabled: !!page.a.available
            onClicked: Backend.androidSetup()
        }
    }

    // Ready.
    Card {
        visible: !!page.a.ready
        Layout.fillWidth: true
        title: qsTr("Android")
        subtitle: page.a.running ? qsTr("Running. Close it when you're done to free memory.")
                                 : qsTr("Stopped. It starts by itself when you open an Android app.")
        icon: "smartphone"
        tone: "ok"
        actions: [ StatusPill { text: page.a.running ? qsTr("Running") : qsTr("Stopped"); tone: page.a.running ? "ok" : "neutral" } ]

        RowLayout {
            spacing: 10
            DButton { text: qsTr("Open Android"); variant: "primary"; iconName: "smartphone"; onClicked: Backend.androidOpen() }
            DButton { visible: !!page.a.running; text: qsTr("Stop"); variant: "secondary"; iconName: "media-playback-stop"; onClicked: Backend.androidStop() }
            DButton { text: qsTr("Install an app file…"); variant: "ghost"; iconName: "list-add"; onClicked: apkDialog.open() }
        }
    }

    InfoBanner {
        visible: !!page.a.softwareOnly
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("Android draws slowly on this computer")
        text: qsTr("With only NVIDIA graphics, Android can't use the graphics card and draws in software. Apps work, games may be slow.")
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Where do Android apps come from?")
        text: qsTr("Install app files (.apk) you trust, or an open app store such as F-Droid from inside Android. Installed apps appear in your app menu.")
    }

    Card {
        visible: !!page.a.ready
        Layout.fillWidth: true
        title: qsTr("Remove Android")
        subtitle: qsTr("Deletes Android with all its apps and their data. You can set it up again later.")
        icon: "edit-delete"
        tone: "error"
        DButton {
            text: page.a.removing ? qsTr("Removing…") : qsTr("Remove Android")
            variant: "danger"
            enabled: !page.a.removing
            onClicked: page.win.confirm({ title: qsTr("Remove Android and all its apps?"),
                                          text: qsTr("Every Android app and everything saved in them is deleted."),
                                          confirmText: qsTr("Remove"), tone: "error", icon: "edit-delete" },
                                        () => Backend.androidRemove())
        }
    }

    FileDialog {
        id: apkDialog
        title: qsTr("Choose an Android app file")
        nameFilters: [qsTr("Android apps (*.apk)")]
        onAccepted: Backend.androidInstall(selectedFile)
    }
}
