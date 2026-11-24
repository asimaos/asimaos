pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

DPage {
    id: page
    title: qsTr("Containers")
    subtitle: qsTr("A full Linux system per project, sharing your home folder: install anything inside without touching the system.")

    readonly property var w: Backend.workspaces
    readonly property var containers: w.containers || []
    property string image: "docker.io/library/ubuntu:24.04"
    readonly property var images: [
        { id: "docker.io/library/ubuntu:24.04", title: "Ubuntu 24.04" },
        { id: "registry.fedoraproject.org/fedora:45", title: "Fedora 45" },
        { id: "docker.io/library/archlinux:latest", title: "Arch Linux" },
        { id: "docker.io/library/debian:13", title: "Debian 13" }
    ]

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 820 ? 2 : 1
        columnSpacing: 16
        rowSpacing: 16
        StatTile {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            iconName: "virtual-desktops"
            label: qsTr("Containers")
            value: qsTr("%n container(s)", "", page.containers.length)
            caption: qsTr("apt, dnf or pacman inside, your system stays clean")
        }
        StatTile {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            iconName: "package"
            iconTone: page.w.podman && page.w.distrobox ? "ok" : "warn"
            label: qsTr("Tools")
            value: page.w.podman ? qsTr("Podman ready") : qsTr("Podman missing")
            caption: page.w.distrobox ? qsTr("Distrobox makes each container feel like part of the desktop")
                                      : qsTr("Distrobox isn't installed")
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Your containers")
        subtitle: qsTr("Programs inside can show up in your app menu; your files are shared.")
        icon: "virtual-desktops"
        spacing: 0

        Repeater {
            model: page.containers
            delegate: ListRow {
                id: entry
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: "package"
                iconTone: modelData.state === "running" ? "ok" : "info"
                title: modelData.name
                subtitle: modelData.image + (modelData.size > 0 ? "  ·  " + Theme.bytes(modelData.size) : "")
                value: modelData.state === "running" ? qsTr("Running") : modelData.state === "created" ? qsTr("New") : qsTr("Stopped")
                tone: modelData.state === "running" ? "ok" : "neutral"
                DButton { text: qsTr("Open"); variant: "ghost"; iconName: "utilities-terminal-symbolic"; onClicked: Backend.openContainer(entry.modelData.name) }
            }
        }
        UiText {
            visible: page.containers.length === 0
            text: page.w.podman ? qsTr("No containers yet.") : qsTr("Podman is not available right now.")
            tone: "muted"
            Layout.bottomMargin: 8
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.topMargin: 16
            spacing: 10
            UiText { text: qsTr("New container"); font.weight: Font.DemiBold }
            ChoiceChips {
                Layout.fillWidth: true
                options: page.images
                current: page.image
                onPicked: id => page.image = id
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 10
                DTextField {
                    id: nameField
                    Layout.preferredWidth: 260
                    placeholderText: qsTr("Name, e.g. my-project")
                    validator: RegularExpressionValidator { regularExpression: /[a-z0-9][a-z0-9_.-]{0,40}/ }
                }
                DButton {
                    text: qsTr("Create")
                    variant: "primary"
                    iconName: "list-add"
                    enabled: nameField.text.length > 0
                    onClicked: {
                        Backend.createContainer(page.image, nameField.text)
                        nameField.text = ""
                    }
                }
            }
        }
    }
}
