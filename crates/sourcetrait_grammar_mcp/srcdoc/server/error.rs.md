# error.rs

ONE unified wire envelope for every tool-execution failure, built from typed Rust
values. No stringly-keyed disambiguation, no rendered-text middle layer, and no
JSON-RPC error codes for domain failures.

## struct Source
`path` is rigs-dir-relative (`<rig>/<file>`) for a rig-validation diagnostic and NULL
for a run or interact body diagnostic, because a body diagnostic is located in the
body and there is no file to name. `position` is 1-based, and `[0, 0]` means
file-level rather than line 0.

Both fields serialize as PRESENT, with `path` as null when absent rather than omitted.
The whole `Source` is null on the `Diagnostic` for a non-located condition - an eval
timeout, an unregistered-rig error.

The shape stays flat - `path` plus `position` - which is what the agent consumes
directly, rather than a nested source-carrier the reader would have to unwrap.

## struct Diagnostic
The single type for both a rig-validation finding and a body-lint finding - one shape
the wire and the check summary both carry.

THE BUCKET CONVEYS SEVERITY, which is why `severity` is `#[serde(skip)]` - absent from
the wire AND from the emitted JSON schema. It is an internal partition key only.

Per-condition data (`timeout_ms`, names, reason) lives IN `message`, not a typed
per-variant field: it is text either way, and a per-variant field would vary its shape
by kind.

## enum Error
Stays the ergonomic typed value the impls build and bubble with `?`; it is not
serde-tagged on the wire. Two variants carry already-unified rows, and the rest are
single-condition.

`From<io::Error>` blanket-converts to `Internal { phase: "io" }` so the rig impls can
use `?` freely rather than mapping at every filesystem call.

### fn kind_str
The two violation variants `unreachable!()` here deliberately: they render through
`bucket`, and reaching this arm would mean a caller bypassed that path.

`module::circular_import` is a TOP-LEVEL namespace rather than a `rig::` kind, because
a resolution cycle is not a rig-structure rule even when the validator is what finds
it. The validator already emits non-`rig::` kinds, `lint::summary_length` among them.

`remote::open_failed` is the SYNCHRONOUS bind/connect failure of a blocking
remote_channel_open (ConnectionWoes): distinct from the async `mcp/remote/Disconnected
{error}` Channel model, which now fires only for a listener's post-bind accept failure
or an established link's later teardown.

## fn error_to_call_result
WHY SUCCESS-SHAPE ERRORS: in the Claude Code CLI a success-shape result carrying
`structured_content` renders a green bullet whose body is visible on expand, while
every `is_error = true` variant and every JSON-RPC error renders a red bullet with NO
body. Success-shape is the ONLY path where the agent can actually read the typed error,
which is the entire reason the wire looks like this.

`Err(mcp::ErrorData)` stays reserved for genuine rmcp-boundary failures. The nonce
attaches when dispatch had already allocated a per-call log dir, so the agent can fetch
that call's stdout and stderr for a failure that produced output before dying.
