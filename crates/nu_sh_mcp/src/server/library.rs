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

/// What: returns `$XDG_DATA_HOME/nu_sh_mcp/keypair/`, the directory
/// holding the MCP's git-signing keypair.
///
/// Why: keypair lives in data_dir (not cache) because losing it would
/// orphan the git history's signatures; centralizing the path keeps
/// every keypair-related path derived from one root.
///
/// Where: called by the four keypair-file path helpers below, and by
/// `ensure_keypair` which creates the dir on first startup.
pub(crate) fn keypair_dir() -> PathBuf {
    data_base_dir().join("keypair")
}

/// What: path to the private SSH key file (`id_nu_sh_mcp`).
///
/// Why: ed25519 private key the MCP uses to sign every commit it
/// makes to the libraries repo; per-repo `user.signingKey` is set to
/// this absolute path.
///
/// Where: used by `ensure_keypair` (writes via ssh-keygen) and
/// `configure_repo` (sets `user.signingKey` in the libraries repo's
/// per-repo git config).
pub(crate) fn private_key_path() -> PathBuf {
    keypair_dir().join("id_nu_sh_mcp")
}

/// What: path to the public key file (`id_nu_sh_mcp.pub`).
///
/// Why: needed to populate the `allowed_signers` file used by git's
/// SSH signature verification.
///
/// Where: read by `ensure_keypair` to compose the allowed_signers
/// line; written by `ssh-keygen` as a side effect of creating the
/// private key.
pub(crate) fn public_key_path() -> PathBuf {
    keypair_dir().join("id_nu_sh_mcp.pub")
}

/// What: path to the `allowed_signers` file, a one-line file pairing
/// the key comment with the public key bytes.
///
/// Why: git's SSH signature verification consults this file (via
/// `gpg.ssh.allowedSignersFile`) when checking commit sigs. Without
/// it, `git log --show-signature` would mark our own commits as
/// untrusted.
///
/// Where: written by `ensure_keypair` every startup so it always
/// matches the current public key; referenced from per-repo git
/// config by `configure_repo`.
pub(crate) fn allowed_signers_path() -> PathBuf {
    keypair_dir().join("allowed_signers")
}

/// What: returns `$XDG_DATA_HOME/nu_sh_mcp/libraries/`, the root of
/// the MCP-managed git repo holding every registered library.
///
/// Why: one git repo for all libraries gives us a single audit log
/// across slice-3 lifecycle ops (register / define / undefine /
/// import / reimport / unregister); each library is a top-level
/// subdir within it.
///
/// Where: called by every git-aware helper (`register_library_impl`,
/// `define_function_impl`, etc.) to compose paths and by `run_git`
/// callers to set the `git -C` directory.
pub(crate) fn libraries_dir() -> PathBuf {
    data_base_dir().join("libraries")
}

/// What: returns the directory for a specific library inside the
/// libraries repo: `<libraries_dir>/<library>/`.
///
/// Why: every library lives under its own top-level subdir;
/// composing paths through this helper avoids hardcoding the
/// `<library>/` segment in every caller.
///
/// Where: called by every library-coordinate path helper
/// (`library_meta_path`, `library_root_modnu_path`, `call_file_path`)
/// and by lifecycle ops to wipe / create the library subtree.
pub(crate) fn library_dir(library: &str) -> PathBuf {
    libraries_dir().join(library)
}

/// What: path to a library's metadata sidecar
/// (`<library_dir>/.nu_sh_mcp_meta.json`).
///
/// Why: the sidecar carries the library's `kind`
/// (registered/imported) plus the original source_path; both are
/// needed by the lifecycle ops to drive correct behavior (mirror on
/// define if registered; re-read on reimport if imported).
///
/// Where: written by `register_library_impl` + `import_library_impl`
/// + `reimport_library_impl`; read by `load_meta`.
pub(crate) fn library_meta_path(library: &str) -> PathBuf {
    library_dir(library).join(META_FILE)
}

/// What: path to a library's root `mod.nu` file.
///
/// Why: the root mod.nu is the entry point of a library's cascade --
/// `use <library>` resolves to it. Always exists for a registered
/// library (created empty at register time).
///
/// Where: created by `register_library_impl` (empty); regenerated by
/// `cascade_up` whenever a function is added/removed at the library
/// root or a subdirectory cascade is touched.
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
    /// What: constructs a fresh `LibraryLocks` with an empty map.
    ///
    /// Why: server startup builds an empty registry, then immediately
    /// calls `hydrate_from_disk` to repopulate from `<libraries>/`
    /// before any tool call can land.
    ///
    /// Where: called by `ensure_substrate` during server startup;
    /// tests build their own via the Host helpers.
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

/// What: marker error returned by `LibraryLocks::register` when the
/// requested name is already present in the registry.
///
/// Why: register's contract is "create new"; an already-present name
/// is a client error, not a server error. The marker shape (no
/// payload) keeps the type minimal because the tool handler always
/// pairs it with a human-readable message that includes the name.
///
/// Where: returned by `LibraryLocks::register`; matched by
/// `NuSh::register_library` and `NuSh::import_library` to surface
/// `-32602 invalid_params` to the agent.
pub(crate) struct AlreadyRegistered;

/// What: marker error returned by `LibraryLocks::unregister` when
/// the requested name is not present in the registry.
///
/// Why: unregister requires an existing entry to remove. Same
/// rationale as `AlreadyRegistered` for the marker shape -- the
/// tool handler builds the user-facing message.
///
/// Where: returned by `LibraryLocks::unregister`; matched by
/// `NuSh::unregister_library` to surface `-32602 invalid_params`.
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

