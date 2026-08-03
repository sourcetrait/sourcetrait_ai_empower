# cert.rs

## fn ensure_cert_profile

Ship-and-continue, deliberately NOT the library's shut-down-if-absent
pattern. srcert's README prescribes a consumer that ships its profile then
shuts down until the operator runs the tool - right for a service whose TLS
identity is mandatory. grammar_mcp's channel is optional and lazy: a session
may never open one, and the host does far more than the channel, so stopping
it over a missing optional-feature cert would be wrong (the_user confirmed).
So this ships the profile when absent, logs the operator hint once (on the
first ship), and returns; a still-missing cert surfaces later as the
channel's own `ChannelStart` error at open, exactly as before.

Every path is a stderr line, never a return value or a stop - grammar_mcp's
fd 1 is the JSON-RPC channel, so operator messages ride stderr like every
other host log line. The profile text is embedded (`include_str!`), so
changing it needs a rebuild, as with the config defaults.
