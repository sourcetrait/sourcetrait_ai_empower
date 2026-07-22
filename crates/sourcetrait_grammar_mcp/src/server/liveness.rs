use crate::*;

/// The namespace's host-lock filename, under `data_base_dir()`.
///
/// The DATA tier rather than the cache: a wiped cache would delete a live host's lock
/// file, and a watcher would then create a fresh one, lock a DIFFERENT inode, and read a
/// living host as gone.
pub(crate) const HOST_LOCK_FILE: &str = "host.lock";

pub(crate) fn host_lock_path() -> PathBuf {
    data_base_dir().join(HOST_LOCK_FILE)
}

/// The per-namespace host lock: a liveness signal any watcher can read, with no MCP call, no
/// nushell, and no cooperation from the host.
///
/// A watcher tries a NON-BLOCKING exclusive lock on this file. Acquiring it means the
/// owner is GONE; failing to acquire means the owner is ALIVE. That polarity is the whole
/// design. An exit hook cannot run on SIGKILL / abort / OOM-kill, which is precisely the
/// death that orphans a background job's external child - so a file that merely EXISTS
/// would read as a stale POSITIVE and every watcher would believe in a supervisor that
/// had died. The kernel drops a lock on ANY death, so "gone" is the default and liveness
/// has to be actively proven. It also removes the pid-reuse ambiguity a bare pid file has.
///
/// The CONTENTS carry identity (`mcp_nom` + pid) for a reader that wants to know WHICH
/// host holds it; they are not load-bearing for the liveness answer.
pub(crate) struct HostLock {
    /// Held for the process lifetime: dropping it releases the advisory lock, which is
    /// why the guard is stored rather than discarded, and why it is otherwise unread.
    _flock: nix::fcntl::Flock<fs::File>,
}

/// Take the namespace's host lock, or report why not.
///
/// NOT a singleton guard. Two hosts may legitimately share one namespace - their
/// per-process artifacts already namespace by `mcp_nom` - so the caller logs a failure
/// and carries on. The signal stays correct in aggregate, because "locked" answers the
/// question a watcher actually asks: is a host alive on this namespace.
pub(crate) fn acquire(mcp_nom: &str) -> io::Result<HostLock> {
    let path = host_lock_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // O_CLOEXEC EXPLICITLY, because this is the one gotcha that inverts the answer in
    // exactly the case the lock exists for: a `flock` attaches to the open file
    // DESCRIPTION, so an external the host spawns INHERITS the fd and keeps the lock held
    // after the host dies - and a watcher would read "still locked" and conclude a dead
    // host is alive. Rust's std already sets it on every file it opens; naming it here
    // keeps the invariant visible rather than load-bearing on a default nothing states.
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .custom_flags(nix::libc::O_CLOEXEC)
        .open(&path)?;
    let mut flock = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock)
        .map_err(|(_, errno)| {
            io::Error::other(format!("{} is held by a live host: {errno}", path.display()))
        })?;
    // Written AFTER the lock, so a reader can never see one host's identity underneath
    // another host's lock. NUON, like everything else we persist.
    flock.set_len(0)?;
    flock.write_all(format!("{{mcp_nom: \"{mcp_nom}\", pid: {}}}\n", process::id()).as_bytes())?;
    flock.flush()?;
    Ok(HostLock { _flock: flock })
}
