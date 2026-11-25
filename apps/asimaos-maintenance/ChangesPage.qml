pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// The /etc overlay: what this computer changed compared with the factory configuration.
DPage {
    id: page
    title: qsTr("System changes")
    subtitle: qsTr("Asima OS keeps the factory settings untouched: reset them all at once, or look at each file in /etc that was changed on this computer.")

    readonly property var win: B.ApplicationWindow.window
    property int filter: 0
    readonly property var kinds: ["", "modified", "added", "deleted"]
    readonly property var shown: Backend.changes.filter(c => page.filter === 0 || c.kind === page.kinds[page.filter])
    property string selected: Backend.changes.length > 0 ? Backend.changes[0].path : ""
    readonly property var current: Backend.changes.find(c => c.path === page.selected) || null
    readonly property string currentDiff: current ? Backend.diff(current.path) : ""

    function kindTitle(kind) {
        return kind === "added" ? qsTr("Added") : kind === "deleted" ? qsTr("Removed") : qsTr("Changed")
    }
    function kindTone(kind) {
        return kind === "added" ? "ok" : kind === "deleted" ? "error" : "info"
    }
    function count(kind) { return Backend.changes.filter(c => c.kind === kind).length }

    headerActions: [
        SegmentedTabs {
            segmentWidth: 104
            options: [qsTr("All"), qsTr("Changed"), qsTr("Added"), qsTr("Removed")]
            currentIndex: page.filter
            onActivated: index => page.filter = index
        }
    ]

    // Updates that skip versions (D52): after more than a year without updates, Asima Rescue or
    // the USB stick's repair installed this version, and some settings may be older than its
    // migration steps reach.
    InfoBanner {
        visible: !!Backend.system.settingsBehind
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("Some settings come from an older Asima OS")
        text: qsTr("This computer went from Asima OS %1 straight to %2, more than a year of versions at once. If something doesn't work as it should, Reset settings brings every setting up to date; your files stay.").arg(Backend.system.settingsBehind || "").arg(Backend.system.version || "")
    }

    // For everyone: every setting back to this version's defaults in one step. The list of files
    // below stays for those who want to pick.
    Card {
        id: resetCard
        Layout.fillWidth: true
        title: qsTr("Reset settings")
        subtitle: qsTr("When something doesn't work as it should, put the settings back to how Asima OS %1 comes. Nothing of yours is deleted.").arg(Backend.system.version || "")
        icon: "edit-undo"
        tone: "accent"
        spacing: 12

        readonly property var reset: [
            { title: qsTr("System settings"), detail: qsTr("The firewall, sharing, printers, services, power and battery care") },
            { title: qsTr("Your desktop"), detail: qsTr("Look, panels and widgets, shortcuts, mouse and touchpad, sound, displays") },
            { title: qsTr("The apps that come with Asima OS"), detail: qsTr("Dolphin, Konsole, Kate and the other preinstalled apps, and the Asima apps") }
        ]
        readonly property var kept: [
            { title: qsTr("Your files and accounts"), detail: qsTr("Everything in your home folder, passwords and KDE Wallet") },
            { title: qsTr("Wi-Fi, language, keyboard and time zone"), detail: qsTr("Networks and VPNs, the computer's name") },
            { title: qsTr("Installed apps and their settings"), detail: qsTr("Flatpak apps, games, snaps, Nix programs, virtual machines, backups") }
        ]

        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 820 ? 2 : 1
            columnSpacing: 24
            rowSpacing: 8
            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                spacing: 0
                UiText { text: qsTr("Back to the defaults"); variant: "small"; font.weight: Font.DemiBold; tone: "muted" }
                Repeater {
                    model: resetCard.reset
                    delegate: ListRow {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        iconName: "edit-undo"
                        title: modelData.title
                        subtitle: modelData.detail
                        separator: index < resetCard.reset.length - 1
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                spacing: 0
                UiText { text: qsTr("Kept as they are"); variant: "small"; font.weight: Font.DemiBold; tone: "muted" }
                Repeater {
                    model: resetCard.kept
                    delegate: ListRow {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        iconName: "dialog-ok-apply"
                        iconTone: "ok"
                        title: modelData.title
                        subtitle: modelData.detail
                        separator: index < resetCard.kept.length - 1
                    }
                }
            }
        }

        DButton {
            Layout.topMargin: 4
            text: qsTr("Reset settings…")
            variant: "secondary"
            iconName: "edit-undo"
            onClicked: page.win.confirm({
                title: qsTr("Reset settings and restart?"),
                text: qsTr("The computer restarts now and comes back with the default settings; your desktop's settings are reset for you only. Your old settings are kept in a folder, so nothing is lost. Save your work first."),
                icon: "edit-undo", tone: "warn", confirmText: qsTr("Reset and restart")
            }, () => Backend.resetSettings())
        }
    }

    // An update changed the default of files this computer had changed.
    InfoBanner {
        readonly property int n: Backend.changes.filter(c => c.newDefault).length
        visible: n > 0
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("An update changed the default of %n file(s) you changed", "", n)
        text: qsTr("Your version is kept. Compare it with the new default below; restoring a file takes the new default from the next start.")
    }

    InfoBanner {
        visible: Backend.changes.length === 0
        Layout.fillWidth: true
        tone: "ok"
        title: qsTr("Everything is factory fresh")
        text: qsTr("No configuration file differs from what Asima OS ships.")
    }

    GridLayout {
        visible: Backend.changes.length > 0
        Layout.fillWidth: true
        columns: page.contentWidth > 900 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 5
            Layout.alignment: Qt.AlignTop
            title: qsTr("%n file(s)", "", page.shown.length)
            subtitle: qsTr("%1 changed · %2 added · %3 removed").arg(page.count("modified")).arg(page.count("added")).arg(page.count("deleted"))
            icon: "document-edit"
            spacing: 4
            Repeater {
                model: page.shown
                delegate: Rectangle {
                    id: row
                    required property var modelData
                    readonly property bool chosen: modelData.path === page.selected
                    Layout.fillWidth: true
                    implicitHeight: rowLayout.implicitHeight + 18
                    radius: Theme.radiusM
                    color: chosen ? Theme.accentSoft : (hover.containsMouse ? Theme.surfaceHover : "transparent")
                    RowLayout {
                        id: rowLayout
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12
                        spacing: 10
                        Icon {
                            name: row.modelData.secret ? "object-locked" : "document-edit"
                            size: 16
                            color: Theme.textMuted
                        }
                        UiText {
                            text: row.modelData.path
                            font.family: "Noto Sans Mono"
                            font.pixelSize: Theme.textSm
                            font.weight: row.chosen ? Font.DemiBold : Font.Normal
                            Layout.fillWidth: true
                            elide: Text.ElideMiddle
                        }
                        StatusPill { visible: !!row.modelData.newDefault; text: qsTr("New default"); tone: "warn"; dot: false }
                        StatusPill { text: page.kindTitle(row.modelData.kind); tone: page.kindTone(row.modelData.kind); dot: false }
                    }
                    MouseArea {
                        id: hover
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.selected = row.modelData.path
                    }
                }
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 6
            Layout.alignment: Qt.AlignTop
            visible: page.current !== null
            title: page.current ? page.current.path.split("/").pop() : ""
            subtitle: page.current ? page.kindTitle(page.current.kind) + "  ·  " + page.current.path : ""
            icon: "document-edit"
            tone: page.current ? page.kindTone(page.current.kind) : "info"
            actions: [
                DButton {
                    text: qsTr("Restore factory version")
                    variant: "secondary"
                    iconName: "edit-undo"
                    onClicked: page.win.confirm({
                        title: qsTr("Restore %1?").arg(page.current.path),
                        text: qsTr("Your change to this file is undone. Asima OS applies it at the next start, so nothing breaks while you work."),
                        icon: "edit-undo", confirmText: qsTr("Restore")
                    }, () => Backend.revertChange(page.current.path))
                }
            ]
            DiffView {
                visible: page.currentDiff.length > 0
                Layout.fillWidth: true
                diff: page.currentDiff
            }
            InfoBanner {
                visible: page.current !== null && page.current.secret === true
                Layout.fillWidth: true
                tone: "info"
                iconName: "object-locked"
                title: qsTr("Contains secrets")
                text: qsTr("This file holds passwords (for example a Wi-Fi key), so its contents aren't shown.")
            }
        }
    }
}
