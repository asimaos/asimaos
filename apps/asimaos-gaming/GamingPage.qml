pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

DPage {
    id: page
    title: qsTr("Gaming")
    subtitle: qsTr("Performance, scheduling and graphics: tuned for play, without breaking anything else.")

    readonly property var g: Backend.gaming
    readonly property var gpus: Backend.device.gpus || []

    // ── Power mode (power-profiles-daemon) ───────────────────────────────────
    UiText { text: qsTr("Power mode"); variant: "overline"; tone: "subtle" }
    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 820 ? 3 : 1
        columnSpacing: 14
        rowSpacing: 14
        Repeater {
            model: [
                { id: "power-saver", icon: "battery-080", title: qsTr("Power saver"), detail: qsTr("Longest battery, cooler and quieter") },
                { id: "balanced", icon: "speedometer", title: qsTr("Balanced"), detail: qsTr("Fast when needed, efficient otherwise") },
                { id: "performance", icon: "games-achievements", title: qsTr("Performance"), detail: qsTr("Highest clocks for games and builds") }
            ]
            delegate: SelectCard {
                required property var modelData
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                iconName: modelData.icon
                title: modelData.title
                subtitle: modelData.detail
                selected: Backend.power.profile === modelData.id
                disabledLabel: Backend.power.available ? "" : qsTr("Unavailable")
                onToggled: Backend.setPowerProfile(modelData.id)
            }
        }
    }

    // ── CPU scheduler (sched-ext) ────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("CPU scheduler")
        subtitle: qsTr("Decides which program runs next. The gaming scheduler is on from the start: it keeps frame times smooth even while something compiles in the background. Your choice stays after a restart.")
        icon: "view-process-system"
        actions: [ StatusPill { visible: !page.g.schedulerAvailable; text: qsTr("Not on this computer"); tone: "neutral" } ]

        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 820 ? 3 : 1
            columnSpacing: 12
            rowSpacing: 12
            Repeater {
                model: [
                    { id: "default", title: qsTr("Standard"), detail: qsTr("Linux's built-in scheduler, good for everything") },
                    { id: "lavd", title: qsTr("Gaming"), detail: qsTr("scx_lavd: lowest latency for games") },
                    { id: "bpfland", title: qsTr("Desktop"), detail: qsTr("scx_bpfland: snappy apps under heavy load") }
                ]
                delegate: SelectCard {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.fillHeight: true   // the same height, however the descriptions wrap
                    Layout.preferredWidth: 1
                    padding: 14
                    title: modelData.title
                    subtitle: modelData.detail
                    selected: page.g.scheduler === modelData.id
                    disabledLabel: page.g.schedulerAvailable || modelData.id === "default" ? "" : qsTr("Later")
                    onToggled: Backend.set("gaming", "scheduler", modelData.id)
                }
            }
        }
    }

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 860 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        // ── In-game helpers ──────────────────────────────────────────────────
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("In games")
            icon: "games-achievements"
            spacing: 0
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Performance overlay")
                subtitle: qsTr("FPS, temperatures and load on top of games (MangoHud)")
                DSwitch { checked: !!page.g.overlay; onToggled: Backend.set("gaming", "overlay", checked) }
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("GameMode")
                subtitle: page.g.gamemodeClients > 0 ? qsTr("Boosting %n game(s) right now", "", page.g.gamemodeClients)
                                                    : qsTr("Turns on automatically when a game asks for it")
                value: page.g.gamemode ? qsTr("Ready") : qsTr("Not running")
                tone: page.g.gamemode ? "ok" : "neutral"
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Fast Windows-game sync (ntsync)")
                subtitle: qsTr("Makes games running through Proton smoother")
                value: page.g.ntsync ? qsTr("Active") : qsTr("Not available")
                tone: page.g.ntsync ? "ok" : "warn"
                separator: page.g.shaderCache > 0
            }
            ListRow {
                Layout.fillWidth: true
                visible: page.g.shaderCache > 0
                title: qsTr("Shader cache")
                subtitle: qsTr("Pre-compiled shaders make games stutter less")
                value: Theme.bytes(page.g.shaderCache)
                separator: false
            }
        }

        // ── Graphics ─────────────────────────────────────────────────────────
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Graphics")
            icon: "monitor"
            tone: "info"
            spacing: 0
            Repeater {
                model: page.gpus
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    iconName: "monitor"
                    title: modelData.name || qsTr("Graphics")
                    subtitle: qsTr("Driver: %1").arg(modelData.driver || "–")
                    value: page.gpus.length > 1 ? (modelData.primary ? qsTr("Desktop") : qsTr("Games")) : qsTr("In use")
                    tone: "ok"
                    separator: index < page.gpus.length - 1 || page.g.hybrid
                }
            }
            ListRow {
                Layout.fillWidth: true
                visible: !!page.g.hybrid
                title: qsTr("Two GPUs")
                subtitle: qsTr("Games and Steam start on the dedicated GPU; the desktop stays on the efficient one")
                separator: false
            }
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "accent"
        iconName: "input-gamepad"
        title: page.g.steam ? qsTr("Steam is installed") : qsTr("Get your games")
        text: qsTr("Steam, Heroic (Epic, GOG) and Lutris come from Flathub. Asima OS already has the drivers, GameMode and controller support they need.")
        DButton { text: qsTr("Open Discover"); variant: "secondary"; onClicked: Backend.openApp("plasma-discover") }
    }
}
