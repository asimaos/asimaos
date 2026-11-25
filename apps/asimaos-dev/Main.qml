import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

B.ApplicationWindow {
    id: win
    width: 1280
    height: 820
    minimumWidth: 980
    minimumHeight: 640
    visible: true
    title: qsTr("Asima Dev")
    color: Theme.window
    font.family: Theme.fontFamily

    property string startPage: "containers"
    property string themeMode: "system"
    property string page: startPage

    readonly property var containers: Backend.workspaces.containers || []
    readonly property var vms: Backend.workspaces.vms || []
    readonly property var pages: [
        { key: "containers", label: qsTr("Containers"), icon: "virtual-desktops",
          badge: containers.length > 0 ? String(containers.length) : "", badgeTone: "neutral" },
        { key: "machines", label: qsTr("Virtual machines"), icon: "computer",
          badge: vms.length > 0 ? String(vms.length) : "", badgeTone: "neutral" },
        { key: "nix", label: qsTr("Nix programs"), icon: "package",
          badge: (Backend.nix.programs || []).length > 0 ? String(Backend.nix.programs.length) : "", badgeTone: "neutral" },
        { key: "android", label: qsTr("Android apps"), icon: "smartphone" },
        { key: "storage", label: qsTr("Disk use"), icon: "drive-harddisk" }
    ]

    function navigate(key) { page = key }
    function notify(message, tone) { toast.show(message, tone) }
    function confirm(options, callback) { confirmDialog.ask(options, callback) }

    Component.onCompleted: Theme.mode = themeMode

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Sidebar {
            Layout.fillHeight: true
            appName: qsTr("Asima Dev")
            appSubtitle: qsTr("Workspaces")
            appIcon: "dev"
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
                    StatusGlyph { tone: Backend.workspaces.podman ? "ok" : "warn"; size: 22 }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        UiText {
                            text: Backend.workspaces.podman ? qsTr("Podman ready") : qsTr("Podman not available")
                            variant: "small"
                            font.weight: Font.DemiBold
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                        UiText {
                            text: Backend.demo ? qsTr("Demo data") : (Backend.workspaces.distrobox ? qsTr("Distrobox installed") : qsTr("No Distrobox"))
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
                    case "machines": return machinesPage
                    case "nix": return nixPage
                    case "android": return androidPage
                    case "storage": return storagePage
                    default: return containersPage
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

    Component { id: containersPage; ContainersPage {} }
    Component { id: machinesPage; MachinesPage {} }
    Component { id: nixPage; NixPage {} }
    Component { id: androidPage; AndroidPage {} }
    Component { id: storagePage; StoragePage {} }

    ConfirmDialog { id: confirmDialog }
    Toast { id: toast }

    Connections {
        target: Backend
        function onToast(message, tone) { toast.show(message, tone) }
    }
}
