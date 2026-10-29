#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""THE jev REFERENCE RELAY: the control and the negative control of the jev conformance suite.

A jev relay written from the decisions plane's rules alone (plane-decisions/src/driven.rs, the
README's "jev" section), in the standard library and nothing of busbar's: what the jev suite's
wire checks hold busbar to, a correct relay does. It is the control (`--break` absent: every wire
check must pass against it) and, with one deliberate deviation at a time, the negative control
(`--break <name>`: the wire check that deviation breaks must grade red).

The rules it serves:
  * `POST /v1/systemone` speaks `jev`, `POST /api/evaluate` speaks `nanojev`; any other path is the
    absence code, 404.
  * the caller's credential (`authorization: Bearer <key>`) must be one it was given, else `401`
    `invalid_request`, before any hop.
  * the request routes by its top-level `model`: none named and exactly one model of the path's
    dialect configured routes to it; none named and several is `400` `invalid_request`; a `model`
    no entry of the path's dialect has is `404` `invalid_params`.
  * the body is relayed byte for byte, but for the top-level `model` value spliced to the model's
    `upstream_model` when one is set.
  * the far end receives every caller header but the credential carriers, `host`, `content-type`
    and the hop-by-hop set, plus the provider's own credential (`authorization: Bearer`).
  * a far-end 2xx or 4xx reaches the caller as the far end's own status and bytes; a far-end 5xx
    ends in a terminal `502` in jev's refusal shape, never the far end's bytes; a `nanojev` success
    with no `execution.states` count is refused `502` too.
  * every refusal is exactly `{"error":{"code":<word>,"message":<text>}}`, the code word chosen by
    status as `refusal_body` chooses it.

Usage: reference_relay.py <config.json>; the config names `listen` (a port), `far` (the far end's
base URL), `callers` (the caller keys), `providers` ({name: {"key", "protocol"}}), `models`
({name: {"provider", "upstream_model"?}}) and `break` (a name in BREAKS, or "").
"""
import http.client
import http.server
import json
import sys
import urllib.parse

# Each deliberate deviation, and the wire check it must turn red (testing/jev-conformance/judge.py).
BREAKS = {
    "reserialise": "relay.request",
    "leak-credential": "relay.credential",
    "drop-headers": "relay.client-headers",
    "rewrap-response": "relay.response",
    "mask-error": "relay.error",
    "anonymous": "refusal.unauthenticated",
    "guess-model": "models.unnamed-400",
    "no-splice": "models.upstream-model",
    "relay-5xx": "relay.transient-terminal",
    "refuse-uncounted": "nanojev.no-count-floor",
    "cross-dialect": "nanojev.wrong-dialect-404",
}

DIALECT_OF_PATH = {"/v1/systemone": "jev", "/api/evaluate": "nanojev"}
# The contract's governed credential carriers (NEVER_KEPT) and the fields the plane writes itself.
NEVER_KEPT = {"authorization", "proxy-authorization", "cookie", "x-api-key", "api-key",
              "x-goog-api-key", "x-amz-security-token"}
NOT_RELAYED = {"content-type", "host"}
HOP_BY_HOP = {"connection", "keep-alive", "te", "trailer", "transfer-encoding", "upgrade",
              "content-length", "proxy-connection"}


def refusal(status, text):
    if status in (403, 429):
        code = "unsupported_operation"
    elif status == 404:
        code = "invalid_params"
    elif 400 <= status <= 499:
        code = "invalid_request"
    elif status == 500:
        code = "internal"
    else:
        code = "unsupported_operation"
    return status, json.dumps({"error": {"code": code, "message": text}}, separators=(",", ":")).encode()


def top_level_model_span(raw):
    """The byte span of the top-level `model` member's string value (quotes included), or None."""
    i, n, depth, key, expect_value = 0, len(raw), 0, None, False
    while i < n:
        c = raw[i:i + 1]
        if c == b'"':
            j = i + 1
            while j < n and raw[j:j + 1] != b'"':
                j += 2 if raw[j:j + 1] == b"\\" else 1
            s = raw[i:j + 1]
            if depth == 1 and expect_value:
                if key == b'"model"':
                    return (i, j + 1)
                expect_value = False
            elif depth == 1:
                key = s
            i = j + 1
            continue
        if c in (b"{", b"["):
            depth += 1
            expect_value = False
        elif c in (b"}", b"]"):
            depth -= 1
        elif c == b":" and depth == 1:
            expect_value = True
        elif c == b"," and depth == 1:
            expect_value, key = False, None
        elif depth == 1 and expect_value and not c.isspace():
            expect_value = False
        i += 1
    return None


