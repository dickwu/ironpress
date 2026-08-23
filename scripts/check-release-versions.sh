#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPOSITORY_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPOSITORY_ROOT"

package_version() {
  sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' "$1" |
    head -n 1
}

ruby_runtime_version() {
  sed -n 's/^[[:space:]]*VERSION[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' "$1" |
    head -n 1
}

ruby_lock_version() {
  sed -n 's/^[[:space:]]*ironpress (\([^)]*\))$/\1/p' "$1" |
    head -n 1
}

core_requirement() {
  sed -n 's/^ironpress-core.*version[[:space:]]*=[[:space:]]*"=\([^"]*\)".*/\1/p' "$1"
}

dotnet_version() {
  sed -n 's/.*<Version>\([^<]*\)<\/Version>.*/\1/p' "$1" |
    head -n 1
}

maven_version() {
  sed -n 's/^[[:space:]]*<version>\([^<]*\)<\/version>.*/\1/p' "$1" |
    head -n 1
}

java_runtime_version() {
  sed -n 's/.*PACKAGE_VERSION[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$1" |
    head -n 1
}

html_maven_version() {
  sed -n 's/.*&lt;version&gt;\([^&]*\)&lt;\/version&gt;.*/\1/p' "$1" |
    head -n 1
}

if [[ -n "${1:-}" ]]; then
  EXPECTED_VERSION="$1"
elif [[ "${GITHUB_REF_TYPE:-}" == "tag" && "${GITHUB_REF_NAME:-}" == v* ]]; then
  EXPECTED_VERSION="${GITHUB_REF_NAME#v}"
else
  EXPECTED_VERSION="$(package_version Cargo.toml)"
fi
MISMATCHES=0

check_version() {
  local label="$1"
  local actual="$2"

  if [[ "$actual" != "$EXPECTED_VERSION" ]]; then
    printf '%s: expected %s, found %s\n' "$label" "$EXPECTED_VERSION" "${actual:-<missing>}" >&2
    MISMATCHES=$((MISMATCHES + 1))
  fi
}

check_version "Rust crate" "$(package_version Cargo.toml)"
check_version "C crate" "$(package_version bindings/c/Cargo.toml)"
check_version "C core requirement" "$(core_requirement bindings/c/Cargo.toml)"
check_version ".NET package" "$(dotnet_version bindings/dotnet/src/Ironpress/Ironpress.csproj)"
check_version "Java package" "$(maven_version bindings/java/pom.xml)"
check_version "Java runtime" "$(java_runtime_version bindings/java/src/main/java/io/github/gastongouron/ironpress/IronpressInfo.java)"
check_version "Java README" "$(maven_version bindings/java/README.md)"
check_version "Java root README" "$(maven_version README.md)"
check_version "Java website" "$(html_maven_version site/get-started/java/index.html)"
check_version "Python crate" "$(package_version bindings/python/Cargo.toml)"
check_version "Python distribution" "$(package_version bindings/python/pyproject.toml)"
check_version "Python core requirement" "$(core_requirement bindings/python/Cargo.toml)"
check_version "Ruby crate" "$(package_version bindings/ruby/ext/ironpress/Cargo.toml)"
check_version "Ruby core requirement" "$(core_requirement bindings/ruby/ext/ironpress/Cargo.toml)"
check_version "Ruby runtime" "$(ruby_runtime_version bindings/ruby/lib/ironpress/version.rb)"
check_version "Ruby lockfile" "$(ruby_lock_version bindings/ruby/Gemfile.lock)"

if ((MISMATCHES > 0)); then
  exit 1
fi

printf 'Release versions agree on %s.\n' "$EXPECTED_VERSION"
