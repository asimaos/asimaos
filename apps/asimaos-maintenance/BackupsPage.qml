pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import QtQuick.Dialogs
import Asima.Ui

DPage {
    id: page
    title: qsTr("Snapshots & Backup")
    subtitle: qsTr("Snapshots undo mistakes on this disk. Backups keep your files safe if the disk fails.")

    readonly property var b: Backend.backup
    readonly property var win: B.ApplicationWindow.window
    property int tab: Backend.uiHint === "backup-tab" ? 1 : 0
    readonly property string scheduleText: ({
        "daily": qsTr("Your files and settings, every night"),
        "weekly": qsTr("Your files and settings, every week"),
        "monthly": qsTr("Your files and settings, every month"),
        "off": qsTr("Only when you take one")
    })[page.b.snapshotSchedule || "daily"]

    headerActions: [
        SegmentedTabs {
            options: [qsTr("Snapshots"), qsTr("Backup")]
            segmentWidth: 128
            currentIndex: page.tab
            onActivated: index => page.tab = index
        }
    ]

    // ── Snapshots ────────────────────────────────────────────────────────────
    GridLayout {
        visible: page.tab === 0
        Layout.fillWidth: true
        columns: page.contentWidth > 820 ? 3 : 1
        columnSpacing: 20
        rowSpacing: 20

        StatTile {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            iconName: "view-calendar"
            iconTone: "info"
            label: qsTr("Snapshots")
            value: qsTr("%1 kept").arg(page.b.snapshotsDaily || 0)
            caption: page.scheduleText
        }
        StatTile {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            iconName: "download"
            label: qsTr("Before updates")
            value: qsTr("%1 kept").arg(page.b.snapshotsUpdate || 0)
            caption: qsTr("Taken right before each update")
        }
        StatTile {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            iconName: "drive-harddisk-symbolic"
            iconTone: "ok"
            label: qsTr("Space used")
            value: (page.b.snapshotsSize || 0) >= 0 ? Theme.bytes(page.b.snapshotsSize || 0) : qsTr("Shared")
            caption: qsTr("Fewer are kept when space runs low")
        }
    }

    Card {
        visible: page.tab === 0
        Layout.fillWidth: true
        title: qsTr("Snapshots of your files")
        subtitle: qsTr("Open a snapshot to copy a file or folder back, exactly as it was.")
        icon: "view-history"
        spacing: 0
        actions: [
            DButton {
                text: qsTr("Snapshot now")
                variant: "ghost"
                iconName: "camera-photo"
                onClicked: Backend.snapshotNow()
            },
            DButton {
                text: qsTr("Open in Dolphin")
                iconName: "folder-open"
                onClicked: Backend.openSnapshots()
            }
        ]

        Repeater {
            model: Backend.snapshots
            delegate: ListRow {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: modelData.kind === "update" ? "download" : "view-calendar"
                iconTone: modelData.kind === "update" ? "accent" : "info"
                title: modelData.label
                subtitle: modelData.size >= 0 ? modelData.when + " · " + Theme.bytes(modelData.size) : modelData.when
                value: modelData.kind === "update" ? qsTr("Before update") : qsTr("Daily")
                tone: modelData.kind === "update" ? "accent" : "neutral"
                separator: index < Backend.snapshots.length - 1
            }
        }
        UiText {
            visible: Backend.snapshots.length === 0
            text: qsTr("No snapshots yet. The first one is taken tonight.")
            tone: "muted"
        }
    }

    // How often snapshots are taken by themselves (the newest 14 are kept).
    Card {
        visible: page.tab === 0
        Layout.fillWidth: true
        title: qsTr("Automatic snapshots")
        subtitle: qsTr("A snapshot keeps your files and the computer's settings and accounts as they were. The newest 14 are kept.")
        icon: "clock"
        ChoiceChips {
            options: [{ id: "daily", title: qsTr("Every day") }, { id: "weekly", title: qsTr("Every week") },
                      { id: "monthly", title: qsTr("Every month") }, { id: "off", title: qsTr("Off") }]
            current: page.b.snapshotSchedule || "daily"
            onPicked: id => Backend.setSnapshotSchedule(id)
        }
    }

    InfoBanner {
        visible: page.tab === 0
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("Snapshots are not a backup")
        text: qsTr("They live on the same disk, and a factory reset deletes them. Use Backup to keep copies on another drive.")
        DButton { text: qsTr("Set up backup"); variant: "secondary"; onClicked: page.tab = 1 }
    }

    // ── Backup ───────────────────────────────────────────────────────────────
    Card {
        visible: page.tab === 1 && !!page.b.configured
        Layout.fillWidth: true
        title: qsTr("Backing up to %1").arg(page.b.destination || "")
        subtitle: page.b.destinationDetail || ""
        icon: "drive-removable-media"
        tone: "ok"
        spacing: 0
        actions: [ StatusPill { text: qsTr("On"); tone: "ok" } ]

        ListRow { Layout.fillWidth: true; iconName: "clock"; title: qsTr("Schedule"); value: page.b.schedule || "" }
        ListRow {
            Layout.fillWidth: true
            iconName: "backup"
            title: qsTr("Last backup")
            subtitle: page.b.lastRun || ""
            value: page.b.lastText || ""
            tone: page.b.lastTone || ""
        }
        ListRow {
            Layout.fillWidth: true
            iconName: "kt-check-data"
            title: qsTr("Restore test")
            subtitle: qsTr("Asima OS regularly restores a sample to prove your backup works")
            value: page.b.restoreTest || ""
            tone: page.b.restoreTone || ""
        }
        ListRow { Layout.fillWidth: true; iconName: "view-history"; title: qsTr("Versions kept"); value: page.b.kept || "" }
        ListRow {
            visible: !!Backend.protection.encrypted
            Layout.fillWidth: true
            iconName: "object-locked"
            title: qsTr("Disk encryption header")
            subtitle: qsTr("A copy in the backup: a damaged header would otherwise lose the whole disk")
            value: page.b.headerText || ""
        }
        ListRow { Layout.fillWidth: true; iconName: "drive-harddisk-symbolic"; title: qsTr("Backup size"); value: Theme.bytes(page.b.repoSize); separator: false }

        RowLayout {
            Layout.topMargin: 14
            spacing: 10
            DButton {
                text: page.b.running ? qsTr("Backing up…") : qsTr("Back up now")
                enabled: !page.b.running
                variant: "primary"
                iconName: "backup"
                onClicked: Backend.backupNow()
            }
            DButton { text: qsTr("Change drive…"); variant: "ghost"; onClicked: page.changing = true }
        }
    }

    // One backup back into a new folder in the person's home; nothing is overwritten.
    Card {
        visible: page.tab === 1 && !!page.b.configured && !page.changing
        Layout.fillWidth: true
        title: qsTr("Restore from the backup")
        subtitle: qsTr("Brings your home folder back as it was at the time of a backup, into a new folder next to your files. Nothing is overwritten.")
        icon: "edit-undo"
        tone: "info"
        spacing: 0
        actions: [
            DButton {
                text: Backend.backupList.length > 0 ? qsTr("Refresh") : qsTr("Show backups")
                variant: "ghost"
                iconName: "view-refresh"
                onClicked: Backend.loadBackups()
            }
        ]

        Repeater {
            model: Backend.backupList
            delegate: ListRow {
                id: entry
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: "backup"
                title: modelData.when
                subtitle: modelData.detail
                separator: index < Backend.backupList.length - 1
                DButton {
                    text: qsTr("Restore")
                    variant: "secondary"
                    enabled: !page.b.restoreRunning
                    onClicked: Backend.restoreBackup(entry.modelData.id)
                }
            }
        }
        UiText {
            visible: !!page.b.restoreText
            Layout.topMargin: 10
            Layout.fillWidth: true
            text: page.b.restoreText || ""
            color: page.b.restoreTone === "error" ? Theme.error : page.b.restoreTone === "ok" ? Theme.ok : Theme.textMuted
            wrapMode: Text.WordWrap
        }
    }

    // Set up (or move) the backup: a folder on another drive or a restic address, and a password
    // that encrypts it. The password is needed to restore on another computer: keep it safe.
    property bool changing: false
    Card {
        id: setup
        visible: page.tab === 1 && (!page.b.configured || page.changing)
        Layout.fillWidth: true
        title: page.b.configured ? qsTr("Move the backup") : qsTr("Set up a backup")
        subtitle: qsTr("Choose a folder on a USB drive or a network share, or a restic address such as sftp:user@host:/backups. Backups are encrypted, kept for weeks, and tested by restoring a sample.")
        icon: "backup"
        tone: "warn"

        readonly property bool destinationOk: /^\//.test(destination.text) || /^(sftp|rest|s3|b2|azure|gs|swift):/.test(destination.text)
        readonly property bool passwordOk: password.text.length >= 8 && password.text === repeat.text

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            DTextField {
                id: destination
                Layout.fillWidth: true
                placeholderText: qsTr("Folder or restic address")
                text: page.b.destination || ""
                tone: destination.text.length > 0 && !setup.destinationOk ? "error" : ""
            }
            DButton { text: qsTr("Choose folder…"); variant: "secondary"; iconName: "folder-open"; onClicked: folderDialog.open() }
        }
        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 640 ? 2 : 1
            columnSpacing: 16
            rowSpacing: 10
            PasswordField { id: password; Layout.fillWidth: true; placeholderText: qsTr("Backup password (at least 8 characters)") }
            PasswordField {
                id: repeat
                Layout.fillWidth: true
                placeholderText: qsTr("Repeat the password")
                tone: repeat.text.length > 0 && repeat.text !== password.text ? "error" : ""
            }
        }
        RowLayout {
            spacing: 10
            DButton {
                text: page.b.configured ? qsTr("Move the backup") : qsTr("Start backing up")
                variant: "primary"
                iconName: "backup"
                enabled: setup.destinationOk && setup.passwordOk
                onClicked: {
                    Backend.configureBackup(destination.text.trim(), password.text)
                    password.text = ""      // not kept in memory longer than needed
                    repeat.text = ""
                    page.changing = false
                }
            }
            DButton { visible: page.changing; text: qsTr("Cancel"); variant: "ghost"; onClicked: page.changing = false }
        }
    }

    FolderDialog {
        id: folderDialog
        title: qsTr("Choose the backup folder")
        onAccepted: destination.text = decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, ""))
    }

    InfoBanner {
        visible: page.tab === 1
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Your safety net for a factory reset")
        text: qsTr("A factory reset deletes everything on this computer, but never touches your backup drive.")
    }
}