class Relay(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    cfg = {}

    def log_message(self, *_):
        pass

    def answer(self, status, body, headers=()):
        self.send_response(status)
        sent = set()
        for k, v in headers:
            if k.lower() in HOP_BY_HOP:
                continue
            self.send_header(k, v)
            sent.add(k.lower())
        if "content-type" not in sent:
            self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/healthz":
            return self.answer(200, b"ok")
        return self.answer(*refusal(404, "no route"))

    def do_POST(self):
        cfg, brk = self.cfg, self.cfg.get("break", "")
        raw = self.rfile.read(int(self.headers.get("content-length") or 0))
        dialect = DIALECT_OF_PATH.get(self.path)
        if dialect is None:
            return self.answer(*refusal(404, "no route serves this path"))
        auth = self.headers.get("authorization", "")
        if brk != "anonymous" and not (auth.startswith("Bearer ") and auth[7:] in cfg["callers"]):
            return self.answer(*refusal(401, "a credential is required"))
        try:
            doc = json.loads(raw)
        except ValueError:
            return self.answer(*refusal(400, "We could not parse the JSON body of your request."))
        if not isinstance(doc, dict):
            return self.answer(*refusal(400, "We could not parse the JSON body of your request."))
        models = cfg["models"]
        of_dialect = [m for m, e in models.items() if cfg["providers"][e["provider"]]["protocol"] == dialect]
        named = doc.get("model")
        if named is None:
            if len(of_dialect) == 1 or (brk == "guess-model" and of_dialect):
                model = of_dialect[0]
            else:
                return self.answer(*refusal(400, "the request names no model, and several decisions models are configured"))
        elif not isinstance(named, str) or not named:
            return self.answer(*refusal(400, "Missing required parameter: 'model'."))
        elif named in of_dialect or (brk == "cross-dialect" and named in models):
            model = named
        else:
            return self.answer(*refusal(404, f"The model '{named}' does not exist"))
        entry = models[model]
        provider = cfg["providers"][entry["provider"]]
        body = raw
        if named is not None and entry.get("upstream_model") and brk != "no-splice":
            span = top_level_model_span(raw)
            if span:
                body = raw[:span[0]] + json.dumps(entry["upstream_model"]).encode() + raw[span[1]:]
        if brk == "reserialise":
            body = json.dumps(json.loads(body)).encode()
        headers = {}
        if brk != "drop-headers":
            for k, v in self.headers.items():
                lk = k.lower()
                if lk in NEVER_KEPT or lk in NOT_RELAYED or lk in HOP_BY_HOP:
                    continue
                headers[k] = v
        headers["content-type"] = "application/json"
        headers["authorization"] = auth if brk == "leak-credential" else "Bearer " + provider["key"]
        far = urllib.parse.urlsplit(cfg["far"])
        conn = http.client.HTTPConnection(far.hostname, far.port, timeout=30)
        try:
            conn.request("POST", self.path, body=body, headers=headers)
            resp = conn.getresponse()
            status, answer = resp.status, resp.read()
            ctype = resp.getheader("content-type", "application/json")
        except OSError as e:
            return self.answer(*refusal(502, f"the decision provider could not be reached: {e}"))
        finally:
            conn.close()
        if status >= 500:
            if brk == "relay-5xx":
                return self.answer(status, answer, [("content-type", ctype)])
            return self.answer(*refusal(502, "the decision provider failed"))
        if 400 <= status < 500:
            if brk == "mask-error":
                return self.answer(*refusal(502, "the decision provider refused"))
            return self.answer(status, answer, [("content-type", ctype)])
        if dialect == "nanojev" and brk == "refuse-uncounted":
            try:
                states = json.loads(answer).get("execution", {}).get("states")
            except (ValueError, AttributeError):
                states = None
            if not isinstance(states, int) or isinstance(states, bool):
                return self.answer(*refusal(502, "the provider's answer states no execution.states count"))
        if brk == "rewrap-response":
            answer = json.dumps(json.loads(answer)).encode()
        return self.answer(status, answer, [("content-type", ctype)])


def main():
    cfg = json.load(open(sys.argv[1]))
    if cfg.get("break") and cfg["break"] not in BREAKS:
        sys.exit(f"reference_relay: unknown break {cfg['break']!r}")
    Relay.cfg = cfg
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", int(cfg["listen"])), Relay)
    srv.daemon_threads = True
    srv.serve_forever()


if __name__ == "__main__":
    main()
