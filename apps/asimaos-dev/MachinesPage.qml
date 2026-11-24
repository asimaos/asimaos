pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQuick.Controls.Basic as B
import Asima.Ui

// Virtual machines in your own libvirt session: made here from an installer disc,
// started and stopped here, their screen in Virtual Machine Manager (Flathub).
DPage {
    id: page
    title: qsTr("Virtual machines")
    subtitle: qsTr("Windows and other systems in a window, with UEFI, Secure Boot and a TPM for Windows 11.")

    readonly property var vms: Backend.workspaces.vms || []
    readonly property var win: B.ApplicationWindow.window
    property string kind: "windows"
    property url iso: ""
    readonly property string isoName: String(iso).split("/").pop()
    readonly property var pt: Backend.passthrough
    readonly property var gpus: pt.gpus || []
    readonly property var spareGpus: gpus.filter(g => !g.screen)
    readonly property var readyGpu: gpus.find(g => g.ready) || null

    Card {
        Layout.fillWidth: true
        title: qsTr("New virtual machine")
        subtitle: qsTr("Pick the system and its installer disc (an .iso file). The disk grows only as it fills.")
        icon: "list-add"

        GridLayout {
            Layout.fillWidth: true
            columns: page.contentWidth > 820 ? 2 : 1
            columnSpacing: 12
            rowSpacing: 12
            SelectCard {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                padding: 14
                iconName: "computer"
                title: qsTr("Windows 11")
                subtitle: qsTr("UEFI, Secure Boot and a TPM, as Windows 11 needs")
                selected: page.kind === "windows"
                onToggled: { page.kind = "windows"; memory.value = Math.min(8, Backend.hostMemoryGb() / 2) }
            }
            SelectCard {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                padding: 14
                iconName: "computer"
                title: qsTr("Linux or another system")
                subtitle: qsTr("UEFI with the fast virtio devices")
                selected: page.kind === "linux"
                onToggled: { page.kind = "linux"; memory.value = Math.min(4, Backend.hostMemoryGb() / 2) }
            }
        }

        GridLayout {
            Layout.fillWidth: true
            Layout.topMargin: 6
            columns: 3
            columnSpacing: 12
            rowSpacing: 10

            UiText { text: qsTr("Name"); font.weight: Font.Medium }
            DTextField {
                id: vmName
                Layout.fillWidth: true
                Layout.columnSpan: 2
                text: page.kind === "windows" ? "Windows 11" : "Linux"
            }

            UiText { text: qsTr("Installer disc"); font.weight: Font.Medium }
            UiText {
                Layout.fillWidth: true
                text: page.isoName !== "" ? page.isoName : qsTr("None chosen yet")
                tone: page.isoName !== "" ? "default" : "muted"
                elide: Text.ElideMiddle
            }
            DButton { text: qsTr("Choose…"); variant: "secondary"; iconName: "media-optical"; onClicked: isoDialog.open() }

            UiText { text: qsTr("Memory"); font.weight: Font.Medium }
            DSlider {
                id: memory
                Layout.fillWidth: true
                from: 2
                to: Math.max(4, Backend.hostMemoryGb() - 2)
                stepSize: 1
                value: Math.min(8, Backend.hostMemoryGb() / 2)
            }
            UiText { text: qsTr("%1 GB").arg(Math.round(memory.value)); tone: "muted"; Layout.preferredWidth: 64 }

            UiText { text: qsTr("Processor cores"); font.weight: Font.Medium }
            DSlider {
                id: cores
                Layout.fillWidth: true
                from: 1
                to: Math.max(2, Backend.hostCpus())
                stepSize: 1
                value: Math.min(4, Math.max(2, Backend.hostCpus() / 2))
            }
            UiText { text: Math.round(cores.value); tone: "muted"; Layout.preferredWidth: 64 }

            UiText { text: qsTr("Disk"); font.weight: Font.Medium }
            DSlider {
                id: disk
                Layout.fillWidth: true
                from: 32
                to: 512
                stepSize: 16
                value: page.kind === "windows" ? 80 : 48
            }
            UiText { text: qsTr("%1 GB").arg(Math.round(disk.value)); tone: "muted"; Layout.preferredWidth: 64 }
        }

        ListRow {
            visible: !!page.readyGpu
            Layout.fillWidth: true
            title: qsTr("Give it %1").arg(page.readyGpu ? page.readyGpu.name : "")
            subtitle: qsTr("The virtual machine gets the whole graphics card; its screen shows on the monitor connected to that card.")
            separator: false
            DSwitch { id: giveGpu; checked: !!page.readyGpu }
        }

        DButton {
            text: qsTr("Make the virtual machine")
            variant: "primary"
            iconName: "list-add"
            enabled: vmName.text.length > 0 && (page.isoName !== "" || Backend.demo)
            onClicked: Backend.createVm(vmName.text, page.kind, page.iso, Math.round(memory.value), Math.round(cores.value),
                                        Math.round(disk.value), page.readyGpu && giveGpu.checked ? page.readyGpu.slot : "")
        }
    }

    // GPU passthrough: a second graphics card, kept for virtual machines.
    Card {
        Layout.fillWidth: true
        title: qsTr("A graphics card for virtual machines")
        subtitle: page.spareGpus.length > 0
                  ? qsTr("Games and 3D programs in a virtual machine need a real graphics card. The card that shows your screen always stays with Asima OS.")
                  : qsTr("This needs a second graphics card: the one that shows your screen always stays with Asima OS.")
        icon: "video-display"
        tone: "info"
        spacing: 0
        Repeater {
            model: page.gpus
            delegate: ListRow {
                id: gpu
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: "video-display"
                title: modelData.name
                subtitle: modelData.screen ? qsTr("Shows your screen: stays with Asima OS")
                          : modelData.ready ? qsTr("Ready for virtual machines")
                          : modelData.kept ? (page.pt.iommu ? qsTr("Kept for virtual machines from the next start") : qsTr("Restart to switch the IOMMU on and keep it"))
                          : modelData.shared.length > 0 ? qsTr("Shares its IOMMU group with %1: passthrough may not work").arg(modelData.shared.join(", "))
                          : qsTr("Used by Asima OS (%1)").arg(modelData.driver || qsTr("no driver"))
                separator: index < page.gpus.length - 1
                RowLayout {
                    spacing: 6
                    StatusPill {
                        visible: gpu.modelData.ready || gpu.modelData.kept
                        text: gpu.modelData.ready ? qsTr("Ready") : qsTr("After a restart")
                        tone: gpu.modelData.ready ? "ok" : "info"
                    }
                    DButton {
                        visible: !gpu.modelData.screen && !gpu.modelData.kept
                        enabled: page.pt.kept === ""
                        text: qsTr("Keep for virtual machines")
                        variant: "ghost"
                        onClicked: page.win.confirm({
                            title: qsTr("Keep %1 for virtual machines?").arg(gpu.modelData.name),
                            text: qsTr("From the next start Asima OS leaves this card alone, and virtual machines made here can have it. Monitors connected to it go dark until a virtual machine uses it. The IOMMU is switched on with a signed boot add-on."),
                            confirmText: qsTr("Keep it"), icon: "video-display"
                        }, () => Backend.keepGpu(gpu.modelData.address))
                    }
                    DButton {
                        visible: !!gpu.modelData.kept
                        text: qsTr("Give back")
                        variant: "ghost"
                        onClicked: Backend.releaseGpu()
                    }
                }
            }
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Your virtual machines")
        icon: "computer"
        tone: "info"
        spacing: 0
        actions: [ DButton { text: qsTr("Open Virtual Machine Manager"); variant: "ghost"; onClicked: Backend.openApp("flatpak run org.virt_manager.virt-manager") } ]
        Repeater {
            model: page.vms
            delegate: ListRow {
                id: vm
                required property var modelData
                required property int index
                readonly property bool running: modelData.state === "running"
                Layout.fillWidth: true
                iconName: "computer"
                title: modelData.name
                subtitle: modelData.detail + (modelData.size > 0 ? "  ·  " + Theme.bytes(modelData.size) : "")
                separator: index < page.vms.length - 1
                RowLayout {
                    spacing: 6
                    StatusPill { text: vm.running ? qsTr("Running") : qsTr("Off"); tone: vm.running ? "ok" : "neutral" }
                    DButton {
                        text: vm.running ? qsTr("Shut down") : qsTr("Start")
                        variant: "ghost"
                        onClicked: vm.running ? Backend.stopVm(vm.modelData.name) : Backend.startVm(vm.modelData.name)
                    }
                    DButton {
                        visible: !vm.running
                        text: qsTr("Remove")
                        variant: "ghost"
                        onClicked: page.win.confirm({ title: qsTr("Remove \"%1\"?").arg(vm.modelData.name),
                                                      text: qsTr("Its disk and everything installed in it are deleted. The installer disc stays."),
                                                      confirmText: qsTr("Remove"), tone: "error", icon: "edit-delete" },
                                                    () => Backend.removeVm(vm.modelData.name))
                    }
                }
            }
        }
        UiText {
            visible: page.vms.length === 0
            text: qsTr("No virtual machines yet. Make one above.")
            tone: "muted"
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }
    }

    FileDialog {
        id: isoDialog
        title: qsTr("Choose the installer disc")
        nameFilters: [qsTr("Disc images (*.iso)")]
        onAccepted: page.iso = selectedFile
    }
}
