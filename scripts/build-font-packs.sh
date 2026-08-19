#!/usr/bin/env bash
set -euo pipefail

readonly script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repository_dir="$(cd "${script_dir}/.." && pwd)"
readonly output_dir="${1:-${repository_dir}/dist/font-packs}"
readonly fonttools_version="4.59.2"
readonly work_dir="$(mktemp -d)"

cleanup() {
  rm -rf "${work_dir}"
}
trap cleanup EXIT

font_hash() {
  shasum -a 256 "$1" | awk '{print $1}'
}

require_hash() {
  local file="$1"
  local expected="$2"
  local actual
  actual="$(font_hash "${file}")"
  if [[ "${actual}" != "${expected}" ]]; then
    echo "sha256 mismatch for ${file}: expected ${expected}, got ${actual}" >&2
    return 1
  fi
}

mkdir -p "${output_dir}"

while IFS='|' read -r pack artifact transform source_hash artifact_hash source_url license; do
  [[ -z "${pack}" || "${pack}" == \#* ]] && continue

  source_font="${work_dir}/${pack}-source.ttf"
  output_font="${output_dir}/${artifact}"
  curl --fail --location --retry 3 --silent --show-error "${source_url}" \
    --output "${source_font}"
  require_hash "${source_font}" "${source_hash}"

  case "${transform}" in
    copy)
      cp "${source_font}" "${output_font}"
      ;;
    static-regular)
      uvx --from "fonttools==${fonttools_version}" fonttools varLib.instancer \
        "${source_font}" wght=400 --output "${output_font}" --static \
        --update-name-table --no-recalc-timestamp
      ;;
    *)
      echo "unknown font-pack transform: ${transform}" >&2
      exit 1
      ;;
  esac

  require_hash "${output_font}" "${artifact_hash}"
  cp "${repository_dir}/${license}" \
    "${output_dir}/${artifact%.ttf}.LICENSE.txt"
done < "${repository_dir}/font-packs/sources.lock"

cp "${repository_dir}/font-packs/sources.lock" "${output_dir}/sources.lock"
