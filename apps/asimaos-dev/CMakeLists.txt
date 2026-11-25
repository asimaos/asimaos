qt_add_executable(asimaos-dev
    src/main.cpp
    src/devengine.h
    src/devengine.cpp
)

# The QML type registration (generated) includes the singletons' headers from src/.
target_include_directories(asimaos-dev PRIVATE src)

qt_add_qml_module(asimaos-dev
    URI AsimaDev
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/AsimaDev
    QML_FILES
        Main.qml
        ContainersPage.qml
        MachinesPage.qml
        StoragePage.qml
)

target_link_libraries(asimaos-dev PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2 Qt6::DBus
    asimaos_ui
    asimaos_uiplugin
    asimaos_common
)

install(TARGETS asimaos-dev RUNTIME DESTINATION bin)
install(FILES io.github.asimaos.Dev.desktop DESTINATION share/applications)
