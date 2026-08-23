#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "Usage: $0 <package.jar> <version> <custom-font.ttf> <font-pack.ttf>" >&2
  exit 2
fi

PACKAGE="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
VERSION="$2"
CUSTOM_FONT="$(cd "$(dirname "$3")" && pwd)/$(basename "$3")"
FONT_PACK="$(cd "$(dirname "$4")" && pwd)/$(basename "$4")"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOCAL_REPOSITORY="$(mktemp -d)"

cleanup() {
  rm -rf "$LOCAL_REPOSITORY"
}
trap cleanup EXIT

mvn -B -Dmaven.repo.local="$LOCAL_REPOSITORY" \
  org.apache.maven.plugins:maven-install-plugin:3.1.4:install-file \
  -Dfile="$PACKAGE" \
  -DpomFile="$SCRIPT_DIR/pom.xml"

mvn -B -Dmaven.repo.local="$LOCAL_REPOSITORY" \
  -f "$SCRIPT_DIR/tests/consumer/pom.xml" \
  -Dironpress.version="$VERSION" \
  -Dironpress.expectedVersion="$VERSION" \
  -Dironpress.customFont="$CUSTOM_FONT" \
  -Dironpress.fontPack="$FONT_PACK" \
  verify

