pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Handheld")
    subtitle: qsTr("Power and controls for %1. Change them in the middle of a game.").arg(Backend.device.model || qsTr("this handheld"))

    readonly property var h: Backend.handheld
    readonly property var win: B.ApplicationWindow.window

    // ── Power limit (TDP) ────────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("Power limit")
        subtitle: qsTr("How many watts the chip may use. More power means more frames per second, but more heat, fan noise and battery use.")
        icon: "speedometer"

        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 820 ? 4 : 2
            columnSpacing: 12
            rowSpacing: 12
            Repeater {
                model: [
                    { id: "silent", title: qsTr("Silent"), watts: 10, detail: qsTr("Quiet, longest play") },
                    { id: "balanced", title: qsTr("Balanced"), watts: 17, detail: qsTr("Most games") },
                    { id: "turbo", title: qsTr("Turbo"), watts: 25, detail: qsTr("Demanding games") },
                    { id: "max", title: qsTr("Max"), watts: page.h.tdpMax || 30, detail: qsTr("Plugged in") }
                ]
                delegate: SelectCard {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    padding: 14
                    title: modelData.title
                    subtitle: modelData.watts + " W"
                    detail: modelData.detail
                    selected: page.h.preset === modelData.id
                    onToggled: Backend.set("handheld", "preset", modelData.id)
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.topMargin: 6
            spacing: 2
            RowLayout {
                UiText { text: qsTr("Custom"); font.weight: Font.Medium; Layout.fillWidth: true }
                UiText { text: (page.h.tdp || 0) + " W"; variant: "subtitle"; tone: "accent"; font.weight: Font.Bold }
            }
            DSlider {
                Layout.fillWidth: true
                from: page.h.tdpMin || 5
                to: page.h.tdpMax || 30
                stepSize: 1
                value: page.h.tdp || 15
                onMoved: Backend.set("handheld", "tdp", Math.round(value))
            }
            RowLayout {
                UiText { text: (page.h.tdpMin || 5) + " W"; variant: "caption"; tone: "subtle"; Layout.fillWidth: true }
                UiText { text: (page.h.tdpMax || 30) + " W"; variant: "caption"; tone: "subtle" }
            }
        }
    }

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 860 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        // ── Screen & frames ──────────────────────────────────────────────────
        // Refresh rate through KScreen; frame limit (MangoHud) and FSR (Proton) for Flatpak Steam.
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Screen and frames")
            icon: "monitor"
            tone: "info"
            UiText { visible: (page.h.refreshOptions || []).length > 1; text: qsTr("Refresh rate"); font.weight: Font.Medium }
            ChoiceChips {
                visible: (page.h.refreshOptions || []).length > 1
                Layout.fillWidth: true
                options: (page.h.refreshOptions || []).map(r => ({ id: r, title: r + " Hz" }))
                current: page.h.refresh
                onPicked: id => Backend.set("handheld", "refresh", id)
            }
            UiText { text: qsTr("Frame limit in Steam games"); font.weight: Font.Medium; Layout.topMargin: 6 }
            ChoiceChips {
                Layout.fillWidth: true
                options: [{ id: 30, title: "30" }, { id: 40, title: "40" }, { id: 60, title: "60" }, { id: 0, title: qsTr("Off") }]
                current: page.h.frameLimit
                onPicked: id => Backend.set("handheld", "frameLimit", id)
            }
            UiText {
                text: qsTr("A 40 fps limit at 120 Hz often feels smooth and almost doubles battery life.")
                variant: "small"
                tone: "muted"
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Sharper upscaling (FSR)")
                subtitle: qsTr("Steam games set to a lower resolution in full screen are scaled up and sharpened (Proton)")
                separator: false
                DSwitch { checked: !!page.h.fsr; onToggled: Backend.set("handheld", "fsr", checked) }
            }
        }

        // ── Cooling ──────────────────────────────────────────────────────────
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Cooling")
            icon: "temperature-normal"
            tone: "info"
            // Full speed or automatic, where the laptop's driver allows it; quieter or faster
            // otherwise comes with the power mode (power-profiles-daemon).
            UiText { visible: (page.h.fanModes || []).length > 0; text: qsTr("Fan"); font.weight: Font.Medium }
            ChoiceChips {
                visible: (page.h.fanModes || []).length > 0
                Layout.fillWidth: true
                options: [{ id: "auto", title: qsTr("Automatic") }, { id: "max", title: qsTr("Full speed") }]
                current: page.h.fan
                onPicked: id => Backend.set("handheld", "fan", id)
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Processor")
                value: (Backend.thermals.cpu || 0) + " °C"
                tone: Backend.thermals.cpu >= 85 ? "warn" : "ok"
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Fans")
                value: (Backend.thermals.fans || []).map(f => f.rpm + " rpm").join(" · ") || "–"
                separator: false
            }
        }
    }

    // ── Game mode ────────────────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("Game mode")
        subtitle: qsTr("Steam's full-screen interface as the whole screen, like a games console, in gamescope as on SteamOS. It's also on the login screen as “Game mode (Steam)”; Power → Switch to Desktop brings the login screen back.")
        icon: "input-gamepad"
        tone: "accent"
        spacing: 0
        ListRow {
            Layout.fillWidth: true
            title: qsTr("Start in game mode")
            subtitle: qsTr("Switching on goes straight into Steam, without signing in: anyone who turns this device on can play and use your Steam account. The disk's passphrase or TPM unlock still protect your files at start-up.")
            separator: false
            DSwitch {
                id: gameModeSwitch
                checked: !!page.h.gameMode
                onToggled: {
                    if (!checked)
                        return Backend.set("handheld", "gameMode", false)
                    // Stays off until confirmed (and the service has switched it on).
                    gameModeSwitch.checked = Qt.binding(() => !!page.h.gameMode)
                    page.win.confirm({
                        title: qsTr("Start in game mode?"),
                        text: qsTr("This device then signs you in by itself, straight into Steam. Anyone who turns it on can play and use your Steam account."),
                        icon: "input-gamepad", tone: "warn", confirmText: qsTr("Start in game mode")
                    }, () => Backend.set("handheld", "gameMode", true))
                }
            }
        }
    }

    // ── Buttons & controls ───────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("Controls")
        icon: "input-gamepad"
        spacing: 0

        // The built-in controller through InputPlumber (D55): which controller Steam and the games
        // see. Shown only where InputPlumber manages the built-in controls.
        ColumnLayout {
            visible: !!page.h.controllerName
            Layout.fillWidth: true
            Layout.bottomMargin: 12
            spacing: 8
            UiText { text: qsTr("The built-in controls appear as"); font.weight: Font.Medium }
            GridLayout {
                Layout.fillWidth: true
                columns: page.contentWidth > 820 ? 3 : 1
                columnSpacing: 12
                rowSpacing: 12
                Repeater {
                    model: [
                        { id: "deck", title: qsTr("Steam Deck controller"),
                          detail: qsTr("Steam sets up every button for each game: the back buttons, the quick-access button and aiming with motion (gyro)") },
                        { id: "xbox-elite", title: qsTr("Xbox Elite controller"),
                          detail: qsTr("Every game knows it; the back buttons are extra buttons") },
                        { id: "ds5-edge", title: qsTr("DualSense Edge"),
                          detail: qsTr("Motion and the touchpad for emulators and games with PlayStation controls") }
                    ]
                    delegate: SelectCard {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        padding: 14
                        title: modelData.title
                        subtitle: modelData.id === page.h.controllerDefault ? qsTr("The default") : ""
                        detail: modelData.detail
                        selected: page.h.controller === modelData.id
                        onToggled: Backend.set("handheld", "controller", modelData.id)
                    }
                }
            }
        }
        ListRow {
            Layout.fillWidth: true
            title: qsTr("Use the controls as mouse and keyboard on the desktop")
            separator: !!page.h.rotationSensor
            subtitle: qsTr("Right stick: pointer; triggers: click; left stick: scroll. Paused while Steam or a game runs.")
            DSwitch { checked: !!page.h.desktopControls; onToggled: Backend.set("handheld", "desktopControls", checked) }
        }
        // Plasma owns screen rotation (its display settings): this opens it there.
        ListRow {
            visible: !!page.h.rotationSensor
            Layout.fillWidth: true
            title: qsTr("Screen rotation")
            subtitle: qsTr("Whether the screen turns with the device is set in Plasma's display settings")
            separator: false
            DButton { text: qsTr("Open"); variant: "ghost"; onClicked: Backend.openDisplaySettings() }
        }
    }
}
