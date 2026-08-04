# remote_channel_open.rs

## fn remote_channel_open
VOID on (partial) success - the open is only UNDERWAY, the link established async by
open_remote's lifecycle task (link.rs), which emits mcp/remote/Connected or
Disconnected{error} later. Only the two IMMEDIATE failures are an error envelope: no
such alias in remotes.toml, or a link already open for that alias. So a successful
return means "the open started", never "the link is up" - the agent watches its
Channel for the outcome. The keyset comes from config (understood/11), so there is no
inline address/key param (the_user).
