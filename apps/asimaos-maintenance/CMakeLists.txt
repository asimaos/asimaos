qt_add_executable(asimaos-maintenance
    src/main.cpp
    src/maintenanceengine.h
    src/maintenanceengine.cpp
)

# The QML type registration (generated) includes the singletons' headers from src/.
target_include_directories(asimaos-maintenance PRIVATE src)

qt_add_qml_module(asimaos-maintenance
    URI AsimaMaintenance
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/AsimaMaintenance
    QML_FILES
        Main.qml
        OverviewPage.qml
        HealthPage.qml
        SecurityPage.qml
        BackupsPage.qml
        ChangesPage.qml
        ResetPage.qml
)

target_link_libraries(asimaos-maintenance PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2 Qt6::DBus
    asimaos_ui
    asimaos_uiplugin
    asimaos_common
)

install(TARGETS asimaos-maintenance RUNTIME DESTINATION bin)
install(FILES io.github.asimaos.Maintenance.desktop DESTINATION share/applications)
