qt_add_executable(asimaos-gaming
    src/main.cpp
    src/gamingengine.h
    src/gamingengine.cpp
    src/controllertester.h
    src/controllertester.cpp
)

# The QML type registration (generated) includes the singletons' headers from src/.
target_include_directories(asimaos-gaming PRIVATE src)

qt_add_qml_module(asimaos-gaming
    URI AsimaGaming
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/AsimaGaming
    QML_FILES
        Main.qml
        ControllerVisual.qml
        GamingPage.qml
        ControllersPage.qml
        HandheldPage.qml
        LightingPage.qml
)

target_link_libraries(asimaos-gaming PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2 Qt6::DBus
    asimaos_ui
    asimaos_uiplugin
    asimaos_common
)

install(TARGETS asimaos-gaming RUNTIME DESTINATION bin)
install(FILES io.github.asimaos.Gaming.desktop DESTINATION share/applications)
