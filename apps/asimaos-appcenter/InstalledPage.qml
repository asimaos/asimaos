pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

// What is installed, one row per kind, with each kind's tools (Flatpak clean-up and repair,
// AppImage menu clean-up, Nix space, Android); then the kinds still to set up.
DPage {
    id: page
    title: qsTr("Installed")
    subtitle: qsTr("Everything installed on this computer, of every kind, and the tools to keep it tidy.")
    readonly property var win: B.ApplicationWindow.window
    readonly property var notReady: Backend.kinds.filter(k => !k.ready)

    headerActions: [ DButton { text: qsTr("Look again"); variant: "ghost"; iconName: "view-refresh"; onClicked: Backend.refreshInstalled() } ]

    Repeater {
        model: Backend.installed.filter(r => (r.items || []).length > 0 || (r.tools || []).length > 0)
        delegate: Card {
            id: kindCard
            required property var modelData
            Layout.fillWidth: true
            title: modelData.title
            subtitle: qsTr("%n installed", "", (modelData.items || []).length)
            icon: modelData.icon || "package"
            tone: "info"
            spacing: 0
            actions: [
                Repeater {
                    model: kindCard.modelData.tools || []
                    delegate: DButton {
                        required property var modelData
                        text: modelData.title
                        variant: "ghost"
                        onClicked: modelData.id === "removeandroid"
                                   ? page.win.confirm({ title: qsTr("Remove Android?"), text: qsTr("Android and every Android app with its data are deleted."),
                                                        icon: "edit-delete", tone: "error", confirmText: qsTr("Remove Android") },
                                                      () => Backend.tool(kindCard.modelData.kind, "removeandroid"))
                                   : Backend.tool(kindCard.modelData.kind, modelData.id)
                    }
                }
            ]
            Repeater {
                model: kindCard.modelData.items || []
                delegate: ListRow {
                    id: appRow
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    title: modelData.name || modelData.id
                    subtitle: modelData.missing ? qsTr("Its file is gone: “Clean up the menu” removes the entry")
                              : [modelData.version ? qsTr("Version %1").arg(modelData.version) : "", modelData.summary || ""].filter(x => !!x).join(" · ")
                    separator: index < (kindCard.modelData.items || []).length - 1
                    RowLayout {
                        spacing: 6
                        DButton { visible: !appRow.modelData.missing; text: qsTr("Open"); variant: "ghost"; onClicked: Backend.open(appRow.modelData) }
                        DButton {
                            text: qsTr("Remove")
                            variant: "ghost"
                            onClicked: page.win.confirm({
                                title: qsTr("Remove %1?").arg(appRow.modelData.name),
                                text: appRow.modelData.container ? qsTr("The container and everything installed in it are deleted.")
                                                                 : qsTr("It is uninstalled; your own files stay."),
                                icon: "edit-delete", tone: "warn", confirmText: qsTr("Remove")
                            }, () => Backend.remove(appRow.modelData))
                        }
                    }
                }
            }
        }
    }

    Card {
        visible: page.notReady.length > 0
        Layout.fillWidth: true
        title: qsTr("More kinds of software")
        subtitle: qsTr("Each is set up once, in your home folder or as an app, without changing the system itself.")
        icon: "list-add"
        spacing: 0
        Repeater {
            model: page.notReady
            delegate: ListRow {
                id: kindRow
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: modelData.icon
                title: modelData.title
                separator: index < page.notReady.length - 1
                DButton {
                    visible: kindRow.modelData.id !== "snap"
                    text: qsTr("Set up")
                    variant: "secondary"
                    onClicked: Backend.setUp(kindRow.modelData.id)
                }
            }
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Containers next to Distrobox")
        subtitle: qsTr("JuNest: Arch Linux in your home folder, without root and without Podman.")
        icon: "virtual-desktops"
        actions: [ DButton { text: qsTr("Set up JuNest"); variant: "ghost"; onClicked: Backend.setUp("containers") } ]
    }
}
