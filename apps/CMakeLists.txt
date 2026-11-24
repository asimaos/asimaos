cmake_minimum_required(VERSION 3.24)
project(asimaos-apps VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Quick QuickControls2 Svg DBus Network LinguistTools)
# Translations: the installer's languages, bn hi ja, for every app.
qt_standard_project_setup(REQUIRES 6.8 I18N_SOURCE_LANGUAGE en I18N_TRANSLATED_LANGUAGES bn hi ja)

# Every QML module is built under one folder, which qmllint and the QML compiler then use as an
# import path (apps/build.sh qmllint), so the apps' imports of Asima.Ui resolve.
set(QT_QML_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml)

# Shared design system (QML module "Asima.Ui"), hardware facts and the engines' plumbing (service
# client, power modes, device readings), linked statically into the apps that use them.
add_subdirectory(ui)
add_subdirectory(sysinfo)
add_subdirectory(common)

# System apps, one job each (apps/DESIGN.md), so each can be finished and left alone on its own
add_subdirectory(asimaos-updater)
add_subdirectory(asimaos-maintenance)
add_subdirectory(asimaos-manager)
add_subdirectory(asimaos-gaming)
add_subdirectory(asimaos-dev)
# The password dialog for administrator tasks, with fingerprint and face buttons (D37)
add_subdirectory(asimaos-polkit-agent)
# Installing and repairing
add_subdirectory(asimaos-installer)
add_subdirectory(asimaos-rescue)

# Translations: one set of files for every app, apps/i18n/asimaos_<lang>.ts, built into
# each app (":/i18n"). `cmake --build build/apps --target update_translations` collects new strings.
qt_add_translations(TARGETS asimaos-updater asimaos-maintenance asimaos-manager asimaos-gaming asimaos-dev
                            asimaos-polkit-agent asimaos-installer asimaos-rescue
                    TS_FILE_BASE asimaos TS_FILE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/i18n RESOURCE_PREFIX /i18n)
