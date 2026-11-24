pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

// Nix (D28): terminal and developer tools for you alone, from nixpkgs, outside the OS image.
DPage {
    id: page
    title: qsTr("Nix programs")
    subtitle: qsTr("Over 100,000 terminal and developer tools, installed for you alone. They run directly on the system and never change it.")

    readonly property var n: Backend.nix
    readonly property var programs: n.programs || []

    headerActions: [
        DButton {
            text: qsTr("Update all")
            variant: "secondary"
            iconName: "view-refresh"
            enabled: !!page.n.available && !page.n.busy && page.programs.length > 0
            onClicked: Backend.nixUpgrade()
        }
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
            iconName: "package"
            label: qsTr("Installed with Nix")
            value: qsTr("%n program(s)", "", page.programs.length)
            caption: qsTr("Yours only; other accounts have their own")
        }
        StatTile {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            iconName: "kt-check-data"
            iconTone: page.n.available ? "ok" : "warn"
            label: qsTr("Nix")
            value: page.n.available ? qsTr("Ready · %1").arg(page.n.version || "") : qsTr("Not available")
            caption: page.n.busy ? qsTr("Working…") : qsTr("Starts by itself when you use it")
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Install a program")
        subtitle: qsTr("Type its name as nixpkgs knows it, for example ripgrep, nodejs_22 or neovim.")
        icon: "list-add"
        tone: "info"
        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            DTextField {
                id: name
                Layout.fillWidth: true
                placeholderText: qsTr("Program name")
                onAccepted: if (text.trim().length > 0) { Backend.nixInstall(text.trim()); text = "" }
            }
            DButton {
                text: qsTr("Install")
                variant: "primary"
                iconName: "download"
                busy: !!page.n.busy
                enabled: !!page.n.available && !page.n.busy && name.text.trim().length > 0
                onClicked: { Backend.nixInstall(name.text.trim()); name.text = "" }
            }
            DButton {
                text: qsTr("Find names")
                variant: "ghost"
                iconName: "search"
                onClicked: Backend.openUrl("https://search.nixos.org/packages")
            }
        }
    }

    Card {
        Layout.fillWidth: true
        title: qsTr("Your programs")
        subtitle: qsTr("Available in every new terminal.")
        icon: "package"
        spacing: 0
        actions: [
            DButton {
                text: qsTr("Free up space")
                variant: "ghost"
                iconName: "edit-clear-all"
                enabled: !!page.n.available && !page.n.busy
                onClicked: Backend.nixClean()
            }
        ]

        Repeater {
            model: page.programs
            delegate: ListRow {
                id: entry
                required property var modelData
                required property int index
                Layout.fillWidth: true
                iconName: "utilities-terminal-symbolic"
                title: modelData.name
                subtitle: modelData.detail
                separator: index < page.programs.length - 1
                DButton {
                    text: qsTr("Remove")
                    variant: "ghost"
                    enabled: !page.n.busy
                    onClicked: Backend.nixRemove(entry.modelData.name)
                }
            }
        }
        UiText {
            visible: page.programs.length === 0
            text: qsTr("Nothing installed with Nix yet.")
            tone: "muted"
        }
    }

    InfoBanner {
        Layout.fillWidth: true
        tone: "info"
        title: qsTr("Outside Asima OS, kept safe")
        text: qsTr("Nix keeps everything in /nix, on its own part of the disk. Updates and going back to an older Asima OS leave your programs alone; a factory reset removes them.")
    }
}