/// What: sets the six per-repo git config pairs that wire up the
/// libraries repo for SSH signing: user.name, user.email, gpg.format,
/// user.signingKey, gpg.ssh.allowedSignersFile, commit.gpgSign.
///
/// Why: per-repo config keeps our signing setup from leaking into the
/// user's global git config; `commit.gpgSign=true` makes plain `git
/// commit` sign automatically so every lifecycle commit is signed
/// without a flag at the call site.
///
/// Where: called by `ensure_libraries_repo` (twice -- on fresh init
/// and on re-startup to re-apply config in case keypair paths
/// changed).
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
// Name + path validation
// ============================================================================

/// Identifier shape used for library names and function names:
/// `[a-zA-Z_][a-zA-Z0-9_-]*`. Conservative: rejects dots, slashes,
/// and anything that could escape a path or shadow nushell keywords.
pub(crate) fn is_valid_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Reserved names that would collide with our on-disk shape.
    if s == "mod" {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().expect("non-empty");
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Validate a module path. Empty allowed (function at library root);
/// otherwise slash-separated segments each satisfying `is_valid_ident`.
/// Rejects `..`, leading/trailing slashes, double slashes.
pub(crate) fn is_valid_module_path(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if s.starts_with('/') || s.ends_with('/') || s.contains("//") {
        return false;
    }
    s.split('/').all(is_valid_ident)
}

// ============================================================================
// Function source synthesis
// ============================================================================

/// Build the on-disk shape of a `<name>.nu` file: `export def main`
/// (the function body) + `export def resolve` (passthrough typecheck).
/// Matches the convention enforced by the strict validator and used by
/// the standalone driver pattern.
pub(crate) fn synthesize_function_source(
    args_schema: &str,
    result_schema: &str,
    body: &str,
) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("export def main [args: record<");
    out.push_str(args_schema);
    out.push_str(">] {\n");
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("}\n\n");
    out.push_str("export def resolve [args: record<");
    out.push_str(result_schema);
    out.push_str(">] {\n    $args\n}\n");
    out
}

// ============================================================================
// LibraryMeta load helper
// ============================================================================

/// What: reads + parses `<library>/.nu_sh_mcp_meta.json`, returning
/// the deserialized `LibraryMeta`. Returns an io::Error wrapping the
/// JSON decode error if the file is malformed.
///
/// Why: the meta is the source of truth for `kind` (registered vs
/// imported) and `source_path`; both drive lifecycle decisions
/// (whether to mirror on define, where to re-read on reimport).
/// Wrapping JSON-decode errors as io::Error keeps the impl signature
/// uniform with other library impls.
///
/// Where: called by `define_function_impl` + `undefine_function_impl`
/// (to decide whether to mirror) and `reimport_library_impl` (to
/// recover the source_path).
pub(crate) fn load_meta(library: &str) -> io::Result<LibraryMeta> {
    let bytes = fs::read(library_meta_path(library))?;
    json::from_slice(&bytes).map_err(|e| {
        io::Error::other(format!("decode meta for {library}: {e}"))
    })
}

// ============================================================================
// mod.nu cascade regeneration
// ============================================================================

/// Re-derive `<dir>/mod.nu` from the directory's current children:
/// - `<subdir>` with its own `mod.nu` -> `export module <subdir>`
/// - `<file>.nu` (excluding mod.nu) -> `export use ./<file>.nu`
/// Sorted for stable output. Idempotent.
pub(crate) fn regenerate_mod_nu(dir: &std::path::Path) -> io::Result<()> {
    let mut lines = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "mod.nu" || name == META_FILE {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() {
            if entry.path().join("mod.nu").exists() {
                lines.push(format!("export module {name}"));
            }
        } else if ft.is_file() && name.ends_with(".nu") {
            lines.push(format!("export use ./{name}"));
        }
    }
    lines.sort();
    let mut content = lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    fs::write(dir.join("mod.nu"), content)?;
    Ok(())
}

/// After writing a new function file, regenerate `mod.nu` from the
/// target dir back to (and including) the library root. Caller passes
/// the library root and a relative path to the dir holding the new
/// file; the helper walks up the tree.
fn cascade_up(
    lib_root: &std::path::Path,
    rel_dir: &std::path::Path,
) -> io::Result<()> {
    let mut current = rel_dir.to_path_buf();
    loop {
        let dir = lib_root.join(&current);
        regenerate_mod_nu(&dir)?;
        if current.as_os_str().is_empty() {
            break;
        }
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None => break,
        }
    }
    Ok(())
}

/// After removing a function file, regenerate `mod.nu` and PRUNE any
/// intermediate dir that is now an empty module (no `.nu` files and
/// no submodule subdirs). Cascades up; never prunes the library root.
fn cascade_up_and_prune(
    lib_root: &std::path::Path,
    rel_dir: &std::path::Path,
) -> io::Result<()> {
    let mut current = rel_dir.to_path_buf();
    loop {
        let dir = lib_root.join(&current);
        if dir.exists() {
            if !current.as_os_str().is_empty() && dir_is_empty_module(&dir)? {
                fs::remove_dir_all(&dir)?;
            } else {
                regenerate_mod_nu(&dir)?;
            }
        }
        if current.as_os_str().is_empty() {
            break;
        }
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None => break,
        }
    }
    Ok(())
}

/// What: returns true if `dir` contains no `.nu` files and no
/// subdirectories that themselves have a `mod.nu` -- i.e. nothing
/// that justifies keeping the directory as a module.
///
/// Why: `cascade_up_and_prune` uses this to decide whether to prune
/// an intermediate dir after undefine. Without the prune, undefine
/// would leave empty dirs scattered through the tree.
///
/// Where: called only by `cascade_up_and_prune`; the library root is
/// never pruned even if empty (caller-side guard).
fn dir_is_empty_module(dir: &std::path::Path) -> io::Result<bool> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "mod.nu" || name == META_FILE {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_file() && name.ends_with(".nu") {
            return Ok(false);
        }
        if ft.is_dir() && entry.path().join("mod.nu").exists() {
            return Ok(false);
        }
    }
    Ok(true)
}

