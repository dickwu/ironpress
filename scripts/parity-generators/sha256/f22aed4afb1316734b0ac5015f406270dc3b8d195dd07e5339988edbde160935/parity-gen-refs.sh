#!/usr/bin/env bash
#
# Generate authenticated browser PDF oracles. PNGs are never committed: the
# parity test rasterizes oracle and candidate PDFs through the same runtime
# pdftoppm executable and writes ignored PNG previews for the HTML report.

set -euo pipefail

VALIDATION_DPI=300

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PARITY="$ROOT/tests/parity"
CASES="$PARITY/cases"
ORACLES="$PARITY/oracles"
FONTS="$PARITY/fonts"
TMP="$ROOT/target/parity-tmp"
mkdir -p "$TMP"

FORCE="${FORCE:-0}"
ONLY_CATEGORY=""
CHECK=0
for arg in "$@"; do
  case "$arg" in
    --force) FORCE=1 ;;
    --check) CHECK=1 ;;
    -*) echo "unknown flag: $arg" >&2; exit 2 ;;
    *)
      if [ -n "$ONLY_CATEGORY" ]; then
        echo "only one category may be selected" >&2
        exit 2
      fi
      ONLY_CATEGORY="$arg"
      ;;
  esac
done

"$SCRIPT_DIR/parity-normalize-page-sizes.py"

