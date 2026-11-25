pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Controllers")
    subtitle: qsTr("Every gamepad you connect: battery, a live button test, rumble and lights.")

    readonly property var win: B.ApplicationWindow.window
    property string selected: Backend.controllers.length > 0 ? Backend.controllers[0].path : ""
    readonly property var current: Backend.controllers.find(c => c.path === page.selected) || ({})

    function busLabel(pad) {
        const how = pad.bus === "bluetooth" ? qsTr("Bluetooth") : pad.bus === "usb" ? qsTr("USB (cable or receiver)")
                  : pad.bus === "wireless" ? qsTr("Xbox Wireless Adapter") : qsTr("Built in")
        return pad.input === "dinput" ? qsTr("%1 · DirectInput").arg(how) : pad.input === "xinput" ? qsTr("%1 · XInput").arg(how) : how
    }

    // In demo mode the tester runs right away so the page is alive.
    Component.onCompleted: if (Backend.demo && page.selected.length) Tester.start(page.selected, page.current.kind)
    Component.onDestruction: Tester.stop()

    headerActions: [
        DButton { text: qsTr("Pair a controller"); iconName: "network-bluetooth"; onClicked: Backend.openSettings("kcm_bluetooth") }
    ]

    InfoBanner {
        visible: Backend.controllers.length === 0
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("No controllers connected")
        text: qsTr("Plug one in with a cable or its wireless receiver, or pair it over Bluetooth. XInput and DirectInput controllers, Xbox, PlayStation, Nintendo, 8BitDo and most others work right away.")
    }

    // A version without xone's adapter driver (it didn't build for this kernel): said plainly.
    InfoBanner {
        visible: (Backend.dongles || []).some(d => !d.driver)
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("The Xbox Wireless Adapter isn't supported in this version")
        text: qsTr("Its driver didn't build for this version's Linux kernel. Use your controller over Bluetooth or with a USB cable until an update brings the driver back.")
    }

    // The Xbox Wireless Adapter needs Microsoft's firmware, which Asima OS can't ship (D55).
    Repeater {
        model: (Backend.dongles || []).filter(d => d.driver && !d.firmware)
        delegate: InfoBanner {
            id: dongleBanner
            required property var modelData
            Layout.fillWidth: true
            tone: "warn"
            title: qsTr("The Xbox Wireless Adapter needs its firmware")
            text: qsTr("Its firmware is Microsoft's, so Asima OS can't include it. Asima Gaming can download it from Microsoft (Windows Update); using it means accepting Microsoft's terms of use.")
            DButton {
                text: qsTr("Download from Microsoft")
                variant: "secondary"
                iconName: "download"
                onClicked: page.win.confirm({
                    title: qsTr("Download the adapter's firmware?"),
                    text: qsTr("It comes from Microsoft's Windows Update, under Microsoft's terms of use (microsoft.com/legal/terms-of-use). Asima Gaming checks it, then starts the adapter again."),
                    icon: "download", tone: "accent", confirmText: qsTr("Download")
                }, () => Backend.set("controllers", "dongleFirmware", dongleBanner.modelData.product))
            }
        }
    }

    GridLayout {
        visible: Backend.controllers.length > 0
        Layout.fillWidth: true
        columns: page.contentWidth > 900 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        // ── Connected list ───────────────────────────────────────────────────
        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 4
            Layout.alignment: Qt.AlignTop
            spacing: 12
            Repeater {
                model: Backend.controllers
                delegate: SelectCard {
                    id: padCard
                    required property var modelData
                    Layout.fillWidth: true
                    padding: 16
                    iconName: modelData.bus === "bluetooth" ? "network-bluetooth" : "input-gamepad"
                    title: modelData.name
                    subtitle: page.busLabel(modelData)
                    selected: page.selected === modelData.path
                    onToggled: {
                        page.selected = modelData.path
                        if (Tester.active) Tester.start(modelData.path, modelData.kind)
                    }
                    RowLayout {
                        visible: padCard.modelData.battery >= 0
                        Layout.fillWidth: true
                        spacing: 10
                        DProgressBar {
                            Layout.fillWidth: true
                            value: (padCard.modelData.battery || 0) / 100
                            tone: padCard.modelData.battery < 20 ? "warn" : "ok"
                        }
                        UiText {
                            text: padCard.modelData.charging ? qsTr("%1 % · charging").arg(padCard.modelData.battery) : padCard.modelData.battery + " %"
                            variant: "small"
                            font.weight: Font.DemiBold
                        }
                    }
                }
            }
        }

        // ── Live tester ──────────────────────────────────────────────────────
        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 6
            Layout.alignment: Qt.AlignTop
            title: qsTr("Test")
            subtitle: Tester.active ? qsTr("Press buttons and move the sticks on %1").arg(page.current.name || "")
                                    : qsTr("See every button, stick and trigger respond live")
            icon: "input-gamepad"
            actions: [
                StatusPill { visible: Tester.active; text: qsTr("Live"); tone: "ok" }
            ]

            Item {
                Layout.fillWidth: true
                implicitHeight: 270 * visual.scale + 8
                ControllerVisual {
                    id: visual
                    anchors.horizontalCenter: parent.horizontalCenter
                    transformOrigin: Item.Top
                    scale: Math.min(1.0, parent.width / implicitWidth)
                    buttons: Tester.state
                    kind: page.current.kind || "xbox"
                    opacity: Tester.active ? 1 : 0.55
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                DButton {
                    text: Tester.active ? qsTr("Stop test") : qsTr("Start test")
                    variant: Tester.active ? "secondary" : "primary"
                    iconName: Tester.active ? "media-playback-pause" : "media-playback-start"
                    enabled: page.selected.length > 0
                    onClicked: Tester.active ? Tester.stop() : Tester.start(page.selected, page.current.kind)
                }
                DButton {
                    text: qsTr("Rumble")
                    iconName: "view-media-equalizer"
                    enabled: page.selected.length > 0
                    onClicked: {
                        if (Tester.rumble(page.selected, 500))
                            page.win.notify(Backend.demo ? qsTr("Demo: the controller would rumble now.") : qsTr("Rumbling for half a second."), "info")
                    }
                }
                Item { Layout.fillWidth: true }
                UiText {
                    text: qsTr("Left stick %1, %2").arg((Tester.state.lx || 0).toFixed(2)).arg((Tester.state.ly || 0).toFixed(2))
                    variant: "caption"
                    tone: "subtle"
                    font.family: "Noto Sans Mono"
                }
            }
        }
    }

    // ── Options ──────────────────────────────────────────────────────────────
    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 860 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Behaviour")
            icon: "configure"
            spacing: 0
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Use a controller as mouse and keyboard")
                subtitle: qsTr("On the desktop: the right stick moves the pointer, the triggers click, the left stick scrolls, A is Enter and B is Escape. Paused while Steam or a game runs.")
                DSwitch {
                    checked: !!Backend.controllerOptions.desktopControls
                    onToggled: Backend.set("controllers", "desktopControls", checked)
                }
            }
            // The rumble strength of every controller, kept by asimaos-gamingd and set as each connects.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.topMargin: 12
                spacing: 4
                RowLayout {
                    UiText { text: qsTr("Rumble strength"); font.weight: Font.Medium; Layout.fillWidth: true }
                    UiText { text: (Backend.controllerOptions.rumble || 0) + " %"; tone: "muted" }
                }
                DSlider {
                    Layout.fillWidth: true
                    from: 0
                    to: 100
                    stepSize: 10
                    value: Backend.controllerOptions.rumble || 0
                    onMoved: Backend.set("controllers", "rumble", value)
                }
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Supported controllers")
            icon: "dialog-ok-apply"
            tone: "ok"
            spacing: 0
            ListRow { Layout.fillWidth: true; title: qsTr("Xbox"); subtitle: qsTr("USB, Bluetooth and the Xbox Wireless Adapter (its firmware from Microsoft, on request)"); value: qsTr("Ready"); tone: "ok" }
            ListRow { Layout.fillWidth: true; title: qsTr("PlayStation (DualSense, DualShock 4)"); subtitle: qsTr("Light bar, rumble, touchpad and motion"); value: qsTr("Ready"); tone: "ok" }
            ListRow { Layout.fillWidth: true; title: qsTr("Nintendo Switch Pro, Joy-Con"); subtitle: qsTr("USB and Bluetooth"); value: qsTr("Ready"); tone: "ok" }
            ListRow { Layout.fillWidth: true; title: qsTr("8BitDo and others"); subtitle: qsTr("Firmware updates through Discover"); value: qsTr("Ready"); tone: "ok"; separator: false }
        }
    }
}
