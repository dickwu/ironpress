cmake_minimum_required(VERSION 3.15)

project(ironpress_c_package_consumer LANGUAGES C)

find_package(Ironpress ${IRONPRESS_TEST_VERSION} EXACT CONFIG REQUIRED)

add_executable(ironpress_c_package_consumer main.c)
target_compile_features(ironpress_c_package_consumer PRIVATE c_std_11)
target_link_libraries(
    ironpress_c_package_consumer
    PRIVATE "${IRONPRESS_TEST_TARGET}"
)
