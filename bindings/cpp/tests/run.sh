#!/usr/bin/env bash

set -euo pipefail

readonly test_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_dir="$(cd "${test_dir}/../../.." && pwd)"
readonly profile="${IRONPRESS_PROFILE:-debug}"
readonly library_dir="${repository_dir}/target/${profile}"
readonly work_dir="$(mktemp -d)"
readonly host="$(uname -s)"
trap 'rm -rf "${work_dir}"' EXIT

cd "${repository_dir}"
build_arguments=(build -p ironpress-ffi)
if [[ "${profile}" == "release" ]]; then
    build_arguments+=(--release)
elif [[ "${profile}" != "debug" ]]; then
    echo "unsupported Rust build profile: ${profile}" >&2
    exit 1
fi
cargo "${build_arguments[@]}"

compile_static_smoke() {
    "${CXX:-c++}" \
        -std=c++17 \
        -g \
        -Wall \
        -Wextra \
        -Wpedantic \
        -Werror \
        -I bindings/c/include \
        -I bindings/cpp/include \
        bindings/cpp/tests/smoke.cpp \
        "${library_dir}/libironpress_ffi.a" \
        "$@" \
        -o "${work_dir}/ironpress-cpp-static-smoke"
}

if [[ "${host}" == "Linux" ]]; then
    compile_static_smoke -ldl -lpthread -lm
else
    compile_static_smoke
fi

"${CXX:-c++}" \
    -std=c++17 \
    -g \
    -Wall \
    -Wextra \
    -Wpedantic \
    -Werror \
    -I bindings/c/include \
    -I bindings/cpp/include \
    bindings/cpp/tests/smoke.cpp \
    -L "${library_dir}" \
    -lironpress_ffi \
    -o "${work_dir}/ironpress-cpp-smoke"

test_arguments=(
    tests/parity/fonts/ParitySans.ttf
    tests/fonts/NotoEmoji-TestSubset.ttf
)
"${work_dir}/ironpress-cpp-static-smoke" "${test_arguments[@]}"

case "${host}" in
    Darwin)
        DYLD_LIBRARY_PATH="${library_dir}" \
            "${work_dir}/ironpress-cpp-smoke" "${test_arguments[@]}"
        ;;
    Linux)
        if [[ "${IRONPRESS_VALGRIND:-0}" == "1" ]]; then
            command -v valgrind >/dev/null
            LD_LIBRARY_PATH="${library_dir}" valgrind \
                --leak-check=full \
                --track-origins=yes \
                --show-leak-kinds=definite \
                --errors-for-leak-kinds=definite \
                --error-exitcode=1 \
                "${work_dir}/ironpress-cpp-smoke" "${test_arguments[@]}"
        else
            LD_LIBRARY_PATH="${library_dir}" \
                "${work_dir}/ironpress-cpp-smoke" "${test_arguments[@]}"
        fi
        ;;
    *)
        echo "unsupported C++ smoke-test host: ${host}" >&2
        exit 1
        ;;
esac
