import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Asima Manager")
    subtitle: qsTr("Your device, your way: power, display, sign-in and protection in one place.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var d: Backend.device
    readonly property var bat: Backend.battery
    readonly property var temp: Backend.thermals
    readonly property var profileTitles: ({ "power-saver": qsTr("Power saver"), "balanced": qsTr("Balanced"), "performance": qsTr("Performance") })
    readonly property var zoneTitles: ({ "public": qsTr("Public"), "home": qsTr("Home"), "developer": qsTr("Developer") })

    function nextProfile() {
        const order = ["power-saver", "balanced", "performance"]
        Backend.setPowerProfile(order[(order.indexOf(Backend.power.profile) + 1) % order.length])
    }

    // ── Device hero ──────────────────────────────────────────────────────────
    Rectangle {
        id: hero
        Layout.fillWidth: true
        implicitHeight: heroRow.implicitHeight + 60
        radius: Theme.radiusXl
        clip: true
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0.0; color: Theme.heroFrom }
            GradientStop { position: 0.55; color: Theme.heroMid }
            GradientStop { position: 1.0; color: Theme.heroTo }
        }
        Rectangle { width: 420; height: 420; radius: 210; color: "#FFFFFF"; opacity: 0.07; x: hero.width - 300; y: -190 }

        RowLayout {
            id: heroRow
            anchors.fill: parent
            anchors.margins: 30
            spacing: 26

            Rectangle {
                implicitWidth: 96
                implicitHeight: 96
                radius: 28
                color: Qt.rgba(1, 1, 1, 0.16)
                border.width: 1
                border.color: Qt.rgba(1, 1, 1, 0.25)
                DeviceArt { anchors.centerIn: parent; type: page.d.type || "desktop" }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4
                UiText { text: page.d.typeLabel || ""; variant: "overline"; color: Qt.rgba(1, 1, 1, 0.82) }
                UiText { text: page.d.model || ""; variant: "display"; color: "#FFFFFF"; Layout.fillWidth: true; elide: Text.ElideRight }
                UiText {
                    text: [page.d.cpu, page.d.memory > 0 ? qsTr("%1 GB memory").arg(Math.round(page.d.memory / 1073741824)) : ""]
                          .filter(s => !!s).join("  ·  ")
                    color: Qt.rgba(1, 1, 1, 0.9)
                }
                Flow {
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                    spacing: 8
                    Repeater {
                        model: [
                            page.bat.present ? { icon: "battery-080", text: qsTr("Battery %1 %").arg(page.bat.percent) } : null,
                            page.temp.cpu ? { icon: "temperature-normal", text: qsTr("CPU %1 °C").arg(page.temp.cpu) } : null,
                            Backend.power.available ? { icon: "speedometer", text: page.profileTitles[Backend.power.profile] || "" } : null,
                            { icon: "security-high", text: qsTr("Shield: %1").arg(page.zoneTitles[Backend.shield.zone] || "") }
                        ].filter(c => c !== null)
                        delegate: Rectangle {
                            id: chip
                            required property var modelData
                            height: 30
                            width: chipRow.implicitWidth + 24
                            radius: 15
                            color: Qt.rgba(1, 1, 1, 0.16)
                            border.width: 1
                            border.color: Qt.rgba(1, 1, 1, 0.24)
                            Row {
                                id: chipRow
                                anchors.centerIn: parent
                                spacing: 7
                                Icon { name: chip.modelData.icon; size: 14; color: "#FFFFFF"; anchors.verticalCenter: parent.verticalCenter }
                                UiText { text: chip.modelData.text; variant: "small"; color: "#FFFFFF"; font.weight: Font.DemiBold; anchors.verticalCenter: parent.verticalCenter }
                            }
                        }
                    }
                }
            }
            StatusRing {
                visible: !!page.bat.present && page.contentWidth > 760
                size: 112
                thickness: 9
                value: (page.bat.percent || 0) / 100
                color: "#FFFFFF"
                trackColor: Qt.rgba(1, 1, 1, 0.22)
                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: 0
                    UiText { text: (page.bat.percent || 0) + "%"; variant: "subtitle"; color: "#FFFFFF"; font.weight: Font.Bold; Layout.alignment: Qt.AlignHCenter }
                    UiText { text: page.bat.status === "Charging" ? qsTr("charging") : qsTr("battery"); variant: "caption"; color: Qt.rgba(1, 1, 1, 0.82); Layout.alignment: Qt.AlignHCenter }
                }
            }
        }
    }

    // ── Quick settings ───────────────────────────────────────────────────────
    // Four tiles everywhere, plus battery care and the keyboard light where they exist: one row of
    // four or five, or two rows of three, so the grid never ends in a gap.
    readonly property bool kbdTile: !page.win.handheld && !!Backend.display.kbdAvailable
    readonly property int tileCount: 4 + (page.bat.present ? 1 : 0) + (kbdTile ? 1 : 0)
    readonly property string signIn: [Face.faces.length > 0 ? qsTr("Face") : "",
                                      Fingerprint.enrolled.length > 0 ? qsTr("%n finger(s)", "", Fingerprint.enrolled.length) : ""]
                                     .filter(s => !!s).join(" + ")

    UiText { text: qsTr("Quick settings"); variant: "overline"; tone: "subtle"; Layout.topMargin: 4 }
    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 960 ? (page.tileCount === 6 ? 3 : page.tileCount) : 2
        columnSpacing: 14
        rowSpacing: 14

        QuickToggle {
            Layout.fillWidth: true
            iconName: "speedometer"
            title: qsTr("Power mode")
            value: page.profileTitles[Backend.power.profile] || qsTr("Not available")
            on: Backend.power.profile === "performance"
            onClicked: page.nextProfile()
        }
        QuickToggle {
            Layout.fillWidth: true
            iconName: "security-high"
            title: qsTr("Shield")
            value: qsTr("%1 mode").arg(page.zoneTitles[Backend.shield.zone] || "")
            on: true
            onClicked: page.win.navigate("shield")
        }
        QuickToggle {
            Layout.fillWidth: true
            iconName: "battery-080"
            title: qsTr("Battery care")
            value: Backend.battery.limitEnabled ? qsTr("Charge to %1 %").arg(Backend.battery.chargeLimit) : qsTr("Charge to 100 %")
            on: !!Backend.battery.limitEnabled
            visible: !!Backend.battery.present
            onClicked: Backend.set("power", "limitEnabled", !Backend.battery.limitEnabled)
        }
        QuickToggle {
            Layout.fillWidth: true
            visible: page.kbdTile
            iconName: "input-keyboard"
            title: qsTr("Keyboard light")
            value: Backend.display.kbdLevel > 0 ? qsTr("Level %1 of %2").arg(Backend.display.kbdLevel).arg(Backend.display.kbdMax) : qsTr("Off")
            on: Backend.display.kbdLevel > 0
            onClicked: Backend.setKeyboardLight((Backend.display.kbdLevel + 1) % (Backend.display.kbdMax + 1))
        }
        QuickToggle {
            Layout.fillWidth: true
            iconName: Face.faces.length > 0 && Fingerprint.enrolled.length === 0 ? "camera-web" : "fingerprint"
            title: qsTr("Sign-in")
            value: page.signIn || (Fingerprint.available || Face.camera ? qsTr("Password only") : qsTr("Password"))
            on: page.signIn.length > 0
            onClicked: page.win.navigate("biometrics")
        }
        QuickToggle {
            Layout.fillWidth: true
            iconName: "network-vpn"
            title: qsTr("VPN kill-switch")
            value: Backend.shield.killSwitch ? qsTr("Blocks traffic without VPN") : qsTr("Off")
            on: !!Backend.shield.killSwitch
            onClicked: Backend.set("shield", "killSwitch", !Backend.shield.killSwitch)
        }
    }

    // ── At a glance ──────────────────────────────────────────────────────────
    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 860 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Protection")
            subtitle: qsTr("Firewall and sign-in")
            icon: "security-high"
            tone: "ok"
            spacing: 0
            actions: [ DButton { text: qsTr("Shield"); variant: "ghost"; onClicked: page.win.navigate("shield") } ]
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Shield mode")
                subtitle: qsTr("Who may connect to this computer")
                value: page.zoneTitles[Backend.shield.zone] || ""
                tone: "ok"
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("VPN kill-switch")
                value: Backend.shield.killSwitch ? qsTr("On") : qsTr("Off")
                tone: Backend.shield.killSwitch ? "ok" : "neutral"
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Sign-in")
                value: page.signIn || qsTr("Password")
                separator: false
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Right now")
            subtitle: qsTr("Live from the sensors")
            icon: "temperature-normal"
            tone: "info"
            spacing: 0
            actions: [ DButton { text: qsTr("Details"); variant: "ghost"; onClicked: page.win.navigate("hardware") } ]
            ListRow {
                Layout.fillWidth: true
                visible: page.temp.cpu !== undefined
                title: qsTr("Processor")
                value: (page.temp.cpu || 0) + " °C"
                tone: page.temp.cpu >= 90 ? "error" : page.temp.cpu >= 80 ? "warn" : "ok"
            }
            ListRow {
                Layout.fillWidth: true
                visible: page.temp.gpu !== undefined
                title: qsTr("Graphics")
                value: (page.temp.gpu || 0) + " °C"
                tone: page.temp.gpu >= 90 ? "error" : page.temp.gpu >= 80 ? "warn" : "ok"
            }
            ListRow {
                Layout.fillWidth: true
                visible: (page.temp.fans || []).length > 0
                title: qsTr("Fans")
                value: (page.temp.fans || []).map(f => f.rpm + " rpm").join(" · ")
            }
            ListRow {
                Layout.fillWidth: true
                visible: !!page.bat.present
                title: qsTr("Power draw")
                value: page.bat.powerW > 0 ? page.bat.powerW.toFixed(1) + " W" : "–"
                separator: false
            }
        }
    }

    // ── Asima Hub (D39): the online app, never part of the system (D3) ──────
    Card {
        Layout.fillWidth: true
        title: qsTr("Asima Hub")
        subtitle: Backend.hubInstalled
                  ? qsTr("News, the newest versions of Asima OS, the guides and problem reports.")
                  : qsTr("The online app: news, the newest versions of Asima OS, the guides and problem reports. It isn't part of the system, so you get it once, here.")
        icon: "applications-internet"
        tone: "info"
        actions: [ DButton {
            text: Backend.hubInstalled ? qsTr("Open Asima Hub") : qsTr("Get Asima Hub")
            variant: Backend.hubInstalled ? "secondary" : "primary"
            iconName: Backend.hubInstalled ? "applications-internet" : "download"
            onClicked: Backend.hubInstalled ? Backend.openHub() : Backend.getHub()
        } ]
    }
}
