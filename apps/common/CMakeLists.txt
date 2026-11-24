# Plumbing shared by the Asima OS apps' engines: the client for their system services, power modes,
# readings of this device (battery, temperatures, controllers) and the sample devices of demo mode.
qt_add_library(asimaos_common STATIC
    util.h
    serviceclient.h serviceclient.cpp
    powerprofiles.h powerprofiles.cpp
    devicestate.h devicestate.cpp
)
target_include_directories(asimaos_common PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(asimaos_common PUBLIC Qt6::Core Qt6::DBus asimaos_sysinfo)
