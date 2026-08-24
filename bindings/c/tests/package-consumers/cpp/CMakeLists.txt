cmake_minimum_required(VERSION 3.15)

project(ironpress_cpp_package_consumer LANGUAGES CXX)

find_package(Ironpress ${IRONPRESS_TEST_VERSION} EXACT CONFIG REQUIRED)

add_executable(ironpress_cpp_package_consumer main.cpp)
target_link_libraries(
    ironpress_cpp_package_consumer
    PRIVATE "${IRONPRESS_TEST_TARGET}"
)
