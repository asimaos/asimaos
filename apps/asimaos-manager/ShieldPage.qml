pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// Shield (D55): each network keeps its own zone; the others get the zone for other networks.
DPage {
    id: page
    title: qsTr("Shield")
    subtitle: qsTr("Asima OS's firewall. Each network keeps its own zone: how visible this device is on it.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var s: Backend.shield
    readonly property var nets: s.networks || []
    readonly property var vpn: s.vpn || []
    property string chosenUuid: ""
    readonly property var net: nets.find(n => n.uuid === page.chosenUuid) || (nets.length > 0 ? nets[0] : null)
    readonly property var zones: [
        { id: "public", icon: "security-high", tone: "accent", title: qsTr("Public"),
          detail: qsTr("Invisible to other devices. For cafés, airports, hotels and unknown Wi-Fi."),
          allows: [qsTr("Nothing can connect to this device")] },
        { id: "home", icon: "go-home", tone: "accent", title: qsTr("Home"),
          detail: qsTr("Trusted network. Your other devices can find this one."),
          allows: [qsTr("KDE Connect with your phone"), qsTr("Finding printers and other devices"), qsTr("Game streaming (Steam Link, Moonlight)")] },
        { id: "developer", icon: "format-text-code", tone: "accent", title: qsTr("Developer"),
          detail: qsTr("Everything from Home, plus what development needs."),
          allows: [qsTr("Everything in Home"), qsTr("Containers and virtual machines on the network"), qsTr("Local dev servers (ports 3000–9000)")] },
        { id: "open", icon: "security-low", tone: "warn", title: qsTr("Not secure"),
          detail: qsTr("Shield stops nothing. Only for a network you trust completely, or for testing."),
          allows: [qsTr("Every device on the network can reach every service on this one")] }
    ]
    function zoneTitle(id) { return (page.zones.find(z => z.id === id) || { title: id }).title }
    function kindIcon(kind) {
        return kind === "wifi" ? "network-wireless" : kind === "cable" ? "network-wired" : kind === "vpn" ? "network-vpn" : "network-connect"
    }
    // "Not secure" is confirmed first; then the zone is kept for that network (or for other networks).
    function choose(id, forNetwork) {
        const apply = () => forNetwork && page.net ? Backend.set("shield", "networkZone", page.net.uuid + " " + id)
                                                   : Backend.set("shield", "zone", id)
        if (id !== "open")
            return apply()
        page.win.confirm({
            title: qsTr("Stop nothing on this network?"),
            text: forNetwork && page.net
                  ? qsTr("Every device on %1 can then reach every service on this computer, including ones you didn't mean to share.").arg(page.net.name)
                  : qsTr("Every device on a network without its own zone can then reach every service on this computer."),
            icon: "security-low", tone: "warn", confirmText: qsTr("Stop nothing")
        }, apply)
    }

    InfoBanner {
        visible: !page.s.managed
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Showing the saved setting")
        text: qsTr("Switching zones needs the Asima Manager service (asimaos-managerd), which isn't available on this computer.")
    }

    // ── The network the zones below are for ─────────────────────────────────
    Card {
        Layout.fillWidth: true
        icon: page.net ? page.kindIcon(page.net.kind) : "network-disconnect"
        title: page.net ? qsTr("This network: %1").arg(page.net.name) : qsTr("No network connected")
        subtitle: !page.net ? qsTr("Each network you join keeps the zone you choose for it here.")
                  : page.net.own ? qsTr("It keeps the zone chosen below whenever you're on it.")
                  : qsTr("New here: it has the zone for other networks until you choose one.")
        ChoiceChips {
            visible: page.nets.length > 1
            Layout.fillWidth: true
            options: page.nets.map(n => ({ id: n.uuid, title: n.name }))
            current: page.net ? page.net.uuid : ""
            onPicked: id => page.chosenUuid = id
        }
    }

    GridLayout {
        visible: !!page.net
        Layout.fillWidth: true
        columns: page.contentWidth > 1100 ? 4 : page.contentWidth > 620 ? 2 : 1
        columnSpacing: 14
        rowSpacing: 14
        Repeater {
            model: page.zones
            delegate: SelectCard {
                id: zoneCard
                required property var modelData
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 1
                iconName: modelData.icon
                iconTone: modelData.tone
                title: modelData.title
                subtitle: modelData.detail
                selected: !!page.net && page.net.zone === modelData.id
                onToggled: page.choose(modelData.id, true)
                Repeater {
                    model: zoneCard.modelData.allows
                    delegate: RowLayout {
                        required property string modelData
                        Layout.fillWidth: true
                        spacing: 8
                        Icon { name: "checkmark"; size: 14; color: zoneCard.modelData.id === "open" ? Theme.warn : Theme.ok; Layout.alignment: Qt.AlignTop; Layout.topMargin: 2 }
                        UiText { text: parent.modelData; variant: "small"; tone: "muted"; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                    }
                }
            }
        }
    }

    // ── Other networks, and the ones Shield remembers ───────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("Other networks")
        subtitle: qsTr("New networks, and the ones you haven't chosen a zone for, get this zone.")
        icon: "network-workgroup"
        spacing: 8
        ChoiceChips {
            Layout.fillWidth: true
            options: page.zones.map(z => ({ id: z.id, title: z.title }))
            current: page.s.zone
            onPicked: id => page.choose(id, false)
        }
        Repeater {
            model: page.s.remembered || []
            delegate: ListRow {
                id: rememberedRow
                required property var modelData
                required property int index
                Layout.fillWidth: true
                title: modelData.name || qsTr("A network")
                subtitle: qsTr("Remembered: %1").arg(page.zoneTitle(modelData.zone))
                separator: index < (page.s.remembered || []).length - 1
                DButton { text: qsTr("Forget"); variant: "ghost"; onClicked: Backend.set("shield", "forget", rememberedRow.modelData.uuid) }
            }
        }
    }

    // ── VPN ───────────────────────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("VPN")
        subtitle: page.vpn.length > 0 ? qsTr("In use: %1").arg(page.vpn.join(", ")) : qsTr("No VPN is connected.")
        icon: "network-vpn"
        tone: page.vpn.length > 0 ? "ok" : "info"
        spacing: 0
        ListRow {
            Layout.fillWidth: true
            title: qsTr("VPN kill-switch")
            subtitle: qsTr("If the VPN drops, block all internet traffic until it's back, so nothing leaks.")
            separator: false
            DSwitch { checked: !!page.s.killSwitch; onToggled: Backend.set("shield", "killSwitch", checked) }
        }
        InfoBanner {
            visible: !!page.s.killSwitch && page.vpn.length === 0
            Layout.fillWidth: true
            tone: "warn"
            title: qsTr("Nothing reaches the internet")
            text: qsTr("The kill-switch is on and no VPN is connected. Connect one, or switch the kill-switch off.")
        }
    }

    // ── What Shield stopped ───────────────────────────────────────────────────
    ListRow {
        visible: page.s.stopped !== undefined && page.s.stopped !== null
        Layout.fillWidth: true
        iconName: "security-high"
        title: qsTr("Stopped since the computer started")
        subtitle: page.s.since ? qsTr("Since %1: attempts from other devices to connect to this one").arg(
                      Qt.formatTime(new Date(page.s.since * 1000), Locale.ShortFormat))
                  : qsTr("Attempts from other devices to connect to this one")
        value: qsTr("%n attempt(s)", "", page.s.stopped || 0)
        tone: "info"
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "ok"
        title: qsTr("One firewall, no surprises")
        text: qsTr("Shield is the only firewall on Asima OS. Apps from Flathub can't open ports by themselves.")
    }
}
