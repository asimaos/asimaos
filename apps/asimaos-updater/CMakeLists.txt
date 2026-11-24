qt_add_executable(asimaos-updater
    src/main.cpp
    src/updaterengine.h
    src/updaterengine.cpp
)

# The QML type registration (generated) includes the singletons' headers from src/.
target_include_directories(asimaos-updater PRIVATE src)

qt_add_qml_module(asimaos-updater
    URI AsimaUpdater
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/AsimaUpdater
    QML_FILES
        Main.qml
        UpdatesPage.qml
        VersionsPage.qml
)

target_link_libraries(asimaos-updater PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2 Qt6::DBus
    asimaos_ui
    asimaos_uiplugin
    asimaos_common
)

install(TARGETS asimaos-updater RUNTIME DESTINATION bin)
install(FILES io.github.asimaos.Updater.desktop DESTINATION share/applications)
