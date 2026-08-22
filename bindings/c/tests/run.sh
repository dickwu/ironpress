#!/usr/bin/env bash

set -euo pipefail

readonly test_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_dir="$(cd "${test_dir}/../../.." && pwd)"
readonly library_dir="${repository_dir}/target/debug"
readonly work_dir="$(mktemp -d)"
trap 'rm -rf "${work_dir}"' EXIT

cd "${repository_dir}"
cargo build -p ironpress-ffi

"${CC:-cc}" \
    -std=c11 \
    -Wall \
    -Wextra \
    -Werror \
    -I bindings/c/include \
    bindings/c/tests/smoke.c \
    -L "${library_dir}" \
    -lironpress_ffi \
    -o "${work_dir}/ironpress-c-smoke"

test_command=(
    "${work_dir}/ironpress-c-smoke"
    tests/parity/fonts/ParitySans.ttf
    tests/fonts/NotoEmoji-TestSubset.ttf
)

case "$(uname -s)" in
    Darwin)
        DYLD_LIBRARY_PATH="${library_dir}" "${test_command[@]}"
        ;;
    Linux)
        if [[ "${IRONPRESS_VALGRIND:-0}" == "1" ]]; then
            command -v valgrind >/dev/null
            LD_LIBRARY_PATH="${library_dir}" valgrind \
                --leak-check=full \
                --show-leak-kinds=definite \
                --errors-for-leak-kinds=definite \
                --error-exitcode=1 \
                "${test_command[@]}"
        else
            LD_LIBRARY_PATH="${library_dir}" "${test_command[@]}"
        fi
        ;;
    *)
        echo "unsupported C smoke-test host: $(uname -s)" >&2
        exit 1
        ;;
esac
