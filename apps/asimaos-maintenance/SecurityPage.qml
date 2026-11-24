pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// Where Secure Boot (D16), automatic unlock with the TPM (D10) and the disk's secrets are set up.
DPage {
    id: page
    title: qsTr("Security")
    subtitle: qsTr("How Asima OS protects your computer and your data, and where you set it up.")

    readonly property var win: B.ApplicationWindow.window
    readonly property var p: Backend.protection
    readonly property bool sbOn: p.secureBoot === "on"
    property string newKey: ""   // a freshly made recovery key, shown once while this page is open

    readonly property var mokSteps: [
        qsTr("Restart the computer."),
        qsTr("A blue screen, “Perform MOK management”, appears. Choose Enroll MOK, then Continue, then Yes."),
        qsTr("Type the code above and press Enter. It's digits only, so any keyboard works."),
        qsTr("Choose Reboot. Then switch Secure Boot on in the firmware settings; Asima Maintenance can take you there.")
    ]

    Connections {
        target: Backend
        function onRecoveryKeyMade(key) { page.newKey = key }
    }

    InfoBanner {
        visible: !page.p.managed && !Backend.demo
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Showing what this computer reports")
        text: qsTr("Changes here need the Asima Maintenance service (asimaos-maintenanced), which comes with the next system build.")
    }

    // ── Secure Boot ──────────────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        title: qsTr("Secure Boot")
        subtitle: page.sbOn ? qsTr("Only boot files signed with the Asima OS key can start this computer.")
                  : page.p.secureBoot === "unsupported" ? qsTr("This computer isn't started in UEFI mode, so Secure Boot isn't available.")
                  : page.p.keyPending ? qsTr("The Asima OS key is waiting for your approval at the next start.")
                  : page.p.keyApproved ? qsTr("The Asima OS key is approved. Switch Secure Boot on in the firmware settings to finish.")
                  : qsTr("Off: any boot file can start this computer. Approve the Asima OS key once, then switch Secure Boot on.")
        icon: "view-certificate"
        tone: page.sbOn ? "ok" : page.p.keyPending ? "accent" : "warn"
        actions: [
            StatusPill {
                text: page.sbOn ? qsTr("On") : page.p.keyPending ? qsTr("Waiting for restart") : qsTr("Off")
                tone: page.sbOn ? "ok" : page.p.keyPending ? "info" : "warn"
            }
        ]

        // Set up: one button, no jargon
        RowLayout {
            visible: !page.sbOn && !page.p.keyApproved && !page.p.keyPending && page.p.secureBoot !== "unsupported"
            Layout.fillWidth: true
            spacing: 14
            DButton {
                text: qsTr("Set up Secure Boot")
                variant: "primary"
                iconName: "view-certificate"
                onClicked: Backend.approveAsimaKey()
            }
            UiText {
                text: qsTr("Takes one restart. Nothing changes until you confirm on the screen that appears.")
                variant: "small"
                tone: "muted"
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
        }

        // Approval queued: the code and what to do on the blue screen
        RowLayout {
            visible: !!page.p.keyPending
            Layout.fillWidth: true
            spacing: 24

            Rectangle {
                Layout.alignment: Qt.AlignTop
                implicitWidth: codeColumn.implicitWidth + 48
                implicitHeight: codeColumn.implicitHeight + 32
                radius: Theme.radiusL
                color: Theme.accentSoft
                border.width: 1
                border.color: Qt.alpha(Theme.accent, 0.3)
                ColumnLayout {
                    id: codeColumn
                    anchors.centerIn: parent
                    spacing: 4
                    UiText { text: qsTr("Your one-time code"); variant: "overline"; tone: "accent"; Layout.alignment: Qt.AlignHCenter }
                    UiText {
                        text: page.p.pendingCode || ""
                        font.family: "Noto Sans Mono"
                        font.pixelSize: 34
                        font.weight: Font.Bold
                        font.letterSpacing: 3
                        Layout.alignment: Qt.AlignHCenter
                    }
                    UiText { text: qsTr("Write it down"); variant: "caption"; tone: "muted"; Layout.alignment: Qt.AlignHCenter }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 10
                Repeater {
                    model: page.mokSteps
                    delegate: RowLayout {
                        id: stepRow
                        required property string modelData
                        required property int index
                        Layout.fillWidth: true
                        spacing: 12
                        Rectangle {
                            Layout.alignment: Qt.AlignTop
                            implicitWidth: 24
                            implicitHeight: 24
                            radius: 12
                            color: Theme.accent
                            UiText {
                                anchors.centerIn: parent
                                text: String(stepRow.index + 1)
                                variant: "small"
                                font.weight: Font.Bold
                                color: Theme.textOnAccent
                            }
                        }
                        UiText {
                            text: stepRow.modelData
                            Layout.fillWidth: true
                            Layout.topMargin: 2
                            wrapMode: Text.WordWrap
                        }
                    }
                }
                RowLayout {
                    Layout.topMargin: 6
                    spacing: 10
                    DButton {
                        text: qsTr("Restart now")
                        variant: "primary"
                        iconName: "system-reboot"
                        onClicked: Backend.restartNow()
                    }
                    DButton { text: qsTr("Cancel"); variant: "ghost"; onClicked: Backend.cancelKeyApproval() }
                }
            }
        }

        // Approved, but still off in the firmware
        RowLayout {
            visible: !page.sbOn && !!page.p.keyApproved && !page.p.keyPending
            Layout.fillWidth: true
            spacing: 14
            DButton {
                text: qsTr("Restart to firmware settings")
                variant: "primary"
                iconName: "system-reboot"
                onClicked: page.win.confirm({
                    title: qsTr("Restart into the firmware settings?"),
                    text: qsTr("Look for “Secure Boot” (often under Boot or Security), set it to Enabled, then save and exit."),
                    icon: "view-certificate", confirmText: qsTr("Restart")
                }, () => Backend.restartToFirmware())
            }
        }

        UiText {
            visible: page.p.secureBoot !== "unsupported"
            text: qsTr("The Asima OS key also stays on the boot partition (EFI/BOOT/asimaos.der). If Secure Boot ever refuses to start Asima OS, approve it from the blue MOK screen with “Enroll key from disk”.")
            variant: "small"
            tone: "subtle"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }
    }

    // ── Own keys in the firmware (instead of approving the key on the blue screen) ──
    Card {
        id: ownCard
        readonly property var o: page.p.ownKeys || ({})
        visible: page.p.secureBoot !== "unsupported" && page.p.managed !== false
        Layout.fillWidth: true
        title: qsTr("Own Secure Boot keys")
        subtitle: ownCard.o.own
            ? qsTr("This computer has keys of its own in the firmware, with the Asima OS key and the certificates of Microsoft and the computer's maker.")
            : qsTr("For those who want it: this computer gets keys of its own in the firmware, which trust the Asima OS key directly. Microsoft's and the maker's certificates stay, so Windows and graphics cards still start. It also clears old keys out of the firmware.")
        icon: "view-certificate"
        tone: ownCard.o.own ? "ok" : "info"
        spacing: 10
        property var dropped: ({})
        readonly property var droppedIds: Object.keys(ownCard.dropped).filter(k => ownCard.dropped[k])

        // Not prepared yet
        DButton {
            visible: !ownCard.o.own && !ownCard.o.prepared
            text: qsTr("Use own keys…")
            variant: "secondary"
            iconName: "view-certificate"
            onClicked: page.win.confirm({
                title: qsTr("Use own Secure Boot keys?"),
                text: qsTr("Asima OS saves the firmware's certificates and makes this computer's keys (they never leave it). Then you put the firmware into Setup Mode in its settings, and come back here to write the keys."),
                icon: "view-certificate", confirmText: qsTr("Continue")
            }, () => Backend.prepareOwnKeys())
        }
        // Prepared: now Setup Mode, in the firmware
        ColumnLayout {
            visible: !ownCard.o.own && !!ownCard.o.prepared && !ownCard.o.setupMode
            Layout.fillWidth: true
            spacing: 6
            Repeater {
                model: [
                    qsTr("Restart into the firmware settings (the button below)."),
                    qsTr("Under Security or Boot, choose “Reset to Setup Mode” or “Clear Secure Boot keys” (the name differs from maker to maker)."),
                    qsTr("Save and exit. Asima OS starts as usual; come back here to write the keys.")
                ]
                delegate: UiText {
                    required property string modelData
                    required property int index
                    Layout.fillWidth: true
                    text: (index + 1) + ".  " + modelData
                    wrapMode: Text.WordWrap
                }
            }
            DButton { text: qsTr("Restart to firmware settings"); variant: "primary"; iconName: "system-reboot"; onClicked: Backend.restartToFirmware() }
        }
        // Setup Mode: which saved certificates stay
        ColumnLayout {
            visible: !ownCard.o.own && !!ownCard.o.prepared && !!ownCard.o.setupMode
            Layout.fillWidth: true
            spacing: 0
            UiText {
                text: qsTr("The firmware is ready. These certificates were in it; untick old ones you no longer want (keep Microsoft's).")
                Layout.fillWidth: true
                Layout.bottomMargin: 6
                wrapMode: Text.WordWrap
            }
            Repeater {
                model: ownCard.o.certificates || []
                delegate: ListRow {
                    id: certRow
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    iconName: "view-certificate"
                    title: modelData.name
                    subtitle: modelData.list === "KEK" ? qsTr("May change the firmware's lists (KEK)") : qsTr("Programs signed with it may start (db)")
                    separator: index < (ownCard.o.certificates || []).length - 1
                    DCheckBox {
                        checked: !ownCard.dropped[certRow.modelData.id]
                        onToggled: {
                            const d = Object.assign({}, ownCard.dropped)
                            d[certRow.modelData.id] = !checked
                            ownCard.dropped = d
                        }
                    }
                }
            }
            DButton {
                Layout.topMargin: 10
                text: qsTr("Write the keys")
                variant: "primary"
                iconName: "view-certificate"
                onClicked: page.win.confirm({
                    title: qsTr("Write the keys into the firmware?"),
                    text: qsTr("The kept certificates, the Asima OS key and this computer's own keys are written. Afterwards switch Secure Boot on in the firmware settings if it is off."),
                    icon: "view-certificate", confirmText: qsTr("Write")
                }, () => Backend.enrollOwnKeys(ownCard.droppedIds))
            }
        }
    }

    // ── Keys approved besides this version's own ─────────────────────────────
    Card {
        id: keysCard
        visible: (page.p.otherKeys || []).length > 0 || !!page.p.retirePending
        Layout.fillWidth: true
        title: qsTr("Other approved keys")
        subtitle: qsTr("Keys this computer trusts besides this version's Asima OS key: those of earlier builds, or of another system on this computer. Retire the ones you no longer use: they are removed and blocked, so nothing signed with them can start again.")
        icon: "view-certificate"
        tone: "info"
        spacing: 0
        property var chosen: ({})
        readonly property var chosenIds: Object.keys(keysCard.chosen).filter(k => keysCard.chosen[k])

        Repeater {
            model: page.p.retirePending ? [] : (page.p.otherKeys || [])
            delegate: ListRow {
                id: keyRow
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: "view-certificate"
                title: modelData.name
                subtitle: qsTr("Made %1").arg(modelData.since)
                separator: index < (page.p.otherKeys || []).length - 1
                DCheckBox {
                    checked: !!keysCard.chosen[keyRow.modelData.id]
                    onToggled: {
                        const c = Object.assign({}, keysCard.chosen)
                        c[keyRow.modelData.id] = checked
                        keysCard.chosen = c
                    }
                }
            }
        }
        DButton {
            visible: !page.p.retirePending
            Layout.topMargin: 12
            enabled: keysCard.chosenIds.length > 0
            text: qsTr("Retire the chosen keys…")
            variant: "secondary"
            iconName: "edit-delete"
            onClicked: page.win.confirm({
                title: qsTr("Retire %n key(s)?", "", keysCard.chosenIds.length),
                text: qsTr("At the next start the blue key screen asks twice: to delete the keys, then to block them. A system that still needs one of them (another Linux on this computer) would no longer start with Secure Boot on."),
                icon: "view-certificate", tone: "warn", confirmText: qsTr("Retire")
            }, () => Backend.retireKeys(keysCard.chosenIds))
        }
        ColumnLayout {
            visible: !!page.p.retirePending
            Layout.fillWidth: true
            spacing: 8
            UiText {
                text: qsTr("Your one-time code: %1").arg(page.p.retireCode || "")
                font.family: "Noto Sans Mono"
                font.pixelSize: Theme.textLg
                font.weight: Font.Bold
            }
            Repeater {
                model: [
                    qsTr("Restart the computer."),
                    qsTr("On the blue screen, choose Delete MOK, then Continue, then Yes, and type the code."),
                    qsTr("Then choose Enroll MOKX, then Continue, then Yes, and type the same code."),
                    qsTr("Choose Reboot.")
                ]
                delegate: UiText {
                    required property string modelData
                    required property int index
                    Layout.fillWidth: true
                    text: (index + 1) + ".  " + modelData
                    wrapMode: Text.WordWrap
                }
            }
            RowLayout {
                spacing: 10
                DButton { text: qsTr("Restart now"); variant: "primary"; iconName: "system-reboot"; onClicked: Backend.restartNow() }
                DButton { text: qsTr("Cancel"); variant: "ghost"; onClicked: Backend.cancelRetireKeys() }
            }
        }
    }

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 820 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        // ── Disk encryption ──────────────────────────────────────────────────
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Disk encryption")
            subtitle: page.p.encrypted ? qsTr("LUKS2 with Argon2id. Everything except the boot partition is encrypted.")
                                       : qsTr("This disk was installed without encryption. Adding it later means installing Asima OS again.")
            icon: "document-encrypted"
            tone: page.p.encrypted ? "ok" : "warn"
            actions: [ StatusPill { text: page.p.encrypted ? qsTr("On") : qsTr("Off"); tone: page.p.encrypted ? "ok" : "warn" } ]
            Item { Layout.fillHeight: true }
            DButton {
                visible: !!page.p.encrypted
                text: qsTr("Change passphrase")
                variant: "secondary"
                iconName: "document-edit"
                onClicked: page.win.confirm({
                    title: qsTr("Change the disk passphrase"),
                    text: qsTr("You'll type the new one when this computer starts, with its keyboard layout. Your recovery key keeps working."),
                    icon: "object-locked", confirmText: qsTr("Change"),
                    secret: qsTr("Current passphrase or recovery key"), newSecret: qsTr("New passphrase"), newSecretMin: 8
                }, v => Backend.changePassphrase(v.secret, v.newSecret))
            }
        }

        // ── Automatic unlock (TPM) ───────────────────────────────────────────
        // D49: automatic (PCR 4 + 7, recommended) or, optionally, with a PIN at every start.
        Card {
            id: tpmCard
            visible: !!page.p.encrypted
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            readonly property bool pin: !!page.p.tpmEnrolled && !!page.p.tpmPin
            readonly property string pinHelp: qsTr("At every start you type this PIN instead of the passphrase. Any characters work; digits are easiest. After a few wrong PINs the chip waits a while before it tries again; your passphrase and recovery key always work.")
            function askPin(title, confirmText) {
                page.win.confirm({
                    title: title, text: tpmCard.pinHelp, icon: "computer", confirmText: confirmText,
                    secret: qsTr("Disk passphrase or recovery key"), newSecret: qsTr("New PIN"), newSecretMin: 4
                }, v => Backend.enrollTpmPin(v.secret, v.newSecret))
            }
            function askAutomatic() {
                page.win.confirm({
                    title: qsTr("Set up automatic unlock"),
                    text: page.sbOn ? qsTr("Type your disk passphrase once. From the next start, this computer unlocks the disk by itself.")
                                    : qsTr("Type your disk passphrase once. Tip: set up Secure Boot first; switching it on later means setting this up again."),
                    icon: "computer", confirmText: qsTr("Set up"),
                    secret: qsTr("Disk passphrase or recovery key")
                }, v => Backend.enrollTpm(v.secret))
            }
            title: qsTr("Automatic unlock")
            subtitle: !page.p.tpm ? qsTr("This computer has no TPM 2.0 chip, so the disk asks for your passphrase at every start.")
                      : tpmCard.pin ? qsTr("The security chip (TPM 2.0) opens the disk when you type your PIN at the start, and only while Secure Boot and a signed Asima OS are intact. Otherwise it asks for your passphrase.")
                      : page.p.tpmEnrolled ? qsTr("The security chip (TPM 2.0) unlocks the disk only while Secure Boot and a signed Asima OS are intact. Otherwise it asks for your passphrase.")
                      : qsTr("Let this computer's security chip (TPM 2.0) unlock the disk at startup, so you don't type the passphrase every time. If you prefer, it can ask for a short PIN instead.")
            icon: "computer"
            tone: page.p.tpmEnrolled ? "ok" : "info"
            actions: [
                StatusPill {
                    text: !page.p.tpm ? qsTr("No TPM") : tpmCard.pin ? qsTr("On, with a PIN") : page.p.tpmEnrolled ? qsTr("On")
                          : page.p.tpmKnown ? qsTr("Off") : qsTr("Available")
                    tone: page.p.tpmEnrolled ? "ok" : "neutral"
                }
            ]
            Item { Layout.fillHeight: true }
            Flow {
                visible: !!page.p.tpm
                Layout.fillWidth: true
                spacing: 10
                DButton {
                    visible: !page.p.tpmEnrolled
                    text: qsTr("Set up automatic unlock")
                    variant: "primary"
                    iconName: "object-unlocked"
                    onClicked: tpmCard.askAutomatic()
                }
                DButton {
                    visible: !page.p.tpmEnrolled
                    text: qsTr("Use a PIN instead")
                    variant: "ghost"
                    onClicked: tpmCard.askPin(qsTr("Unlock with a PIN"), qsTr("Set up"))
                }
                DButton {
                    visible: tpmCard.pin
                    text: qsTr("Change the PIN")
                    variant: "ghost"
                    onClicked: tpmCard.askPin(qsTr("Change the PIN"), qsTr("Change"))
                }
                DButton {
                    visible: tpmCard.pin
                    text: qsTr("Unlock automatically instead")
                    variant: "ghost"
                    onClicked: tpmCard.askAutomatic()
                }
                DButton {
                    visible: !!page.p.tpmEnrolled && !tpmCard.pin
                    text: qsTr("Ask for a PIN")
                    variant: "ghost"
                    onClicked: tpmCard.askPin(qsTr("Unlock with a PIN"), qsTr("Set up"))
                }
                DButton {
                    visible: !!page.p.tpmEnrolled
                    text: qsTr("Turn off")
                    variant: "ghost"
                    onClicked: page.win.confirm({
                        title: qsTr("Turn off automatic unlock?"),
                        text: qsTr("The disk will ask for your passphrase at every start."),
                        icon: "computer", confirmText: qsTr("Turn off")
                    }, () => Backend.removeTpm())
                }
            }
        }

        // ── Recovery key ─────────────────────────────────────────────────────
        Card {
            visible: !!page.p.encrypted
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("Recovery key")
            subtitle: qsTr("Unlocks the disk if the passphrase is forgotten. Asima OS never stores it, so it can't be shown again; a new one replaces the old one.")
            icon: "password-show-on"
            tone: "accent"

            ColumnLayout {
                visible: page.newKey.length > 0
                Layout.fillWidth: true
                spacing: 10
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: keyGrid.implicitHeight + 28
                    radius: Theme.radiusL - 2
                    color: Theme.surfaceAlt
                    border.width: 1
                    border.color: Theme.border
                    GridLayout {
                        id: keyGrid
                        anchors.centerIn: parent
                        columns: 4
                        columnSpacing: 16
                        rowSpacing: 8
                        Repeater {
                            model: page.newKey.length ? page.newKey.split("-") : []
                            delegate: UiText {
                                required property string modelData
                                text: modelData
                                font.family: "Noto Sans Mono"
                                font.pixelSize: 16
                                font.weight: Font.DemiBold
                            }
                        }
                    }
                }
                UiText {
                    text: qsTr("Write it down now: it is shown only this once. The old recovery key no longer works.")
                    variant: "small"
                    tone: "warn"
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
            }
            Item { Layout.fillHeight: true }
            DButton {
                text: page.newKey.length > 0 ? qsTr("Done, I wrote it down") : qsTr("Make a new recovery key")
                variant: "secondary"
                iconName: page.newKey.length > 0 ? "dialog-ok-apply" : "view-refresh"
                onClicked: {
                    if (page.newKey.length > 0) {
                        page.newKey = ""
                        return
                    }
                    page.win.confirm({
                        title: qsTr("Make a new recovery key?"),
                        text: qsTr("The new key is shown once. The old one stops working."),
                        icon: "password-show-on", confirmText: qsTr("Make a new key"),
                        secret: qsTr("Disk passphrase")
                    }, v => {
                        // Demo answers at once; the service answers later (recoveryKeyMade).
                        const key = Backend.newRecoveryKey(v.secret)
                        if (key) page.newKey = key
                    })
                }
            }
        }

        // ── System protection (read-only facts) ──────────────────────────────
        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            title: qsTr("System protection")
            icon: "security-high"
            tone: "info"
            spacing: 0
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Verified system files")
                subtitle: qsTr("Every read of /usr is checked (dm-verity)")
                value: page.p.verity ? qsTr("On") : qsTr("Off")
                tone: page.p.verity ? "ok" : "warn"
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Kernel lockdown")
                subtitle: qsTr("Only drivers signed for Asima OS can load")
                value: page.p.lockdown === "none" ? qsTr("Off") : page.p.lockdown
                tone: page.p.lockdown === "none" ? "warn" : "ok"
            }
            ListRow {
                Layout.fillWidth: true
                title: qsTr("Security modules")
                subtitle: (page.p.lsms || []).join(" · ")
                value: qsTr("%n active", "", (page.p.lsms || []).length)
                tone: "info"
            }
            ListRow {
                Layout.fillWidth: true
                iconName: "security-high"
                title: qsTr("Revoked boot programs")
                subtitle: qsTr("The firmware's list of boot programs no one may start (dbx) comes as a firmware update")
                separator: false
                DButton { text: qsTr("Firmware updates"); variant: "ghost"; onClicked: Backend.openApp("plasma-discover --mode update") }
            }
        }
    }

    InfoBanner {
        visible: !!page.p.encrypted && !!page.p.tpmEnrolled
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("Before updating your BIOS")
        text: qsTr("A BIOS or Secure Boot database update can stop automatic unlock from working once. Keep your recovery key ready; Asima Maintenance sets automatic unlock up again afterwards.")
    }
}
