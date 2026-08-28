vcpkg_from_github(
    OUT_SOURCE_PATH SOURCE_PATH
    REPO gastongouron/ironpress
    REF "v${VERSION}"
    SHA512 216d4a9622908a654e093b5803fccc846d2f8e29caaee4c750e04c0096603b0b78537eac9e4eb91c38ea6b6a0f25681f9d96fb57932dd28870d87226e5fa9a44
    HEAD_REF main
)

find_program(CARGO cargo REQUIRED)

string(REGEX MATCH "^[^-]+" HOST_ARCHITECTURE "${HOST_TRIPLET}")
if(NOT HOST_ARCHITECTURE STREQUAL VCPKG_TARGET_ARCHITECTURE)
    message(FATAL_ERROR "Ironpress requires a native Rust toolchain for ${TARGET_TRIPLET}.")
endif()
if(VCPKG_TARGET_IS_WINDOWS AND NOT VCPKG_HOST_IS_WINDOWS)
    message(FATAL_ERROR "Ironpress cannot cross-compile from ${HOST_TRIPLET} to ${TARGET_TRIPLET}.")
elseif(VCPKG_TARGET_IS_OSX AND NOT VCPKG_HOST_IS_OSX)
    message(FATAL_ERROR "Ironpress cannot cross-compile from ${HOST_TRIPLET} to ${TARGET_TRIPLET}.")
elseif(VCPKG_TARGET_IS_LINUX AND NOT VCPKG_HOST_IS_LINUX)
    message(FATAL_ERROR "Ironpress cannot cross-compile from ${HOST_TRIPLET} to ${TARGET_TRIPLET}.")
endif()

set(CARGO_LOCKED)
if(EXISTS "${SOURCE_PATH}/Cargo.lock")
    set(CARGO_LOCKED --locked)
endif()

function(ironpress_build profile cargo_profile)
    set(target_dir "${CURRENT_BUILDTREES_DIR}/${profile}")
    set(profile_argument)
    if(cargo_profile STREQUAL "release")
        set(profile_argument --release)
    endif()
    vcpkg_execute_required_process(
        COMMAND
            "${CARGO}" build
            ${CARGO_LOCKED}
            --manifest-path "${SOURCE_PATH}/Cargo.toml"
            --package ironpress-ffi
            --target-dir "${target_dir}"
            ${profile_argument}
        WORKING_DIRECTORY "${SOURCE_PATH}"
        LOGNAME "cargo-build-${profile}"
    )
endfunction()

if(NOT DEFINED VCPKG_BUILD_TYPE OR VCPKG_BUILD_TYPE STREQUAL "release")
    ironpress_build(release release)
endif()
if(NOT DEFINED VCPKG_BUILD_TYPE OR VCPKG_BUILD_TYPE STREQUAL "debug")
    ironpress_build(debug debug)
endif()

file(INSTALL "${SOURCE_PATH}/bindings/c/include/ironpress.h"
    DESTINATION "${CURRENT_PACKAGES_DIR}/include")
file(INSTALL "${SOURCE_PATH}/bindings/cpp/include/ironpress.hpp"
    DESTINATION "${CURRENT_PACKAGES_DIR}/include")
file(INSTALL "${SOURCE_PATH}/bindings/cpp/include/ironpress"
    DESTINATION "${CURRENT_PACKAGES_DIR}/include")

function(ironpress_install_library profile destination)
    set(source_dir "${CURRENT_BUILDTREES_DIR}/${profile}/${profile}")
    if(VCPKG_LIBRARY_LINKAGE STREQUAL "dynamic")
        if(VCPKG_TARGET_IS_WINDOWS)
            file(INSTALL "${source_dir}/ironpress_ffi.dll"
                DESTINATION "${CURRENT_PACKAGES_DIR}/${destination}bin")
            file(INSTALL "${source_dir}/ironpress_ffi.dll.lib"
                DESTINATION "${CURRENT_PACKAGES_DIR}/${destination}lib")
        elseif(VCPKG_TARGET_IS_OSX)
            file(INSTALL "${source_dir}/libironpress_ffi.dylib"
                DESTINATION "${CURRENT_PACKAGES_DIR}/${destination}lib")
        else()
            file(INSTALL "${source_dir}/libironpress_ffi.so"
                DESTINATION "${CURRENT_PACKAGES_DIR}/${destination}lib")
        endif()
    elseif(VCPKG_TARGET_IS_WINDOWS)
        file(INSTALL "${source_dir}/ironpress_ffi.lib"
            DESTINATION "${CURRENT_PACKAGES_DIR}/${destination}lib")
    else()
        file(INSTALL "${source_dir}/libironpress_ffi.a"
            DESTINATION "${CURRENT_PACKAGES_DIR}/${destination}lib")
    endif()
endfunction()

if(NOT DEFINED VCPKG_BUILD_TYPE OR VCPKG_BUILD_TYPE STREQUAL "release")
    ironpress_install_library(release "")
endif()
if(NOT DEFINED VCPKG_BUILD_TYPE OR VCPKG_BUILD_TYPE STREQUAL "debug")
    ironpress_install_library(debug "debug/")
endif()

if(VCPKG_LIBRARY_LINKAGE STREQUAL "dynamic")
    set(IRONPRESS_LIBRARY_TYPE SHARED)
else()
    set(IRONPRESS_LIBRARY_TYPE STATIC)
endif()
configure_file(
    "${CMAKE_CURRENT_LIST_DIR}/IronpressConfig.cmake.in"
    "${CURRENT_PACKAGES_DIR}/share/ironpress/IronpressConfig.cmake"
    @ONLY
)
include(CMakePackageConfigHelpers)
write_basic_package_version_file(
    "${CURRENT_PACKAGES_DIR}/share/ironpress/IronpressConfigVersion.cmake"
    VERSION "${VERSION}"
    COMPATIBILITY SameMajorVersion
)
file(INSTALL "${CMAKE_CURRENT_LIST_DIR}/usage"
    DESTINATION "${CURRENT_PACKAGES_DIR}/share/ironpress")
vcpkg_install_copyright(FILE_LIST "${SOURCE_PATH}/LICENSE")
