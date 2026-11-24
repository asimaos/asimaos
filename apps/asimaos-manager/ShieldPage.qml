pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

DPage {
    id: page
    title: qsTr("Shield")
    subtitle: qsTr("Asima OS's firewall. Pick how visible this device is on the network you're on.")

    readonly property var s: Backend.shield
    readonly property var zones: [
        { id: "public", icon: "security-high", title: qsTr("Public"),
          detail: qsTr("Invisible to other devices. For cafés, airports, hotels and unknown Wi-Fi."),
          allows: [qsTr("Nothing can connect to this device")] },
        { id: "home", icon: "go-home", title: qsTr("Home"),
          detail: qsTr("Trusted network. Your other devices can find this one."),
          allows: [qsTr("KDE Connect with your phone"), qsTr("Finding printers and other devices"), qsTr("Game streaming (Steam Link, Moonlight)")] },
        { id: "developer", icon: "format-text-code", title: qsTr("Developer"),
          detail: qsTr("Everything from Home, plus what development needs."),
          allows: [qsTr("Everything in Home"), qsTr("Containers and virtual machines on the network"), qsTr("Local dev servers (ports 3000–9000)")] }
    ]

    InfoBanner {
        visible: !page.s.managed
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Showing the saved setting")
        text: qsTr("Switching modes needs the Asima Manager service (asimaos-managerd), which isn't available on this computer.")
    }

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 900 ? 3 : 1
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
                title: modelData.title
                subtitle: modelData.detail
                selected: page.s.zone === modelData.id
                onToggled: Backend.set("shield", "zone", modelData.id)
                Repeater {
                    model: zoneCard.modelData.allows
                    delegate: RowLayout {
                        required property string modelData
                        Layout.fillWidth: true
                        spacing: 8
                        Icon { name: "checkmark"; size: 14; color: Theme.ok; Layout.alignment: Qt.AlignTop; Layout.topMargin: 2 }
                        UiText { text: parent.modelData; variant: "small"; tone: "muted"; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                    }
                }
            }
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("VPN kill-switch")
        subtitle: qsTr("If the VPN drops, block all internet traffic until it's back, so nothing leaks.")
        icon: "network-vpn"
        actions: [ DSwitch { checked: !!page.s.killSwitch; onToggled: Backend.set("shield", "killSwitch", checked) } ]
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "ok"
        title: qsTr("One firewall, no surprises")
        text: qsTr("Shield is the only firewall on Asima OS. Apps from Flathub can't open ports by themselves.")
    }
}
