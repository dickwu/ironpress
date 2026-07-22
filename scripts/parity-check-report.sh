#!/usr/bin/env bash

set -euo pipefail

if (( $# != 2 )); then
  echo "usage: parity-check-report.sh <repository-root> <invocation-id>" >&2
  exit 64
fi

ROOT="$1"
INVOCATION_ID="$2"
PARITY="$ROOT/tests/parity"
MD_MARKER="<!-- parity-invocation-id: $INVOCATION_ID -->"
HTML_MARKER="<meta name=\"parity-invocation-id\" content=\"$INVOCATION_ID\">"

[[ -s "$PARITY/report.json" ]] || exit 1
[[ -s "$PARITY/REPORT.md" ]] || exit 1
[[ -s "$PARITY/reports/index.html" ]] || exit 1

JSON_SHA256="$(sha256sum "$PARITY/report.json" | awk '{print $1}')"
MD_JSON_MARKER="<!-- parity-report-json-sha256: $JSON_SHA256 -->"
HTML_JSON_MARKER="<meta name=\"parity-report-json-sha256\" content=\"$JSON_SHA256\">"

jq -e --arg invocation_id "$INVOCATION_ID" \
  '(.invocation_id // "") == $invocation_id' "$PARITY/report.json" >/dev/null &&
  grep -Fqx -- "$MD_MARKER" "$PARITY/REPORT.md" &&
  grep -Fqx -- "$MD_JSON_MARKER" "$PARITY/REPORT.md" &&
  grep -Fq -- "$HTML_MARKER" "$PARITY/reports/index.html" &&
  grep -Fq -- "$HTML_JSON_MARKER" "$PARITY/reports/index.html"
