pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Asima.Ui

// An app file opened with Asima App Center: what it is, before anything is installed.
DPage {
    id: page
    readonly property var f: Backend.file
    title: page.f.title || qsTr("App file")
    subtitle: page.f.name || ""

    Card {
        Layout.fillWidth: true
        title: page.f.name || ""
        subtitle: page.f.detail || ""
        icon: page.f.kind === "android" ? "smartphone" : page.f.kind === "windows" ? "wine" : "package-x-generic"
        tone: page.f.ok ? "accent" : "warn"
        ListRow {
            Layout.fillWidth: true
            title: qsTr("Where it is")
            subtitle: page.f.path || ""
            value: Theme.bytes(page.f.size)
            separator: false
        }
        actions: [ DButton { visible: !!page.f.ok; text: qsTr("Install"); variant: "primary"; iconName: "run-install"; onClicked: Backend.installFile() } ]
    }

    InfoBanner {
        visible: !!page.f.problem
        Layout.fillWidth: true
        tone: "warn"
        title: qsTr("This file can't be installed")
        text: page.f.problem || ""
    }
}
