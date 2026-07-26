# run.rs

## fn run
THE ORDER OF THE PRE-DISPATCH STEPS IS THE CONTRACT. Schemas convert FIRST, because the lint
needs the converted positional type to wrap the body with - it must see the same positional
the eval will. Then the lint, then the nonce, then the source. A conversion error and a lint
reject therefore both short-circuit before any log dir exists, which is why neither carries a
nonce.

The nonce is minted over the SERIALIZED PARAMS, so it is derived from the whole request rather
than from the body alone.

`CachedRunBody` is handed to the dispatch rather than written here, because the write has to
land in the per-call log dir the dispatch creates - and it must happen PRE-eval so a timeout
still leaves a rerunnable body.
