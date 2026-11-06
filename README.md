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
route's `upstream_model`):

- `jev`: `POST /v1/systemone`;
- `nanojev`: `POST /api/evaluate`.

A dialect's route is served only while some configured model uses it. It bills in the classes
`decision`, `input_tokens` and `output_tokens`: `jev` reads `usage.units` (one decision when absent)
and the usage's tokens; `nanojev` reads `execution.states`. A successful answer without a decision
count is relayed unchanged and bills one decision at the plane's floor. A missing `nanojev` count
also raises a usage warning; an absent `jev` count does not.

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

`declares.json` states the plane's need (`http`) and no diagnostic code of its own: an answer whose
usage is not a whole count raises the core-owned 5029 (`usage-tap-decode-failed`). An unreadable
decision count bills one decision; an unreadable token count is not billed, and other whole counts
are retained. 5029 is benign-recurring: each plane instance warns once per dialect and member, and logs
later occurrences at debug.

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
