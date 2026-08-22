#!/usr/bin/env bash

set -euo pipefail

readonly cbindgen_version="0.29.4"
readonly binding_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_dir="$(cd "${binding_dir}/../.." && pwd)"
readonly header="${binding_dir}/include/ironpress.h"

if ! command -v cbindgen >/dev/null; then
    echo "cbindgen ${cbindgen_version} is required" >&2
    exit 1
fi

if [[ "$(cbindgen --version)" != "cbindgen ${cbindgen_version}" ]]; then
    echo "expected cbindgen ${cbindgen_version}, found $(cbindgen --version)" >&2
    exit 1
fi

cd "${repository_dir}"

arguments=(
    --config bindings/c/cbindgen.toml
    --crate ironpress-ffi
    --output "${header}"
)

if [[ "${1:-}" == "--check" ]]; then
    arguments=(--verify "${arguments[@]}")
fi

cbindgen "${arguments[@]}" bindings/c
