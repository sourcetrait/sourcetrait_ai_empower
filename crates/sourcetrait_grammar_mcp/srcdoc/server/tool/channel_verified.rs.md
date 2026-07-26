# channel_verified.rs

## fn channel_verified
THIS IS THE AUTHENTICATION. The same agent that drives the MCP over stdio proves it owns the
CLAIMING connection - which is all the Monitor's `{url, protocols}` input can support, since it
carries no headers, no client cert and no auth hook.

So a claim has to EXIST to be owned, which is why `not_claimed` is a distinct refusal rather
than a success on an empty channel.
