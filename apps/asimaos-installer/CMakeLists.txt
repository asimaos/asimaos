qt_add_executable(asimaos-installer
    src/main.cpp
    src/installerengine.h
    src/installerengine.cpp
)

# The QML type registration (generated) includes the singletons' headers from src/.
target_include_directories(asimaos-installer PRIVATE src)

qt_add_qml_module(asimaos-installer
    URI AsimaInstaller
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/AsimaInstaller
    QML_FILES
        Main.qml
        Bullet.qml
        WelcomePage.qml
        LanguagePage.qml
        DiskPage.qml
        AccountPage.qml
        EncryptionPage.qml
        SummaryPage.qml
        InstallPage.qml
        DonePage.qml
)

target_link_libraries(asimaos-installer PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2
    asimaos_ui
    asimaos_uiplugin
    asimaos_sysinfo
)

# Ships only on the live ISO (next to the asimaos-install engine), never in the installed /usr.
install(TARGETS asimaos-installer RUNTIME DESTINATION bin)
install(FILES io.github.asimaos.Installer.desktop DESTINATION share/applications)
