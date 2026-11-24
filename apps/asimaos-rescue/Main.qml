import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// Asima Rescue: full screen, keyboard-first (Tab / arrows / Enter, Esc goes back).
B.ApplicationWindow {
    id: win
    width: 1280
    height: 800
    visible: true
    title: qsTr("Asima Rescue")
    color: Theme.window
    font.family: Theme.fontFamily

    property string startPage: "home"
    property string themeMode: "dark"
    property string page: startPage
    property string afterUnlock: ""
    readonly property var st: Backend.status
    readonly property bool locked: !!st.encrypted && !st.unlocked

    // Pages that read or change the Asima OS pool need it unlocked first.
    readonly property var needsPool: ["disk", "restore", "reinstall", "reset"]
    function open(key) {
        if (win.locked && needsPool.indexOf(key) >= 0) {
            afterUnlock = key
            page = "unlock"
        } else {
            page = key
        }
    }
    function run(action, args) {
        Backend.start(action, args || {})
        page = "job"
    }
    function confirm(options, callback) { confirmDialog.ask(options, callback) }
    function back() {
        if (page === "job" && Backend.job.state === "running")
            return
        Backend.clearJob()
        page = "home"
    }

    Component.onCompleted: Theme.mode = themeMode

    Shortcut { sequence: "Esc"; enabled: win.page !== "home"; onActivated: win.back() }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ── Header ───────────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: 76
            color: Theme.sidebar
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 28
                anchors.rightMargin: 28
                spacing: 14
                AsimaMark { size: 38 }
                ColumnLayout {
                    spacing: 0
                    UiText { text: qsTr("Asima Rescue"); variant: "heading"; font.weight: Font.DemiBold }
                    UiText {
                        text: qsTr("Repairs Asima OS %1 without starting it").arg(win.st.version || "")
                        variant: "small"
                        tone: "muted"
                    }
                }
                Item { Layout.fillWidth: true }
                StatusPill {
                    text: !win.st.encrypted ? qsTr("Disk not encrypted") : win.locked ? qsTr("Disk locked") : qsTr("Disk unlocked")
                    tone: win.locked ? "warn" : "ok"
                }
                DButton {
                    visible: win.page !== "home"
                    text: qsTr("Back")
                    variant: "ghost"
                    iconName: "go-previous"
                    enabled: !(win.page === "job" && Backend.job.state === "running")
                    onClicked: win.back()
                }
                DButton { text: qsTr("Power off"); variant: "ghost"; iconName: "system-shutdown"; onClicked: Backend.powerOff() }
                DButton { text: qsTr("Restart"); variant: "secondary"; iconName: "system-reboot"; onClicked: Backend.restart() }
            }
        }

        Loader {
            id: pageLoader
            Layout.fillWidth: true
            Layout.fillHeight: true
            sourceComponent: {
                switch (win.page) {
                case "unlock": return unlockPage
                case "disk": return diskPage
                case "restore": return restorePage
                case "reinstall": return reinstallPage
                case "keys": return keysPage
                case "reset": return resetPage
                case "wipe": return wipePage
                case "job": return jobPage
                default: return homePage
                }
            }
            onLoaded: enter.restart()
        }
        NumberAnimation { id: enter; target: pageLoader.item; property: "opacity"; from: 0; to: 1; duration: Theme.normal }
    }

    Component { id: homePage; HomePage {} }
    Component { id: unlockPage; UnlockPage {} }
    Component { id: diskPage; DiskPage {} }
    Component { id: restorePage; RestorePage {} }
    Component { id: reinstallPage; ReinstallPage {} }
    Component { id: keysPage; KeysPage {} }
    Component { id: resetPage; ResetPage {} }
    Component { id: wipePage; WipePage {} }
    Component { id: jobPage; JobPage {} }

    ConfirmDialog { id: confirmDialog }
    Toast { id: toast }
}
