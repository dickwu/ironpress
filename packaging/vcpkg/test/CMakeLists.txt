cmake_minimum_required(VERSION 3.15)

project(ironpress_vcpkg_consumers LANGUAGES C CXX)

find_package(Ironpress 1.6.0 EXACT CONFIG REQUIRED)

add_executable(
    ironpress_c_consumer
    ../../../bindings/c/tests/package-consumers/c/main.c
)
target_compile_features(ironpress_c_consumer PRIVATE c_std_11)
target_link_libraries(ironpress_c_consumer PRIVATE Ironpress::C)

add_executable(
    ironpress_cpp_consumer
    ../../../bindings/c/tests/package-consumers/cpp/main.cpp
)
target_link_libraries(ironpress_cpp_consumer PRIVATE Ironpress::CXX)