// ============================================================================
// define_function / undefine_function
// ============================================================================

pub(crate) fn define_function_impl(
    library: &str,
    module_path: &str,
    name: &str,
    args_schema: &str,
    result_schema: &str,
    body: &str,
) -> io::Result<()> {
    if !is_valid_ident(library) {
        return Err(io::Error::other(format!(
            "invalid library name: {library:?}"
        )));
    }
    if !is_valid_module_path(module_path) {
        return Err(io::Error::other(format!(
            "invalid module_path: {module_path:?}"
        )));
    }
    if !is_valid_ident(name) {
        return Err(io::Error::other(format!(
            "invalid function name: {name:?}"
        )));
    }
    let lib_root = library_dir(library);
    if !lib_root.exists() {
        return Err(io::Error::other(format!(
            "library not registered: {library}"
        )));
    }
    let meta = load_meta(library)?;
    let rel_dir = if module_path.is_empty() {
        PathBuf::new()
    } else {
        PathBuf::from(module_path)
    };
    let file_name = format!("{name}.nu");
    let mcp_target_dir = lib_root.join(&rel_dir);
    fs::create_dir_all(&mcp_target_dir)?;
    let source = synthesize_function_source(args_schema, result_schema, body);
    fs::write(mcp_target_dir.join(&file_name), &source)?;
    cascade_up(&lib_root, &rel_dir)?;
    if matches!(meta.kind, LibraryKind::Registered) {
        let mirror_dir = meta.source_path.join(&rel_dir);
        fs::create_dir_all(&mirror_dir)?;
        fs::write(mirror_dir.join(&file_name), &source)?;
        cascade_up(&meta.source_path, &rel_dir)?;
    }
    run_git(&libraries_dir(), &["add", "--", library])?;
    let id = function_id(library, module_path, name);
    run_git(&libraries_dir(), &["commit", "-m", &format!("define {id}")])?;
    Ok(())
}

pub(crate) fn undefine_function_impl(
    library: &str,
    module_path: &str,
    name: &str,
) -> io::Result<()> {
    if !is_valid_ident(library) {
        return Err(io::Error::other(format!(
            "invalid library name: {library:?}"
        )));
    }
    if !is_valid_module_path(module_path) {
        return Err(io::Error::other(format!(
            "invalid module_path: {module_path:?}"
        )));
    }
    if !is_valid_ident(name) {
        return Err(io::Error::other(format!(
            "invalid function name: {name:?}"
        )));
    }
    let lib_root = library_dir(library);
    if !lib_root.exists() {
        return Err(io::Error::other(format!(
            "library not registered: {library}"
        )));
    }
    let meta = load_meta(library)?;
    let rel_dir = if module_path.is_empty() {
        PathBuf::new()
    } else {
        PathBuf::from(module_path)
    };
    let file_name = format!("{name}.nu");
    let mcp_target_file = lib_root.join(&rel_dir).join(&file_name);
    if !mcp_target_file.exists() {
        let id = function_id(library, module_path, name);
        return Err(io::Error::other(format!("function not defined: {id}")));
    }
    fs::remove_file(&mcp_target_file)?;
    cascade_up_and_prune(&lib_root, &rel_dir)?;
    if matches!(meta.kind, LibraryKind::Registered) {
        let mirror_file = meta.source_path.join(&rel_dir).join(&file_name);
        if mirror_file.exists() {
            fs::remove_file(&mirror_file)?;
            cascade_up_and_prune(&meta.source_path, &rel_dir)?;
        }
    }
    run_git(&libraries_dir(), &["add", "--", library])?;
    let id = function_id(library, module_path, name);
    run_git(&libraries_dir(), &["commit", "-m", &format!("undefine {id}")])?;
    Ok(())
}

/// What: composes the coordinate `<library>/<module_path>/<name>` (or
/// `<library>/<name>` when module_path is empty) into a single string
/// for commit messages and error messages.
///
/// Why: keeps the empty-module_path branch out of every caller and
/// gives consistent rendering of the function coordinate everywhere
/// it appears.
///
/// Where: called by `define_function_impl` and `undefine_function_impl`
/// for commit messages, and by the latter for the "function not
/// defined" error.
fn function_id(library: &str, module_path: &str, name: &str) -> String {
    if module_path.is_empty() {
        format!("{library}/{name}")
    } else {
        format!("{library}/{module_path}/{name}")
    }
}

/// Resolve `<library>/<module_path>/<name>.nu` into an absolute path
/// under the MCP libraries dir. Returns None if any name component is
/// invalid (path traversal defense). Does NOT verify the file exists;
/// caller checks.
pub(crate) fn call_file_path(
    library: &str,
    module_path: &str,
    name: &str,
) -> Option<PathBuf> {
    if !is_valid_ident(library) {
        return None;
    }
    if !is_valid_module_path(module_path) {
        return None;
    }
    if !is_valid_ident(name) {
        return None;
    }
    let mut path = library_dir(library);
    if !module_path.is_empty() {
        path = path.join(module_path);
    }
    Some(path.join(format!("{name}.nu")))
}

// ============================================================================
// Strict library source validator (for import_library / reimport_library)
// ============================================================================

#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct Violation {
    /// Path relative to the source root.
    pub path: String,
    /// 1-based line number; 0 means "file-level" (no specific line).
    pub line: usize,
    /// Human-readable description of the violation.
    pub message: String,
}

