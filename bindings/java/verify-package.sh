#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <package.jar> <version>" >&2
  exit 2
fi

PACKAGE="$1"
VERSION="$2"

if [[ ! -f "$PACKAGE" ]]; then
  echo "Maven package not found: $PACKAGE" >&2
  exit 1
fi

REQUIRED_ENTRIES=(
  "META-INF/LICENSE"
  "META-INF/README.md"
  "META-INF/maven/io.github.gastongouron/ironpress/pom.properties"
  "META-INF/maven/io.github.gastongouron/ironpress/pom.xml"
  "io/github/gastongouron/ironpress/HtmlConverter.class"
  "linux-x86-64/libironpress_ffi.so"
  "linux-aarch64/libironpress_ffi.so"
  "darwin-x86-64/libironpress_ffi.dylib"
  "darwin-aarch64/libironpress_ffi.dylib"
  "win32-x86-64/ironpress_ffi.dll"
)

PACKAGE_ENTRIES="$(unzip -Z1 "$PACKAGE")"
for entry in "${REQUIRED_ENTRIES[@]}"; do
  if ! grep -Fqx "$entry" <<<"$PACKAGE_ENTRIES"; then
    echo "Maven package is missing $entry" >&2
    exit 1
  fi
done

NATIVE_COUNT="$(grep -Ec '^(linux|darwin|win32)-[^/]+/[^/]+$' <<<"$PACKAGE_ENTRIES")"
if [[ "$NATIVE_COUNT" -ne 5 ]]; then
  echo "Maven package must contain exactly five native assets; found $NATIVE_COUNT" >&2
  exit 1
fi

if ! unzip -p "$PACKAGE" META-INF/MANIFEST.MF |
  tr -d '\r' |
  grep -Fqx "Implementation-Version: $VERSION"; then
  echo "Maven manifest version does not match $VERSION" >&2
  exit 1
fi

if ! unzip -p "$PACKAGE" \
  META-INF/maven/io.github.gastongouron/ironpress/pom.properties |
  grep -Fqx "version=$VERSION"; then
  echo "Maven metadata version does not match $VERSION" >&2
  exit 1
fi

echo "Maven package contract passed for Ironpress $VERSION."

