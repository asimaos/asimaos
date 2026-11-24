import QtCore
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import QtQuick.Dialogs
import Asima.Ui

B.ApplicationWindow {
    id: win
    width: 1280
    height: 820
    minimumWidth: 980
    minimumHeight: 640
    visible: true
    title: qsTr("Asima Updater")
    color: Theme.window
    font.family: Theme.fontFamily

    property string startPage: "updates"
    // Started by the sign-in safety net: the version whose desktop kept failing.
    property string trouble: ""
    property string themeMode: "system"
    property string page: startPage

    Component.onCompleted: Theme.mode = themeMode

    readonly property var pages: [
        { key: "updates", label: qsTr("Updates"), icon: "download",
          badge: Backend.pendingUpdate.state === "ready" ? qsTr("1") : "" },
        { key: "versions", label: qsTr("Versions"), icon: "edit-undo-history" }
    ]

    // Shared dialogs for every page: ApplicationWindow.window.confirm(...) / .notify(...)
    function confirm(options, callback) { confirmDialog.ask(options, callback) }
    function notify(message, tone) { toast.show(message, tone) }
    function navigate(key) { page = key }
    function chooseUpdateFile() { updateFileDialog.open() }

    FileDialog {
        id: updateFileDialog
        title: qsTr("Choose an Asima OS update file")
        nameFilters: [qsTr("Asima OS updates (*.aosupd)"), qsTr("All files (*)")]
        currentFolder: StandardPaths.writableLocation(StandardPaths.DownloadLocation)
        onAccepted: {
            Backend.selectUpdateFile(selectedFile)
            win.page = "updates"
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Sidebar {
            Layout.fillHeight: true
            appName: qsTr("Asima Updater")
            appSubtitle: "Asima OS " + (Backend.system.version || "")
            appIcon: "updater"
            pages: win.pages
            current: win.page
            onNavigate: key => win.page = key

            // Footer: the version that starts next after an update, or the one to go back to
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
                    StatusGlyph { tone: !Backend.system.pendingVersion && Backend.system.previousVersion ? "ok" : "info"; size: 22 }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        UiText {
                            text: Backend.system.pendingVersion ? qsTr("Next start: %1").arg(Backend.system.pendingVersion)
                                  : Backend.system.previousVersion ? qsTr("Backup: %1").arg(Backend.system.previousVersion)
                                  : qsTr("No backup version yet")
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
                sourceComponent: win.page === "versions" ? versionsPage : updatesPage
                onLoaded: enterAnim.restart()
            }

            ParallelAnimation {
                id: enterAnim
                NumberAnimation { target: pageLoader.item; property: "opacity"; from: 0; to: 1; duration: Theme.normal; easing.type: Easing.OutCubic }
                NumberAnimation { target: pageLoader.item; property: "y"; from: 10; to: 0; duration: Theme.slow; easing.type: Easing.OutCubic }
            }
        }
    }

    Component { id: updatesPage; UpdatesPage {} }
    Component { id: versionsPage; VersionsPage {} }

    ConfirmDialog { id: confirmDialog }
    Toast { id: toast }

    Connections {
        target: Backend
        function onToast(message, tone) { toast.show(message, tone) }
    }
}
