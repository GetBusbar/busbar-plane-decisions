#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""THE jev CONFORMANCE JUDGE: the served legs of the `jev` suite (testing/conformance/jev.sh).

Ported from busbar's retired Rust rig (xtask/src/conformance_record/jev.rs, deleted with the gate
machinery in busbar dd677873c4); the checks, request bytes and judgements are that rig's.

Three legs, each a mode of this program, each judging what it measured itself:

  control   the reference relay (reference_relay.py, unbroken): every WIRE check must pass.
  negative  the reference relay with each deliberate deviation in BREAKS: the wire check that
            deviation breaks must grade red, for every one of them (else the check bites nothing).
  subject   busbar (the binary given) booted three times on loopback with decisions models over a
            far end this program owns: every WIRE check and every BUSBAR check must pass.

The WIRE checks hold any jev relay to the plane's rules: the caller's bytes reach the far end at the
plane's path, the far end sees the provider's credential and never the caller's, a caller header
outside the governed set reaches it unchanged, the far end's 2xx and 4xx reach the caller byte for
byte, a far-end 5xx ends terminal in jev's refusal shape, an unauthenticated or forged caller is
refused 401 before any hop, routing by model (one, several, unknown, `upstream_model` spliced),
and the `nanojev` dialect on `/api/evaluate` (served, the wrong dialect refused, a success with no
`execution.states` relayed and billed at the plane's floor).

The BUSBAR checks need busbar's admin surface and run on the subject only: billable-success-only
on the ledger, and the gating checks, one per Teller step (authenticate, verify, admit, route, meter,
audit, exit), each judged over deltas read on the same boot.

"""
import http.client
import http.server
import json
import os
import secrets
import signal
import socket
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.dont_write_bytecode = True
sys.path.insert(0, HERE)
import reference_relay  # noqa: E402

MODEL = "jev-1"
PROVIDER = "typesafe"
PATH = "/v1/systemone"
UPSTREAM_MODEL = "jev-2"
UPSTREAM_NAME = "jev-upstream-2"
NANOJEV_PROVIDER = "nanoprov"
NANOJEV_MODEL = "nano-1"
PATH_EVALUATE = "/api/evaluate"
PROBE_HEADER = "x-jev-conformance-probe"

# The caller's request: odd whitespace and member order on purpose, so a relay that re-serialises
# the document instead of forwarding its bytes is caught.
REQUEST = b'{ "state" : {"session":"conformance-state","n":[1, 2 ,3]},\n  "context":{} }'
SUCCESS = b'{"request_id" : "req_conformance", "usage":{"units":42},"answers":{"decision":"approve"}}'
UNPROCESSABLE = (b'{"error":{"code":"invalid_state","message":"bad transition"},'
                 b'"state":{"session":"conformance-state"},"answers":{"leaked":"never"}}')
UNAVAILABLE = b'{"error":{"code":"unavailable","message":"the decision service is down"}}'
NANOJEV_REQUEST = b'{ "input" : {"case":"conformance"} }'
NANOJEV_SUCCESS = b'{"result":{"decision":"approve"}, "execution" : {"states":3}}'
NANOJEV_NO_COUNT = b'{"result":{"decision":"approve"}}'
REPORTED_UNITS = 42
# The plane's floor for a success that states no decision count (driven.rs `Source::Floor`): one.
FLOOR_UNITS = 1
AUDIT_OP_CLASS = "systemone"
ONE_REQUEST_GROUP = "jev-conformance-one-request"
ELSEWHERE = "jev-conformance-elsewhere"
ABSENT_PATH = "/.jev-conformance/no-such-route-any-plane-could-own"

SETTLE = 0.3
OWED_WITHIN = 5.0


def show(b):
    s = b.decode("utf-8", "replace") if isinstance(b, (bytes, bytearray)) else str(b)
    return s if len(s) <= 160 else s[:160] + "…"


class Red(Exception):
    pass


def check(ok, finding):
    if not ok:
        raise Red(finding() if callable(finding) else finding)


# What a check that could not read its target raises, beside a Red: a dropped or reset connection,
# a timeout, a malformed answer, a body of an unexpected shape. Graded red per check, never a crash.
UNREAD = (OSError, ValueError, AttributeError, TypeError, KeyError, http.client.HTTPException)
# How run_checks words a check that could not read its target (not a judged finding).
UNREAD_FINDING = "the check could not read the target"


def run_checks(pairs):
    """Each (name, fn) run in turn, as (name, None | finding) pairs: a Red is its finding, and a
    target the check could not read is a finding too, so one bad answer never loses the others."""
    out = []
    for name, fn in pairs:
        try:
            fn()
            out.append((name, None))
        except Red as e:
            out.append((name, str(e)))
        except UNREAD as e:
            out.append((name, f"{UNREAD_FINDING}: {type(e).__name__}: {e}"))
    return out


# ── the far end ──────────────────────────────────────────────────────────────────────────────


class FarEnd:
    """A loopback jev provider: records every request, answers from a queue whose last reply sticks."""

    def __init__(self):
        far = self
        self.seen = []
        self.replies = [(200, SUCCESS)]
        self.lock = threading.Lock()

        class H(http.server.BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *_):
                pass

            def do_POST(self):
                body = self.rfile.read(int(self.headers.get("content-length") or 0))
                with far.lock:
                    far.seen.append({"method": "POST", "path": self.path,
                                     "headers": [(k.lower(), v) for k, v in self.headers.items()],
                                     "body": body})
                    status, answer = far.replies.pop(0) if len(far.replies) > 1 else far.replies[0]
                self.send_response(status)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(answer)))
                self.end_headers()
                self.wfile.write(answer)

        self.srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.srv.daemon_threads = True
        self.port = self.srv.server_address[1]
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()

    def reply(self, *replies):
        with self.lock:
            self.replies = list(replies)

    def hops(self):
        with self.lock:
            return len(self.seen)

    def last(self):
        with self.lock:
            return self.seen[-1] if self.seen else None

    def close(self):
        self.srv.shutdown()
        self.srv.server_close()


def header(req, name):
    return [v for k, v in req["headers"] if k == name]


# ── HTTP ─────────────────────────────────────────────────────────────────────────────────────


def http_call(port, method, path, body=None, headers=None, timeout=30):
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    try:
        conn.request(method, path, body=body, headers=headers or {})
        r = conn.getresponse()
        return r.status, r.read()
    finally:
        conn.close()


class Call:
    def __init__(self, status, body, hops):
        self.status, self.body, self.hops = status, body, hops

    def __repr__(self):
        return f"{self.status} `{show(self.body)}` over {self.hops} hop(s)"


class Target:
    """One served relay: its data port, its far end, a caller credential, and (busbar only) its admin."""

    def __init__(self, data, far, bearer, provider_key, admin=None, admin_token=None, client_id=None,
                 mint=None, stop=None):
        self.data, self.far, self.bearer, self.provider_key = data, far, bearer, provider_key
        self.admin, self.admin_token, self.client_id, self.mint, self.stop = admin, admin_token, client_id, mint, stop
        self.nonce = secrets.token_hex(8)

    def call(self, path, body, bearer="caller", extra=None):
        bearer = self.bearer if bearer == "caller" else bearer
        h = {"content-type": "application/json", PROBE_HEADER: self.nonce}
        if bearer:
            h["authorization"] = bearer
        h.update(extra or {})
        before = self.far.hops()
        status, answer = http_call(self.data, "POST", path, body, h)
        return Call(status, answer, self.far.hops() - before)

    def admin_get(self, path):
        status, body = http_call(self.admin, "GET", path, None, {"authorization": "Bearer " + self.admin_token})
        check(status == 200, lambda: f"GET {path} answered {status} `{show(body)}`")
        return body


# ── pure judgements ─────────────────────────────────────────────────────


def jev_refusal_code(body):
    try:
        v = json.loads(body)
    except ValueError as e:
        raise Red(f"the refusal body is not JSON ({e}): `{show(body)}`")
    check(isinstance(v, dict), "the refusal body is not an object")
    err = v.get("error")
    check(isinstance(err, dict), "the refusal body has no `error` object")
    check(len(v) == 1 and len(err) == 2,
          lambda: f'the refusal body carries members beyond {{"error":{{"code","message"}}}}: {show(body)}')
    c, m = err.get("code"), err.get("message")
    check(isinstance(c, str) and c and isinstance(m, str) and m,
          lambda: f'the refusal body is not {{"error":{{"code":<word>,"message":<text>}}}}: {show(body)}')
    return c


def is_jev_refusal(body, code):
    got = jev_refusal_code(body)
    check(got == code, lambda: f"the refusal's code is `{got}`, not `{code}`")


def judge_refused_before_dial(call, status, code):
    check(call.status == status, lambda: f"answered {call.status} `{show(call.body)}`, not the {status} refusal")
    check(call.hops == 0, lambda: f"the refused unit reached the far end ({call.hops} hop(s)); it must be refused before the dial")
    is_jev_refusal(call.body, code)


def served_once(call, success=SUCCESS):
    check(call.status == 200 and call.body == success and call.hops == 1,
          lambda: f"a success answered {call!r}, not the far end's 200 bytes over one")


def judge_terminal_5xx(call, far_bytes):
    check(500 <= call.status <= 599, lambda: f"the unit answered {call!r}, not a terminal 5xx")
    check(call.hops >= 1, "the unit never dialled the lane it was routed to")
    check(call.body != far_bytes, "the far end's own failure bytes were relayed; a failed unit ends in the relay's terminal")
    jev_refusal_code(call.body)


def judge_upstream_model(call, sent, received, expected=None):
    served_once(call)
    want = expected if expected is not None else sent.replace(f'"{UPSTREAM_MODEL}"'.encode(), f'"{UPSTREAM_NAME}"'.encode(), 1)
    check(received == want, lambda: f"the far end received `{show(received)}`, not the caller's bytes with `model` spliced to `{UPSTREAM_NAME}`: `{show(want)}`")


def ledger_spend(totals, bucket, lane=MODEL):
    """(fees, micros) over GET /api/v1/admin/ledger/totals rows: `bucket` None reads every row; else
    the jev lane's rows (a row naming the lane) or, at the 1.6.0 width (no lane, no provider), the
    caller `bucket`'s own."""
    try:
        v = json.loads(totals)
    except ValueError as e:
        raise Red(f"ledger/totals is not JSON ({e})")
    rows = v.get("rows") if isinstance(v, dict) else None
    check(isinstance(rows, list), "ledger/totals has no `rows`")
    fees = micros = 0
    for r in rows:
        if bucket is not None:
            rl, prov = r.get("lane"), r.get("provider")
            if not ((lane is not None and rl == lane and prov == PROVIDER)
                    or (rl == "" and prov == "" and r.get("bucket") == bucket)):
                continue
        fees += int(r.get("fee_count") or 0)
        text = r.get("priced_micros")
        check(isinstance(text, str), "a ledger row carries no priced_micros text")
        check(text.isdigit(), lambda: f"priced_micros `{text}` is not a non-negative integer")
        micros += int(text)
    return fees, micros


def spend_of(t):
    """The caller's spend on the node: its own bucket's rows (the 1.6.0 width)."""
    return ledger_spend(t.admin_get("/api/v1/admin/ledger/totals"), t.client_id, lane=None)


def judge_jev_ledger(totals, bucket):
    fees, micros = ledger_spend(totals, bucket)
    check(fees == 1, lambda: f"ledger/totals counts {fees} fee(s) on lane {MODEL}/{PROVIDER} after one success and one 422; billable-success-only is exactly 1")
    check(micros == REPORTED_UNITS, lambda: f"the jev lane is priced at {micros} micro-unit(s); the far end reported {REPORTED_UNITS} units at 1 micro-unit each")


def moved_by(before, after, n):
    want = n * REPORTED_UNITS
    check(after[0] - before[0] == n and after[1] - before[1] == want,
          lambda: f"the jev lane moved from {before[0]} fee(s) / {before[1]} micro-unit(s) to {after[0]} / {after[1]} over {n} served request(s); exactly {n} fee(s) and {want} micro-unit(s) are owed")


def unchanged(before, after):
    check(before == after, lambda: f"the ledger moved from {before} to {after}; a refused unit is charged nothing")


def judge_audit_window(rng, want):
    try:
        v = json.loads(rng)
    except ValueError as e:
        raise Red(f"the audit range is not JSON ({e})")
    records = v.get("records") if isinstance(v, dict) else None
    check(isinstance(records, list), "the audit range has no `records`")
    mine = [r for r in records if r.get("op_class") == AUDIT_OP_CLASS]
    check(len(mine) == want, lambda: f"the audit chain gained {len(mine)} `{AUDIT_OP_CLASS}` record(s) over {want} served unit(s); exactly {want} is owed")
    bad = [r for r in mine if r.get("outcome") != "Completed"]
    check(not bad, lambda: f"a served unit's record carries outcome {bad[0].get('outcome')!r}, not Completed")


# ── the WIRE checks: any jev relay ───────────────────────────────────────────────────────────


def wire_one(t):
    """The one-model boot's wire checks, as (name, None | finding) pairs."""
    checks = []

    def run(name, fn):
        checks.append((name, fn))

    absent = http_call(t.data, "POST", ABSENT_PATH, b"{}", {"content-type": "application/json", "authorization": t.bearer})[0]
    t.far.reply((200, SUCCESS))
    ok = t.call(PATH, REQUEST)
    first = t.far.last() if ok.hops else None

    def served():
        check(ok.status != absent, lambda: f"not served: POST {PATH} answers {ok.status}, the absence code on this boot; one decisions model is configured and no route serves its operation")
    run("served", served)

    def relay_request():
        check(first is not None, "the far end received nothing")
        check(ok.hops == 1 and first["path"] == PATH and first["body"] == REQUEST,
              lambda: f"the far end received {ok.hops} request(s); the last was POST {first['path']} with body `{show(first['body'])}`, not POST {PATH} with the caller's bytes")
    run("relay.request", relay_request)

    def relay_credential():
        check(first is not None, "the far end received nothing")
        auth = header(first, "authorization")
        caller_key = t.bearer.split(" ", 1)[1]
        leaked = any(caller_key in v for _, v in first["headers"])
        check(auth == ["Bearer " + t.provider_key] and not leaked,
              lambda: f"the far end saw {len(auth)} authorization value(s){', one carrying the caller key' if leaked else ''}; it must see the provider's credential alone")
    run("relay.credential", relay_credential)

    def client_headers():
        check(first is not None, "the far end received nothing")
        got = header(first, PROBE_HEADER)
        check(got == [t.nonce], lambda: f"the caller's `{PROBE_HEADER}` reached the far end as {got}")
    run("relay.client-headers", client_headers)

    def relay_response():
        check(ok.status == 200 and ok.body == SUCCESS, lambda: f"the caller got {ok!r}, not the far end's 200 bytes")
    run("relay.response", relay_response)

    def relay_error():
        t.far.reply((422, UNPROCESSABLE))
        c = t.call(PATH, REQUEST)
        check(c.status == 422 and c.body == UNPROCESSABLE, lambda: f"the caller got {c!r}, not the far end's 422 bytes")
    run("relay.error", relay_error)

    def unauthenticated():
        judge_refused_before_dial(t.call(PATH, REQUEST, bearer=None), 401, "invalid_request")
    run("refusal.unauthenticated", unauthenticated)

    def forged():
        key = t.bearer.split(" ", 1)[1]
        judge_refused_before_dial(t.call(PATH, REQUEST, bearer="Bearer " + key[:-1] + ("b" if key.endswith("a") else "a")), 401, "invalid_request")
    run("refusal.forged-credential", forged)

    def null_model():
        judge_refused_before_dial(t.call(PATH, b'{"model":null}'), 400, "invalid_request")
    run("models.null-400", null_model)
    out = run_checks(checks)
    t.far.reply((200, SUCCESS))
    return out


def wire_transient(t):
    """Last on a boot: the sole member down benches it for its cooldown."""
    t.far.reply((503, UNAVAILABLE), (503, UNAVAILABLE))
    return run_checks([("relay.transient-terminal",
                        lambda: judge_terminal_5xx(t.call(PATH, REQUEST), UNAVAILABLE))])


def models_request(model):
    if model is None:
        return REQUEST
    return ('{ "model" :  "%s" , "state" : {"session":"conformance-state"},\n  "context":{} }' % model).encode()


def wire_models(t):
    def splice(sent, expected):
        t.far.reply((200, SUCCESS))
        c = t.call(PATH, sent)
        judge_upstream_model(c, sent, (t.far.last() or {}).get("body", b""), expected=expected)

    return run_checks([
        ("models.unnamed-400", lambda: judge_refused_before_dial(t.call(PATH, models_request(None)), 400, "invalid_request")),
        ("models.unknown-404", lambda: judge_refused_before_dial(t.call(PATH, models_request("jev-no-such-model")), 404, "invalid_params")),
        ("models.upstream-model", lambda: (t.far.reply((200, SUCCESS)),
                                           judge_upstream_model(t.call(PATH, models_request(UPSTREAM_MODEL)),
                                                                models_request(UPSTREAM_MODEL),
                                                                (t.far.last() or {}).get("body", b"")))),
        # Explicit expected wire bytes keep the oracle independent of either span scanner.
        ("models.last-model-spliced", lambda: splice(
            b'{"model":"jev-no-such-model", "model":"jev-2","state":{}}',
            b'{"model":"jev-no-such-model", "model":"jev-upstream-2","state":{}}')),
        ("models.escaped-model-spliced", lambda: splice(
            br'{"mod\u0065l":"jev-2","state":{}}',
            br'{"mod\u0065l":"jev-upstream-2","state":{}}')),
    ])


def wire_nanojev(t):
    def served():
        t.far.reply((200, NANOJEV_SUCCESS))
        c = t.call(PATH_EVALUATE, NANOJEV_REQUEST)
        served_once(c, NANOJEV_SUCCESS)
        last = t.far.last()
        check(last["path"] == PATH_EVALUATE and last["body"] == NANOJEV_REQUEST,
              lambda: f"the far end was asked at `{last['path']}` with `{show(last['body'])}`, not at {PATH_EVALUATE} with the caller's bytes")

    def wrong_dialect():
        judge_refused_before_dial(t.call(PATH_EVALUATE, models_request(MODEL)), 404, "invalid_params")

    def no_count():
        # Owner ruling 2026-10-27 (plane-decisions #46): a success with no decision count is relayed
        # as the far end wrote it and bills the plane's floor; a unit that succeeded is never failed.
        t.far.reply((200, NANOJEV_NO_COUNT))
        before = quiet(lambda: spend_of(t)) if t.admin else None
        c = t.call(PATH_EVALUATE, NANOJEV_REQUEST)
        served_once(c, NANOJEV_NO_COUNT)
        if before is not None:
            after = settled(lambda: spend_of(t), lambda s: s[0] >= before[0] + 1)
            check(after[0] - before[0] == 1 and after[1] - before[1] == FLOOR_UNITS,
                  lambda: f"an uncounted success moved the caller's ledger from {before} to {after}; one fee at the plane's floor of {FLOOR_UNITS} decision is owed")

    return run_checks([("nanojev.served", served), ("nanojev.wrong-dialect-404", wrong_dialect),
                       ("nanojev.no-count-floor", no_count)])


# ── the BUSBAR checks: the subject's admin surface ───────────────────────────────────────────


def settled(read, done):
    deadline = time.time() + OWED_WITHIN
    while True:
        v = read()
        if done(v) or time.time() >= deadline:
            time.sleep(SETTLE)
            return read()
        time.sleep(0.1)


def quiet(read):
    """`read()` once the node has stopped moving it: the work an earlier check left in flight (its
    ledger row, its audit seal) has landed, so a baseline taken now is this check's own."""
    deadline = time.time() + OWED_WITHIN
    last = read()
    while True:
        time.sleep(SETTLE)
        now = read()
        if now == last or time.time() >= deadline:
            return now
        last = now


def admin_object(body, what):
    try:
        v = json.loads(body)
    except ValueError as e:
        raise Red(f"{what} is not JSON ({e}): {show(body)}")
    check(isinstance(v, dict), lambda: f"{what} is not a JSON object: {show(body)}")
    return v


def busbar_checks(t):
    spend = lambda bucket: ledger_spend(t.admin_get("/api/v1/admin/ledger/totals"), bucket)  # noqa: E731

    def requests_of(key):
        body = t.admin_get(f"/api/v1/admin/keys/{key}/usage")
        n = admin_object(body, "the key's usage").get("requests")
        check(isinstance(n, int), lambda: f"the key's usage carries no `requests` count: {show(body)}")
        return n

    def next_seq():
        body = t.admin_get("/api/v1/admin/audit/head")
        n = admin_object(body, "the audit head").get("next_seq")
        check(isinstance(n, int), lambda: f"the audit head carries no `next_seq`: {show(body)}")
        return n

    def ops(rng):
        try:
            return sum(1 for r in json.loads(rng).get("records", []) if r.get("op_class") == AUDIT_OP_CLASS)
        except (ValueError, AttributeError):
            return 0

    def sealed_since(frm, want):
        def read():
            nxt = next_seq()
            if nxt <= frm:
                return b'{"records":[]}'
            return t.admin_get(f"/api/v1/admin/audit/range?from={frm}&to={nxt - 1}")
        return settled(read, lambda w: ops(w) >= want)

    def success(bearer="caller"):
        t.far.reply((200, SUCCESS))
        return t.call(PATH, REQUEST, bearer=bearer)

    def billable():
        judge_jev_ledger(settled(lambda: t.admin_get("/api/v1/admin/ledger/totals"),
                                 lambda b: ledger_spend(b, t.client_id)[0] >= 1), t.client_id)

    def authenticate():
        before = spend(None)
        key = t.bearer.split(" ", 1)[1]
        c = t.call(PATH, REQUEST, bearer="Bearer " + key[:-1] + ("b" if key.endswith("a") else "a"))
        after = settled(lambda: spend(None), lambda _: False)
        judge_refused_before_dial(c, 401, "invalid_request")
        unchanged(before, after)

    def meter():
        before = quiet(lambda: spend(t.client_id))
        c = success()
        after = settled(lambda: spend(t.client_id), lambda s: s[0] >= before[0] + 1)
        served_once(c)
        moved_by(before, after, 1)

    def audit():
        frm = quiet(next_seq)
        c = success()
        served_once(c)
        judge_audit_window(sealed_since(frm, 1), 1)

    def exit_():
        frm = quiet(next_seq)
        before = quiet(lambda: spend(t.client_id))
        calls = [success(), success()]
        after = settled(lambda: spend(t.client_id), lambda s: s[0] >= before[0] + 2)
        for c in calls:
            served_once(c)
        moved_by(before, after, 2)
        judge_audit_window(sealed_since(frm, 2), 2)

    def verify():
        bearer, key = t.mint({"name": ELSEWHERE, "allowed_pools": [ELSEWHERE]})
        t.far.reply((200, SUCCESS))
        before, was = spend(None), requests_of(key)
        c = t.call(PATH, REQUEST, bearer=bearer)
        after = settled(lambda: spend(None), lambda _: False)
        judge_refused_before_dial(c, 403, "unsupported_operation")
        unchanged(before, after)
        now = requests_of(key)
        check(was == now, lambda: f"the key's usage counts {was} request(s) before the refusal and {now} after; a refused unit counts none")

    def admit():
        bearer, key = t.mint({"name": ONE_REQUEST_GROUP, "group": ONE_REQUEST_GROUP})
        start = spend(None)
        first = success(bearer)
        before = settled(lambda: spend(None), lambda s: s[0] >= start[0] + 1)
        was = requests_of(key)
        check(first.status == 200 and first.hops == 1,
              lambda: f"the key's one budgeted request answered {first!r}, not a served 200; the refusal after it judges nothing")
        t.far.reply((200, SUCCESS))
        second = t.call(PATH, REQUEST, bearer=bearer)
        after = settled(lambda: spend(None), lambda _: False)
        judge_refused_before_dial(second, 429, "rate_limit")
        unchanged(before, after)
        now = requests_of(key)
        check(was == now, lambda: f"the key's usage counts {was} request(s) before the refusal and {now} after; a refused unit counts none")

    def route():
        t.far.reply((503, UNAVAILABLE), (503, UNAVAILABLE))
        before = spend(None)
        c = t.call(PATH, REQUEST)
        after = settled(lambda: spend(None), lambda _: False)
        judge_terminal_5xx(c, UNAVAILABLE)
        unchanged(before, after)

    # Last: the down lane benches the sole member for its cooldown.
    return run_checks([("usage.billable-success-only", billable), ("h2-authenticate-refusal", authenticate),
                       ("h2-meter-row", meter), ("h2-audit-record", audit), ("h2-exit-terminal", exit_),
                       ("h2-verify-refusal", verify), ("h2-admit-refusal", admit), ("h2-route-terminal", route)])


# ── the targets: the reference relay, or busbar ──────────────────────────────────────────────


def free_ports(n):
    """`n` distinct loopback ports, all held until all are chosen (so no two are the same); each is
    free again for the target to bind."""
    held = []
    try:
        for _ in range(n):
            s = socket.socket()
            held.append(s)
            s.bind(("127.0.0.1", 0))
        return [s.getsockname()[1] for s in held]
    finally:
        for s in held:
            s.close()


def free_port():
    return free_ports(1)[0]


def wait_listening(port, proc, budget, log=None):
    """The target accepts connections on `port` (a listener with no /healthz of its own)."""
    deadline = time.time() + budget
    while time.time() < deadline:
        if proc.poll() is not None:
            tail = open(log, errors="replace").read()[-4000:] if log else ""
            raise Red(f"the target exited ({proc.returncode}) before it listened on {port}: {tail}")
        try:
            socket.create_connection(("127.0.0.1", port), timeout=5).close()
            return
        except OSError:
            time.sleep(0.1)
    raise Red(f"the target did not listen on {port} within {budget}s")


def wait_healthy(port, proc, budget, log=None):
    deadline = time.time() + budget
    while time.time() < deadline:
        if proc.poll() is not None:
            tail = open(log, errors="replace").read()[-4000:] if log else ""
            raise Red(f"the target exited ({proc.returncode}) before it answered /healthz: {tail}")
        try:
            if http_call(port, "GET", "/healthz", timeout=5)[0] == 200:
                return
        except OSError:
            pass
        time.sleep(0.1)
    raise Red(f"the target did not answer /healthz within {budget}s")


SCENARIOS = {
    # name: (providers {name: protocol}, models {name: {provider, upstream_model?}})
    "one": ({PROVIDER: "jev"}, {MODEL: {"provider": PROVIDER}}),
    "models": ({PROVIDER: "jev"}, {MODEL: {"provider": PROVIDER},
                                   UPSTREAM_MODEL: {"provider": PROVIDER, "upstream_model": UPSTREAM_NAME}}),
    "nanojev": ({PROVIDER: "jev", NANOJEV_PROVIDER: "nanojev"},
                {MODEL: {"provider": PROVIDER}, NANOJEV_MODEL: {"provider": NANOJEV_PROVIDER}}),
}


def relay_target(scenario, workdir, brk=""):
    providers, models = SCENARIOS[scenario]
    far = FarEnd()
    try:
        port = free_port()
        caller, provider_key = secrets.token_hex(16), secrets.token_hex(16)
        cfg = {"listen": port, "far": f"http://127.0.0.1:{far.port}", "callers": [caller],
               "providers": {p: {"key": provider_key, "protocol": proto} for p, proto in providers.items()},
               "models": models, "break": brk}
        path = os.path.join(workdir, f"relay-{scenario}-{brk or 'control'}.json")
        with open(path, "w") as f:
            json.dump(cfg, f)
        proc = subprocess.Popen([sys.executable, "-I", os.path.join(HERE, "reference_relay.py"), path])
    except BaseException:
        far.close()
        raise
    try:
        wait_healthy(port, proc, 20)
    except BaseException:
        proc.kill()
        proc.wait()
        far.close()
        raise

    def stop():
        proc.kill()
        proc.wait()
        far.close()
    return Target(port, far, "Bearer " + caller, provider_key, stop=stop)


def busbar_config(scenario, data, admin, key_file):
    providers, models = SCENARIOS[scenario]
    lines = [f'listen: "127.0.0.1:{data}"', f'admin_listen: "127.0.0.1:{admin}"', "store: { module: memory }",
             "admin_require_mtls: false", "providers:"]
    for p in providers:
        lines.append(f"  {p}:\n    api_key: {{ env: JEV_CONFORMANCE_PROVIDER_KEY }}")
    lines += ["models: {}",
              "identity-providers:\n  admin-tokens: { module: admin-tokens, token: { env: BUSBAR_ADMIN_TOKEN } }",
              f"auth:\n  chain: [keys]\n  admin_auth: [admin-tokens]\n  signing_key: {{ file: \"{key_file}\" }}",
              'advanced:\n  allow_destinations: ["127.0.0.1"]']
    if scenario == "one":
        lines.append(f"groups:\n  {ONE_REQUEST_GROUP}:\n    limits:\n      - {{ requests: 1, per: day }}")
    lines.append("decisions:\n  models:")
    for m, e in models.items():
        lines.append(f"    {m}:\n      provider: {e['provider']}")
        if e.get("upstream_model"):
            lines.append(f"      upstream_model: {e['upstream_model']}")
    lines.append("  rate_card:")
    for m in models:
        lines.append(f"    {m}: {{ units: {{ decision: 1, input_tokens: 0, output_tokens: 0 }} }}")
    return "\n".join(lines) + "\n"


def busbar_target(scenario, workdir, binary):
    providers, _ = SCENARIOS[scenario]
    d = os.path.join(workdir, f"busbar-{scenario}")
    os.makedirs(d, exist_ok=True)
    far = FarEnd()
    try:
        data, admin = free_ports(2)
        admin_token, provider_key = secrets.token_hex(24), secrets.token_hex(16)
        key = subprocess.run([binary, "--generate-signing-key"], capture_output=True)
        if key.returncode != 0:
            raise Red(f"busbar --generate-signing-key exited {key.returncode}: {show(key.stderr)}")
        key_file = os.path.join(d, "signing.key")
        with open(key_file, "wb") as f:
            f.write(key.stdout)
        with open(os.path.join(d, "providers.yaml"), "w") as f:
            for p, proto in providers.items():
                f.write(f"{p}:\n  protocol: {proto}\n  base_url: http://127.0.0.1:{far.port}\n  error_map: {{}}\n")
        with open(os.path.join(d, "config.yaml"), "w") as f:
            f.write(busbar_config(scenario, data, admin, key_file))
        log = os.path.join(d, "busbar.log")
        env = dict(os.environ, BUSBAR_CONFIG=os.path.join(d, "config.yaml"),
                   BUSBAR_PROVIDERS=os.path.join(d, "providers.yaml"),
                   BUSBAR_ADMIN_TOKEN=admin_token, JEV_CONFORMANCE_PROVIDER_KEY=provider_key)
        lf = open(log, "wb")
        try:
            proc = subprocess.Popen([binary], env=env, stdout=lf, stderr=subprocess.STDOUT, cwd=d)
        except BaseException:
            lf.close()
            raise
    except BaseException:
        far.close()
        raise

    def stop():
        proc.terminate()
        try:
            proc.wait(15)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        lf.close()
        far.close()

    def mint(body):
        status, answer = http_call(admin, "POST", "/api/v1/admin/keys", json.dumps(body).encode(),
                                   {"authorization": "Bearer " + admin_token, "content-type": "application/json"})
        try:
            v = json.loads(answer)
        except ValueError:
            v = {}
        check(isinstance(v, dict) and v.get("token") and v.get("id"),
              lambda: f"the admin API minted no key with a token and an id ({status}: {show(answer)})")
        return "Bearer " + v["token"], v["id"]

    try:
        wait_healthy(data, proc, 60, log)
        wait_listening(admin, proc, 30, log)
        bearer, client_id = mint({"name": f"jev-{scenario}"})
    except BaseException:
        stop()
        raise
    return Target(data, far, bearer, provider_key, admin=admin, admin_token=admin_token,
                  client_id=client_id, mint=mint, stop=stop)


def run_wire(make, busbar=False):
    """Every boot's checks over the targets `make(scenario)` serves."""
    results = []
    # Last on the one-model boot: the down lane benches the sole member. On busbar the route check
    # (h2-route-terminal) is the transient judgement plus the ledger, so it stands in for it.
    for scenario, checks in [("one", lambda t: wire_one(t) + (busbar_checks(t) if busbar else wire_transient(t))),
                             ("models", wire_models), ("nanojev", wire_nanojev)]:
        try:
            t = make(scenario)
        except Red as e:
            results.append((f"{scenario}.boot", str(e)))
            continue
        except UNREAD as e:
            results.append((f"{scenario}.boot", f"the target could not be booted: {type(e).__name__}: {e}"))
            continue
        try:
            results += checks(t)
        except UNREAD as e:
            # A boot's own reads (before any named check) failed: the boot is red, the others run.
            results.append((f"{scenario}.checks", f"the boot's checks could not read the target: {type(e).__name__}: {e}"))
        finally:
            t.stop()
    return results


def report(results, path):
    with open(path, "w") as f:
        json.dump([{"check": n, "ok": e is None, "finding": e} for n, e in results], f, indent=2)
    for n, e in results:
        print(f"  {'ok ' if e is None else 'RED'} {n}{'' if e is None else ': ' + e}")


USAGE = "usage: judge.py control|negative <out> | subject <out> <busbar>"


def main(argv):
    mode = argv[1] if len(argv) > 1 else ""
    want = {"control": 3, "negative": 3, "subject": 4}.get(mode)
    if want is None or len(argv) != want:
        print(USAGE, file=sys.stderr)
        return 2
    out = argv[2]
    os.makedirs(out, exist_ok=True)
    if mode == "control":
        res = run_wire(lambda s: relay_target(s, out))
        report(res, os.path.join(out, "control.json"))
        red = [n for n, e in res if e]
        print(f"control: {len(res) - len(red)} of {len(res)} wire checks pass on the reference relay")
        return 0 if res and not red else 1
    if mode == "negative":
        missed = []
        for brk, target_check in reference_relay.BREAKS.items():
            res = run_wire(lambda s: relay_target(s, out, brk))
            got = dict(res)
            if target_check not in got:
                missed.append(f"{brk}: the check {target_check} never ran")
            elif got[target_check] is None:
                missed.append(f"{brk}: {target_check} passed on a relay broken for it")
            elif got[target_check].startswith(UNREAD_FINDING):
                missed.append(f"{brk}: {target_check} never judged the break ({got[target_check][:120]})")
            else:
                print(f"  ok  {brk}: {target_check} is red ({got[target_check][:120]})")
        for m in missed:
            print(f"  RED {m}")
        print(f"negative: {len(reference_relay.BREAKS) - len(missed)} of {len(reference_relay.BREAKS)} deliberate breaks graded red")
        return 0 if not missed else 1
    if mode == "subject":
        binary = argv[3]
        res = run_wire(lambda s: busbar_target(s, out, binary), busbar=True)
        report(res, os.path.join(out, "subject.json"))
        red = [n for n, e in res if e]
        print(f"subject: {len(res) - len(red)} of {len(res)} checks pass on busbar")
        return 0 if res and not red else 1
    print(USAGE, file=sys.stderr)
    return 2




def terminated(signum, _frame):
    # A job cancelled with SIGTERM unwinds like an exit, so every target's finally stops it: no busbar,
    # relay or far end outlives the judge.
    raise SystemExit(128 + signum)


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, terminated)
    sys.exit(main(sys.argv))
