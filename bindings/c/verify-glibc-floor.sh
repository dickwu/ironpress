#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <shared-library> <maximum-glibc-version>" >&2
    exit 1
fi

readonly library="$1"
readonly maximum="$2"
readonly required_versions="$(
    readelf --version-info "${library}" | awk '
        {
            while (match($0, /GLIBC_[0-9]+(\.[0-9]+)+/)) {
                print substr($0, RSTART + 6, RLENGTH - 6)
                $0 = substr($0, RSTART + RLENGTH)
            }
        }
    ' | sort -Vu
)"
readonly highest="$(printf '%s\n' "${required_versions}" | tail -n 1)"

if [[ -z "${highest}" ]]; then
    echo "${library} does not declare a readable glibc requirement" >&2
    exit 1
fi

if [[ "$(printf '%s\n' "${maximum}" "${highest}" | sort -V | tail -n 1)" != "${maximum}" ]]; then
    echo "${library} requires GLIBC_${highest}, above the ${maximum} compatibility floor" >&2
    exit 1
fi

echo "${library} requires at most GLIBC_${highest}"
