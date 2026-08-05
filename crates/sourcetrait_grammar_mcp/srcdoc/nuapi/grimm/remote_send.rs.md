# remote_send.rs

Body-facing grimm decls (registered in register_nuapi, ch14), NOT MCP tools - an
agent's willful, non-blocking send to a linked peer. Both mint a message id off the
channel NonceGen, look up the open link by mcp_nom (find_link_send, link.rs),
enqueue, and return the id IMMEDIATELY; the delivery outcome lands async as
mcp/remote/Sent|Unsent on the sender's own Channel, driven by the transport WRITE
(link.rs SendTracker - Sent covers the whole transmission, files + message). The
aggregate-write semantics + the receiver relay live in link.rs; these decls are the
thin body surface over it.

## struct GrimmRemoteChannelSend / struct GrimmRemoteChannelSendWith
Two decls, not one with an optional attachment, because for a REMOTE send the
transferred FILES are the "attached" - _send_with's whole job - so a bare send has no
files at all (unlike channel_send's optional [attached]). model rejects the mcp/
reservation (host-origin only), like channel_send.

### fn read_attachments
Reads each {src, dest} row into (dest, bytes) SYNCHRONOUSLY, before the id is
returned: a local src read failure is the agent's own problem and must surface at the
CALL, not as a later async Unsent. safe_dest guards every dest (relative, no ..) here
too, so a bad dest fails the send outright rather than being caught only receiver-side.

### fn spill_remote_event
CapNoCap, sender side: an event that would overflow the receiver's notification cap
(event_overflows, state.rs) is moved to a transferred file - the reserved
EVENT_SPILL_DEST, pushed alongside any caller files - and replaced on the wire by a
compact pointer, on both _send and _send_with. Sender-side via the file connection, not
a receiver-side render choice, because an event over 1 MiB cannot ride the inline
Deliver frame at all, so only the file transfer reaches the agent. The pointer's path is
dest-relative; the receiver rewrites it under the delivery's inbox dir (link.rs
rewrite_spill_pointer) so the agent resolves it uniformly as <inbox>/<spilled_event_path>,
exactly as for a local channel_send spill.
