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
    title: qsTr("Asima Manager")
    color: Theme.window
    font.family: Theme.fontFamily

    property string startPage: "home"
    property string themeMode: "system"
    property string page: startPage

    readonly property bool handheld: Backend.device.type === "handheld"
    readonly property var pages: [
        { key: "home", label: qsTr("Home"), icon: "go-home" },
        { key: "power", label: qsTr("Power & Battery"), icon: "battery-080", section: qsTr("Device") },
        { key: "display", label: qsTr("Display"), icon: "monitor", section: qsTr("Device") },
        { key: "biometrics", label: qsTr("Sign-in & Biometrics"), icon: "fingerprint", section: qsTr("Device") },
        { key: "hardware", label: qsTr("Hardware"), icon: "view-process-system", section: qsTr("Device") },
        { key: "shield", label: qsTr("Shield"), icon: "security-high", section: qsTr("Protection") }
    ]

    function navigate(key) { page = key }
    function notify(message, tone) { toast.show(message, tone) }
    function confirm(options, callback) { confirmDialog.ask(options, callback) }
    function deviceIcon() {
        switch (Backend.device.type) {
        case "handheld": return "smartphone"
        case "laptop": return "computer-laptop"
        case "tablet": return "tablet"
        default: return "computer"
        }
    }

    Component.onCompleted: Theme.mode = themeMode

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Sidebar {
            Layout.fillHeight: true
            appName: qsTr("Asima Manager")
            appSubtitle: Backend.device.model || ""
            appIcon: "manager"
            pages: win.pages
            current: win.page
            onNavigate: key => win.page = key

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: footer.implicitHeight + 24
                radius: Theme.radiusL - 2
                color: Theme.surface
                border.width: 1
                border.color: Theme.border
                RowLayout {
                    id: footer
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 10
                    IconTile { name: win.deviceIcon(); tone: "accent"; size: 32 }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        UiText {
                            text: Backend.device.typeLabel || ""
                            variant: "small"
                            font.weight: Font.DemiBold
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                        UiText {
                            text: Backend.battery.present ? qsTr("Battery %1 %").arg(Backend.battery.percent)
                                                          : (Backend.demo ? qsTr("Demo device") : qsTr("Plugged in"))
                            variant: "caption"
                            tone: "muted"
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                    }
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
                    case "power": return powerPage
                    case "display": return displayPage
                    case "biometrics": return biometricsPage
                    case "hardware": return hardwarePage
                    case "shield": return shieldPage
                    default: return homePage
                    }
                }
                onLoaded: enter.restart()
            }
            ParallelAnimation {
                id: enter
                NumberAnimation { target: pageLoader.item; property: "opacity"; from: 0; to: 1; duration: Theme.normal; easing.type: Easing.OutCubic }
                NumberAnimation { target: pageLoader.item; property: "y"; from: 10; to: 0; duration: Theme.slow; easing.type: Easing.OutCubic }
            }
        }
    }

    Component { id: homePage; HomePage {} }
    Component { id: powerPage; PowerPage {} }
    Component { id: displayPage; DisplayPage {} }
    Component { id: biometricsPage; BiometricsPage {} }
    Component { id: hardwarePage; HardwarePage {} }
    Component { id: shieldPage; ShieldPage {} }

    ConfirmDialog { id: confirmDialog }
    Toast { id: toast }

    Connections {
        target: Backend
        function onToast(message, tone) { toast.show(message, tone) }
    }
    Connections {
        target: Fingerprint
        function onFinished(ok, message) { toast.show(message, ok ? "ok" : "warn") }
    }
    Connections {
        target: Face
        function onFinished(ok, message) { toast.show(message, ok ? "ok" : "warn") }
    }
}
