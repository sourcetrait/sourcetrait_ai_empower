# channel_close.rs

## const CLOSE_PLANNED
Sending a REAL close frame is what lets the agent tell a planned teardown from a host that
simply died, which arrives as a bare 1006.

## fn channel_close
IDEMPOTENT, like `rig(uninstall)`: closing what is already closed is the requested state
rather than a failure.

A DELIBERATE close PRUNES the inbox. The caller is declaring it is done with the channel,
which is what makes dropping that channel's attachments intentional rather than inferred -
and it leaves less for the system pruner. Scoped to THIS path on purpose: the shutdown close,
the verify-timer expiry and the unverified-emit teardown are all host-initiated, and none of
them is the caller saying it has finished.

The prune is best-effort because the close has already happened and this tool cannot fail.
The path is host-derived - set by `channel_open` under the tmpfs root - and never
caller-supplied, which is what makes a recursive remove bounded here rather than a hazard.
