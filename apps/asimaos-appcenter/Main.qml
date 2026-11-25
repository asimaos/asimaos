import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// Asima App Center (D56): purposes as tabs along the top of Apps, one row per kind of software.
B.ApplicationWindow {
    id: win
    width: 1320
    height: 860
    minimumWidth: 1000
    minimumHeight: 660
    visible: true
    title: qsTr("Asima App Center")
    color: Theme.window
    font.family: Theme.fontFamily

    property string startPage: "store"
    property string themeMode: "system"
    property string page: startPage

    readonly property var pages: [
        { key: "store", label: qsTr("Apps"), icon: "applications-all" },
        { key: "installed", label: qsTr("Installed"), icon: "view-list-details" },
        { key: "updates", label: qsTr("Updates"), icon: "update-none",
          badge: (Backend.updates.total || 0) > 0 ? String(Backend.updates.total) : "", badgeTone: "accent" }
    ].concat(Backend.file.path ? [{ key: "file", label: qsTr("App file"), icon: "document-open" }] : [])

    function navigate(key) { page = key }
    function confirm(options, callback) { confirmDialog.ask(options, callback) }

    Component.onCompleted: Theme.mode = themeMode

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Sidebar {
            Layout.fillHeight: true
            appName: qsTr("Asima App Center")
            appSubtitle: qsTr("Every kind of app")
            appIcon: "appcenter"
            pages: win.pages
            current: win.page
            onNavigate: key => win.page = key

            // The change running now, and how many wait after it.
            Rectangle {
                visible: !!Backend.job.busy
                Layout.fillWidth: true
                implicitHeight: jobRow.implicitHeight + 24
                radius: Theme.radiusL - 2
                color: Theme.surface
                border.width: 1
                border.color: Theme.border
                RowLayout {
                    id: jobRow
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 10
                    Spinner { size: 20 }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        UiText { text: Backend.job.title || ""; variant: "small"; font.weight: Font.DemiBold; Layout.fillWidth: true; elide: Text.ElideRight }
                        UiText {
                            text: (Backend.job.detail || "") + ((Backend.job.waiting || 0) > 0 ? " · " + qsTr("%n more waiting", "", Backend.job.waiting) : "")
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
                    case "installed": return installedPage
                    case "updates": return updatesPage
                    case "file": return filePage
                    default: return storePage
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

    Component { id: storePage; StorePage {} }
    Component { id: installedPage; InstalledPage {} }
    Component { id: updatesPage; UpdatesPage {} }
    Component { id: filePage; FilePage {} }

    ConfirmDialog { id: confirmDialog }
    Toast { id: toast }

    Connections {
        target: Backend
        function onToast(message, tone) { toast.show(message, tone) }
    }
}
