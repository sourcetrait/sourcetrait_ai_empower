# remote_channel_open.rs

## fn remote_channel_open
Blocks on open_remote_blocking (link.rs) and returns synchronously: void on success, an
error envelope on failure. Three failures are an envelope - no such alias in
remotes.toml, a link already open for that alias, or the bind/connect itself failing
(remote::open_failed). A successful return now means the immediate networking step
succeeded - a listener is bound, or a connector is connected - not merely that an open
started (the void-then-async predecessor, RemoteFirstBlood/ConnectionWoes). A listener's
peer-wait (Connected) and any established-link teardown (Disconnected) still surface
later on the Channel. The keyset comes from config (understood/11), so there is no
inline address/key param (the_user).
