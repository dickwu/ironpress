#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <platform-architecture>" >&2
    exit 1
fi

readonly binding_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_dir="$(cd "${binding_dir}/../.." && pwd)"
readonly platform="$1"
readonly version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "${binding_dir}/Cargo.toml" | head -n 1)"
readonly bundle="ironpress-c-${version}-${platform}"
readonly work_dir="$(mktemp -d)"
readonly bundle_dir="${work_dir}/${bundle}"
readonly output_dir="${repository_dir}/dist/c"
trap 'rm -rf "${work_dir}"' EXIT

mkdir -p "${bundle_dir}/include" "${bundle_dir}/lib" "${output_dir}"
cp "${binding_dir}/include/ironpress.h" "${bundle_dir}/include/"
cp "${binding_dir}/ABI.md" "${binding_dir}/README.md" "${repository_dir}/LICENSE" \
    "${bundle_dir}/"
cp "${repository_dir}/target/release/libironpress_ffi.a" "${bundle_dir}/lib/"

case "${platform}" in
    linux-*)
        cp "${repository_dir}/target/release/libironpress_ffi.so" "${bundle_dir}/lib/"
        ;;
    macos-*)
        cp "${repository_dir}/target/release/libironpress_ffi.dylib" "${bundle_dir}/lib/"
        ;;
    *)
        echo "unsupported Unix artifact platform: ${platform}" >&2
        exit 1
        ;;
esac

tar -C "${work_dir}" -czf "${output_dir}/${bundle}.tar.gz" "${bundle}"
