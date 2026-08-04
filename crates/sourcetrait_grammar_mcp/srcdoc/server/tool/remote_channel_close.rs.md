# remote_channel_close.rs

## fn remote_channel_close
Removes the link from the registry and cancels its drivers; it does NOT wait or emit.
The link's own open task (link.rs) observes the cancel, deregisters if still current,
and emits mcp/remote/Disconnected - so a close and the Disconnected notice share the
one lifecycle path rather than each reporting separately. not_open if the alias has no
live link.
