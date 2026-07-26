# processes.rs

## struct ProcessEntry
`source_nonce` and `path` are per-KIND and omitted when absent, so an entry's shape says which
tool produced it without a discriminant field.

`args` is what lets an agent match entries against its OWN send-set, which matters because this
tool shows ALL in-flight work on the host - a subagent sees the primary's calls and vice
versa.

## fn processes
PROCESS-SCOPED. A one-shot CLI invocation has an empty in-flight map by construction, so this
reports nothing there rather than reaching into another host.

The guard is dropped explicitly before building the envelope, so the registry is not held
across the serialization.
