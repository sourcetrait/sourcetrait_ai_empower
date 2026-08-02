# purview_configure.rs

## struct PurviewConfigureEnvelope
THE WHOLE SET, never a delta: the caller needs to CHECK what it just wrote. A delta cannot
answer "what does this configuration actually do now", and against a purview the session is
not looking through - the common case - it answers nothing at all (several distinct writes
would all report an identical null delta).

## fn purview_configure
The `@id` values are validated for SHAPE only. Whether the referenced purview exists yet is
the prune's business, not the parser's, because `a -> @b` is legitimately written before `b`.

`default` CANNOT not exist, so an empty list RESETS it to `*` rather than deleting it - the
same state startup would put it back in.

A DELETION MUST NOT leave the session pointing at something gone, which is what
`retain_known` is for immediately after the save.
