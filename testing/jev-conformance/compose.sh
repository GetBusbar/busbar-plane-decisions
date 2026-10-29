#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# THE SUBJECT'S COMPOSITION: the busbar package the `jev` suite builds, with this checkout's plane in it.
#
#   compose.sh <busbar checkout> <this plane's crate dir> <stage dir>   prints the manifest to build
#   compose.sh --selftest                                               holds the rewrite to planted rows
#
# The package is busbar's default distribution (dist/default): the root crate (crates/busbar) is core
# with no plugin linked, so it serves no decisions. The checkout is copied to <stage> and never
# written. In the copy, the one dist row that names an item inside this plane is pointed at where
# THIS plane keeps it, the way busbar's dist names it once it pins this plane (busbar #1912):
#   linked-entry  plane-decisions  the module the door is exported from: the crate root when the
#                                  plane has no `pub mod plane_door` (it re-exports `door` there)
# Nothing else changes: busbar's code, its other rows and its lockfile are the pin's.
set -euo pipefail

rewrite() { # <manifest> <plane crate dir>
  python3 -I - "$1" "$2" >&2 <<'PY'
import os, re, sys
manifest, plane = sys.argv[1:3]
text = open(manifest).read()
lib = open(os.path.join(plane, "src", "lib.rs")).read()
table, key = "package.metadata.busbar.linked-entry", "plane-decisions"
sec = re.search(r"^\[" + re.escape(table) + r"\]\s*$(.*?)(?=^\[|\Z)", text, re.M | re.S)
if not sec:
    sys.exit(f"compose: {manifest} has no [{table}]")
body = sec.group(1)
row = re.compile(r"^" + re.escape(key) + r'[ \t]*=[ \t]*"([^"\n]*)"[ \t]*$', re.M)
m = row.search(body)
if not m:
    sys.exit(f"compose: [{table}] has no `{key}` row")
if re.search(r"^pub mod plane_door\b", lib, re.M):
    want = "busbar_plane_decisions::plane_door"
elif re.search(r"^pub use plane_door::door;", lib, re.M):
    want = "busbar_plane_decisions"
else:
    sys.exit("compose: this plane exports its door neither as `pub mod plane_door` nor at its root")
if m.group(1) != want:
    body = body[:m.start()] + f'{key} = "{want}"' + body[m.end():]
    text = text[:sec.start(1)] + body + text[sec.end(1):]
    print(f'compose: [{table}] {key} = "{want}" (the pin named "{m.group(1)}")')
else:
    print(f'compose: [{table}] {key} = "{want}" (unchanged)')
open(manifest, "w").write(text)
PY
}

if [ "${1:-}" = --selftest ]; then
  t="$(mktemp -d)"; trap 'rm -rf "$t"' EXIT
  mkdir -p "$t/root/src" "$t/module/src" "$t/none/src"
  printf 'mod plane_door;\npub use plane_door::door;\n' > "$t/root/src/lib.rs"
  printf 'pub mod plane_door;\n' > "$t/module/src/lib.rs"
  printf 'mod plane_door;\n' > "$t/none/src/lib.rs"
  m() { printf '[package.metadata.busbar.linked-entry]\nplane-llm-door = "busbar_plane_llm::plane_door"\nplane-decisions = "%s"\n\n[package.metadata.busbar.linked-declares]\nplane-llm-door = "x"\n' "$1" > "$t/Cargo.toml"; }
  ok=0
  m busbar_plane_decisions::plane_door; rewrite "$t/Cargo.toml" "$t/root" 2>/dev/null
  grep -qx 'plane-decisions = "busbar_plane_decisions"' "$t/Cargo.toml" && grep -qx 'plane-llm-door = "busbar_plane_llm::plane_door"' "$t/Cargo.toml" \
    && echo "  ok: a crate-root door is named at the crate root, other rows kept" || { echo "  FAIL: crate-root rewrite"; ok=1; }
  m busbar_plane_decisions::plane_door; rewrite "$t/Cargo.toml" "$t/module" 2>/dev/null
  grep -qx 'plane-decisions = "busbar_plane_decisions::plane_door"' "$t/Cargo.toml" \
    && echo "  ok: a public plane_door module keeps the pin's row" || { echo "  FAIL: module row"; ok=1; }
  m busbar_plane_decisions::plane_door
  if rewrite "$t/Cargo.toml" "$t/none" 2>/dev/null; then echo "  FAIL: a plane exporting no door was composed"; ok=1; else echo "  ok: a plane exporting no door is refused"; fi
  printf '[package.metadata.busbar.linked-entry]\nplane-llm-door = "x"\n' > "$t/Cargo.toml"
  if rewrite "$t/Cargo.toml" "$t/root" 2>/dev/null; then echo "  FAIL: a dist with no decisions row was composed"; ok=1; else echo "  ok: a dist with no decisions row is refused"; fi
  exit "$ok"
fi

checkout="${1:?busbar checkout}"; plane="${2:?plane crate dir}"; stage="${3:?stage dir}"
plane="$(cd "$plane" && pwd)"
rm -rf "$stage"; mkdir -p "$stage"
tar -C "$checkout" --exclude=./target --exclude=./dist/default/target --exclude=./.git -cf - . | tar -C "$stage" -xf -
rewrite "$stage/dist/default/Cargo.toml" "$plane"
printf '%s\n' "$stage/dist/default/Cargo.toml"
