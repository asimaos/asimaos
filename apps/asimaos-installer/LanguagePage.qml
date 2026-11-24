import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as B
import Asima.Ui

WizardPage {
    id: page
    readonly property var win: B.ApplicationWindow.window

    title: qsTr("Language and keyboard")
    subtitle: qsTr("Choose the language for Asima OS, how your keyboard is laid out, and where you are.")
    canContinue: page.win.language.length > 0
    onNext: win.next()
    onBack: win.back()

    GridLayout {
        Layout.fillWidth: true
        columns: page.contentWidth > 760 ? 2 : 1
        columnSpacing: 20
        rowSpacing: 20

        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            title: qsTr("Language")
            icon: "globe"

            OptionList {
                Layout.fillWidth: true
                model: Backend.languages
                currentId: page.win.language
                placeholder: qsTr("Search languages")
                listHeight: 342
                onPicked: id => page.win.setLanguage(id)
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            spacing: 20

            Card {
                Layout.fillWidth: true
                title: qsTr("Keyboard")
                icon: "input-keyboard"

                DComboBox {
                    id: keyboardBox
                    Layout.fillWidth: true
                    model: Backend.keyboards
                    Component.onCompleted: currentIndex = indexOfValue(page.win.keyboard)
                    onActivated: {
                        page.win.keyboard = currentValue
                        page.win.keyboardTouched = true
                    }
                    Connections {
                        target: page.win
                        function onKeyboardChanged() { keyboardBox.currentIndex = keyboardBox.indexOfValue(page.win.keyboard) }
                    }
                }
                DTextField {
                    Layout.fillWidth: true
                    placeholderText: qsTr("Type here to test your keyboard")
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Time zone")
                icon: "clock"

                OptionList {
                    Layout.fillWidth: true
                    model: Backend.timezones
                    currentId: page.win.timezone
                    placeholder: qsTr("Search a city or country")
                    listHeight: 150
                    onPicked: id => {
                        page.win.timezone = id
                        page.win.timezoneTouched = true
                    }
                }
                UiText {
                    text: qsTr("%1: it's %2 there now").arg(page.win.timezoneInfo().title).arg(Backend.localTime(page.win.timezone))
                    variant: "small"
                    tone: "muted"
                }
            }
        }
    }
}
