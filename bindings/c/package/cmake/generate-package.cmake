cmake_minimum_required(VERSION 3.19)

foreach(
    required_variable
    IRONPRESS_PACKAGE_SOURCE_DIR
    IRONPRESS_PACKAGE_OUTPUT_DIR
    IRONPRESS_PACKAGE_ROOT
    IRONPRESS_VERSION
)
    if(NOT DEFINED ${required_variable})
        message(FATAL_ERROR "${required_variable} is required")
    endif()
endforeach()

include(CMakePackageConfigHelpers)

set(IRONPRESS_INCLUDE_INSTALL_DIR include)
set(IRONPRESS_LIBRARY_INSTALL_DIR lib)

configure_package_config_file(
    "${IRONPRESS_PACKAGE_SOURCE_DIR}/IronpressConfig.cmake.in"
    "${IRONPRESS_PACKAGE_OUTPUT_DIR}/IronpressConfig.cmake"
    INSTALL_DESTINATION lib/cmake/Ironpress
    INSTALL_PREFIX "${IRONPRESS_PACKAGE_ROOT}"
    PATH_VARS
        IRONPRESS_INCLUDE_INSTALL_DIR
        IRONPRESS_LIBRARY_INSTALL_DIR
)
write_basic_package_version_file(
    "${IRONPRESS_PACKAGE_OUTPUT_DIR}/IronpressConfigVersion.cmake"
    VERSION "${IRONPRESS_VERSION}"
    COMPATIBILITY SameMajorVersion
)
