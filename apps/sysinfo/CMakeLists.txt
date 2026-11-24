# Small library that reads hardware facts from sysfs/procfs (no root needed). Shared by Asima OS apps.
qt_add_library(asimaos_sysinfo STATIC
    sysinfo.h
    sysinfo.cpp
)
target_include_directories(asimaos_sysinfo PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(asimaos_sysinfo PUBLIC Qt6::Core)
