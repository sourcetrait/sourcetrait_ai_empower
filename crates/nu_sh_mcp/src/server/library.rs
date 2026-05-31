use crate::*;

// ============================================================================
// Library kind + metadata
// ============================================================================

/// How a library was originally registered. Determines whether
/// `define_function`/`undefine_function` mirror writes happen
/// (registered) and whether `reimport_library` is valid (imported).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ser::Serialize, ser::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LibraryKind {
    Registered,
    Imported,
}

/// On-disk `<library>/.nu_sh_mcp_meta.json` shape. Git-tracked. Lives at
/// the root of every registered/imported library and records both the
/// authoring style and the client-side path the MCP either mirrors to
/// (registered) or re-reads from on reimport (imported).
#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct LibraryMeta {
    pub kind: LibraryKind,
    pub source_path: PathBuf,
}

/// Filename of the per-library metadata sidecar.
pub(crate) const META_FILE: &str = ".nu_sh_mcp_meta.json";

// ============================================================================
// Path helpers
// ============================================================================

/// `$XDG_DATA_HOME/nu_sh_mcp/keypair/`.
pub(crate) fn keypair_dir() -> PathBuf {
    data_base_dir().join("keypair")
}

pub(crate) fn private_key_path() -> PathBuf {
    keypair_dir().join("id_nu_sh_mcp")
}

pub(crate) fn public_key_path() -> PathBuf {
    keypair_dir().join("id_nu_sh_mcp.pub")
}

pub(crate) fn allowed_signers_path() -> PathBuf {
    keypair_dir().join("allowed_signers")
}

/// `$XDG_DATA_HOME/nu_sh_mcp/libraries/` -- the git repo.
pub(crate) fn libraries_dir() -> PathBuf {
    data_base_dir().join("libraries")
}

pub(crate) fn library_dir(library: &str) -> PathBuf {
    libraries_dir().join(library)
}

pub(crate) fn library_meta_path(library: &str) -> PathBuf {
    library_dir(library).join(META_FILE)
}

pub(crate) fn library_root_modnu_path(library: &str) -> PathBuf {
    library_dir(library).join("mod.nu")
}

// ============================================================================
// Per-library lock registry
// ============================================================================

/// `tokio::sync::Mutex<HashMap<library_name, Arc<RwLock<()>>>>`. The
/// outer Mutex protects map mutation (insert/remove/lookup); each entry
/// is its own RwLock so:
///
/// - `call(library, ...)` acquires a READ lock on the entry, allowing
///   concurrent calls to the same library.
/// - Writes (`define_function`/`undefine_function`/`reimport_library`)
///   acquire a WRITE lock on the entry, serializing within a library
///   but not across libraries.
/// - Lifecycle (`register_library`/`unregister_library`) holds the
///   outer Mutex briefly to insert/remove the entry, then proceeds
///   under the per-library write lock.
pub(crate) struct LibraryLocks {
    map: tk::AsyncMutex<HashMap<String, Arc<tk::AsyncRwLock<()>>>>,
}

impl LibraryLocks {
    pub(crate) fn new() -> Self {
        Self {
            map: tk::AsyncMutex::new(HashMap::new()),
        }
    }

    /// Hydrate the registry from the on-disk libraries dir. Called at
    /// startup so existing libraries from prior server runs get their
    /// locks pre-created.
    pub(crate) async fn hydrate_from_disk(&self) -> io::Result<()> {
        let dir = libraries_dir();
        if !dir.exists() {
            return Ok(());
        }
        let mut map = self.map.lock().await;
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = match entry.file_name().into_string() {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Treat any subdir with a META_FILE as a library.
            if entry.path().join(META_FILE).exists() {
                map.entry(name).or_insert_with(|| Arc::new(tk::AsyncRwLock::new(())));
            }
        }
        Ok(())
    }

    /// Atomically insert a new lock entry. Errors if the name is taken.
    /// Returns the Arc-cloned RwLock so the caller can immediately
    /// acquire a write guard on it.
    pub(crate) async fn register(
        &self,
        name: &str,
    ) -> Result<Arc<tk::AsyncRwLock<()>>, AlreadyRegistered> {
        let mut map = self.map.lock().await;
        use std::collections::hash_map::Entry;
        match map.entry(name.to_string()) {
            Entry::Occupied(_) => Err(AlreadyRegistered),
            Entry::Vacant(v) => {
                let lock = Arc::new(tk::AsyncRwLock::new(()));
                v.insert(lock.clone());
                Ok(lock)
            }
        }
    }

    /// Look up an existing lock; None if the library isn't registered.
    /// Unused in 0.0.10; lands when `define_function` arrives in slice 2.
    #[allow(dead_code)]
    pub(crate) async fn lookup(
        &self,
        name: &str,
    ) -> Option<Arc<tk::AsyncRwLock<()>>> {
        let map = self.map.lock().await;
        map.get(name).cloned()
    }

    /// Remove an existing lock entry. Returns the removed lock so the
    /// caller can acquire the final exclusive guard for cleanup.
    /// Errors if the library wasn't registered.
    pub(crate) async fn unregister(
        &self,
        name: &str,
    ) -> Result<Arc<tk::AsyncRwLock<()>>, NotRegistered> {
        let mut map = self.map.lock().await;
        map.remove(name).ok_or(NotRegistered)
    }
}

pub(crate) struct AlreadyRegistered;
pub(crate) struct NotRegistered;

// ============================================================================
// Substrate: keypair generation + libraries repo init
// ============================================================================

const KEY_COMMENT: &str = "nu_sh_mcp@localhost";

