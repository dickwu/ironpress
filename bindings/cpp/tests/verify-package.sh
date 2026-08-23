#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <platform-architecture>" >&2
    exit 1
fi

readonly test_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_dir="$(cd "${test_dir}/../../.." && pwd)"
readonly platform="$1"
readonly version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' \
    "${repository_dir}/bindings/c/Cargo.toml" | head -n 1)"
readonly bundle="ironpress-c-${version}-${platform}"
readonly archive="${repository_dir}/dist/c/${bundle}.tar.gz"
readonly work_dir="$(mktemp -d)"
readonly package_dir="${work_dir}/${bundle}"
readonly host="$(uname -s)"
trap 'rm -rf "${work_dir}"' EXIT

tar -xzf "${archive}" -C "${work_dir}"

compile_static_smoke() {
    "${CXX:-c++}" \
        -std=c++17 \
        -Wall \
        -Wextra \
        -Wpedantic \
        -Werror \
        -I "${package_dir}/include" \
        "${repository_dir}/bindings/cpp/tests/smoke.cpp" \
        "${package_dir}/lib/libironpress_ffi.a" \
        "$@" \
        -o "${work_dir}/ironpress-cpp-package-static-smoke"
}

if [[ "${host}" == "Linux" ]]; then
    compile_static_smoke -ldl -lpthread -lm
else
    compile_static_smoke
fi

"${CXX:-c++}" \
    -std=c++17 \
    -Wall \
    -Wextra \
    -Wpedantic \
    -Werror \
    -I "${package_dir}/include" \
    "${repository_dir}/bindings/cpp/tests/smoke.cpp" \
    -L "${package_dir}/lib" \
    -lironpress_ffi \
    -o "${work_dir}/ironpress-cpp-package-smoke"

test_arguments=(
    "${repository_dir}/tests/parity/fonts/ParitySans.ttf"
    "${repository_dir}/tests/fonts/NotoEmoji-TestSubset.ttf"
)
"${work_dir}/ironpress-cpp-package-static-smoke" "${test_arguments[@]}"

case "${host}" in
    Darwin)
        DYLD_LIBRARY_PATH="${package_dir}/lib" \
            "${work_dir}/ironpress-cpp-package-smoke" "${test_arguments[@]}"
        ;;
    Linux)
        LD_LIBRARY_PATH="${package_dir}/lib" \
            "${work_dir}/ironpress-cpp-package-smoke" "${test_arguments[@]}"
        ;;
    *)
        echo "unsupported C++ package-test host: ${host}" >&2
        exit 1
        ;;
esac
