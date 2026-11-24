import QtQuick
import QtQuick.Layouts
import QtQuick.Window
import Asima.Ui

// The Asima OS password dialog (D37): shown when an app needs an administrator's approval. The
// password always works; the fingerprint and face buttons appear only when this account has them,
// and nothing is looked at until the person taps one.
Window {
    id: win
    property string themeMode: "system"

    width: 480
    height: content.implicitHeight + 48
    minimumWidth: 420
    visible: Agent.active
    flags: Qt.Dialog | Qt.WindowStaysOnTopHint
    title: qsTr("Permission needed")
    color: Theme.window

    Component.onCompleted: Theme.mode = themeMode

    onVisibleChanged: {
        if (visible) {
            password.text = ""
            raise()
            requestActivate()
            password.forceActiveFocus()
        }
    }

    readonly property var who: Agent.identities[Agent.identity] || ({ name: "", user: "" })
    readonly property bool canType: Agent.waiting && Agent.checking === ""

    ColumnLayout {
        id: content
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 24
        spacing: 14

        RowLayout {
            spacing: 14
            IconTile { name: "security-high"; tone: "accent"; size: 44 }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                UiText { text: qsTr("An app needs your permission"); variant: "heading"; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                UiText {
                    text: Agent.identities.length > 1 ? qsTr("Approve as an administrator")
                                                      : qsTr("Approve as %1 (%2)").arg(win.who.name).arg(win.who.user)
                    variant: "small"
                    tone: "muted"
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                }
            }
        }

        UiText {
            text: Agent.message
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }

        DComboBox {
            visible: Agent.identities.length > 1
            Layout.fillWidth: true
            model: Agent.identities.map((i, n) => ({ id: n, title: qsTr("%1 (%2)").arg(i.name).arg(i.user) }))
            currentIndex: Agent.identity
            onActivated: index => Agent.setIdentity(index)
        }

        PasswordField {
            id: password
            Layout.fillWidth: true
            placeholderText: qsTr("Password")
            enabled: win.canType
            tone: Agent.error !== "" ? "error" : ""
            onAccepted: if (text.length > 0) Agent.submit(text)
        }

        UiText {
            visible: Agent.error !== ""
            text: Agent.error
            color: Theme.error
            variant: "small"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }

        // Fingerprint and face: only when this account has them, only when tapped.
        ColumnLayout {
            visible: Agent.methods.length > 0
            Layout.fillWidth: true
            spacing: 8
            UiText { text: qsTr("Or approve with"); variant: "caption"; tone: "subtle" }
            RowLayout {
                spacing: 10
                Repeater {
                    model: Agent.methods
                    delegate: DButton {
                        required property string modelData
                        readonly property bool active: Agent.checking === modelData
                        variant: active ? "primary" : "secondary"
                        iconName: modelData === "face" ? "camera-web" : "fingerprint"
                        busy: active
                        enabled: Agent.active && (Agent.checking === "" || active)
                        text: modelData === "face" ? (active ? qsTr("Look at the camera…") : qsTr("Face"))
                                                   : (active ? qsTr("Touch the reader…") : qsTr("Fingerprint"))
                        onClicked: Agent.useBiometric(modelData)
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 6
            spacing: 10
            UiText {
                text: Agent.actionId
                variant: "caption"
                tone: "subtle"
                Layout.fillWidth: true
                elide: Text.ElideMiddle
            }
            DButton { text: qsTr("Cancel"); variant: "ghost"; onClicked: Agent.cancel() }
            DButton {
                text: qsTr("Approve")
                variant: "primary"
                enabled: win.canType && password.text.length > 0
                onClicked: Agent.submit(password.text)
            }
        }
    }

    // A wrong password: empty the box for the next try.
    property string lastError: Agent.error
    onLastErrorChanged: if (lastError !== "") { password.text = ""; password.forceActiveFocus() }
}
