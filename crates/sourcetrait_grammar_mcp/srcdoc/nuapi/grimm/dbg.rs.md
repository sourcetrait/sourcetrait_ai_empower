# dbg.rs

## struct GrimmDbg
THE DEBUG LANE, and it is deliberately not a print. fd 1 is the JSON-RPC channel,
so a body that printed would corrupt the protocol stream; and a value written here
keeps its TYPES, because it goes out as NUON rather than as text.

What it is FOR: the interesting values are often MID-body rather than in the
return, and this puts them beside the stdout and stderr the eval already captures,
where the agent fetches them by nonce out of band. It does not have to fit in the
result envelope, which is the other half of why it exists.

Its place in the three-way split with `channel_send`: `dbg` is a TRACE with no
notification, `channel_send(model, event)` is a NOTIFICATION with no data, and
`channel_send(model, event, attached)` is a notification POINTING AT data. Nothing
forces an author to choose between telling the agent and moving a payload.
