import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Overview")
    subtitle: qsTr("How your Asima OS is doing today.")

    readonly property var sys: Backend.system
    readonly property var storage: Backend.storage
    readonly property var backup: Backend.backup
    readonly property var win: B.ApplicationWindow.window
    readonly property int tileColumns: contentWidth > 900 ? 4 : (contentWidth > 560 ? 2 : 1)

    // ── Status hero ──────────────────────────────────────────────────────────
    Rectangle {
        id: hero
        Layout.fillWidth: true
        implicitHeight: heroRow.implicitHeight + 64
        radius: Theme.radiusXl
        clip: true
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0.0; color: Theme.heroFrom }
            GradientStop { position: 0.55; color: Theme.heroMid }
            GradientStop { position: 1.0; color: Theme.heroTo }
        }

        // soft decorative light
        Rectangle { width: 380; height: 380; radius: 190; color: "#FFFFFF"; opacity: 0.08; x: hero.width - 260; y: -170 }
        Rectangle { width: 240; height: 240; radius: 120; color: "#FFFFFF"; opacity: 0.05; x: hero.width - 470; y: hero.height - 90 }

        RowLayout {
            id: heroRow
            anchors.fill: parent
            anchors.margins: 32
            spacing: 28

            StatusRing {
                size: 104
                thickness: 8
                value: 1
                color: "#FFFFFF"
                trackColor: Qt.rgba(1, 1, 1, 0.22)
                StatusGlyph {
                    anchors.centerIn: parent
                    tone: page.sys.health || "ok"
                    size: 46
                    color: "#FFFFFF"
                    glyphColor: Theme.heroMid
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6
                UiText { text: qsTr("System status"); variant: "overline"; color: Qt.rgba(1, 1, 1, 0.82) }
                UiText {
                    text: page.sys.healthTitle || ""
                    variant: "display"
                    color: "#FFFFFF"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
                UiText {
                    text: page.sys.healthText || ""
                    color: Qt.rgba(1, 1, 1, 0.9)
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
                Flow {
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                    spacing: 8
                    Repeater {
                        model: page.sys.facts || []
                        delegate: Rectangle {
                            id: fact
                            required property string modelData
                            height: 28
                            width: factRow.implicitWidth + 24
                            radius: 14
                            color: Qt.rgba(1, 1, 1, 0.16)
                            border.width: 1
                            border.color: Qt.rgba(1, 1, 1, 0.24)
                            Row {
                                id: factRow
                                anchors.centerIn: parent
                                spacing: 6
                                Icon { name: "checkmark"; size: 14; color: "#FFFFFF"; anchors.verticalCenter: parent.verticalCenter }
                                UiText {
                                    text: fact.modelData
                                    variant: "small"
                                    color: "#FFFFFF"
                                    font.weight: Font.DemiBold
                                    anchors.verticalCenter: parent.verticalCenter
                                }
                            }
                        }
                    }
                }
            }

            ColumnLayout {
                visible: page.contentWidth > 860
                spacing: 2
                Layout.alignment: Qt.AlignTop | Qt.AlignRight
                UiText { text: page.sys.name || "Asima OS"; variant: "small"; color: Qt.rgba(1, 1, 1, 0.82); Layout.alignment: Qt.AlignRight }
                UiText { text: page.sys.version || ""; variant: "subtitle"; color: "#FFFFFF"; font.weight: Font.Bold; Layout.alignment: Qt.AlignRight }
                UiText {
                    text: [page.sys.kernel, page.sys.desktop].filter(s => !!s).join(" · ")
                    variant: "caption"
                    color: Qt.rgba(1, 1, 1, 0.82)
                    Layout.alignment: Qt.AlignRight
                }
                Item { Layout.preferredHeight: 12 }
                DButton {
                    text: qsTr("Run health check")
                    variant: "onAccent"
                    iconName: "kt-check-data"
                    Layout.alignment: Qt.AlignRight
                    onClicked: {
                        page.win.navigate("health")
                        Backend.runHealthCheck()
                    }
                }
            }
        }
    }

    // Windows (or a firmware update) put itself first in the start order.
    InfoBanner {
        visible: !!Backend.bootOrder.known && Backend.bootOrder.asimaosFirst === false
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("%1 starts first").arg(Backend.bootOrder.first || "")
        text: qsTr("Windows updates sometimes put themselves first in the computer's start order. Until this is fixed, Asima OS starts only when you pick it in the firmware's boot menu.")
        DButton { text: qsTr("Start Asima OS first"); variant: "secondary"; iconName: "go-first"; onClicked: Backend.startAsimaFirst() }
    }

    // ── Key numbers ──────────────────────────────────────────────────────────
    GridLayout {
        Layout.fillWidth: true
        columns: page.tileColumns
        columnSpacing: 20
        rowSpacing: 20

        StatTile {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clickable: true
            onClicked: page.win.navigate("health")
            iconName: "drive-harddisk-symbolic"
            label: qsTr("Storage")
            value: qsTr("%1 free").arg(Theme.bytes(page.storage.free))
            progress: page.storage.total > 0 ? page.storage.used / page.storage.total : 0
            caption: qsTr("%1 of %2 used").arg(Theme.bytes(page.storage.used)).arg(Theme.bytes(page.storage.total))
        }
        StatTile {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clickable: true
            onClicked: page.win.navigate("security")
            iconName: "object-locked"
            iconTone: Backend.protection.secureBoot === "on" ? "ok" : "warn"
            label: qsTr("Security")
            value: Backend.protection.secureBoot === "on" ? qsTr("Secure Boot on") : qsTr("Secure Boot off")
            caption: Backend.protection.encrypted ? qsTr("Disk encrypted") : qsTr("Disk not encrypted")
            captionTone: Backend.protection.secureBoot === "on" ? "muted" : "warn"
        }
        StatTile {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clickable: true
            onClicked: page.win.navigate("backups")
            iconName: "backup"
            iconTone: page.backup.configured ? "ok" : "warn"
            label: qsTr("Last backup")
            value: page.backup.configured ? page.backup.lastRun : qsTr("Not set up")
            caption: page.backup.configured ? page.backup.destination : qsTr("Protects your files if this disk fails")
            captionTone: page.backup.configured ? "muted" : "warn"
        }
        StatTile {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clickable: true
            onClicked: page.win.navigate("backups")
            iconName: "view-history"
            label: qsTr("Snapshots")
            value: qsTr("%1 kept").arg((page.backup.snapshotsDaily || 0) + (page.backup.snapshotsUpdate || 0))
            caption: qsTr("Undo for deleted files · %1").arg(Theme.bytes(page.backup.snapshotsSize || 0))
        }
    }

    // ── Apps + activity ──────────────────────────────────────────────────────
    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 900 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Apps from Flathub")
            subtitle: qsTr("Discover keeps them up to date; the system itself updates with Asima Updater.")
            icon: "package"
            tone: "ok"
            spacing: 0
            actions: [ DButton { text: qsTr("Asima Updater"); variant: "ghost"; iconName: "download"; onClicked: Backend.openApp("asimaos-updater") } ]

            ListRow {
                Layout.fillWidth: true
                title: qsTr("Installed apps")
                value: String(Backend.flatpak.apps || 0)
            }
            ListRow {
                Layout.fillWidth: true
                visible: Backend.flatpak.size > 0
                title: qsTr("On disk")
                value: Theme.bytes(Backend.flatpak.size || 0)
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Unused runtimes")
                subtitle: qsTr("Left behind by apps you removed")
                value: Backend.flatpak.unused > 0 ? Theme.bytes(Backend.flatpak.unused) : Backend.flatpak.unused === 0 ? qsTr("None") : ""
                separator: false
                DButton {
                    visible: Backend.flatpak.unused !== 0
                    text: qsTr("Clean up")
                    variant: "ghost"
                    onClicked: Backend.cleanFlatpak()
                }
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Recent activity")
            subtitle: qsTr("What Asima OS did in the background")
            icon: "view-history"
            tone: "info"
            spacing: 0

            Repeater {
                model: Backend.activity
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    iconName: modelData.icon
                    iconTone: modelData.tone
                    title: modelData.title
                    subtitle: modelData.detail
                    value: modelData.when
                    separator: index < Backend.activity.length - 1
                }
            }
            UiText {
                visible: Backend.activity.length === 0
                text: qsTr("Nothing to show yet.")
                tone: "muted"
            }
        }
    }

    // Asima Hub, the online app, is never preinstalled (rule 11): this is the one place that
    // mentions it.
    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Asima Hub")
        text: qsTr("Downloads updates, opens the guides and the wiki, shows what's new, and sends a problem report when you choose to. It's a separate app: install it once from Discover.")
        DButton { text: qsTr("Get Asima Hub"); variant: "secondary"; iconName: "plasmadiscover"; onClicked: Backend.openHub() }
    }
}