font_bundle_digest() {
  FONTS_DIR="$FONTS" python3 - <<'PY'
import glob, hashlib, os
root = os.environ["FONTS_DIR"]
paths = sorted(glob.glob(os.path.join(root, "Parity*.ttf")))
config = os.path.join(root, "fonts.conf")
if os.path.isfile(config):
    paths.insert(0, config)
external = [
    ("generic-sans", "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"),
    ("generic-serif", "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf"),
    ("generic-monospace", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
    ("cjk-sans", "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
]
missing = [path for _, path in external if not os.path.isfile(path)]
if missing:
    raise SystemExit("required parity font(s) missing: " + ", ".join(missing))
h = hashlib.sha256()
for path in paths:
    h.update(os.path.relpath(path, root).encode())
    h.update(b"\0")
    with open(path, "rb") as fh:
        h.update(fh.read())
    h.update(b"\0")
for label, path in external:
    h.update(label.encode())
    h.update(b"\0")
    with open(path, "rb") as fh:
        h.update(fh.read())
    h.update(b"\0")
print(h.hexdigest())
PY
}

check_refs_lock() {
  local category="${1:-}"
  PARITY_DIR="$PARITY" CHECK_CATEGORY="$category" \
    GENERATOR_SHA="$(sha256sum "$SCRIPT_DIR/parity-gen-refs.sh" | awk '{print $1}')" \
    FONT_SHA="$(font_bundle_digest)" python3 - <<'PY'
import glob, hashlib, json, os, sys

parity = os.environ["PARITY_DIR"]
category_filter = os.environ.get("CHECK_CATEGORY", "")
lock_path = os.path.join(parity, "refs.lock")

def digest(path):
    with open(path, "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()

def provenance_id(record):
    canonical = json.dumps(record, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()

def manifest_digest(entry):
    identity = [
        entry["id"], entry["category"], entry["feature"],
        entry.get("subfeature", ""), entry.get("description", ""), entry["file"],
        entry.get("interaction_of", []), entry.get("base_ids", []),
        entry.get("sanitize", True), entry.get("kind", "feature"),
        entry.get("depends_on", []), entry.get("expected_support", "implemented"),
        entry.get("oracle", "chrome"),
    ]
    canonical = json.dumps(identity, ensure_ascii=False, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()

def is_sha256(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)

errors = []
try:
    with open(lock_path, encoding="utf-8") as fh:
        lock = json.load(fh)
except (OSError, json.JSONDecodeError) as error:
    print(f"parity-gen-refs: refs.lock unreadable: {error}", file=sys.stderr)
    sys.exit(1)

if not isinstance(lock, dict) or lock.get("schema") != 3:
    errors.append("refs.lock must use PDF-oracle schema 3")
fixtures = lock.get("fixtures", {})
provenance = lock.get("provenance", {})
if not isinstance(fixtures, dict) or not isinstance(provenance, dict):
    errors.append("refs.lock fixtures/provenance must be JSON objects")
    fixtures, provenance = {}, {}

manifest = {}
for path in sorted(glob.glob(os.path.join(parity, "manifest", "*.json"))):
    with open(path, encoding="utf-8") as fh:
        entries = json.load(fh)
    for entry in entries:
        if category_filter and entry.get("category") != category_filter:
            continue
        fid = entry.get("id")
        if fid in manifest:
            errors.append(f"duplicate manifest id: {fid}")
        manifest[fid] = entry

locked_ids = {
    fid for fid, entry in fixtures.items()
    if not category_filter or entry.get("category") == category_filter
}
for fid in sorted(set(manifest) | locked_ids):
    current = manifest.get(fid)
    locked = fixtures.get(fid)
    if current is None:
        errors.append(f"{fid}: removed fixture remains in refs.lock")
        continue
    if not isinstance(locked, dict):
        errors.append(f"{fid}: absent from refs.lock")
        continue
    rel = current.get("file")
    oracle = current.get("oracle", "chrome")
    html = os.path.join(parity, rel) if isinstance(rel, str) else ""
    expected_pdf = None
    if oracle != "none":
        pdf_rel = f"oracles/{current.get('category')}/{fid}.pdf"
        pdf_path = os.path.join(parity, pdf_rel)
        if not os.path.isfile(pdf_path):
            errors.append(f"{fid}: missing oracle PDF {pdf_rel}")
        else:
            expected_pdf = {"file": pdf_rel, "sha256": digest(pdf_path)}
    expected = {
        "category": current.get("category"),
        "file": rel,
        "manifest_sha256": manifest_digest(current),
        "html_sha256": digest(html) if os.path.isfile(html) else "",
        "oracle": oracle,
        "pdf": expected_pdf,
    }
    for field, value in expected.items():
        if locked.get(field) != value:
            errors.append(f"{fid}: {field} differs from refs.lock")

    key = locked.get("provenance")
    record = provenance.get(key) if isinstance(key, str) else None
    if not isinstance(record, dict):
        errors.append(f"{fid}: missing provenance record {key!r}")
        continue
    expected_renderer = {
        "chrome": {"chromium", "chromium+pagedjs"},
        "weasyprint": {"weasyprint"},
        "none": {"none"},
    }.get(oracle, set())
    if (record.get("generator") != "scripts/parity-gen-refs.sh"
            or record.get("generator_sha256") != os.environ["GENERATOR_SHA"]
            or record.get("font_bundle_sha256") != os.environ["FONT_SHA"]
            or record.get("oracle") != oracle
            or record.get("renderer") not in expected_renderer
            or not record.get("renderer_version")
            or not isinstance(record.get("pagedjs"), bool)
            or key != provenance_id(record)):
        errors.append(f"provenance {key}: stale or invalid PDF renderer provenance")

if not category_filter:
    used = {
        entry.get("provenance") for entry in fixtures.values() if isinstance(entry, dict)
    }
    unused = sorted(set(provenance) - used)
    if unused:
        errors.append("unused provenance record(s): " + ", ".join(unused))

if errors:
    errors = list(dict.fromkeys(errors))
    print(f"parity-gen-refs: refs.lock integrity FAILED ({len(errors)} issue(s)):", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    sys.exit(1)

scope = category_filter or "complete corpus"
print(f"refs.lock PDF integrity OK: {len(manifest)} fixture(s), scope={scope}")
PY
}

if [ "$CHECK" = "1" ]; then
  check_refs_lock "$ONLY_CATEGORY"
  exit 0
fi

PAGEDJS="${PAGEDJS:-0}"
PAGE_CSS="$TMP/pagedjs-page.css"
PAGEDJS_BIN=""
if [ "$PAGEDJS" = "1" ]; then
  printf '@page{size:Letter;margin:28.8pt;}\n' > "$PAGE_CSS"
  if command -v pagedjs-cli >/dev/null 2>&1; then
    PAGEDJS_BIN="$(command -v pagedjs-cli)"
  elif [ -x "$ROOT/target/pagedtool/node_modules/.bin/pagedjs-cli" ]; then
    PAGEDJS_BIN="$ROOT/target/pagedtool/node_modules/.bin/pagedjs-cli"
  else
    echo "parity-gen-refs: PAGEDJS=1 but pagedjs-cli is unavailable" >&2
    exit 1
  fi
fi

if [ -f "$FONTS/fonts.conf" ]; then
  export FONTCONFIG_FILE="$FONTS/fonts.conf"
  export FONTCONFIG_PATH="$FONTS"
fi
FONTS_SRC="$PARITY/fonts"
USER_FONTS="${XDG_DATA_HOME:-$HOME/.local/share}/fonts"
if ls "$FONTS_SRC"/Parity*.ttf >/dev/null 2>&1; then
  mkdir -p "$USER_FONTS"
  cp -f "$FONTS_SRC"/Parity*.ttf "$USER_FONTS"/
  command -v fc-cache >/dev/null 2>&1 || {
    echo "parity-gen-refs: fc-cache is required" >&2
    exit 1
  }
  fc-cache -f "$USER_FONTS" >/dev/null
fi

CHROMIUM=""
for candidate in chromium-browser /snap/bin/chromium chromium google-chrome google-chrome-stable; do
  if command -v "$candidate" >/dev/null 2>&1; then
    CHROMIUM="$candidate"
    break
  fi
done
if [ -z "$CHROMIUM" ]; then
  echo "parity-gen-refs: chromium not found" >&2
  exit 1
fi
CHROMIUM_ABS="$(command -v "$CHROMIUM" 2>/dev/null || echo "$CHROMIUM")"
if ! command -v pdftoppm >/dev/null 2>&1; then
  echo "parity-gen-refs: pdftoppm is required to validate generated PDFs" >&2
  exit 1
fi

NCPU="$(nproc 2>/dev/null || echo 4)"
DEFAULT_JOBS=$((NCPU - 2))
[ "$DEFAULT_JOBS" -lt 1 ] && DEFAULT_JOBS=1
[ "$DEFAULT_JOBS" -gt 8 ] && DEFAULT_JOBS=8
JOBS="${PARITY_JOBS:-$DEFAULT_JOBS}"
if ! [[ "$JOBS" =~ ^[1-9][0-9]*$ ]]; then
  echo "PARITY_JOBS must be a positive integer" >&2
  exit 2
fi

# An oracle may be skipped only when its recorded generator is this exact
# script. Otherwise preserving the old provenance while re-writing refs.lock
# would either lie about how the PDF was made or leave the corpus permanently
# stale. A generator change therefore forces a real regeneration automatically.
if [ "$FORCE" != "1" ]; then
  current_generator_sha="$(sha256sum "$SCRIPT_DIR/parity-gen-refs.sh" | awk '{print $1}')"
  current_font_sha="$(font_bundle_digest)"
  if ! PARITY_DIR="$PARITY" GENERATOR_SHA="$current_generator_sha" \
    FONT_SHA="$current_font_sha" PAGEDJS_ENABLED="$PAGEDJS" python3 - <<'PY'
import json, os, sys
try:
    with open(os.path.join(os.environ["PARITY_DIR"], "refs.lock"), encoding="utf-8") as fh:
        lock = json.load(fh)
    provenance = lock["provenance"]
    requested_pagedjs = os.environ["PAGEDJS_ENABLED"] == "1"
    valid = (lock.get("schema") == 3 and isinstance(provenance, dict)
             and provenance
             and all(
                 record.get("generator_sha256") == os.environ["GENERATOR_SHA"]
                 and record.get("font_bundle_sha256") == os.environ["FONT_SHA"]
                 and (record.get("oracle") != "chrome"
                      or record.get("pagedjs") == requested_pagedjs)
                 for record in provenance.values()))
except (OSError, KeyError, TypeError, json.JSONDecodeError):
    valid = False
sys.exit(0 if valid else 1)
PY
  then
    FORCE=1
    echo "parity-gen-refs: oracle-generation inputs changed; regenerating every oracle PDF" >&2
    if [ -n "$ONLY_CATEGORY" ]; then
      echo "parity-gen-refs: ignoring category filter because provenance is corpus-wide" >&2
      ONLY_CATEGORY=""
    fi
  fi
fi

render_one() {
  local html="$1"
  local rel category id oracle oracle_current oracle_pdf pdf profile validation_prefix pages first_page sd
  rel="${html#"$CASES"/}"
  category="${rel%%/*}"
  # Existing PDFs may be skipped only when refs.lock proves that this exact HTML
  # and PDF pair was generated under the still-current corpus-wide provenance.
  # Merely finding a PDF at the expected path is not enough: after an HTML edit
  # that would preserve stale pixels while the rewritten lock drops the fixture.
  read -r id oracle oracle_current < <(python3 - \
    "$PARITY/manifest/$category.json" "cases/$rel" "$PARITY/refs.lock" "$PARITY" <<'PY'
import hashlib, json, os, sys
manifest, fixture, lock_path, parity = sys.argv[1:]
matches = [entry for entry in json.load(open(manifest, encoding="utf-8"))
           if entry.get("file") == fixture]
if len(matches) == 1:
    entry = matches[0]
    fid = entry["id"]
    oracle = entry.get("oracle", "chrome")
    current = False
    try:
        lock = json.load(open(lock_path, encoding="utf-8"))
        locked = lock.get("fixtures", {}).get(fid, {})
        provenance = lock.get("provenance", {})
        html_path = os.path.join(parity, fixture)
        pdf_rel = f"oracles/{entry['category']}/{fid}.pdf"
        pdf_path = os.path.join(parity, pdf_rel)

        def digest(path):
            with open(path, "rb") as fh:
                return hashlib.sha256(fh.read()).hexdigest()

        expected_pdf = None if oracle == "none" else {
            "file": pdf_rel,
            "sha256": digest(pdf_path),
        }
        current = (
            lock.get("schema") == 3
            and locked.get("category") == entry["category"]
            and locked.get("file") == fixture
            and locked.get("html_sha256") == digest(html_path)
            and locked.get("oracle") == oracle
            and locked.get("pdf") == expected_pdf
            and locked.get("provenance") in provenance
        )
    except (OSError, TypeError, ValueError, json.JSONDecodeError):
        current = False
    print(fid, oracle, "1" if current else "0")
PY
)
  if [ -z "${id:-}" ]; then
    printf 'F\t%s\n' "$rel"
    return 0
  fi
  if [ "$oracle" = "none" ]; then
    printf 'N\t%s\t%s\n' "$id" "$oracle"
    return 0
  fi

  oracle_pdf="$ORACLES/$category/$id.pdf"
  if [ -f "$oracle_pdf" ] && [ "$FORCE" != "1" ] && [ "$oracle_current" = "1" ]; then
    printf 'S\t%s\t%s\n' "$id" "$oracle"
    return 0
  fi
  mkdir -p "$ORACLES/$category"
  pdf="$(mktemp "$TMP/oracle.XXXXXX.pdf")"
  profile="$(mktemp -d "$TMP/chrome-profile.XXXXXX")"
  validation_prefix="$(mktemp -u "$TMP/oracle-validation.XXXXXX")"
  trap 'rm -rf "$pdf" "$profile" "$validation_prefix"-*.png' RETURN

  local ok=""
  local attempt
  for attempt in 1 2 3; do
    rm -f "$pdf"
    if [ "$oracle" = "weasyprint" ]; then
      timeout -k 5s 120s python3 -m weasyprint "$html" "$pdf" >/dev/null 2>&1 || true
    elif [ "$PAGEDJS" = "1" ]; then
      PUPPETEER_EXECUTABLE_PATH="$CHROMIUM_ABS" PUPPETEER_SKIP_DOWNLOAD=1 \
        timeout -k 5s 120s "$PAGEDJS_BIN" -i "$html" -o "$pdf" \
          --page-size Letter --style "$PAGE_CSS" \
          --browserArgs "--no-sandbox,--disable-gpu,--disable-software-rasterizer" \
          >/dev/null 2>&1 || true
    else
      rm -rf "$profile"
      profile="$(mktemp -d "$TMP/chrome-profile.XXXXXX")"
      timeout -k 5s 60s "$CHROMIUM" --headless=new --disable-gpu --no-sandbox \
        --disable-software-rasterizer --user-data-dir="$profile" \
        --no-pdf-header-footer --print-to-pdf="$pdf" "file://$html" >/dev/null 2>&1 || true
      pkill -9 -f "$profile" 2>/dev/null || true
    fi
    if [ -s "$pdf" ]; then
      ok=1
      break
    fi
    sleep 0.4
  done
  if [ -z "$ok" ]; then
    printf 'F\t%s\n' "$id"
    return 0
  fi

  rm -f "$validation_prefix"-*.png
  if ! timeout 90s pdftoppm -r "$VALIDATION_DPI" -png "$pdf" "$validation_prefix" >/dev/null 2>&1; then
    printf 'F\t%s\n' "$id"
    return 0
  fi
  pages="$(find "$TMP" -maxdepth 1 -type f -name "$(basename "$validation_prefix")-*.png" | sort -V)"
  if [ -z "$pages" ]; then
    printf 'F\t%s\n' "$id"
    return 0
  fi
  # A uniform page is not necessarily blank: several page-selection fixtures
  # intentionally fill the whole first page with one solid colour, and some CSS
  # features legitimately produce a white page. PDF validity plus successful
  # all-page rasterization are the only content-independent checks we can make
  # here; never reject an oracle from a pixel-variance heuristic.

  mv -f "$pdf" "$oracle_pdf"
  pdf=""
  printf 'G\t%s\t%s\n' "$id" "$oracle"
}

export -f render_one
export CHROMIUM CHROMIUM_ABS CASES ORACLES TMP FORCE PARITY PAGEDJS PAGEDJS_BIN PAGE_CSS VALIDATION_DPI

status_file="$(mktemp "$TMP/oracle-status.XXXXXX")"
trap 'rm -f "$status_file"' EXIT
if [ -n "$ONLY_CATEGORY" ]; then
  find "$CASES/$ONLY_CATEGORY" -type f -name '*.html' -print0 2>/dev/null | sort -z | \
    xargs -0 -r -P "$JOBS" -I {} bash -c 'render_one "$1"' _ {} > "$status_file"
else
  find "$CASES" -type f -name '*.html' -print0 | sort -z | \
    xargs -0 -r -P "$JOBS" -I {} bash -c 'render_one "$1"' _ {} > "$status_file"
fi

generated="$(grep -c $'^G\t' "$status_file" || true)"
skipped="$(grep -c $'^S\t' "$status_file" || true)"
failed="$(grep -c $'^F\t' "$status_file" || true)"
echo "parity-gen-refs: PDFs generated=$generated skipped=$skipped failed=$failed"

write_refs_lock() {
  local chromium_version weasyprint_version pagedjs_version
  chromium_version="$("$CHROMIUM" --version 2>/dev/null | head -1 || true)"
  weasyprint_version="$(python3 -m weasyprint --version 2>/dev/null | head -1 || true)"
  if [ -n "$PAGEDJS_BIN" ]; then
    pagedjs_version="$("$PAGEDJS_BIN" --version 2>/dev/null | head -1 || true)"
  else
    pagedjs_version="disabled"
  fi
  [ -n "$chromium_version" ] || chromium_version="unknown Chromium version"
  [ -n "$weasyprint_version" ] || weasyprint_version="unknown WeasyPrint version"

  PARITY_DIR="$PARITY" STATUS_FILE="$status_file" PAGEDJS_ENABLED="$PAGEDJS" \
    CATEGORY_FILTER="$ONLY_CATEGORY" \
    GENERATOR_SHA="$(sha256sum "$SCRIPT_DIR/parity-gen-refs.sh" | awk '{print $1}')" \
    FONT_SHA="$(font_bundle_digest)" CHROMIUM_VERSION="$chromium_version" \
    WEASYPRINT_VERSION="$weasyprint_version" PAGEDJS_VERSION="$pagedjs_version" \
    python3 - <<'PY'
import glob, hashlib, json, os

parity = os.environ["PARITY_DIR"]
lock_path = os.path.join(parity, "refs.lock")
pagedjs = os.environ["PAGEDJS_ENABLED"] == "1"
category_filter = os.environ.get("CATEGORY_FILTER", "")

def digest(path):
    with open(path, "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()

def provenance_record(oracle):
    if oracle == "weasyprint":
        renderer, version, uses_pagedjs = "weasyprint", os.environ["WEASYPRINT_VERSION"], False
    elif oracle == "none":
        renderer, version, uses_pagedjs = "none", "not applicable", False
    elif pagedjs:
        renderer = "chromium+pagedjs"
        version = os.environ["CHROMIUM_VERSION"] + "; " + os.environ["PAGEDJS_VERSION"]
        uses_pagedjs = True
    else:
        renderer, version, uses_pagedjs = "chromium", os.environ["CHROMIUM_VERSION"], False
    return {
        "generator": "scripts/parity-gen-refs.sh",
        "generator_sha256": os.environ["GENERATOR_SHA"],
        "oracle": oracle,
        "renderer": renderer,
        "renderer_version": version,
        "font_bundle_sha256": os.environ["FONT_SHA"],
        "pagedjs": uses_pagedjs,
    }

def provenance_id(record):
    canonical = json.dumps(record, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()

def manifest_digest(entry):
    identity = [
        entry["id"], entry["category"], entry["feature"],
        entry.get("subfeature", ""), entry.get("description", ""), entry["file"],
        entry.get("interaction_of", []), entry.get("base_ids", []),
        entry.get("sanitize", True), entry.get("kind", "feature"),
        entry.get("depends_on", []), entry.get("expected_support", "implemented"),
        entry.get("oracle", "chrome"),
    ]
    canonical = json.dumps(identity, ensure_ascii=False, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()

generated = set()
with open(os.environ["STATUS_FILE"], encoding="utf-8") as status:
    for line in status:
        fields = line.rstrip("\n").split("\t")
        if fields and fields[0] in {"G", "N"} and len(fields) >= 2:
            generated.add(fields[1])

previous_fixtures, previous_provenance = {}, {}
try:
    previous = json.load(open(lock_path, encoding="utf-8"))
    if previous.get("schema") == 3:
        previous_fixtures = previous.get("fixtures", {})
        previous_provenance = previous.get("provenance", {})
except (OSError, json.JSONDecodeError, AttributeError):
    pass

manifest = {}
for path in sorted(glob.glob(os.path.join(parity, "manifest", "*.json"))):
    for entry in json.load(open(path, encoding="utf-8")):
        if entry["id"] in manifest:
            raise SystemExit(f"duplicate manifest id: {entry['id']}")
        manifest[entry["id"]] = entry

fixtures, provenance = {}, {}

# A scoped generation must never erase, refresh, or otherwise conceal lock
# entries outside its category. Preserve those records verbatim, including
# removed or stale fixture IDs, so a later complete integrity check exposes the
# problem instead of observing a silently shrunken corpus.
if category_filter:
    for fid, old in previous_fixtures.items():
        if not isinstance(old, dict) or old.get("category") == category_filter:
            continue
        fixtures[fid] = old
        key = old.get("provenance")
        if key in previous_provenance:
            provenance[key] = previous_provenance[key]

for fid, entry in sorted(manifest.items()):
    if category_filter and entry["category"] != category_filter:
        continue
    oracle = entry.get("oracle", "chrome")
    html = os.path.join(parity, entry["file"])
    pdf_rel = f"oracles/{entry['category']}/{fid}.pdf"
    pdf_path = os.path.join(parity, pdf_rel)
    artifact = None if oracle == "none" else (
        {"file": pdf_rel, "sha256": digest(pdf_path)} if os.path.isfile(pdf_path) else None
    )
    current = {
        "category": entry["category"],
        "file": entry["file"],
        "manifest_sha256": manifest_digest(entry),
        "html_sha256": digest(html),
        "oracle": oracle,
        "pdf": artifact,
    }

    if fid in generated:
        if oracle != "none" and artifact is None:
            continue
        record = provenance_record(oracle)
        key = provenance_id(record)
        provenance[key] = record
        fixtures[fid] = {**current, "provenance": key}
        continue

    old = previous_fixtures.get(fid)
    pdf_identity_fields = ("category", "file", "html_sha256", "oracle", "pdf")
    if isinstance(old, dict) and all(old.get(field) == current[field] for field in pdf_identity_fields):
        key = old.get("provenance")
        if key in previous_provenance:
            fixtures[fid] = {**current, "provenance": key}
            provenance[key] = previous_provenance[key]

ordered = {
    "schema": 3,
    "fixtures": {key: fixtures[key] for key in sorted(fixtures)},
    "provenance": {key: provenance[key] for key in sorted(provenance)},
}
temporary = lock_path + ".tmp"
with open(temporary, "w", encoding="utf-8") as out:
    json.dump(ordered, out, indent=2, sort_keys=True)
    out.write("\n")
os.replace(temporary, lock_path)
print(f"wrote refs.lock PDF schema 3 ({len(fixtures)} fixture identities)")
PY
}

write_refs_lock
if [ "$failed" -ne 0 ]; then
  echo "parity-gen-refs: $failed render(s) failed" >&2
  exit 1
fi
check_refs_lock "$ONLY_CATEGORY"
