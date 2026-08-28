#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <vcpkg-root> <triplet>" >&2
    exit 1
fi

readonly test_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_dir="$(cd "${test_dir}/../../.." && pwd)"
readonly vcpkg_root="$(cd "$1" && pwd)"
readonly triplet="$2"
readonly work_dir="$(mktemp -d)"
readonly installed_dir="${work_dir}/installed"
readonly build_dir="${work_dir}/build"
trap 'rm -rf "${work_dir}"' EXIT

"${vcpkg_root}/vcpkg" install \
    --x-manifest-root="${test_dir}" \
    --x-install-root="${installed_dir}" \
    --overlay-ports="${repository_dir}/packaging/vcpkg/ports" \
    --triplet="${triplet}"

"${CMAKE:-cmake}" \
    -S "${test_dir}" \
    -B "${build_dir}" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_TOOLCHAIN_FILE="${vcpkg_root}/scripts/buildsystems/vcpkg.cmake" \
    -DVCPKG_INSTALLED_DIR="${installed_dir}" \
    -DVCPKG_MANIFEST_MODE=OFF \
    -DVCPKG_TARGET_TRIPLET="${triplet}"
"${CMAKE:-cmake}" --build "${build_dir}" --config Release

case "$(uname -s)" in
    Darwin)
        DYLD_LIBRARY_PATH="${installed_dir}/${triplet}/lib" \
            "${build_dir}/ironpress_c_consumer"
        DYLD_LIBRARY_PATH="${installed_dir}/${triplet}/lib" \
            "${build_dir}/ironpress_cpp_consumer"
        ;;
    Linux)
        LD_LIBRARY_PATH="${installed_dir}/${triplet}/lib" \
            "${build_dir}/ironpress_c_consumer"
        LD_LIBRARY_PATH="${installed_dir}/${triplet}/lib" \
            "${build_dir}/ironpress_cpp_consumer"
        ;;
    *)
        echo "unsupported vcpkg test host: $(uname -s)" >&2
        exit 1
        ;;
esac