/// What: pairs the structural-validator findings with the body-lint
/// findings from a single `validate_library_source` walk. Both vectors
/// are independent; a library can fail one set, the other, or both.
///
/// Why: slice 5.2 broadens the strict library validator to also lint
/// each function file's `export def main` body. Structural and lint
/// violations have different render shapes (the former is
/// `<path>:<line>: <message>` per `format_violations`; the latter is
/// `lint::<class> [L:C] mod <rel_path>` per the skill format
/// discipline), so they stay in separate vectors all the way to the
/// rmcp-error mapping seam.
///
/// Where: produced by `validate_library_source`; consumed by
/// `import_library_impl` / `reimport_library_impl` (folded into
/// `ImportError::Violations(ValidationResult)` when non-empty) and by
/// `server::tool::import_error_to_mcp_error` which renders both
/// sections together.
#[derive(Debug, Clone)]
pub(crate) struct ValidationResult {
    pub structural: Vec<Violation>,
    pub lint: Vec<LintViolation>,
}

impl ValidationResult {
    pub(crate) fn is_empty(&self) -> bool {
        self.structural.is_empty() && self.lint.is_empty()
    }
}

/// Walk every `.nu` under `root`. Apply the strict per-file shape:
/// - `mod.nu`: only `export use ./<file>.nu` or `export module <name>` lines
///   (plus blank lines and `#` comments). Body-lint NOT applied (no agent
///   code lives in mod.nu).
/// - Function files: exactly two exports named `main` and `resolve`;
///   both have `args: record<...>` typed positionals; resolve's body is
///   exactly the expression `$args`. Additionally (slice 5.2) main's
///   body is body-linted for hardcoded paths and blacklisted externals,
///   with source tag `mod <rel_path>`.
///
/// Each file is parsed through `nu_parser::parse` (in a
/// `module __v_<stem> { ... }` wrapper) so syntax errors land as
/// structural violations with line numbers. Dotfile entries (e.g.
/// `.git`, `.nu_sh_mcp_meta.json`) are skipped. Returns ALL violations
/// (structural + lint) -- no bail-on-first; no auto-fix.
pub(crate) fn validate_library_source(
    root: &std::path::Path,
    engine: &ParseEngine,
) -> io::Result<ValidationResult> {
    let mut result = ValidationResult {
        structural: Vec::new(),
        lint: Vec::new(),
    };
    validate_walk(root, root, engine, &mut result)?;
    Ok(result)
}

fn validate_walk(
    root: &std::path::Path,
    dir: &std::path::Path,
    engine: &ParseEngine,
    result: &mut ValidationResult,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_lossy = name.to_string_lossy();
        if name_lossy.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            validate_walk(root, &path, engine, result)?;
        } else if ft.is_file()
            && path.extension().map(|e| e == "nu").unwrap_or(false)
        {
            validate_one_file(root, &path, engine, result)?;
        }
    }
    Ok(())
}

fn validate_one_file(
    root: &std::path::Path,
    path: &std::path::Path,
    engine: &ParseEngine,
    result: &mut ValidationResult,
) -> io::Result<()> {
    let source = fs::read_to_string(path)?;
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    let is_mod = path.file_name().map(|n| n == "mod.nu").unwrap_or(false);
    if is_mod {
        validate_mod_nu_ast(&rel, stem, &source, parent, engine, &mut result.structural);
    } else {
        validate_function_file_ast(
            &rel,
            stem,
            &source,
            parent,
            engine,
            &mut result.structural,
            &mut result.lint,
        );
    }
    Ok(())
}

