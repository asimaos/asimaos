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
    title: qsTr("Asima Gaming")
    color: Theme.window
    font.family: Theme.fontFamily

    property string startPage: "gaming"
    property string themeMode: "system"
    property string page: startPage

    readonly property bool handheld: Backend.device.type === "handheld"
    // Most PCs have no lights Asima OS can control: then there is no Lighting page (D34).
    readonly property bool hasLights: (Backend.lighting.lights || []).length > 0
    readonly property var pages: [
        { key: "gaming", label: qsTr("Gaming"), icon: "games-achievements" },
        { key: "controllers", label: qsTr("Controllers"), icon: "input-gamepad",
          badge: Backend.controllers.length > 0 ? String(Backend.controllers.length) : "" }
    ].concat(handheld ? [{ key: "handheld", label: qsTr("Handheld"), icon: "smartphone" }] : [])
     .concat(hasLights ? [{ key: "lighting", label: qsTr("Lighting"), icon: "brightness-high" }] : [])

    function navigate(key) { page = key }
    function notify(message, tone) { toast.show(message, tone) }
    function confirm(options, callback) { confirmDialog.ask(options, callback) }

    Component.onCompleted: Theme.mode = themeMode

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Sidebar {
            Layout.fillHeight: true
            appName: qsTr("Asima Gaming")
            appSubtitle: Backend.device.model || ""
            appIcon: "gaming"
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
                    IconTile { name: "input-gamepad"; tone: "accent"; size: 32 }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        UiText {
                            text: Backend.controllers.length > 0 ? qsTr("%n controller(s)", "", Backend.controllers.length)
                                                                 : qsTr("No controller")
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
                    case "controllers": return controllersPage
                    case "handheld": return win.handheld ? handheldPage : gamingPage
                    case "lighting": return win.hasLights ? lightingPage : gamingPage
                    default: return gamingPage
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

    Component { id: gamingPage; GamingPage {} }
    Component { id: controllersPage; ControllersPage {} }
    Component { id: handheldPage; HandheldPage {} }
    Component { id: lightingPage; LightingPage {} }

    ConfirmDialog { id: confirmDialog }
    Toast { id: toast }

    Connections {
        target: Backend
        function onToast(message, tone) { toast.show(message, tone) }
    }
    Connections {
        target: Tester
        function onError(message) { toast.show(message, "warn") }
    }
}
