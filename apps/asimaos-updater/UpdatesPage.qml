pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

DPage {
    id: page
    title: qsTr("Updates")

    subtitle: qsTr("Install an Asima OS update from a file. Your current version always stays as a safe fallback.")

    readonly property var upd: Backend.pendingUpdate
    readonly property var sys: Backend.system
    readonly property var win: B.ApplicationWindow.window
    readonly property string state_: upd.state || "idle"

    // ── Installed version ────────────────────────────────────────────────────
    Card {
        Layout.fillWidth: true
        padding: 18

        RowLayout {
            Layout.fillWidth: true
            spacing: 14
            AsimaMark { size: 44 }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 3
                RowLayout {
                    spacing: 10
                    UiText { text: qsTr("Asima OS %1").arg(page.sys.version || ""); variant: "heading" }
                    StatusPill { text: qsTr("Installed"); tone: "ok" }
                }
                UiText {
                    text: page.sys.pendingVersion
                          ? qsTr("Asima OS %1 is installed and starts after a restart").arg(page.sys.pendingVersion)
                          : page.sys.previousVersion
                          ? qsTr("Backup version %1 stays in the start menu, so you can go back").arg(page.sys.previousVersion)
                          : qsTr("No backup version yet: it appears after your first update")
                    variant: "small"
                    tone: "muted"
                    Layout.fillWidth: true
                }
            }
            DButton {
                text: qsTr("Versions")
                variant: "ghost"
                iconName: "edit-undo-history"
                onClicked: page.win.navigate("versions")
            }
        }
    }

    // ── No file yet ──────────────────────────────────────────────────────────
    Card {
        visible: page.state_ === "idle"
        Layout.fillWidth: true
        title: qsTr("Install from a file")
        subtitle: qsTr("Update files end in .aosupd. Asima OS checks the signature, every part of the file, the version and the free space before anything changes.")
        icon: "document-import"

        DropZone {
            Layout.fillWidth: true
            Layout.preferredHeight: 230
            iconName: "download"
            title: qsTr("Drop an Asima OS update file here")
            text: qsTr("Only files signed by Asima OS can be installed.")
            onFileDropped: file => Backend.selectUpdateFile(file)
            DButton {
                text: qsTr("Choose file…")
                variant: "primary"
                iconName: "document-open"
                onClicked: page.win.chooseUpdateFile()
            }
        }
    }

    // ── A file was chosen ────────────────────────────────────────────────────
    Card {
        visible: page.state_ !== "idle"
        Layout.fillWidth: true
        title: qsTr("Asima OS %1").arg(page.upd.version || "")
        subtitle: (page.upd.fileName || "") + (page.upd.fileSize ? " · " + Theme.bytes(page.upd.fileSize) : "")
        icon: page.state_ === "error" ? "dialog-close" : page.state_ === "done" ? "dialog-ok-apply" : "download"
        tone: page.state_ === "error" ? "error" : page.state_ === "done" ? "ok" : "accent"
        spacing: 18
        actions: [
            StatusPill {
                text: {
                    switch (page.state_) {
                    case "ready": return qsTr("Verified")
                    case "installing": return qsTr("Installing")
                    case "done": return qsTr("Installed")
                    case "error": return qsTr("Rejected")
                    default: return ""
                    }
                }
                tone: page.state_ === "error" ? "error" : page.state_ === "installing" ? "accent" : "ok"
            }
        ]

        // Ready: checks + release notes
        GridLayout {
            visible: page.state_ === "ready"
            Layout.fillWidth: true
            columns: page.contentWidth > 860 ? 2 : 1
            columnSpacing: 36
            rowSpacing: 18

            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                spacing: 0
                UiText { text: qsTr("Checks"); variant: "overline"; tone: "subtle"; Layout.bottomMargin: 2 }
                Repeater {
                    model: page.upd.checks || []
                    delegate: ListRow {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        title: modelData.title
                        subtitle: modelData.detail
                        value: modelData.value
                        tone: modelData.tone
                        separator: index < (page.upd.checks || []).length - 1
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                spacing: 12
                UiText { text: qsTr("What's new"); variant: "overline"; tone: "subtle"; Layout.bottomMargin: 6 }
                Repeater {
                    model: page.upd.notes || []
                    delegate: RowLayout {
                        id: note
                        required property string modelData
                        Layout.fillWidth: true
                        spacing: 12
                        Rectangle {
                            implicitWidth: 7; implicitHeight: 7; radius: 3.5
                            color: Theme.accent
                            Layout.alignment: Qt.AlignTop
                            Layout.topMargin: 7
                        }
                        UiText { text: note.modelData; Layout.fillWidth: true; wrapMode: Text.WordWrap }
                    }
                }
            }
        }

        RowLayout {
            visible: page.state_ === "ready"
            Layout.fillWidth: true
            Layout.topMargin: 4
            spacing: 10
            DButton {
                // The running version's own file writes a fresh copy of it (D21).
                text: page.upd.reinstall ? qsTr("Reinstall this version") : qsTr("Install update")
                variant: "primary"
                large: true
                iconName: page.upd.reinstall ? "view-refresh" : "download"
                onClicked: Backend.installUpdate()
            }
            DButton {
                text: qsTr("Choose another file")
                variant: "ghost"
                large: true
                onClicked: page.win.chooseUpdateFile()
            }
            Item { Layout.fillWidth: true }
            UiText {
                text: qsTr("Needs %1 · about 5 minutes").arg(Theme.bytes(page.upd.unpackedSize))
                variant: "small"
                tone: "muted"
            }
        }

        // Installing / done
        RowLayout {
            visible: page.state_ === "installing"
            Layout.fillWidth: true
            UiText { text: qsTr("Installing…"); font.weight: Font.DemiBold; Layout.fillWidth: true }
            UiText { text: Math.round((page.upd.progress || 0) * 100) + " %"; font.weight: Font.Bold; tone: "accent" }
        }
        DProgressBar {
            visible: page.state_ === "installing"
            Layout.fillWidth: true
            implicitHeight: 10
            value: page.upd.progress || 0
        }
        InfoBanner {
            visible: page.state_ === "done"
            Layout.fillWidth: true
            tone: "ok"
            title: qsTr("Ready to restart")
            text: qsTr("Restart to start using Asima OS %1. If it doesn't start properly, your computer goes back to %2 by itself.")
                  .arg(page.upd.version || "").arg(page.sys.version || "")
        }
        StepList {
            visible: page.state_ === "installing" || page.state_ === "done"
            Layout.fillWidth: true
            steps: page.upd.steps || []
        }
        InfoBanner {
            visible: page.state_ === "installing"
            Layout.fillWidth: true
            tone: "info"
            title: qsTr("You can keep working")
            text: qsTr("The update is written next to the running system. Please don't turn the computer off.")
        }
        RowLayout {
            visible: page.state_ === "done"
            spacing: 10
            DButton {
                text: qsTr("Restart now")
                variant: "primary"
                large: true
                iconName: "system-reboot"
                onClicked: Backend.restartNow()
            }
            DButton {
                text: qsTr("Later")
                variant: "ghost"
                large: true
                onClicked: Backend.clearUpdate()
            }
        }

        // Rejected
        InfoBanner {
            visible: page.state_ === "error"
            Layout.fillWidth: true
            tone: "error"
            title: page.upd.errorTitle || ""
            text: page.upd.errorText || ""
        }
        RowLayout {
            visible: page.state_ === "error"
            spacing: 10
            DButton {
                text: qsTr("Choose another file")
                variant: "primary"
                iconName: "document-open"
                onClicked: page.win.chooseUpdateFile()
            }
            DButton { text: qsTr("Dismiss"); variant: "ghost"; onClicked: Backend.clearUpdate() }
        }
    }
}
