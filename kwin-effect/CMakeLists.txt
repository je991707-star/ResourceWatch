# KWin effect that blurs behind ResourceWatch windows only. Must be built
# against the exact KWin version that will load it (see build.sh).
cmake_minimum_required(VERSION 3.20)
project(resourcewatch_blur LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(ECM REQUIRED NO_MODULE)
set(CMAKE_MODULE_PATH ${ECM_MODULE_PATH})
include(KDEInstallDirs)
include(KDECMakeSettings)
include(KDECompilerSettings NO_POLICY_SCOPE)

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
# The factory macro hides Q_OBJECT from automoc.
list(APPEND CMAKE_AUTOMOC_MACRO_NAMES KWIN_EFFECT_FACTORY_SUPPORTED)
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)

find_package(Qt6 REQUIRED COMPONENTS Core Gui DBus Quick Widgets) # DBus/Quick/Widgets: KWin::kwin links them
find_package(KF6 REQUIRED COMPONENTS CoreAddons)
find_package(KWin REQUIRED COMPONENTS kwin)

kcoreaddons_add_plugin(resourcewatch_blur INSTALL_NAMESPACE "kwin/effects/plugins"
    SOURCES blur.cpp blur.qrc main.cpp)
target_link_libraries(resourcewatch_blur PRIVATE KWin::kwin Qt6::Gui)
