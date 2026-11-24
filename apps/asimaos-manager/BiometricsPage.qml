import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Sign-in & Biometrics")
    subtitle: qsTr("Sign in with your finger or face. Your biometric data never leaves this device.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var bio: Backend.biometrics
    readonly property var freeFingers: Fingerprint.fingers.filter(f => Fingerprint.enrolled.indexOf(f.id) < 0)

    // ── Fingerprint ──────────────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("Fingerprint")
        subtitle: Fingerprint.available ? Fingerprint.reader : qsTr("No fingerprint reader found on this device")
        icon: "fingerprint"
        actions: [
            StatusPill {
                text: Fingerprint.enrolled.length > 0 ? qsTr("%n finger(s)", "", Fingerprint.enrolled.length)
                                                      : Fingerprint.available ? qsTr("Not set up") : qsTr("No reader")
                tone: Fingerprint.enrolled.length > 0 ? "ok" : "neutral"
            }
        ]

        // Enrolling: big fingerprint inside a progress ring
        RowLayout {
            visible: Fingerprint.enrolling
            Layout.fillWidth: true
            spacing: 26
            StatusRing {
                size: 148
                thickness: 10
                value: Fingerprint.stages > 0 ? Fingerprint.stage / Fingerprint.stages : 0
                color: Theme.accent
                Icon { anchors.centerIn: parent; name: "fingerprint"; size: 64; color: Theme.accentText }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 8
                UiText { text: qsTr("Setting up your %1").arg(Fingerprint.fingerTitle(Fingerprint.finger).toLowerCase()); variant: "subtitle" }
                UiText { text: Fingerprint.message; tone: "muted"; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                DProgressBar { Layout.fillWidth: true; value: Fingerprint.stages > 0 ? Fingerprint.stage / Fingerprint.stages : 0 }
                DButton { text: qsTr("Cancel"); variant: "ghost"; onClicked: Fingerprint.cancel() }
            }
        }

        // Enrolled fingers
        Flow {
            visible: !Fingerprint.enrolling && Fingerprint.enrolled.length > 0
            Layout.fillWidth: true
            spacing: 10
            Repeater {
                model: Fingerprint.enrolled
                delegate: Rectangle {
                    id: fingerChip
                    required property string modelData
                    height: 40
                    width: chipRow.implicitWidth + 28
                    radius: 20
                    color: Theme.okSoft
                    Row {
                        id: chipRow
                        anchors.centerIn: parent
                        spacing: 8
                        Icon { name: "fingerprint"; size: 16; color: Theme.ok; anchors.verticalCenter: parent.verticalCenter }
                        UiText { text: Fingerprint.fingerTitle(fingerChip.modelData); font.weight: Font.DemiBold; color: Theme.ok; anchors.verticalCenter: parent.verticalCenter }
                    }
                }
            }
        }

        RowLayout {
            visible: Fingerprint.available && !Fingerprint.enrolling
            Layout.fillWidth: true
            spacing: 10
            DComboBox {
                id: fingerBox
                implicitWidth: 240
                model: page.freeFingers
                enabled: page.freeFingers.length > 0
            }
            DButton {
                text: qsTr("Add this finger")
                variant: "primary"
                iconName: "fingerprint"
                enabled: page.freeFingers.length > 0
                onClicked: Fingerprint.enroll(fingerBox.currentValue)
            }
            Item { Layout.fillWidth: true }
            DButton {
                visible: Fingerprint.enrolled.length > 0
                text: qsTr("Remove all")
                variant: "ghost"
                onClicked: page.win.confirm({
                    title: qsTr("Remove all fingerprints?"),
                    text: qsTr("You'll sign in with your password until you add a finger again."),
                    tone: "error", icon: "fingerprint", confirmText: qsTr("Remove")
                }, () => Fingerprint.removeAll())
            }
        }
    }

    // ── Face ─────────────────────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("Face sign-in")
        subtitle: Face.cameraKind === "infrared"
                  ? qsTr("%1 · infrared, so a photo can't fool it, and it works in the dark").arg(Face.cameraName)
                  : Face.cameraKind === "webcam"
                    ? qsTr("%1 · a normal webcam: it unlocks the screen and approves admin tasks after you look at it and blink twice").arg(Face.cameraName)
                    : Face.webcam ? qsTr("%1 can unlock the screen and approve admin tasks once you switch it on below").arg(Face.webcam)
                                  : qsTr("No camera found on this device")
        icon: "camera-web"
        actions: [
            StatusPill {
                text: Face.faces.length > 0 ? qsTr("Ready") : !Face.camera ? (Face.webcam ? qsTr("Off") : qsTr("No camera"))
                      : !Face.service ? qsTr("Not on this computer") : qsTr("Not set up")
                tone: Face.faces.length > 0 ? "ok" : "neutral"
            }
        ]

        // Scanning
        RowLayout {
            visible: Face.enrolling
            Layout.fillWidth: true
            spacing: 28
            FaceScan { active: true; progress: Face.progress }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 8
                UiText { text: qsTr("Setting up face sign-in"); variant: "subtitle" }
                UiText { text: Face.message; tone: "muted"; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                DProgressBar { Layout.fillWidth: true; value: Face.progress }
                UiText {
                    text: qsTr("Keep your face inside the circle, about an arm's length away. Glasses are fine.")
                    variant: "small"
                    tone: "subtle"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
                DButton { text: qsTr("Cancel"); variant: "ghost"; onClicked: Face.cancel() }
            }
        }

        // Set up / ready
        RowLayout {
            visible: !Face.enrolling && Face.camera
            Layout.fillWidth: true
            spacing: 24
            FaceScan {
                implicitWidth: 132
                implicitHeight: 132
                ticks: 44
                progress: Face.faces.length > 0 ? 1 : 0
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 12
                UiText {
                    text: Face.faces.length > 0
                          ? qsTr("Look at the screen to sign in or unlock. Add an alternative look if you often wear something that changes how you look, like glasses.")
                          : qsTr("Sign in just by looking at the screen. Setting up takes about ten seconds.")
                    tone: "muted"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
                Flow {
                    visible: Face.faces.length > 0
                    Layout.fillWidth: true
                    spacing: 10
                    Repeater {
                        model: Face.faces
                        delegate: Rectangle {
                            id: faceChip
                            required property string modelData
                            height: 40
                            width: faceRow.implicitWidth + 28
                            radius: 20
                            color: Theme.okSoft
                            Row {
                                id: faceRow
                                anchors.centerIn: parent
                                spacing: 8
                                Icon { name: "user-identity"; size: 16; color: Theme.ok; anchors.verticalCenter: parent.verticalCenter }
                                UiText { text: faceChip.modelData; font.weight: Font.DemiBold; color: Theme.ok; anchors.verticalCenter: parent.verticalCenter }
                            }
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    DButton {
                        visible: Face.faces.length < 2
                        text: Face.faces.length > 0 ? qsTr("Add an alternative look") : qsTr("Set up face sign-in")
                        variant: Face.faces.length > 0 ? "secondary" : "primary"
                        iconName: "camera-web"
                        enabled: Face.service
                        onClicked: Face.enroll()
                    }
                    Item { Layout.fillWidth: true }
                    DButton {
                        visible: Face.faces.length > 0
                        text: qsTr("Remove face data")
                        variant: "ghost"
                        onClicked: page.win.confirm({
                            title: qsTr("Remove face sign-in?"),
                            text: qsTr("Your face models are deleted from this device. You can set it up again any time."),
                            tone: "error", icon: "camera-web", confirmText: qsTr("Remove")
                        }, () => Face.removeAll())
                    }
                }
            }
        }

        InfoBanner {
            visible: Face.camera && !Face.service
            Layout.fillWidth: true
            tone: "info"
            title: qsTr("Your camera is ready for it")
            text: qsTr("Face sign-in needs the Asima OS face service (asimaos-faced), which isn't available on this computer.")
        }
        // A normal webcam (D37): off until the person chooses it, knowing what it means.
        InfoBanner {
            visible: !!Face.webcam && Face.cameraKind !== "infrared" && !Face.enrolling
            Layout.fillWidth: true
            tone: "warn"
            title: qsTr("A normal webcam is easier to fool than an infrared camera")
            text: qsTr("A photo of you can't blink, so Asima OS asks you to blink twice; a video could still get past it. It never signs you in at the login screen, and it asks for the password after a restart and after three failed tries.")
            DSwitch {
                checked: !!page.bio.useWebcam
                Accessible.name: qsTr("Use this webcam for face unlock")
                onToggled: { Backend.set("biometrics", "useWebcam", checked); faceRefresh.restart() }
            }
        }
        Timer { id: faceRefresh; interval: 800; onTriggered: Face.refresh() }
        UiText {
            visible: !Face.camera && !Face.webcam
            text: qsTr("Face sign-in needs a camera: an infrared one, the kind laptops have for Windows Hello, or a normal webcam that you switch on here.")
            tone: "muted"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }
    }

    // ── Where biometrics work ────────────────────────────────────────────────
    Card {
        visible: Fingerprint.available || Face.camera
        Layout.fillWidth: true
        title: qsTr("Use your finger or face for")
        icon: "user-identity"
        tone: "info"
        spacing: 0
        ListRow {
            Layout.fillWidth: true
            title: qsTr("Signing in")
            subtitle: qsTr("After a restart your password is needed once, so your saved passwords unlock too. A webcam face doesn't sign in here.")
            DSwitch { checked: !!page.bio.useLogin; onToggled: Backend.set("biometrics", "useLogin", checked) }
        }
        ListRow {
            Layout.fillWidth: true
            title: qsTr("Unlocking the screen")
            subtitle: qsTr("Choose Face or Fingerprint on the lock screen, next to the password")
            DSwitch { checked: !!page.bio.useUnlock; onToggled: Backend.set("biometrics", "useUnlock", checked) }
        }
        // D37, D47: never on its own; the person taps the button in the password dialog each time.
        ListRow {
            Layout.fillWidth: true
            title: qsTr("Approving admin tasks")
            subtitle: qsTr("Tap the fingerprint or face button in the password dialog when an app asks. Typing the password always works too; the terminal (sudo) always asks for it.")
            value: qsTr("In the dialog")
            separator: false
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "ok"
        title: qsTr("Private by design")
        text: qsTr("Fingerprint and face templates stay on this device, readable only by the system. A factory reset deletes them.")
    }
}