/// Idempotent. Creates the keypair dir if absent; generates the
/// ed25519 keypair via `ssh-keygen` if the private key file doesn't
/// exist; writes the allowed_signers file. Safe to call on every
/// startup -- skips the heavy steps if files already exist.
pub(crate) fn ensure_keypair() -> io::Result<()> {
    let dir = keypair_dir();
    fs::create_dir_all(&dir)?;
    let priv_path = private_key_path();
    if !priv_path.exists() {
        let status = process::Command::new("ssh-keygen")
            .arg("-q")
            .arg("-t").arg("ed25519")
            .arg("-f").arg(&priv_path)
            .arg("-N").arg("")
            .arg("-C").arg(KEY_COMMENT)
            .status()
            .map_err(|e| io::Error::other(format!("ssh-keygen: {e}")))?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "ssh-keygen failed with status {status}",
            )));
        }
    }
    // (Re)write allowed_signers from the current pub key. Cheap and
    // ensures the file is always in sync with the key on disk.
    let pub_key = fs::read_to_string(public_key_path())?;
    let allowed = format!("{} {}", KEY_COMMENT, pub_key.trim());
    fs::write(allowed_signers_path(), allowed.as_bytes())?;
    Ok(())
}

/// Idempotent. Creates the libraries dir if absent; runs `git init -b
/// main` if `.git` doesn't exist; sets per-repo config (user, signing,
/// gpg.format=ssh); creates an initial empty commit so HEAD is valid
/// for subsequent operations.
pub(crate) fn ensure_libraries_repo() -> io::Result<()> {
    let dir = libraries_dir();
    fs::create_dir_all(&dir)?;
    let git_dir = dir.join(".git");
    if !git_dir.exists() {
        run_git(&dir, &["init", "-b", "main"])?;
        configure_repo(&dir)?;
        // Initial commit so HEAD exists.
        run_git(&dir, &["commit", "--allow-empty", "-m", "init libraries repo"])?;
    } else {
        // Re-apply config in case paths moved.
        configure_repo(&dir)?;
    }
    Ok(())
}

fn configure_repo(dir: &std::path::Path) -> io::Result<()> {
    let priv_path = private_key_path();
    let priv_path_str = priv_path
        .to_str()
        .ok_or_else(|| io::Error::other("private key path is not valid UTF-8"))?;
    let allowed_path = allowed_signers_path();
    let allowed_path_str = allowed_path
        .to_str()
        .ok_or_else(|| io::Error::other("allowed_signers path is not valid UTF-8"))?;
    let pairs: [(&str, &str); 6] = [
        ("user.name", "nu_sh_mcp"),
        ("user.email", KEY_COMMENT),
        ("gpg.format", "ssh"),
        ("user.signingKey", priv_path_str),
        ("gpg.ssh.allowedSignersFile", allowed_path_str),
        ("commit.gpgSign", "true"),
    ];
    for (k, v) in pairs {
        run_git(dir, &["config", k, v])?;
    }
    Ok(())
}

/// Run `git` in `dir` with the given args; error if non-zero exit.
pub(crate) fn run_git(
    dir: &std::path::Path,
    args: &[&str],
) -> io::Result<()> {
    let out = process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| io::Error::other(format!("git: {e}")))?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        )));
    }
    Ok(())
}

// ============================================================================
// register_library / unregister_library
// ============================================================================

/// Implementation for `register_library(name, path)`. Caller (the
/// rmcp tool handler) holds the per-library write lock (just acquired
/// at register time) and frames errors as `mcp::ErrorData`.
pub(crate) fn register_library_impl(
    name: &str,
    client_path: &std::path::Path,
) -> io::Result<()> {
    let lib_dir = library_dir(name);
    fs::create_dir_all(&lib_dir)?;
    // Empty cascade -- mod.nu re-exports nothing until define_function
    // populates it.
    fs::write(library_root_modnu_path(name), b"")?;
    let meta = LibraryMeta {
        kind: LibraryKind::Registered,
        source_path: client_path.to_path_buf(),
    };
    let meta_bytes = json::to_vec(&meta).map_err(|e| {
        io::Error::other(format!("serialize meta: {e}"))
    })?;
    fs::write(library_meta_path(name), &meta_bytes)?;
    // Mirror the (empty) library at the client path.
    fs::create_dir_all(client_path)?;
    fs::write(client_path.join("mod.nu"), b"")?;
    // Commit on the MCP side.
    run_git(&libraries_dir(), &["add", "--", name])?;
    let msg = format!("register library {name}");
    run_git(&libraries_dir(), &["commit", "-m", &msg])?;
    Ok(())
}

/// Implementation for `unregister_library(name)`. Removes the
/// library subtree from the MCP repo and commits. Does NOT touch
/// the client mirror (the_user 2026-05-31 design -- unregister is
/// MCP-side only; client manages its own copies).
pub(crate) fn unregister_library_impl(name: &str) -> io::Result<()> {
    let lib_dir = library_dir(name);
    if lib_dir.exists() {
        fs::remove_dir_all(&lib_dir)?;
        run_git(&libraries_dir(), &["add", "--", name])?;
        let msg = format!("unregister library {name}");
        run_git(&libraries_dir(), &["commit", "-m", &msg])?;
    }
    Ok(())
}

// ============================================================================
// Server startup substrate
// ============================================================================

/// Run the one-time-per-startup substrate: keypair gen + repo init.
/// Idempotent. Returns a fresh `LibraryLocks` hydrated from the
/// on-disk libraries dir so previously-registered libraries get their
/// locks pre-created.
pub(crate) async fn ensure_substrate() -> io::Result<Arc<LibraryLocks>> {
    ensure_keypair()?;
    ensure_libraries_repo()?;
    let locks = Arc::new(LibraryLocks::new());
    locks.hydrate_from_disk().await?;
    Ok(locks)
}
