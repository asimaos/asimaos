qt_add_executable(asimaos-rescue
    src/main.cpp
    src/rescueengine.h
    src/rescueengine.cpp
)

# The QML type registration (generated) includes the singletons' headers from src/.
target_include_directories(asimaos-rescue PRIVATE src)

qt_add_qml_module(asimaos-rescue
    URI AsimaRescue
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/AsimaRescue
    QML_FILES
        Main.qml
        ToolTile.qml
        HomePage.qml
        UnlockPage.qml
        DiskPage.qml
        RestorePage.qml
        ReinstallPage.qml
        KeysPage.qml
        ResetPage.qml
        WipePage.qml
        JobPage.qml
)

target_link_libraries(asimaos-rescue PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2
    asimaos_ui
    asimaos_uiplugin
)

# Ships only inside the Asima Rescue UKI on the ESP, next to asimaos-rescue-engine.
install(TARGETS asimaos-rescue RUNTIME DESTINATION bin)
