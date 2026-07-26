use crate::*;

/// The namespace's host-lock filename, under `data_base_dir()`.
pub(crate) const HOST_LOCK_FILE: &str = "host.lock";

pub(crate) fn host_lock_path() -> PathBuf {
    data_base_dir().join(HOST_LOCK_FILE)
}

/// The per-namespace host lock: a liveness signal any watcher can read.
pub(crate) struct HostLock {
    /// Held for the process lifetime; dropping it releases the advisory lock.
    _flock: nix::fcntl::Flock<fs::File>,
}

/// Take the namespace's host lock, or report why not.
pub(crate) fn acquire(mcp_nom: &str) -> io::Result<HostLock> {
    let path = host_lock_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
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
    flock.set_len(0)?;
    flock.write_all(format!("{{mcp_nom: \"{mcp_nom}\", pid: {}}}\n", process::id()).as_bytes())?;
    flock.flush()?;
    Ok(HostLock { _flock: flock })
}
