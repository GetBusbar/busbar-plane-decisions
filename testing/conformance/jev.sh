#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# The `jev` conformance suite (busbar conformance/registry.toml `jev`): one command, five judged legs,
# one verdict. Ported from busbar's retired Rust rig (xtask/src/conformance_record/jev.rs, deleted
# with the gate machinery in busbar dd677873c4); its checks and bytes are that rig's, judged by
# testing/jev-conformance/judge.py.
#
#   judge-regressions the judge's own regression tests (testing/jev-conformance/test_judge.py).
#   battery           this plane's own tests (`cargo test -p busbar-plane-decisions`): the codec, the
#                     PII witness and the door. A battery that passes zero tests is red.
#   control           the jev reference relay (testing/jev-conformance/reference_relay.py): every
#                     wire check must pass against a relay that keeps the plane's rules.
#   negative control  the reference relay with one deliberate deviation at a time: every deviation
#                     must turn the wire check it breaks red.
#   subject           busbar at the pin ($BUSBAR_CHECKOUT, its dist/default composition) with THIS
#                     checkout's plane patched in by a run-time cargo --config (no manifest is
#                     written), booted on loopback three times (one model; two models with an
#                     upstream_model; a jev and a nanojev model): every wire check and every busbar
#                     check (ledger, audit, the Teller-step gating checks) must pass.
#
# Run by testing/conformance/run.sh with CONFORMANCE_OUT, BUSBAR_CHECKOUT and CARGO_TARGET_DIR set.
# `pass` only when every leg passed; a subject that does not build is `fail`, armed, with the reason.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$here" || exit 2
: "${CONFORMANCE_OUT:?}" "${BUSBAR_CHECKOUT:?}"
out="$CONFORMANCE_OUT"; mkdir -p "$out"
target="${CARGO_TARGET_DIR:-$here/target}"
judge="$here/testing/jev-conformance/judge.py"

say() { printf 'jev: %s\n' "$*" >&2; }
# The negative controls the verdict's plan counts: reference_relay.BREAKS, read where it is defined.
breaks="$(python3 -I -B -c 'import sys; sys.path.insert(0, sys.argv[1]); import reference_relay; print(len(reference_relay.BREAKS))' "$here/testing/jev-conformance" 2>/dev/null || echo "?")"
failed=""

verdict() { # <status> <reason>
  python3 -I - "$out/verdict.json" "$1" "$2" "$breaks" <<'PY'
import json, sys
path, status, reason, breaks = sys.argv[1:5]
json.dump({"suite": "jev", "status": status, "armed": True, "standard": "jev decision plane",
           "plan": f"judge regressions + plane battery + reference-relay control + {breaks} broken-relay negative controls + "
                   "served POST /v1/systemone (one model; two models with upstream_model) and "
                   "POST /api/evaluate (nanojev) on booted busbars, with the ledger, audit and "
                   "Teller-step gating checks",
           "reason": reason}, open(path, "w"))
PY
}

leg() { # <name> <command...>: run, tee to $out/<name>.log, record failure
  local name="$1"; shift
  say "== $name"
  if "$@" >"$out/$name.log" 2>&1; then say "$name: pass"; tail -1 "$out/$name.log" >&2; else
    say "$name: FAIL (tail of $name.log follows)"; tail -40 "$out/$name.log" >&2; failed="$failed $name"; fi
}

battery() {
  cargo test --locked -p busbar-plane-decisions --target-dir "$target" 2>&1 | tee "$out/battery.out"
  local rc="${PIPESTATUS[0]}"
  [ "$rc" -eq 0 ] || return "$rc"
  local n
  n="$(sed -nE 's/^test result: ok\. ([0-9]+) passed.*/\1/p' "$out/battery.out" | awk '{s+=$1} END {print s+0}')"
  echo "battery: $n test(s) passed"
  [ "$n" -gt 0 ] || { echo "battery: cargo test passed 0 tests; a battery that selects nothing proves nothing"; return 1; }
}

leg judge-regressions python3 -I -B "$here/testing/jev-conformance/test_judge.py"
leg battery battery
leg control python3 -I "$judge" control "$out/control"
leg negative-control python3 -I "$judge" negative "$out/negative"

# The subject: busbar at the pin, its default distribution staged with its decisions row pointed at
# this plane's door (testing/jev-conformance/compose.sh), this checkout's plane patched in.
# Debug profile, no debug info: the served behaviour is the release's, the build a fraction of it.
say "== build subject (busbar ${BUSBAR_PIN:-?} dist/default with this plane)"
bin=""
if ! manifest="$(bash "$here/testing/jev-conformance/compose.sh" "$BUSBAR_CHECKOUT" "$here/plane-decisions" "$out/busbar-staged" 2>"$out/compose.log")"; then
  cat "$out/compose.log" >&2
  verdict fail "the subject's composition could not be staged: $(tail -1 "$out/compose.log")"
  exit 1
fi
cat "$out/compose.log" >&2
# Cargo keys a [patch] by the dependency's source URL, so the patch names the git source the staged
# distribution pulls this plane from (its `busbar-plane-decisions = { git = ... }` row), whatever host
# that is at the pin.
source="$(python3 -I - "$manifest" <<'PY'
import re, sys
m = re.search(r'^busbar-plane-decisions\s*=\s*\{[^}\n]*\bgit\s*=\s*"([^"]+)"', open(sys.argv[1]).read(), re.M)
print(m.group(1) if m else "")
PY
)"
if [ -z "$source" ]; then
  verdict fail "the staged distribution ($manifest) names no git source for busbar-plane-decisions to patch"
  exit 1
fi
patch="patch.\"$source\".busbar-plane-decisions.path=\"$here/plane-decisions\""
if CARGO_PROFILE_DEV_DEBUG=0 cargo build --manifest-path "$manifest" --bin busbar \
      --target-dir "$target" \
      --config "$patch" >"$out/subject-build.log" 2>&1; then
  bin="$target/debug/busbar"
  [ -x "$bin" ] || { say "built binary missing at $bin"; bin=""; }
fi
if [ -z "$bin" ]; then
  tail -40 "$out/subject-build.log" >&2
  verdict fail "the subject (busbar at the pin with this plane) did not build; legs failed before:${failed:- none}"
  exit 1
fi
# The plane busbar links must be this checkout's, not the pin's: the patch is refused silently
# when it matches nothing, so the build log must show it compiled from here.
if ! grep -q "busbar-plane-decisions .*($here/plane-decisions)" "$out/subject-build.log"; then
  if ! cargo tree --manifest-path "$manifest" -i busbar-plane-decisions --config "$patch" 2>/dev/null \
        | head -1 | grep -q "($here/plane-decisions)"; then
    verdict fail "the subject did not link this checkout's plane (the patch matched nothing)"
    exit 1
  fi
fi
leg subject python3 -I "$judge" subject "$out/subject" "$bin"

if [ -z "$failed" ]; then
  # A pass whose verdict was not written is no pass: the harness would read none, or a stale one.
  verdict pass "every leg passed (judge regressions, battery, control, negative control, subject)" || exit 1
  exit 0
fi
verdict fail "legs failed:$failed (logs in the check output)"; exit 1
