#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <platform-architecture>" >&2
    exit 1
fi

readonly test_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_dir="$(cd "${test_dir}/../../.." && pwd)"
readonly version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' \
    "${repository_dir}/bindings/c/Cargo.toml" | head -n 1)"
readonly bundle="ironpress-c-${version}-$1"
readonly archive="${repository_dir}/dist/c/${bundle}.tar.gz"
readonly work_dir="$(mktemp -d)"
readonly package_dir="${work_dir}/${bundle}"
readonly cmake_command="${CMAKE:-cmake}"
readonly pkg_config_command="${PKG_CONFIG:-pkg-config}"
trap 'rm -rf "${work_dir}"' EXIT

tar -xzf "${archive}" -C "${work_dir}"

run_with_shared_library() {
    local executable="$1"
    case "$(uname -s)" in
        Darwin)
            DYLD_LIBRARY_PATH="${package_dir}/lib" "${executable}"
            ;;
        Linux)
            LD_LIBRARY_PATH="${package_dir}/lib" "${executable}"
            ;;
        *)
            echo "unsupported package metadata host: $(uname -s)" >&2
            exit 1
            ;;
    esac
}

verify_cmake_consumer() {
    local language="$1"
    local target="$2"
    local linkage="$3"
    local source_dir="${test_dir}/package-consumers/${language}"
    local build_dir="${work_dir}/cmake-${language}-${linkage}"
    local executable="${build_dir}/ironpress_${language}_package_consumer"

    "${cmake_command}" \
        -S "${source_dir}" \
        -B "${build_dir}" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_PREFIX_PATH="${package_dir}" \
        -DIRONPRESS_TEST_VERSION="${version}" \
        -DIRONPRESS_TEST_TARGET="${target}"
    "${cmake_command}" --build "${build_dir}" --config Release

    if [[ "${linkage}" == "shared" ]]; then
        run_with_shared_library "${executable}"
    else
        "${executable}"
    fi
}

verify_cmake_consumer c Ironpress::C shared
verify_cmake_consumer c Ironpress::CStatic static
verify_cmake_consumer cpp Ironpress::CXX shared
verify_cmake_consumer cpp Ironpress::CXXStatic static

"${cmake_command}" \
    -S "${test_dir}/package-consumers/version" \
    -B "${work_dir}/cmake-compatible-version" \
    -DCMAKE_PREFIX_PATH="${package_dir}" \
    -DIRONPRESS_TEST_VERSION_RANGE="1.5...<2.0"
if "${cmake_command}" \
    -S "${test_dir}/package-consumers/version" \
    -B "${work_dir}/cmake-excluded-version" \
    -DCMAKE_PREFIX_PATH="${package_dir}" \
    -DIRONPRESS_TEST_VERSION_RANGE="1.5...<${version}"; then
    echo "excluded Ironpress version range was accepted" >&2
    exit 1
fi

export PKG_CONFIG_PATH="${package_dir}/lib/pkgconfig"
"${pkg_config_command}" --atleast-version="${version}" ironpress

"${CC:-cc}" \
    -std=c11 \
    -Wall \
    -Wextra \
    -Werror \
    "${test_dir}/package-consumers/c/main.c" \
    $("${pkg_config_command}" --cflags --libs ironpress) \
    -o "${work_dir}/pkg-config-shared"
run_with_shared_library "${work_dir}/pkg-config-shared"

"${CC:-cc}" \
    -std=c11 \
    -Wall \
    -Wextra \
    -Werror \
    "${test_dir}/package-consumers/c/main.c" \
    $("${pkg_config_command}" --cflags --libs --static ironpress) \
    -o "${work_dir}/pkg-config-static"
"${work_dir}/pkg-config-static"
