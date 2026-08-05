# remote_channels.rs

## fn remote_channels
Two read-only sets from the process-global registries (remote_links + bound_listeners,
link.rs): `channels` (established links, {alias, remote_mcp_nom, address}, alias-sorted)
and `listening` (bound-but-unpaired listeners, {remote, address}, alias-sorted;
RemoteFirstBlood/RemoteChannelListeners). `channels` is where the agent correlates an
alias (what it opened) to the peer mcp_nom (what Sent/Unsent and a relayed packet's
from-field carry), since a clean Connected carries only the nom. `listening` fills the
gap that a bound listener was invisible until it paired - it has no remote_mcp_nom (no
peer yet), and a listener that pairs moves from `listening` into `channels`.
