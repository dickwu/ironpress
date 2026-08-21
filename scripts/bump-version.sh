#!/usr/bin/env bash
# Bump version across the Rust crate, Python wheel, Ruby gem, and binding sub-crates.
# Usage: ./scripts/bump-version.sh <new-version>
#
# Updates:
#   Cargo.toml                          (ironpress crate)
#   bindings/python/pyproject.toml      (PyPI wheel)
#   bindings/python/Cargo.toml          (ironpress-python internal crate)
#   bindings/ruby/lib/ironpress/version.rb
#   bindings/ruby/ext/ironpress/Cargo.toml
#   bindings/ruby/Gemfile.lock
#
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <new-version>" >&2
    echo "Example: $0 1.4.3" >&2
    exit 1
fi

NEW="$1"

# Validate semver-ish: N.N.N or N.N.N-pre
if ! [[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    echo "Error: '$NEW' is not a valid version (expected X.Y.Z or X.Y.Z-pre)" >&2
    exit 1
fi

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_DIR"

bump_toml_version() {
    local file="$1"
    # Replace the first `version = "..."` line (the [package] version, which comes first).
    perl -i -pe 'BEGIN{$n=0} if (!$n && /^version\s*=\s*"[^"]+"/) { s/"[^"]+"/"'"$NEW"'"/; $n=1 }' "$file"
}

bump_pyproject() {
    local file="$1"
    # [project] table owns `version = "..."`; match the first one (top of file).
    perl -i -pe 'BEGIN{$n=0} if (!$n && /^version\s*=\s*"[^"]+"/) { s/"[^"]+"/"'"$NEW"'"/; $n=1 }' "$file"
}

update_ruby_version() {
    local file="$1"
    perl -i -pe 's/(VERSION\s*=\s*)"[^"]+"/$1"'"$NEW"'"/' "$file"
}

update_ruby_lock_version() {
    local file="$1"
    perl -i -pe 's/^(\s*ironpress \()[^)]*(\))$/${1}'"$NEW"'${2}/' "$file"
}

update_core_requirement() {
    local file="$1"
    perl -i -pe 'if (/^ironpress-core\s*=/) { s/version\s*=\s*"=[^"]+"/version = "='"$NEW"'"/ }' "$file"
}

echo "Bumping to $NEW"

bump_toml_version  "Cargo.toml"                        && echo "  Cargo.toml"
bump_pyproject     "bindings/python/pyproject.toml"    && echo "  bindings/python/pyproject.toml"
bump_toml_version  "bindings/python/Cargo.toml"        && echo "  bindings/python/Cargo.toml"
update_core_requirement "bindings/python/Cargo.toml"   && echo "  Python core requirement"
bump_toml_version  "bindings/ruby/ext/ironpress/Cargo.toml" && echo "  bindings/ruby/ext/ironpress/Cargo.toml"
update_core_requirement "bindings/ruby/ext/ironpress/Cargo.toml" && echo "  Ruby core requirement"
update_ruby_version "bindings/ruby/lib/ironpress/version.rb" && echo "  bindings/ruby/lib/ironpress/version.rb"
update_ruby_lock_version "bindings/ruby/Gemfile.lock" && echo "  bindings/ruby/Gemfile.lock"

echo ""
scripts/check-release-versions.sh "$NEW"
echo "Done. Review the diff before committing."