/// Pure-AST mod.nu validator. Parses the source in a
/// `module __v_<stem> { ... }` wrapper with `$env.PWD = parent` so
/// `export use ./<file>.nu` + `export module <name>` resolve cleanly
/// (slice 4.5 PWD guard). Then walks the body block via the outer
/// `module` call's second argument (a `Block` expression), and for
/// each pipeline element enforces:
///
/// - Must be a `Call` (NOT `Garbage`, not raw expressions).
/// - Call's decl name must be `"export use"` or `"export module"`.
/// - Any other decl (def/const/alias/let/mut/etc.) is a violation.
///
/// This replaces the prior text-based `validate_mod_nu`. Empower is
/// source-of-truth -- the validator walks the same AST the parser
/// produces, so it can't drift from nushell's grammar (the_user
/// 2026-05-31 slice 4.6).
fn validate_mod_nu_ast(
    rel: &str,
    stem: &str,
    source: &str,
    parent: &std::path::Path,
    engine: &ParseEngine,
    violations: &mut Vec<Violation>,
) {
    let wrapper_name = format!("__v_{stem}");
    let (wrapped, prefix_len) = wrap_as_module(source, &wrapper_name);
    let engine_state = engine.engine_state_for_file(parent);
    let mut working_set = nu::StateWorkingSet::new(&engine_state);
    let outer_block = nu::parse(&mut working_set, Some(rel), wrapped.as_bytes(), false);

    // 1. Surface parse errors with source-relative lines.
    for err in &working_set.parse_errors {
        let span_start = err.span().start.saturating_sub(prefix_len);
        let (line, _col) = span_to_line_col(source, span_start);
        violations.push(Violation {
            path: rel.to_string(),
            line,
            message: format!("parse error: {err:?}"),
        });
    }

    // 2. Locate the wrapper's body block via the outer `module` call's
    //    second positional argument. Probe (slice 4.6) confirmed:
    //    outer block has exactly 1 pipeline -> 1 element -> Call decl
    //    "module" with args[0]=name String + args[1]=Block(body_id).
    let body_block_id = outer_block
        .pipelines
        .first()
        .and_then(|p| p.elements.first())
        .and_then(|elem| match &elem.expr.expr {
            nu::Expr::Call(call) => call.arguments.iter().find_map(|arg| {
                if let nu::Argument::Positional(e) = arg {
                    if let nu::Expr::Block(id) = &e.expr {
                        Some(*id)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }),
            _ => None,
        });

    let body_block_id: nu::BlockId = match body_block_id {
        Some(id) => id,
        None => {
            // Parse-level failure; parse_errors already populated above.
            return;
        }
    };

    // 3. Walk body block. Each pipeline element must be a Call to one of
    //    the allowed decl names.
    let body = working_set.get_block(body_block_id);
    for pipeline in &body.pipelines {
        for elem in &pipeline.elements {
            check_mod_nu_pipeline_element(rel, &elem.expr, &working_set, source, prefix_len, violations);
        }
    }
}

/// Inspect a single body-block pipeline element. The only allowed
/// shape is `Expr::Call` whose decl name is `"export use"` or
/// `"export module"`. Everything else is a violation -- including
/// `Garbage` (already covered by parse_errors but worth a structural
/// note), other Call decls (`def`/`const`/`alias`/...), and bare
/// expressions if they somehow survived parsing.
fn check_mod_nu_pipeline_element(
    rel: &str,
    expr: &nu::Expression,
    working_set: &nu::StateWorkingSet,
    source: &str,
    prefix_len: usize,
    violations: &mut Vec<Violation>,
) {
    let span_start = expr.span.start.saturating_sub(prefix_len);
    let (line, _col) = span_to_line_col(source, span_start);
    match &expr.expr {
        nu::Expr::Call(call) => {
            let decl = working_set.get_decl(call.decl_id);
            let name = decl.name();
            if name == "export use" || name == "export module" {
                return;
            }
            violations.push(Violation {
                path: rel.to_string(),
                line,
                message: format!(
                    "mod.nu may only contain `export use ./<file>.nu` or `export module <name>` statements; got call to `{name}`",
                ),
            });
        }
        nu::Expr::Garbage => {
            // Parse error already surfaced; don't double-report.
        }
        other => {
            violations.push(Violation {
                path: rel.to_string(),
                line,
                message: format!(
                    "mod.nu may only contain `export use ./<file>.nu` or `export module <name>` statements; got `{}`",
                    short_expr_label(other),
                ),
            });
        }
    }
}

/// What: renders an `Expr` variant as a short, human-readable label
/// for inclusion in mod.nu validator violation messages (e.g.
/// `"variable reference"`, `"string literal"`).
///
/// Why: the variants we encounter in mod.nu body validation are
/// mostly non-allowed forms (variables, literals, binary ops) and
/// reporting them by name to the agent is more actionable than the
/// raw `Debug` repr.
///
/// Where: called by `check_mod_nu_pipeline_element`'s `other` branch
/// when constructing the violation message.
fn short_expr_label(expr: &nu::Expr) -> &'static str {
    match expr {
        nu::Expr::FullCellPath(_) => "cell-path expression",
        nu::Expr::Var(_) => "variable reference",
        nu::Expr::String(_) => "string literal",
        nu::Expr::Int(_) => "integer literal",
        nu::Expr::Float(_) => "float literal",
        nu::Expr::Bool(_) => "bool literal",
        nu::Expr::Block(_) => "block",
        nu::Expr::Closure(_) => "closure",
        nu::Expr::BinaryOp(_, _, _) => "binary operation",
        nu::Expr::Subexpression(_) => "subexpression",
        nu::Expr::Keyword(_) => "keyword",
        _ => "other expression",
    }
}

/// AST-based function file validator. Wraps `source` in
/// `module __v_<stem> { ... }`, runs nu_parser (with `$env.PWD =
/// parent` so any sibling `use` resolves cleanly), then walks the
/// resulting `Module` to enforce:
///   - parse cleanly (no syntax errors)
///   - exactly two exports named `main` and `resolve`
///   - main has a typed `args: record<...>` positional
///   - resolve has a typed `args: record<...>` positional AND its body
///     block is exactly the expression `$args` (passthrough)
fn validate_function_file_ast(
    rel: &str,
    stem: &str,
    source: &str,
    parent: &std::path::Path,
    engine: &ParseEngine,
    violations: &mut Vec<Violation>,
    lint: &mut Vec<LintViolation>,
) {
    let wrapper_name = format!("__v_{stem}");
    let (wrapped, prefix_len) = wrap_as_module(source, &wrapper_name);
    let engine_state = engine.engine_state_for_file(parent);
    let mut working_set = nu::StateWorkingSet::new(&engine_state);
    let _ = nu::parse(&mut working_set, Some(rel), wrapped.as_bytes(), false);

    // 1. Surface parse errors with source-relative line numbers.
    for err in &working_set.parse_errors {
        let span_start = err.span().start.saturating_sub(prefix_len);
        let (line, _col) = span_to_line_col(source, span_start);
        violations.push(Violation {
            path: rel.to_string(),
            line,
            message: format!("parse error: {err:?}"),
        });
    }
    if !working_set.parse_errors.is_empty() {
        // Don't try to walk a half-parsed AST. Leave structural checks
        // for the next round; parse errors already cover the file.
        return;
    }

    // 2. Find the wrapper module. The parser registers it under
    //    `wrapper_name` in the working set; `find_module` walks delta + base.
    let wrapper_name_bytes = wrapper_name.as_bytes();
    let module_id = match working_set.find_module(wrapper_name_bytes) {
        Some(id) => id,
        None => {
            violations.push(Violation {
                path: rel.to_string(),
                line: 0,
                message: "internal: wrapper module not found after parse".to_string(),
            });
            return;
        }
    };
    let module: &nu::Module = working_set.get_module(module_id);

    // 3. Enumerate the wrapper's exports. nu's Module type tracks `main`
    //    as Option<DeclId> separately from the `decls` map (which holds
    //    everything else). For the strict "exactly main + resolve" rule
    //    we require BOTH to be set, and `decls` to contain exactly one
    //    entry named "resolve" (plus `main` may or may not appear in
    //    decls depending on parser version -- normalize).
    let main_decl = module.main;
    let mut other_decls: Vec<(String, nu::DeclId)> = module
        .decls
        .iter()
        .filter(|(name_bytes, _)| name_bytes.as_slice() != b"main")
        .map(|(name_bytes, decl_id)| {
            (
                String::from_utf8_lossy(name_bytes).into_owned(),
                *decl_id,
            )
        })
        .collect();
    other_decls.sort_by(|a, b| a.0.cmp(&b.0));

    if main_decl.is_none() {
        violations.push(Violation {
            path: rel.to_string(),
            line: 0,
            message: "function file must contain `export def main [args: record<...>]`"
                .to_string(),
        });
    }
    let resolve_decl = other_decls
        .iter()
        .find(|(name, _)| name == "resolve")
        .map(|(_, id)| *id);
    if resolve_decl.is_none() {
        violations.push(Violation {
            path: rel.to_string(),
            line: 0,
            message: "function file must contain `export def resolve [args: record<...>] { $args }`"
                .to_string(),
        });
    }
    for (name, _) in &other_decls {
        if name != "resolve" {
            violations.push(Violation {
                path: rel.to_string(),
                line: 0,
                message: format!(
                    "function file may only export `main` and `resolve`; saw `export def {name}`",
                ),
            });
        }
    }

    // 4. Check signatures. Both main and resolve must have a single
    //    typed positional named `args` of shape Record.
    if let Some(id) = main_decl {
        check_args_record_positional(rel, &working_set, id, "main", source, prefix_len, violations);
    }
    if let Some(id) = resolve_decl {
        let resolve_args_var =
            check_args_record_positional(rel, &working_set, id, "resolve", source, prefix_len, violations);
        // 5. resolve's body must be exactly `$args` (passthrough).
        check_resolve_body_is_args(
            rel,
            &working_set,
            id,
            resolve_args_var,
            source,
            prefix_len,
            violations,
        );
    }

    // 6. Slice 5.2: lint main's body for hardcoded paths and blacklisted
    //    externals. Source tag is `mod <rel_path>` so the rendered line
    //    has the file context per the_user 2026-05-31 format choice.
    //    Skipped when main wasn't found or its body block isn't resolvable
    //    (those cases already pushed structural violations).
    if let Some(id) = main_decl {
        let decl = working_set.get_decl(id);
        if let Some(main_block_id) = decl.block_id() {
            let main_block = working_set.get_block(main_block_id);
            let source_tag = format!("mod {rel}");
            let mut lvs = lint_block(
                main_block,
                &working_set,
                source,
                prefix_len,
                Some(&source_tag),
            );
            lint.append(&mut lvs);
        }
    }
}

/// Confirm the decl's first required positional is named `args` with a
/// `record<...>` shape. Returns the positional's VarId so the caller can
/// match it against resolve's body `$args` expression.
fn check_args_record_positional(
    rel: &str,
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    fn_name: &str,
    source: &str,
    prefix_len: usize,
    violations: &mut Vec<Violation>,
) -> Option<nu::VarId> {
    let decl = working_set.get_decl(decl_id);
    let sig = decl.signature();
    let positional = sig.required_positional.first();
    let (bad, var_id) = match positional {
        None => (true, None),
        Some(p) => {
            let shape_ok = matches!(p.shape, nu::SyntaxShape::Record(_));
            ((p.name != "args" || !shape_ok), p.var_id)
        }
    };
    if bad {
        let line = decl_line(working_set, decl_id, source, prefix_len);
        violations.push(Violation {
            path: rel.to_string(),
            line,
            message: format!(
                "{fn_name} must take a typed positional `args: record<...>`",
            ),
        });
    }
    var_id
}

/// Confirm `resolve`'s body block contains exactly one pipeline with one
/// element whose expression is `$args` (passthrough). The expression's
/// AST shape for `$args` is `Expr::FullCellPath` wrapping a head that is
/// `Expr::Var(args_var_id)` with no tail.
fn check_resolve_body_is_args(
    rel: &str,
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    args_var_id: Option<nu::VarId>,
    source: &str,
    prefix_len: usize,
    violations: &mut Vec<Violation>,
) {
    let decl = working_set.get_decl(decl_id);
    let block_id = match decl.block_id() {
        Some(id) => id,
        None => {
            violations.push(Violation {
                path: rel.to_string(),
                line: 0,
                message: "resolve must be a user-defined `def`".to_string(),
            });
            return;
        }
    };
    let block = working_set.get_block(block_id);
    let line_of_decl = decl_line(working_set, decl_id, source, prefix_len);
    let mut ok = false;
    if block.pipelines.len() == 1 {
        let pipeline = &block.pipelines[0];
        if pipeline.elements.len() == 1 {
            let elem = &pipeline.elements[0];
            ok = is_args_var(&elem.expr, args_var_id);
        }
    }
    if !ok {
        violations.push(Violation {
            path: rel.to_string(),
            line: line_of_decl,
            message: "resolve's body must be exactly `$args`".to_string(),
        });
    }
}

/// Recognize the AST shape of the literal expression `$args`: a
/// `FullCellPath` with a `Var(args_var_id)` head and an empty tail.
fn is_args_var(expr: &nu::Expression, args_var_id: Option<nu::VarId>) -> bool {
    let expected = match args_var_id {
        Some(id) => id,
        None => return false,
    };
    let var_id = match &expr.expr {
        nu::Expr::FullCellPath(fcp) if fcp.tail.is_empty() => match &fcp.head.expr {
            nu::Expr::Var(id) => *id,
            _ => return false,
        },
        nu::Expr::Var(id) => *id,
        _ => return false,
    };
    var_id == expected
}

/// Best-effort: locate the source line where a Decl's `def` lives via
/// its name span. Falls back to line 0 if the span is in the wrapper
/// prefix or otherwise unrecoverable.
fn decl_line(
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    source: &str,
    prefix_len: usize,
) -> usize {
    let decl = working_set.get_decl(decl_id);
    let span = decl.signature().name.is_empty();
    let _ = span;
    // Decl doesn't expose its span via Command trait; signature span lives
    // on the block. As a best-effort, walk the block's span.
    if let Some(block_id) = decl.block_id() {
        let block = working_set.get_block(block_id);
        if let Some(span) = block.span {
            let src_offset = span.start.saturating_sub(prefix_len);
            let (line, _col) = span_to_line_col(source, src_offset);
            return line;
        }
    }
    0
}

#[cfg(any())]
fn validate_mod_nu(rel: &str, source: &str, violations: &mut Vec<Violation>) {
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("export use ") || trimmed.starts_with("export module ") {
            continue;
        }
        violations.push(Violation {
            path: rel.to_string(),
            line: i + 1,
            message: format!(
                "mod.nu may only contain `export use ./<file>.nu` or `export module <name>` lines (or comments / blanks); got: {trimmed}",
            ),
        });
    }
}

