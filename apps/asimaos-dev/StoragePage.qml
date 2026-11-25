pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

// Disk use and limits (D55): containers, virtual machines and Nix programs, so none of them can
// fill the disk. Counted by Btrfs simple quotas; limits through Asima Maintenance's service.
DPage {
    id: page
    title: qsTr("Disk use")
    subtitle: qsTr("How much your containers, virtual machines and Nix programs take, with a limit for each so none of them can fill the disk.")

    readonly property var st: Backend.storage
    readonly property var areas: st.areas || []
    readonly property real gb: 1e9      // as Theme.bytes shows sizes
    readonly property var sizes: [10, 25, 50, 100, 200, 500, 1000]

    function heading(a) {
        const kind = a.id.split(":")[0]
        if (a.person)
            return kind === "vm" ? qsTr("Your virtual machines") : qsTr("Your containers")
        return kind === "vm" ? qsTr("The system's virtual machines")
             : kind === "nix" ? qsTr("Nix programs (everyone's)")
             : qsTr("The system's containers")
    }
    function where(a) {
        const kind = a.id.split(":")[0]
        if (a.person)
            return kind === "vm" ? qsTr("Their disks, in ~/.local/share/libvirt/images") : qsTr("Distrobox and Podman, in ~/.local/share/containers")
        return kind === "vm" ? qsTr("Virtual Machine Manager's, in /var/lib/libvirt")
             : kind === "nix" ? qsTr("In /nix") : qsTr("Podman's for the whole system, in /var/lib/containers")
    }
    function used(a) {
        if (a.used === null || a.used === undefined)
            return qsTr("Not counted")
        return a.limit ? qsTr("%1 of %2").arg(Theme.bytes(a.used)).arg(Theme.bytes(a.limit)) : Theme.bytes(a.used)
    }
    function options(a) {
        const list = [{ id: 0, title: qsTr("No limit") }].concat(page.sizes.map(s => ({ id: s, title: qsTr("%1 GB").arg(s) })))
        const now = a.limit ? Math.round(a.limit / page.gb) : 0
        if (now > 0 && page.sizes.indexOf(now) < 0)
            list.push({ id: now, title: qsTr("%1 GB").arg(now) })
        return list
    }

    Component.onCompleted: Backend.readStorage()

    InfoBanner {
        visible: page.st.available === false
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Disk use isn't available")
        text: qsTr("It comes from Asima Maintenance's service, which isn't available on this computer.")
    }
    InfoBanner {
        visible: page.st.available === true && !page.st.counted
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Disk use isn't counted yet")
        text: qsTr("Setting a first limit switches counting on; from then on, what each area writes is counted.")
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Areas")
        icon: "drive-harddisk"
        spacing: 0
        Repeater {
            model: page.areas
            delegate: ColumnLayout {
                id: row
                required property var modelData
                required property int index
                Layout.fillWidth: true
                spacing: 6
                ListRow {
                    Layout.fillWidth: true
                    title: page.heading(row.modelData)
                    subtitle: row.modelData.separate ? page.where(row.modelData)
                              : qsTr("Made before Asima OS kept it apart, so it is counted with your home folder and can't have a limit of its own.")
                    value: row.modelData.separate ? page.used(row.modelData) : ""
                    separator: false
                    DComboBox {
                        visible: !!row.modelData.separate
                        implicitWidth: 170
                        model: page.options(row.modelData)
                        Component.onCompleted: currentIndex = indexOfValue(row.modelData.limit ? Math.round(row.modelData.limit / page.gb) : 0)
                        onActivated: Backend.setStorageLimit(row.modelData.id, currentValue)
                    }
                }
                DProgressBar {
                    visible: !!row.modelData.limit && row.modelData.used !== null && row.modelData.used !== undefined
                    Layout.fillWidth: true
                    Layout.bottomMargin: 10
                    value: row.modelData.limit ? Math.min(1, row.modelData.used / row.modelData.limit) : 0
                    tone: row.modelData.limit && row.modelData.used / row.modelData.limit > 0.9 ? "warn" : "accent"
                }
            }
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("When an area reaches its limit")
        text: qsTr("Programs writing there are told the disk is full (“disk quota exceeded”), while the rest of the computer carries on. Remove containers, images or old Nix generations, or raise the limit.")
    }
}
