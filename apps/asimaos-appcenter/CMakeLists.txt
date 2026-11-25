qt_add_executable(asimaos-appcenter
    src/main.cpp
    src/appcenterengine.h
    src/appcenterengine.cpp
    src/snapd.h
    src/snapd.cpp
)

# The QML type registration (generated) includes the singletons' headers from src/.
target_include_directories(asimaos-appcenter PRIVATE src)

qt_add_qml_module(asimaos-appcenter
    URI AsimaAppCenter
    VERSION 1.0
    OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/AsimaAppCenter
    QML_FILES
        Main.qml
        StorePage.qml
        KindRow.qml
        AppCard.qml
        InstalledPage.qml
        UpdatesPage.qml
        FilePage.qml
)

# The signed feed's app groups are checked with OpenSSL (Ed25519), as Asima Hub does.
find_package(OpenSSL REQUIRED COMPONENTS Crypto)

target_link_libraries(asimaos-appcenter PRIVATE
    Qt6::Gui Qt6::Quick Qt6::QuickControls2 Qt6::DBus Qt6::Network
    OpenSSL::Crypto
    asimaos_ui
    asimaos_uiplugin
    asimaos_common
)

install(TARGETS asimaos-appcenter RUNTIME DESTINATION bin)
install(FILES io.github.asimaos.AppCenter.desktop DESTINATION share/applications)
