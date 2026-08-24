cmake_minimum_required(VERSION 3.19)

project(ironpress_package_version_consumer LANGUAGES C)

find_package(Ironpress ${IRONPRESS_TEST_VERSION_RANGE} CONFIG REQUIRED)
