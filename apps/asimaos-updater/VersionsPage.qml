pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Versions")
    subtitle: qsTr("The start menu keeps two versions: the current one on top and a backup, so you can always go back.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var backupVersion: (Backend.versions || []).find(v => v.role === "backup") || null

    // The failed update had changed the system; it goes back at the next start.
    InfoBanner {
        visible: !!Backend.system.settingsRollback
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Settings go back at the next start")
        text: qsTr("The update to %1 didn't start and had changed system settings. At the next start they go back to how they were before it. Your files are not touched.").arg(Backend.system.settingsRollback || "")
    }

    // The sign-in safety net: the desktop kept failing since an update.
    InfoBanner {
        visible: !!page.win.trouble
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("The desktop kept failing since the update to %1").arg(page.win.trouble)
        text: page.backupVersion
              ? qsTr("Go back to %1 to keep working. Your files, apps and settings are not touched, and the update can be tried again later.").arg(page.backupVersion.version)
              : qsTr("There is no other version to go back to. Asima Rescue (in the start menu) can reinstall Asima OS from a USB stick.")
        DButton {
            visible: !!page.backupVersion
            text: qsTr("Go back to %1").arg(page.backupVersion ? page.backupVersion.version : "")
            variant: "primary"
            iconName: "edit-undo"
            onClicked: page.win.confirm({
                title: qsTr("Go back to %1?").arg(page.backupVersion.version),
                text: qsTr("%1 becomes the current version and starts from the next start. Restart afterwards.").arg(page.backupVersion.version),
                confirmText: qsTr("Go back"),
                icon: "edit-undo"
            }, () => Backend.makeVersionCurrent(page.backupVersion.version))
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Automatic safety net")
        text: qsTr("If a new version fails to start three times in a row, your computer starts the backup by itself and makes it the current version again. Going back never touches your files.")
    }

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 800 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Repeater {
            model: Backend.versions
            delegate: Card {
                id: versionCard
                required property var modelData
                readonly property bool running: modelData.role === "running"
                readonly property bool next: modelData.role === "next"     // installed, starts after a restart
                readonly property bool backup: modelData.role === "backup"
                readonly property bool failed: modelData.role === "failed"
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1
                title: running ? qsTr("Running now") : next ? (modelData.fresh ? qsTr("New version") : qsTr("Current version"))
                       : backup ? qsTr("Backup version")
                       : failed ? qsTr("Didn't start") : qsTr("Older version")
                icon: running ? "system-run" : next ? "download" : failed ? "dialog-error" : "edit-undo"
                tone: running ? "ok" : next ? "accent" : failed ? "error" : "info"
                actions: [
                    StatusPill {
                        visible: !!versionCard.modelData.menuTitle
                        text: qsTr("Start menu: %1").arg(versionCard.modelData.menuTitle || "")
                        tone: "neutral"
                    },
                    StatusPill { text: versionCard.modelData.statusText; tone: versionCard.modelData.tone }
                ]

                UiText {
                    text: versionCard.modelData.version
                    variant: "display"
                    Layout.topMargin: 2
                }
                UiText {
                    text: versionCard.modelData.detail
                    tone: "muted"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    ListRow { Layout.fillWidth: true; title: qsTr("Installed"); value: versionCard.modelData.installed || "–" }
                    ListRow { Layout.fillWidth: true; title: qsTr("Size on disk"); value: Theme.bytes(versionCard.modelData.size) }
                    ListRow { Layout.fillWidth: true; title: qsTr("Last start"); value: versionCard.modelData.boot || "–"; separator: false }
                }
                Item { Layout.fillHeight: true }
                DButton {
                    visible: versionCard.next && !!versionCard.modelData.fresh
                    text: qsTr("Restart now")
                    variant: "primary"
                    iconName: "system-reboot"
                    onClicked: Backend.restartNow()
                }
                RowLayout {
                    visible: versionCard.backup
                    spacing: 10
                    DButton {
                        text: qsTr("Make it the current version")
                        variant: "primary"
                        iconName: "edit-undo"
                        onClicked: page.win.confirm({
                            title: qsTr("Go back to %1?").arg(versionCard.modelData.version),
                            text: qsTr("%1 becomes the current version and starts from the next start; %2 becomes the backup. Your files, apps and settings are not touched.")
                                  .arg(versionCard.modelData.version).arg(Backend.system.version),
                            confirmText: qsTr("Go back"),
                            icon: "edit-undo"
                        }, () => Backend.makeVersionCurrent(versionCard.modelData.version))
                    }
                    DButton {
                        text: qsTr("Start it once")
                        variant: "secondary"
                        iconName: "system-reboot"
                        onClicked: Backend.startVersionOnce(versionCard.modelData.version)
                    }
                }
                DButton {
                    // Started once from the backup: keep it.
                    visible: !!versionCard.modelData.startedOnce
                    text: qsTr("Keep using this version")
                    variant: "primary"
                    iconName: "dialog-ok"
                    onClicked: Backend.makeVersionCurrent(versionCard.modelData.version)
                }
                DButton {
                    visible: versionCard.failed
                    text: qsTr("Try it again")
                    variant: "secondary"
                    iconName: "view-refresh"
                    onClicked: page.win.confirm({
                        title: qsTr("Try %1 again?").arg(versionCard.modelData.version),
                        text: qsTr("It becomes the current version with three new tries. If it fails again, your computer goes back to %1 by itself.")
                              .arg(Backend.system.version),
                        confirmText: qsTr("Try again"),
                        icon: "view-refresh"
                    }, () => Backend.makeVersionCurrent(versionCard.modelData.version))
                }
                UiText {
                    visible: versionCard.running
                    text: qsTr("This is the version you are using now.")
                    variant: "small"
                    tone: "subtle"
                }
            }
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("What going back changes")
        icon: "help-about"
        tone: "info"
        spacing: 0

        ListRow {
            Layout.fillWidth: true
            iconName: "system-run"
            title: qsTr("The system")
            subtitle: qsTr("Programs, drivers and the desktop return to the previous version.")
            value: qsTr("Changes")
            tone: "accent"
        }
        ListRow {
            Layout.fillWidth: true
            iconName: "folder-open"
            title: qsTr("Your files, apps and settings")
            subtitle: qsTr("Documents, Flatpak apps, containers, VMs and your settings stay exactly as they are.")
            value: qsTr("Kept")
            tone: "ok"
        }
        ListRow {
            Layout.fillWidth: true
            iconName: "download"
            title: qsTr("The newer version")
            subtitle: qsTr("Stays installed. You can switch back to it at any time.")
            value: qsTr("Kept")
            tone: "ok"
            separator: false
        }
    }
}
