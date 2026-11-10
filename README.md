<!-- fleet:header:begin (rendered by `busbar-release plugin heal` from GetBusbar/busbar-release template/ and busbar's plugins.yaml; edit it there) -->
# busbar-plane-decisions

First-party signed kind:plane plugin cdylib: the decisions plane, packaged as a droppable busbar plugin. Drop the signed tarball into plugins/.

| kind | alias | crate | busbar | license |
|---|---|---|---|---|
| `plane` | `decisions` | `busbar-plane-decisions-plugin` | 1.6.0 (pinned in `.busbar-ref`) | Apache-2.0 |

<!-- fleet:header:end -->

## What it is for

`busbar-plane-decisions` is busbar's decisions plane: a decision request routed by its `model` to a
decision provider, governed and metered as busbar meters a request. It speaks two dialects, each a
direct HTTP+JSON relay of the caller's body (only the top-level `model` is rewritten, to the
route's `upstream_model`; a bound request-stage hook's rewrite replaces the document):

- `jev`: `POST /v1/systemone`;
- `nanojev`: `POST /api/evaluate`.

A dialect's route is served only while some configured model uses it. It bills in the classes
`decision`, `input_tokens` and `output_tokens`: `jev` reads `usage.units` (one decision when absent)
and the usage's tokens; `nanojev` reads `execution.states`. A count is an exact non-negative
decimal: a whole one bills whole units, a fractional one (`2.5`) bills exact micro-units. A
successful answer without a decision count is relayed unchanged and bills one decision at the
plane's floor. A missing `nanojev` count, or a success whose body is not a JSON object, also raises
a usage warning; an absent `jev` count does not.

## Config

Off unless the deployment writes a `decisions:` section. The plane declares `decisions:` and reads
the `providers:` section beside it (each model's provider and its `protocol`); it owns no other
section. Unknown keys are refused.

- `models`: the model names callers target, each `{ provider, upstream_model, protocol, ... }`
  (the shared model entry). A model's dialect is its own `protocol`, else its provider's; a dialect
  the plane does not speak is refused, naming `jev, nanojev`.
- `hooks`: hook names (from the top-level `hooks:`) bound to every decisions request.
- `upstream_credentials`: `own` (the default) or `passthrough`.
- `rate_card` and `fees`: the prices of the plane's classes (`decision`, `input_tokens`,
  `output_tokens`) and of its `per_request` fee. busbar reads these two itself, before the section
  reaches the plane, so they are not refused as unknown.

## Diagnostics

`declares.json` states the plane's need (`http`) and no diagnostic code of its own: a success whose
usage count cannot be read raises the core-owned 5029 (`usage-tap-decode-failed`). A count that is
present but unreadable (not an exact non-negative decimal within the store's range, e.g. a
string, `null`, a negative number, one finer than a micro-unit, or one spelled in more than 31
bytes) is refused, never
replaced by the floor: the plane marks the answer `count refused`, so busbar refuses the unit rather
than bill it. A missing count (see above) is not refused; it bills the floor. 5029 is
benign-recurring: each plane instance warns once per dialect and member, and logs later occurrences
at debug.

## Build

```bash
cargo build --release -p busbar-plane-decisions-plugin
```

## Tests

```bash
cargo test --workspace --locked
```

## License

Apache-2.0. See [LICENSE](LICENSE).

## Versioned text classification

`POST /busbar/decisions/v1/classify` is a decisions-plugin operation (`classify_v1`), served whenever a decision model is configured. Normal authentication, model authorization, budgets and usage metering apply. It accepts a maximum 64 KiB document:

```json
{"version":1,"model":"classifier","messages":[{"role":"user","text":"Summarize this sentence"}],"tiers":[{"label":"easy","description":"Simple requests","examples":[]},{"label":"hard","description":"Requests requiring several reasoning steps","examples":[]}]}
```

The bounded window must end in a nonempty user message, contain only user/assistant roles and at most 33 messages, and name 2–32 distinct tiers. Jev routes to its native `questions.complexity` Choice; NanoJev routes one `states` item to `/api/evaluate`. NanoJev's strict batch envelope contains only `states`; routing model selection stays in host custody. The plane buffers at most 64 KiB of provider response and returns only `version`, `label`, optional `confidence`/`probabilities`, `source` and `schema`. The result schema is `jev-choice-v1` or `nanojev-choice-v1`. Unknown labels, invalid probabilities, unsupported response shapes and overflow produce a bounded generic 502 response. Reported upstream usage still bills even if normalization fails. Ordinary `/v1/systemone` and `/api/evaluate` requests retain byte-faithful relay behavior.

Jev confidence uses its documented explicit field when supplied; absent confidence stays absent. Native NanoJev has no recognized confidence field, so its normalized confidence remains absent. No formula is inferred from arbitrary scores, and NanoJev probabilities are not claimed to be calibrated. Consumers such as `busbar-hook-complexity` default to abstention on missing confidence.

Provider contracts: [TypeSafe System One API](https://docs.typesafe.ai/api/system-one), [NanoJev inference input and output implementation](https://github.com/TianyuCodings/NanoJev/blob/main/scripts/predict_toy_decisions.py). Fixtures follow the Choice schemas inspected on 10 November 2026; unsupported provider variants fail normalization.

The owner-supplied root cell `assembled_confidence_routing_preserves_custody_usage_and_fallback` composes the actual LLM and Decisions doors with a signed dropped complexity hook. It requires the fresh complexity cdylib in `BUSBAR_TEST_COMPLEXITY_CDYLIB`, declared by that product's wrapper metadata, plus the root's ordinary Store/transport fixture inputs. Run this named case through the consuming distribution's binary test harness. Synthetic upstream answers cover normalization, absent grants, incomplete modality projection, independent child usage and authorized provider fallback; they make no classifier-quality claim.
