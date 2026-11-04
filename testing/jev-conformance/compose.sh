#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# THE SUBJECT'S COMPOSITION: the busbar package the `jev` suite builds, with this checkout's plane in it.
#
#   compose.sh <busbar checkout> <this plane's crate dir> <stage dir>   prints the manifest to build
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


checkout="${1:?busbar checkout}"; plane="${2:?plane crate dir}"; stage="${3:?stage dir}"
plane="$(cd "$plane" && pwd)"
rm -rf "$stage"; mkdir -p "$stage"
tar -C "$checkout" --exclude=./target --exclude=./dist/default/target --exclude=./.git -cf - . | tar -C "$stage" -xf -
rewrite "$stage/dist/default/Cargo.toml" "$plane"
printf '%s\n' "$stage/dist/default/Cargo.toml"