#[cfg(any())]
fn validate_function_file(rel: &str, source: &str, violations: &mut Vec<Violation>) {
    let lines: Vec<&str> = source.lines().collect();

    // Find all top-level `export def <name>` declarations by scanning lines.
    // Lightweight: doesn't track string/comment context. Function files are
    // small + author-curated; a `# export def fake` inside a comment is the
    // author's tell-tale and gets flagged. Acceptable trade-off for v1.
    let mut exports: Vec<(usize, String)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("export def ") {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '[')
                .unwrap_or(rest.len());
            exports.push((i + 1, rest[..end].to_string()));
        }
    }

    let main_line = exports.iter().find(|(_, n)| n == "main").map(|(l, _)| *l);
    let resolve_line = exports.iter().find(|(_, n)| n == "resolve").map(|(l, _)| *l);

    if main_line.is_none() {
        violations.push(Violation {
            path: rel.to_string(),
            line: 0,
            message: "function file must contain `export def main [args: record<...>]`".to_string(),
        });
    }
    if resolve_line.is_none() {
        violations.push(Violation {
            path: rel.to_string(),
            line: 0,
            message: "function file must contain `export def resolve [args: record<...>] { $args }`".to_string(),
        });
    }
    for (line, name) in &exports {
        if name != "main" && name != "resolve" {
            violations.push(Violation {
                path: rel.to_string(),
                line: *line,
                message: format!(
                    "function file may only export `main` and `resolve`; saw `export def {name}`",
                ),
            });
        }
    }

    if let Some(line) = main_line {
        let sig_line = lines.get(line - 1).copied().unwrap_or("");
        if !sig_line.contains("args: record<") {
            violations.push(Violation {
                path: rel.to_string(),
                line,
                message: "main must take a typed positional `args: record<...>`".to_string(),
            });
        }
    }

    if let Some(line) = resolve_line {
        let sig_line = lines.get(line - 1).copied().unwrap_or("");
        if !sig_line.contains("args: record<") {
            violations.push(Violation {
                path: rel.to_string(),
                line,
                message: "resolve must take a typed positional `args: record<...>`".to_string(),
            });
        }
        match extract_def_body(source, "resolve") {
            Some(body) => {
                if body.trim() != "$args" {
                    violations.push(Violation {
                        path: rel.to_string(),
                        line,
                        message: format!(
                            "resolve's body must be exactly `$args`; got `{}`",
                            body.trim(),
                        ),
                    });
                }
            }
            None => {
                violations.push(Violation {
                    path: rel.to_string(),
                    line,
                    message: "resolve's body could not be located (parens/brackets imbalanced?)".to_string(),
                });
            }
        }
    }
}

