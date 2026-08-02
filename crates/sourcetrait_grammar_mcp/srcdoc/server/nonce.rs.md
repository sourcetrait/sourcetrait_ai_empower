# nonce.rs

## struct Nonce
Renders through `lib_grammar::base62::fmt_base62` rather than a local formatter, so
its string form stays interchangeable with claudeline's session nom as a filename
fragment and a URL component. Centralizing that alphabet is the reason
`sourcetrait_grammar_lib` still exists at all.

## struct McpNom
Distinct from `Nonce` in LIFETIME, not in shape: a Nonce names one eval, this names
the host that ran it.

It namespaces per-process artifacts - today the emergency log - so two hosts sharing
one namespace never clobber each other's records. And `info()` reports it because a
CHANGED value across two calls is how an agent learns the server restarted; a pid
cannot serve that, since pids are reused.

## struct NonceGen
The counter and the nanosecond timestamp are both mixed in, which is what makes
concurrent `next()` calls yield distinct nonces AND makes two runs of an identical
payload distinct. That last property is deliberate: there is no content-addressed
dedup, so each run gets its own body cache rather than sharing one.
