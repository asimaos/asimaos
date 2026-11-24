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
    title: qsTr("Asima Maintenance")
    color: Theme.window
    font.family: Theme.fontFamily

    property string startPage: "overview"
    property string themeMode: "system"
    property string page: startPage

    Component.onCompleted: Theme.mode = themeMode

    readonly property var pages: [
        { key: "overview", label: qsTr("Overview"), icon: "go-home" },
        { key: "health", label: qsTr("Health"), icon: "office-chart-line" },
        { key: "security", label: qsTr("Security"), icon: "object-locked" },
        { key: "backups", label: qsTr("Snapshots & Backup"), icon: "backup" },
        { key: "changes", label: qsTr("System changes"), icon: "document-edit",
          badge: Backend.changes.length > 0 ? String(Backend.changes.length) : "", badgeTone: "neutral" },
        { key: "reset", label: qsTr("Reset"), icon: "edit-reset", section: qsTr("Danger zone") }
    ]

    // Shared dialogs for every page: ApplicationWindow.window.confirm(...) / .notify(...)
    function confirm(options, callback) { confirmDialog.ask(options, callback) }
    function notify(message, tone) { toast.show(message, tone) }
    function navigate(key) { page = key }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Sidebar {
            Layout.fillHeight: true
            appName: qsTr("Asima Maintenance")
            appSubtitle: "Asima OS " + (Backend.system.version || "")
            appIcon: "maintenance"
            pages: win.pages
            current: win.page
            onNavigate: key => win.page = key

            // Footer: compact system status
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: footerRow.implicitHeight + 24
                radius: Theme.radiusL - 2
                color: Theme.surface
                border.width: 1
                border.color: Theme.border

                RowLayout {
                    id: footerRow
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 10
                    StatusGlyph { tone: Backend.system.health || "info"; size: 22 }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        UiText {
                            text: Backend.system.healthShort || (Backend.system.health === "ok" ? qsTr("System healthy") : qsTr("Limited view"))
                            variant: "small"
                            font.weight: Font.DemiBold
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }
                        UiText {
                            text: Backend.demo ? qsTr("Demo data") : (Backend.system.hostname || "")
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
                    case "health": return healthPage
                    case "security": return securityPage
                    case "backups": return backupsPage
                    case "changes": return changesPage
                    case "reset": return resetPage
                    default: return overviewPage
                    }
                }
                onLoaded: enterAnim.restart()
            }

            ParallelAnimation {
                id: enterAnim
                NumberAnimation { target: pageLoader.item; property: "opacity"; from: 0; to: 1; duration: Theme.normal; easing.type: Easing.OutCubic }
                NumberAnimation { target: pageLoader.item; property: "y"; from: 10; to: 0; duration: Theme.slow; easing.type: Easing.OutCubic }
            }
        }
    }

    Component { id: overviewPage; OverviewPage {} }
    Component { id: healthPage; HealthPage {} }
    Component { id: securityPage; SecurityPage {} }
    Component { id: backupsPage; BackupsPage {} }
    Component { id: changesPage; ChangesPage {} }
    Component { id: resetPage; ResetPage {} }

    ConfirmDialog { id: confirmDialog }
    Toast { id: toast }

    Connections {
        target: Backend
        function onToast(message, tone) { toast.show(message, tone) }
    }
}