/// Extract the body content of an `export def <name>` block: everything
/// between the body's opening `{` and its matching `}`. Skips the
/// parameter list (handles balanced `[ ]`). Returns None if the
/// brackets/braces are imbalanced or the def isn't found.
#[cfg(any())]
fn extract_def_body(source: &str, fn_name: &str) -> Option<String> {
    let pat = format!("export def {fn_name}");
    let pos = source.find(&pat)?;
    let bytes = source.as_bytes();
    let mut i = pos + pat.len();
    // Skip whitespace until `[`.
    while i < bytes.len() && bytes[i] != b'[' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    // Walk through the parameter list (balanced `[ ]`).
    let mut depth = 1usize;
    i += 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    // Skip whitespace until `{`.
    while i < bytes.len() && bytes[i] != b'{' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    // Body opens at `{`; walk until matching `}`.
    i += 1;
    let body_start = i;
    let mut depth = 1usize;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if depth != 0 {
        return None;
    }
    Some(source[body_start..i].to_string())
}

// ============================================================================
// import_library / reimport_library
// ============================================================================

/// What: imports a pre-authored library from `source_path` into the
/// canonical libraries repo under `<libraries>/<name>/`. Validates
/// the source tree first; on success, removes any existing subtree
/// for `name`, copies the source in (skipping dotfiles), writes the
/// kind=imported meta sidecar, and creates a signed git commit.
///
/// Why: import is the agent-authored path -- the agent builds and
/// tests the source locally, then asks the MCP to vendor it. The
/// strict pre-copy validation enforces our convention before any
/// disk mutation; failing fast keeps half-imported libraries from
/// appearing in the repo.
///
/// Where: called by `NuSh::import_library` after acquiring the per-
/// library write lock; the tool handler maps `ImportError` to
/// `mcp::ErrorData` via `import_error_to_mcp_error`.
pub(crate) fn import_library_impl(
    name: &str,
    source_path: &std::path::Path,
    engine: &ParseEngine,
) -> Result<(), ImportError> {
    if !is_valid_ident(name) {
        return Err(ImportError::InvalidLibraryName(name.to_string()));
    }
    if !source_path.exists() || !source_path.is_dir() {
        return Err(ImportError::SourceMissing(source_path.to_path_buf()));
    }
    let result =
        validate_library_source(source_path, engine).map_err(ImportError::Io)?;
    if !result.is_empty() {
        return Err(ImportError::Violations(result));
    }
    let dest = library_dir(name);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(ImportError::Io)?;
    }
    copy_dir_recursive(source_path, &dest).map_err(ImportError::Io)?;
    let meta = LibraryMeta {
        kind: LibraryKind::Imported,
        source_path: source_path.to_path_buf(),
    };
    let meta_bytes = json::to_vec(&meta)
        .map_err(|e| ImportError::Io(io::Error::other(format!("serialize meta: {e}"))))?;
    fs::write(library_meta_path(name), &meta_bytes).map_err(ImportError::Io)?;
    run_git(&libraries_dir(), &["add", "--", name]).map_err(ImportError::Io)?;
    let msg = format!("import library {name} from {}", source_path.display());
    run_git(&libraries_dir(), &["commit", "-m", &msg]).map_err(ImportError::Io)?;
    Ok(())
}

