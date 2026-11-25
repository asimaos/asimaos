pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// "Repair this computer" (D55), step 1: the disk with Asima OS, and its passphrase when it is
// encrypted. Opening it only reads what is there; nothing changes yet.
WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window
    readonly property var r: Backend.repair
    readonly property var disks: r.disks || []
    readonly property var chosen: disks.find(d => d.path === page.win.repairDisk) || ({})
    readonly property bool looking: r.phase === "disks" && r.state === "working"
    readonly property bool opening: r.phase === "probe" && r.state === "working"

    title: qsTr("Repair this computer")
    subtitle: qsTr("Asima OS starts again, and nobody's files are touched. Choose the disk Asima OS is on.")
    backText: qsTr("Back")
    nextText: page.opening ? qsTr("Opening…") : qsTr("Continue")
    canContinue: !!page.chosen.path && !page.opening && (!page.chosen.encrypted || pass.text.length > 0)
    onBack: page.win.go("welcome")
    onNext: {
        page.win.repairPassphrase = pass.text
        Backend.probeRepair(page.chosen.path, pass.text)
    }

    Component.onCompleted: if (page.disks.length === 1) page.win.repairDisk = page.disks[0].path
    Connections {
        target: Backend
        function onRepairChanged() {
            if (Backend.repair.phase === "probe" && Backend.repair.state === "ok")
                page.win.go("repair-choice")
            else if (Backend.repair.phase === "disks" && Backend.repair.state === "ok" && (Backend.repair.disks || []).length === 1)
                page.win.repairDisk = Backend.repair.disks[0].path
        }
    }

    RowLayout {
        visible: page.looking
        spacing: 12
        Spinner { size: 20 }
        UiText { text: qsTr("Looking for Asima OS on this computer's disks…"); tone: "muted" }
    }

    Repeater {
        model: page.disks
        delegate: SelectCard {
            required property var modelData
            Layout.fillWidth: true
            iconName: "drive-harddisk"
            title: modelData.model || modelData.path
            subtitle: [modelData.path, modelData.size].filter(x => !!x).join(" · ")
            detail: modelData.encrypted ? qsTr("Encrypted: it opens with its passphrase or recovery key") : qsTr("Not encrypted")
            selected: page.win.repairDisk === modelData.path
            onToggled: page.win.repairDisk = modelData.path
        }
    }

    InfoBanner {
        visible: r.phase === "disks" && r.state === "ok" && page.disks.length === 0
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("No disk with Asima OS was found")
        text: qsTr("Connect the disk Asima OS is on, then look again. To start afresh, install Asima OS instead.")
        DButton { text: qsTr("Look again"); variant: "ghost"; iconName: "view-refresh"; onClicked: Backend.findRepairDisks() }
    }

    ColumnLayout {
        visible: !!page.chosen.encrypted
        Layout.fillWidth: true
        spacing: 8
        UiText { text: qsTr("Passphrase or recovery key"); variant: "small"; font.weight: Font.DemiBold }
        PasswordField {
            id: pass
            Layout.fillWidth: true
            placeholderText: qsTr("The disk's passphrase, or its recovery key")
            onAccepted: if (page.canContinue) page.next()
        }
        UiText {
            text: qsTr("The keyboard layout here is the USB stick's; a recovery key is typed the same on every layout.")
            variant: "small"
            tone: "muted"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }
    }

    InfoBanner {
        visible: (r.phase === "probe" || r.phase === "disks") && r.state === "error"
        Layout.fillWidth: true
        tone: "error"
        title: qsTr("The disk could not be opened")
        text: r.message || ""
    }
}
