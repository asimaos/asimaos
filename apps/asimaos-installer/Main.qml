import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

B.ApplicationWindow {
    id: win
    width: 1320
    height: 860
    minimumWidth: 1024
    minimumHeight: 680
    visible: true
    title: qsTr("Install Asima OS")
    color: Theme.window
    font.family: Theme.fontFamily

    property string startPage: "welcome"
    property string themeMode: "system"
    property string demoState: ""
    property string autotestTarget: ""     // developer test only (--dry-run --autotest-target)

    readonly property var stepKeys: ["welcome", "language", "disk", "account", "encryption", "summary", "install"]
    readonly property var stepLabels: [qsTr("Welcome"), qsTr("Language"), qsTr("Disk"), qsTr("Account"),
                                       qsTr("Encryption"), qsTr("Summary"), qsTr("Install")]
    property string page: startPage
    readonly property int stepIndex: page === "done" ? stepKeys.length - 1 : Math.max(0, stepKeys.indexOf(page))

    // ── The user's choices ───────────────────────────────────────────────────
    property string language: Backend.defaults.language || ""
    property string keyboard: Backend.defaults.keyboard || "us"
    property string timezone: Backend.defaults.timezone || "UTC"
    property bool keyboardTouched: false
    property bool timezoneTouched: false
    property string disk: Backend.defaults.disk || ""
    property string fullName: ""
    property string username: ""
    property bool usernameTouched: false
    property string hostname: ""
    property bool hostnameTouched: false
    property string password: ""
    property bool encrypt: true              // on by default, never mandatory (decision D9)
    property bool useAccountPassword: true   // the account password also unlocks the disk
    property string passphrase: ""           // a separate disk passphrase, when chosen

    readonly property bool accountReady: fullName.length > 0 && Backend.usernameProblem(username) === ""
                                         && Backend.hostnameProblem(hostname) === "" && password.length >= 4
    readonly property bool encryptionReady: !encrypt || (useAccountPassword ? password.length >= 8 : passphrase.length >= 8)

    function go(key) { page = key }
    function next() {
        const i = stepKeys.indexOf(page)
        if (i >= 0 && i < stepKeys.length - 1)
            page = stepKeys[i + 1]
    }
    function back() {
        const i = stepKeys.indexOf(page)
        if (i > 0)
            page = stepKeys[i - 1]
    }
    function setLanguage(id) {
        language = id
        const l = Backend.languages.find(x => x.id === id)
        if (!l)
            return
        if (!keyboardTouched) keyboard = l.keyboard
        if (!timezoneTouched) timezone = l.timezone
    }
    function languageInfo() { return Backend.languages.find(x => x.id === language) || ({ title: language }) }
    function keyboardInfo() { return Backend.keyboards.find(x => x.id === keyboard) || ({ title: keyboard }) }
    function timezoneInfo() { return Backend.timezones.find(x => x.id === timezone) || ({ title: timezone, subtitle: "" }) }
    function diskInfo() { return Backend.disks.find(d => d.path === disk) || ({}) }
    function confirm(options, callback) { confirmDialog.ask(options, callback) }
    function startInstall() {
        Backend.startInstall({ disk: disk, encrypt: encrypt,
                               passphrase: encrypt ? (useAccountPassword ? password : passphrase) : "",
                               user: username, fullName: fullName, password: password, hostname: hostname,
                               language: language, keyboard: keyboard, timezone: timezone })
        password = ""     // secrets are not kept in memory longer than needed
        passphrase = ""
        page = "install"
    }

    Component.onCompleted: {
        Theme.mode = themeMode
        if (demoState.indexOf("filled") >= 0) {
            fullName = "Asha Roy"
            username = "asha"
            hostname = Backend.suggestHostname(username)
            password = "Blue-Lantern-42"
        }
        if (demoState.indexOf("plain") >= 0)
            encrypt = false
        if (autotestTarget.length > 0) {
            disk = autotestTarget
            fullName = "Integration Test"
            username = "tester"
            hostname = "tester-pc"
            password = "integration-test-pw"
            useAccountPassword = false
            passphrase = "integration-test-passphrase"
            startInstall()
        }
    }

    Connections {
        target: Backend
        function onInstallChanged() {
            if (Backend.install.state === "done" && win.page === "install")
                doneTimer.start()
        }
        function onToast(message, tone) { toast.show(message, tone) }
    }
    Timer { id: doneTimer; interval: 900; onTriggered: win.page = "done" }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        WizardRail {
            Layout.fillHeight: true
            Layout.preferredWidth: 300
            subtitle: Backend.demo ? qsTr("Installer · demo") : qsTr("Installer · %1").arg(Backend.system.version)
            steps: win.stepLabels
            current: win.stepIndex
            finished: win.page === "done"
            footnote: {
                if (win.stepIndex < win.stepKeys.length - 1)
                    return qsTr("Nothing on your disks changes until you press Install.")
                switch (Backend.install.state) {
                case "done": return win.encrypt ? qsTr("Your disk is encrypted with your passphrase.")
                                                : qsTr("Asima OS is on your disk.")
                case "error": return qsTr("Your other disks were not touched.")
                default: return qsTr("Please keep the computer switched on.")
                }
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Loader {
                id: pageLoader
                anchors.fill: parent
                sourceComponent: {
                    switch (win.page) {
                    case "language": return languagePage
                    case "disk": return diskPage
                    case "account": return accountPage
                    case "encryption": return encryptionPage
                    case "summary": return summaryPage
                    case "install": return installPage
                    case "done": return donePage
                    default: return welcomePage
                    }
                }
                onLoaded: enter.restart()
            }
            ParallelAnimation {
                id: enter
                NumberAnimation { target: pageLoader.item; property: "opacity"; from: 0; to: 1; duration: Theme.normal; easing.type: Easing.OutCubic }
                NumberAnimation { target: pageLoader.item; property: "x"; from: 24; to: 0; duration: Theme.slow; easing.type: Easing.OutCubic }
            }
        }
    }

    Component { id: welcomePage; WelcomePage {} }
    Component { id: languagePage; LanguagePage {} }
    Component { id: diskPage; DiskPage {} }
    Component { id: accountPage; AccountPage {} }
    Component { id: encryptionPage; EncryptionPage {} }
    Component { id: summaryPage; SummaryPage {} }
    Component { id: installPage; InstallPage {} }
    Component { id: donePage; DonePage {} }

    ConfirmDialog { id: confirmDialog }
    Toast { id: toast }
}