/// What: re-imports a library by re-reading the path recorded in its
/// meta sidecar, re-validating, and replacing the canonical copy.
/// Errors with `WrongKind` if the library was created via
/// register_library (only imported libraries reimport).
///
/// Why: agents iterate their library source locally; reimport lets
/// them publish a fresh snapshot without re-supplying the path.
/// Reading the path from meta (instead of taking it as a parameter)
/// prevents accidental redirection of the registration to a
/// different source.
///
/// Where: called by `NuSh::reimport_library` under the per-library
/// write lock; same error-mapping seam as import_library_impl.
pub(crate) fn reimport_library_impl(
    name: &str,
    engine: &ParseEngine,
) -> Result<(), ImportError> {
    if !is_valid_ident(name) {
        return Err(ImportError::InvalidLibraryName(name.to_string()));
    }
    let lib_root = library_dir(name);
    if !lib_root.exists() {
        return Err(ImportError::NotRegistered(name.to_string()));
    }
    let meta = load_meta(name).map_err(ImportError::Io)?;
    match meta.kind {
        LibraryKind::Imported => {}
        LibraryKind::Registered => {
            return Err(ImportError::WrongKind);
        }
    }
    let source_path = meta.source_path.clone();
    if !source_path.exists() || !source_path.is_dir() {
        return Err(ImportError::SourceMissing(source_path));
    }
    let result =
        validate_library_source(&source_path, engine).map_err(ImportError::Io)?;
    if !result.is_empty() {
        return Err(ImportError::Violations(result));
    }
    if lib_root.exists() {
        fs::remove_dir_all(&lib_root).map_err(ImportError::Io)?;
    }
    copy_dir_recursive(&source_path, &lib_root).map_err(ImportError::Io)?;
    let meta_bytes = json::to_vec(&meta)
        .map_err(|e| ImportError::Io(io::Error::other(format!("serialize meta: {e}"))))?;
    fs::write(library_meta_path(name), &meta_bytes).map_err(ImportError::Io)?;
    run_git(&libraries_dir(), &["add", "--", name]).map_err(ImportError::Io)?;
    let msg = format!("reimport library {name} from {}", source_path.display());
    run_git(&libraries_dir(), &["commit", "-m", &msg]).map_err(ImportError::Io)?;
    Ok(())
}

/// What: typed error variants for `import_library_impl` and
/// `reimport_library_impl`. Carries the offending value where
/// useful so the tool handler can build a precise message.
///
/// Why: keeping the typed error internal lets the library module
/// stay testable without rmcp; the mapping to `mcp::ErrorData` at
/// the seam preserves JSON-RPC error code semantics.
///
/// Where: returned by import + reimport impls; mapped by
/// `import_error_to_mcp_error` in `server::tool` to invalid_params
/// (client) or internal_error (server) variants.
#[derive(Debug)]
pub(crate) enum ImportError {
    Io(io::Error),
    InvalidLibraryName(String),
    SourceMissing(PathBuf),
    NotRegistered(String),
    WrongKind,
    Violations(ValidationResult),
}

/// What: copies a directory tree recursively. Skips dotfile entries
/// at every level (so a client `.git` doesn't bleed into the MCP
/// repo). Creates `dst` and all parents if needed.
///
/// Why: import_library copies a pre-authored source tree into the
/// canonical repo; dotfile skipping is mandatory to keep client-side
/// VCS metadata out of our git history. Recursive walk handles
/// arbitrary nesting without bookkeeping.
///
/// Where: called by `import_library_impl` and `reimport_library_impl`
/// after validation succeeds and any pre-existing subtree has been
/// wiped.
fn copy_dir_recursive(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if ft.is_file() {
            fs::copy(&src_path, &dst_path)?;
        }
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
