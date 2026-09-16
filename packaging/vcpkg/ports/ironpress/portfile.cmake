vcpkg_from_github(
    OUT_SOURCE_PATH SOURCE_PATH
    REPO gastongouron/ironpress
    REF "v${VERSION}"
    SHA512 a0342384cc530fd2fa1bb470a53df097d8bbb49e3faa21dee3eb0cba0c8db27d2bb4e6db70e169d4e61245061b5cded19fc443d160a0f611a6b226b12751c891
    HEAD_REF main
)

vcpkg_download_distfile(
    CARGO_LOCK
    URLS "https://raw.githubusercontent.com/gastongouron/ironpress/0266ae633b064832c375add242200ade1e21bd37/Cargo.lock"
    FILENAME "ironpress-${VERSION}-Cargo.lock"
    SHA512 c2ed6d40ff9e3c87e16cd5daec68f00b6a29b64dd47fa08d448af6a6ef673ee52e290dca7294bffe13b8be793c3ce1c9c9c0c1f771d5c7ca28d25719250b287e
)
configure_file("${CARGO_LOCK}" "${SOURCE_PATH}/Cargo.lock" COPYONLY)

find_program(
    CARGO
    NAMES cargo cargo.exe
    HINTS
        "$ENV{CARGO_HOME}/bin"
        "$ENV{USERPROFILE}/.cargo/bin"
        "$ENV{HOME}/.cargo/bin"
    REQUIRED
)

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

function(ironpress_build profile cargo_profile)
    set(target_dir "${CURRENT_BUILDTREES_DIR}/${profile}")
    set(profile_argument)
    if(cargo_profile STREQUAL "release")
        set(profile_argument --release)
    endif()
    vcpkg_execute_required_process(
        COMMAND
            "${CARGO}" build
            --locked
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
