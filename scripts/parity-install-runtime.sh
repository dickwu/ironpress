#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RUNTIME_CACHE_DIR="${IRONPRESS_PARITY_RUNTIME_CACHE:-$HOME/.cache/ironpress/parity-runtime}"
POPPLER_DIR="$RUNTIME_CACHE_DIR/poppler-24.08.0"
FONT_DIR="$RUNTIME_CACHE_DIR/ubuntu-noble-fonts"

readonly -a CURL_OPTIONS=(
  --fail
  --location
  --retry 3
  --retry-all-errors
  --connect-timeout 15
  --max-time 180
  --silent
  --show-error
)

download() {
  local url="$1"
  local destination="$2"
  local expected_hash="$3"
  local partial="${destination}.part"

  if [[ -f "$destination" ]] && printf '%s  %s\n' "$expected_hash" "$destination" |
    sha256sum --check --strict --status; then
    return
  fi

  curl "${CURL_OPTIONS[@]}" --output "$partial" "$url"
  printf '%s  %s\n' "$expected_hash" "$partial" |
    sha256sum --check --strict
  mv "$partial" "$destination"
}

# Every report-producing job must use the rasterizer authenticated by the
# committed parity baseline. Package-manager defaults are not deterministic.
if [[ "$(dpkg --print-architecture)" != "amd64" ]]; then
  echo "parity runtime requires Ubuntu 24.04 amd64" >&2
  exit 1
fi

mkdir -p "$POPPLER_DIR" "$FONT_DIR"
download \
  https://archive.neon.kde.org/user/pool/main/l/lcms2/liblcms2-2_2.16-2+24.04+noble+release+build2_amd64.deb \
  "$POPPLER_DIR/liblcms2-2.deb" \
  369ab216d40364743188a3df30b3a86285aede504ddde89eea9b1bab8dbcbda5
download \
  https://archive.neon.kde.org/user/pool/main/p/poppler/libpoppler140_24.08.0-1+24.04+noble+release+build15_amd64.deb \
  "$POPPLER_DIR/libpoppler140.deb" \
  189bb9e6c22fa0f49f4ee8e802f62324a366ca776a52ebb8965fe1bb6affa448
download \
  https://archive.neon.kde.org/user/pool/main/p/poppler/poppler-utils_24.08.0-1+24.04+noble+release+build15_amd64.deb \
  "$POPPLER_DIR/poppler-utils.deb" \
  af3d09ab4a363949efba54e3a589888c032c7a1616a6039453f65e99e03f358e
download \
  https://archive.ubuntu.com/ubuntu/pool/main/f/fonts-dejavu/fonts-dejavu-mono_2.37-8_all.deb \
  "$FONT_DIR/fonts-dejavu-mono.deb" \
  8a599d6553307db7ecb795d2f0e5a301e03234afc75c7358b0ba43466454c89a
download \
  https://archive.ubuntu.com/ubuntu/pool/main/f/fonts-dejavu/fonts-dejavu-core_2.37-8_all.deb \
  "$FONT_DIR/fonts-dejavu-core.deb" \
  40049660c194f3b8a2541fc7369efebb10e9f94bdac836a2f38fafedd10fa73a
download \
  https://archive.ubuntu.com/ubuntu/pool/main/f/fonts-liberation/fonts-liberation_2.1.5-3_all.deb \
  "$FONT_DIR/fonts-liberation.deb" \
  065c2ab1abc9108b17d401016dc594b79750904390f095845c93bb06e1153acc
download \
  https://archive.ubuntu.com/ubuntu/pool/main/f/fonts-noto-cjk/fonts-noto-cjk_20230817+repack1-3_all.deb \
  "$FONT_DIR/fonts-noto-cjk.deb" \
  7d64b985f6fe128c99eae5610d5c047338e572bdcfb2bb09736be01b824a7f6c

printf '%s  %s\n' \
  369ab216d40364743188a3df30b3a86285aede504ddde89eea9b1bab8dbcbda5 \
  "$POPPLER_DIR/liblcms2-2.deb" \
  189bb9e6c22fa0f49f4ee8e802f62324a366ca776a52ebb8965fe1bb6affa448 \
  "$POPPLER_DIR/libpoppler140.deb" \
  af3d09ab4a363949efba54e3a589888c032c7a1616a6039453f65e99e03f358e \
  "$POPPLER_DIR/poppler-utils.deb" |
  sha256sum --check --strict

sudo env DEBIAN_FRONTEND=noninteractive timeout 5m apt-get \
  -o Acquire::Retries=3 \
  -o Acquire::http::Timeout=30 \
  -o Acquire::https::Timeout=30 \
  update
sudo env DEBIAN_FRONTEND=noninteractive timeout 10m apt-get \
  -o Acquire::Retries=3 \
  -o Acquire::http::Timeout=30 \
  -o Acquire::https::Timeout=30 \
  -o Dpkg::Use-Pty=0 \
  install -y --no-install-recommends \
  "$POPPLER_DIR/liblcms2-2.deb" \
  "$POPPLER_DIR/libpoppler140.deb" \
  "$POPPLER_DIR/poppler-utils.deb" \
  "$FONT_DIR/fonts-dejavu-mono.deb" \
  "$FONT_DIR/fonts-dejavu-core.deb" \
  "$FONT_DIR/fonts-liberation.deb" \
  "$FONT_DIR/fonts-noto-cjk.deb" \
  jq \
  fontconfig

printf '%s  %s\n' \
  b1f76a56605df368efd233e09faad3bd910e50c0d6556c616a7c0b0adebf6013 \
  /usr/bin/pdftoppm |
  sha256sum --check --strict
test "$(/usr/bin/pdftoppm -v 2>&1 | head -n 1)" = "pdftoppm version 24.08.0"

mkdir -p "$HOME/.local/share/fonts"
cp "$ROOT"/tests/parity/fonts/Parity*.ttf "$HOME/.local/share/fonts/"
timeout 2m fc-cache -f -v
