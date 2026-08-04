# remote_channels.rs

## fn remote_channels
A read-only snapshot of the open links: {alias, remote_mcp_nom, address}, alias-sorted.
The one place the agent correlates an alias (what it opened) to the peer mcp_nom (what
Sent/Unsent and a relayed packet's from-field carry), since a clean Connected carries
only the nom.
