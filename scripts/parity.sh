#!/usr/bin/env bash
#
# parity.sh — convenience runner for the feature-parity engine.
#
# Runs the in-process parity gate (renders every fixture, diffs against the
# committed oracle PDFs, rewrites report.json + REPORT.md, and enforces the
# regression gate). This wrapper intentionally accepts no test filters: it is
# the full-corpus gate, not a diagnostic runner.
#
# Usage:
#   scripts/parity.sh
#
# For a filtered diagnostic run, invoke the integration test directly with
# PARITY_ONLY as documented in tests/parity/README.md. Filtered runs fail closed
# and never replace the durable full-run report.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if (( $# != 0 )); then
  echo "parity: this command runs the complete gate and accepts no arguments" >&2
  echo "parity: use PARITY_ONLY=<fixture-id> cargo test --test feature_parity -- --nocapture for diagnostics" >&2
  exit 64
fi

if [[ -v PARITY_ONLY ]]; then
  echo "parity: scripts/parity.sh cannot inherit PARITY_ONLY; use the diagnostic cargo command directly" >&2
  exit 64
fi

for required in jq sha256sum; do
  if ! command -v "$required" >/dev/null 2>&1; then
    echo "parity: $required is required to verify this invocation's report identity" >&2
    exit 1
  fi
done

# Durable cleanup and publication belong to the Rust runner after it acquires
# FullRunLock. The wrapper only supplies a fresh identity so a zero-test libtest
# run, or a racing direct invocation, cannot make stale evidence look current.
INVOCATION_ID="$(od -An -N16 -tx1 /dev/urandom | tr -d '[:space:]')"
if [[ ! "$INVOCATION_ID" =~ ^[0-9a-f]{32}$ ]]; then
  echo "parity: could not generate a full-run invocation identity" >&2
  exit 1
fi

TEST_STATUS=0
PARITY_INVOCATION_ID="$INVOCATION_ID" \
  cargo test --manifest-path "$ROOT/Cargo.toml" --test feature_parity -- --nocapture --exact feature_parity || TEST_STATUS=$?

if ! bash "$SCRIPT_DIR/parity-check-report.sh" "$ROOT" "$INVOCATION_ID"; then
  echo "parity: run produced no fresh JSON/Markdown/HTML report for this invocation" >&2
  exit 1
fi

if (( TEST_STATUS != 0 )); then
  echo >&2
  echo "parity: failing scorecard written to $ROOT/tests/parity/REPORT.md (machine: report.json)" >&2
  exit "$TEST_STATUS"
fi

if ! jq -e '.run_complete == true' "$ROOT/tests/parity/report.json" >/dev/null; then
  echo "parity: test returned success without a complete report" >&2
  exit 1
fi

echo
echo "parity: scorecard written to $ROOT/tests/parity/REPORT.md (machine: report.json)"
