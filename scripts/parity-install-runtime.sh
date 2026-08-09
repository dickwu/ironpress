#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
POPPLER_DIR="$(mktemp -d)"

# Every report-producing job must use the rasterizer authenticated by the
# committed parity baseline. Package-manager defaults are not deterministic.
if [[ "$(dpkg --print-architecture)" != "amd64" ]]; then
  echo "parity runtime requires Ubuntu 24.04 amd64" >&2
  exit 1
fi

curl --fail --location --retry 3 \
  --output "$POPPLER_DIR/liblcms2-2.deb" \
  https://archive.neon.kde.org/user/pool/main/l/lcms2/liblcms2-2_2.16-2+24.04+noble+release+build2_amd64.deb
curl --fail --location --retry 3 \
  --output "$POPPLER_DIR/libpoppler140.deb" \
  https://archive.neon.kde.org/user/pool/main/p/poppler/libpoppler140_24.08.0-1+24.04+noble+release+build15_amd64.deb
curl --fail --location --retry 3 \
  --output "$POPPLER_DIR/poppler-utils.deb" \
  https://archive.neon.kde.org/user/pool/main/p/poppler/poppler-utils_24.08.0-1+24.04+noble+release+build15_amd64.deb

printf '%s  %s\n' \
  369ab216d40364743188a3df30b3a86285aede504ddde89eea9b1bab8dbcbda5 \
  "$POPPLER_DIR/liblcms2-2.deb" \
  189bb9e6c22fa0f49f4ee8e802f62324a366ca776a52ebb8965fe1bb6affa448 \
  "$POPPLER_DIR/libpoppler140.deb" \
  af3d09ab4a363949efba54e3a589888c032c7a1616a6039453f65e99e03f358e \
  "$POPPLER_DIR/poppler-utils.deb" |
  sha256sum --check --strict

sudo apt-get update
sudo apt-get install -y \
  "$POPPLER_DIR/liblcms2-2.deb" \
  "$POPPLER_DIR/libpoppler140.deb" \
  "$POPPLER_DIR/poppler-utils.deb" \
  jq \
  fontconfig \
  fonts-noto-cjk \
  fonts-liberation \
  fonts-dejavu-core

printf '%s  %s\n' \
  b1f76a56605df368efd233e09faad3bd910e50c0d6556c616a7c0b0adebf6013 \
  /usr/bin/pdftoppm |
  sha256sum --check --strict
test "$(/usr/bin/pdftoppm -v 2>&1 | head -n 1)" = "pdftoppm version 24.08.0"

mkdir -p "$HOME/.local/share/fonts"
cp "$ROOT"/tests/parity/fonts/Parity*.ttf "$HOME/.local/share/fonts/"
fc-cache -f
