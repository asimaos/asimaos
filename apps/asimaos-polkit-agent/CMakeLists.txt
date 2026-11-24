# The Asima OS password dialog (D37): the session's polkit agent, with fingerprint and face buttons.
# It runs in place of KDE's agent (plasma-polkit-agent.service, drop-in in builder/rootfs/) and installs to
# /usr/lib/asimaos, the one path asimaos-faced gives administrator tokens to.
find_package(PolkitQt6-1 REQUIRED)

qt_add_executable(asimaos-polkit-agent
    src/main.cpp
    src/agent.h
    src/agent.cpp
)

# The QML type registration (generated) includes the singletons' headers from src/.
target_include_directories(asimaos-polkit-agent PRIVATE src)

qt_add_qml_module(asimaos-polkit-agent
    URI AsimaPolkitAgent
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/AsimaPolkitAgent
    QML_FILES
        Main.qml
)

target_link_libraries(asimaos-polkit-agent PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2 Qt6::DBus
    PolkitQt6-1::Agent PolkitQt6-1::Core
    asimaos_ui
    asimaos_uiplugin
)

install(TARGETS asimaos-polkit-agent RUNTIME DESTINATION lib/asimaos)
