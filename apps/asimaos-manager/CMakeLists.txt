qt_add_executable(asimaos-manager
    src/main.cpp
    src/managerengine.h
    src/managerengine.cpp
    src/fingerprint.h
    src/fingerprint.cpp
    src/faceunlock.h
    src/faceunlock.cpp
)

# The QML type registration (generated) includes the singletons' headers from src/.
target_include_directories(asimaos-manager PRIVATE src)

qt_add_qml_module(asimaos-manager
    URI AsimaManager
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/AsimaManager
    QML_FILES
        Main.qml
        DeviceArt.qml
        FaceScan.qml
        HomePage.qml
        PowerPage.qml
        DisplayPage.qml
        BiometricsPage.qml
        HardwarePage.qml
        ShieldPage.qml
)

target_link_libraries(asimaos-manager PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2 Qt6::DBus
    asimaos_ui
    asimaos_uiplugin
    asimaos_common
)

install(TARGETS asimaos-manager RUNTIME DESTINATION bin)
install(FILES io.github.asimaos.Manager.desktop DESTINATION share/applications)
