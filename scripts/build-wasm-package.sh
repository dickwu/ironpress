#!/usr/bin/env bash
set -euo pipefail

wasm-pack build --target web --features wasm --no-default-features
node scripts/prepare-wasm-package.mjs
