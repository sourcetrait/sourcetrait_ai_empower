# liveness.rs

Background work has no other way to learn its host died. The channel cannot serve
as that signal - channels are OPTIONAL and start lazily, so "never opened" and
"opened then died" read identically, and a background process on a channel-less
session would stop for no reason.

## const HOST_LOCK_FILE
THE DATA TIER RATHER THAN THE CACHE, and the reason is specific: a wiped cache
would delete a LIVE host's lock file, and a watcher would then create a fresh one,
lock a DIFFERENT inode, and read a living host as gone.

## struct HostLock
THE POLARITY IS THE WHOLE DESIGN. A watcher tries a NON-BLOCKING exclusive lock:
acquiring it means the owner is GONE, failing to acquire means the owner is ALIVE.

A file that merely EXISTS could not do this. No exit hook runs on SIGKILL, abort or
OOM-kill - which is precisely the death that orphans a background job's external
child - so a surviving file would read as a stale POSITIVE and every watcher would
believe in a supervisor that had died. The kernel drops a lock on ANY death, so
"gone" is the default and liveness has to be actively proven. It also removes the
pid-reuse ambiguity a bare pid file carries.

The CONTENTS carry identity for a reader that wants to know WHICH host holds it;
they are not load-bearing for the liveness answer.

### field _flock
Stored rather than discarded because dropping it releases the lock. That is the
only reason it exists, and it is otherwise unread - hence the underscore.

## fn acquire
NOT A SINGLETON GUARD. Two hosts may legitimately share one namespace - their
per-process artifacts already namespace by `mcp_nom` - so the caller logs a failure
and carries on. The signal stays correct in aggregate, because "locked" answers the
question a watcher actually asks: is a host alive on this namespace.

O_CLOEXEC IS EXPLICIT, and it is the one gotcha that INVERTS the answer in exactly
the case the lock exists for. A `flock` attaches to the open file DESCRIPTION, so an
external the host spawns would INHERIT the fd and keep the lock held after the host
died - and a watcher would read "still locked" and conclude a dead host is alive.
Rust's std already sets it on every file it opens; naming it here keeps the
invariant visible rather than resting on a default nothing states.

The contents are written AFTER the lock, so a reader can never see one host's
identity underneath another host's lock. NUON, like everything else persisted here.
