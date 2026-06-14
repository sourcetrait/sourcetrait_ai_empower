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

/// On-disk `<library>/.nushell_mcp_meta.json` shape. Git-tracked. Lives at
/// the root of every registered/imported library and records both the
/// authoring style and the client-side path the MCP either mirrors to
/// (registered) or re-reads from on reimport (imported).
#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct LibraryMeta {
    pub kind: LibraryKind,
    pub source_path: PathBuf,
}

/// Filename of the per-library metadata sidecar.
pub(crate) const META_FILE: &str = ".nushell_mcp_meta.json";

// ============================================================================
// Path helpers
// ============================================================================

/// What: returns `$XDG_DATA_HOME/sourcetrait/nushell_mcp/keypair/`,
/// the directory holding the MCP's git-signing keypair.
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

/// What: path to the private SSH key file (`id_nushell_mcp`).
///
/// Why: ed25519 private key the MCP uses to sign every commit it
/// makes to the libraries repo; per-repo `user.signingKey` is set to
/// this absolute path.
///
/// Where: used by `ensure_keypair` (writes via ssh-keygen) and
/// `configure_repo` (sets `user.signingKey` in the libraries repo's
/// per-repo git config).
pub(crate) fn private_key_path() -> PathBuf {
    keypair_dir().join("id_nushell_mcp")
}

/// What: path to the public key file (`id_nushell_mcp.pub`).
///
/// Why: needed to populate the `allowed_signers` file used by git's
/// SSH signature verification.
///
/// Where: read by `ensure_keypair` to compose the allowed_signers
/// line; written by `ssh-keygen` as a side effect of creating the
/// private key.
pub(crate) fn public_key_path() -> PathBuf {
    keypair_dir().join("id_nushell_mcp.pub")
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

/// What: returns `$XDG_DATA_HOME/sourcetrait/nushell_mcp/libraries/`,
/// the root of the MCP-managed git repo holding every registered
/// library.
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
/// (`<library_dir>/.nushell_mcp_meta.json`).
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
    /// Used by the call / define / undefine / reimport handlers and by
    /// `enumerate_libraries` to take the per-library guard.
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

const KEY_COMMENT: &str = "nushell_mcp@localhost";

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
        ("user.name", "nushell_mcp"),
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
) -> Result<(), Error> {
    if !is_valid_ident(name) {
        return Err(Error::LibraryInvalidName {
            library: name.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]*".to_string(),
        });
    }
    if build_target().is_test() && !name.ends_with("_test") {
        return Err(Error::LibraryTestSuffixRequired {
            library: name.to_string(),
        });
    }
    let lib_dir = library_dir(name);
    fs::create_dir_all(&lib_dir)?;
    // Empty cascade -- mod.nu re-exports nothing until define_function
    // populates it.
    fs::write(library_root_modnu_path(name), b"")?;
    let meta = LibraryMeta {
        kind: LibraryKind::Registered,
        source_path: client_path.to_path_buf(),
    };
    let meta_bytes = json::to_vec(&meta).map_err(|e| Error::Internal {
        phase: "register_library::serialize_meta".to_string(),
        reason: e.to_string(),
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
pub(crate) fn unregister_library_impl(name: &str) -> Result<(), Error> {
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
// leg 3: new() - the scaffold authoring surface
// ============================================================================

/// Result of `new()`: the source tree it scaffolded into + the paths
/// created (the agent now edits these, then `commit()`s).
#[derive(Debug, ser::Serialize)]
pub(crate) struct NewResult {
    pub source_path: String,
    pub created: Vec<String>,
}

/// The call/resolve/main skeleton a fresh function file is scaffolded
/// with: `record<>` placeholders (the unfleshed-skeleton marker the
/// validator rejects until real fields land), no doc. (leg 3)
fn skeleton_function_source() -> String {
    "export def call [args: record<>] {\n    # the function's raw logic; replace record<> with the real fields\n    {}\n}\n\nexport def resolve [args: record<>] {\n    $args\n}\n\nexport def main [args: record<>] {\n    resolve (call $args)\n}\n".to_string()
}

/// Append `entry` to `modnu` if not already present, preserving existing
/// (leg-1 authored) content -- the ADDITIVE cascade wiring that replaces
/// regenerate_mod_nu's full rewrite. (leg 3)
fn additively_wire_modnu(modnu: &std::path::Path, entry: &str) -> io::Result<()> {
    let existing = if modnu.exists() {
        fs::read_to_string(modnu)?
    } else {
        String::new()
    };
    if existing.lines().any(|l| l.trim() == entry) {
        return Ok(());
    }
    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(entry);
    out.push('\n');
    fs::write(modnu, out)
}

/// Validate the new() coordinate idents + apply the leg-1b reserved-terms
/// ban (no library / module segment / function named `call` or `resolve`).
fn validate_new_coordinate(
    library: &str,
    module_path: &str,
    name: Option<&str>,
) -> Result<(), Error> {
    if !is_valid_ident(library) || is_reserved_term(library) {
        return Err(Error::LibraryInvalidName {
            library: library.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]* and not be the reserved `call`/`resolve`".to_string(),
        });
    }
    if build_target().is_test() && !library.ends_with("_test") {
        return Err(Error::LibraryTestSuffixRequired {
            library: library.to_string(),
        });
    }
    if !is_valid_module_path(module_path) {
        return Err(Error::LibraryInvalidName {
            library: module_path.to_string(),
            reason: "invalid module path".to_string(),
        });
    }
    for seg in module_path.split('/').filter(|s| !s.is_empty()) {
        if is_reserved_term(seg) {
            return Err(Error::LibraryInvalidName {
                library: seg.to_string(),
                reason: "reserved `call`/`resolve` cannot name a module".to_string(),
            });
        }
    }
    if let Some(n) = name {
        if !is_valid_ident(n) || is_reserved_term(n) {
            return Err(Error::LibraryInvalidName {
                library: n.to_string(),
                reason: "invalid or reserved (`call`/`resolve`) function name".to_string(),
            });
        }
    }
    Ok(())
}

/// Implementation for `new(library, source_path?, module_path, name?)` -
/// the scaffold tool (leg 3). The FIRST call for a library establishes it
/// (records source_path in the canonical meta, immutable thereafter); it
/// then additively scaffolds the named leaf INTO the agent's source tree
/// (module dir + fresh mod.nu, or the call/resolve/main function skeleton),
/// where the agent edits it before `commit()`. LEAF-GUARD: refuses the
/// terminal coordinate if it already exists (ancestors are mkdir -p'd).
pub(crate) fn new_impl(
    library: &str,
    source_path: Option<&std::path::Path>,
    module_path: &str,
    name: Option<&str>,
) -> Result<NewResult, Error> {
    validate_new_coordinate(library, module_path, name)?;

    let canonical = library_dir(library);
    let established = canonical.exists();

    if !established {
        let sp = source_path.ok_or_else(|| Error::LibraryInvalidName {
            library: library.to_string(),
            reason: "source_path is required on the establishing new() call".to_string(),
        })?;
        fs::create_dir_all(&canonical)?;
        fs::write(library_root_modnu_path(library), b"")?;
        let meta = LibraryMeta {
            kind: LibraryKind::Imported,
            source_path: sp.to_path_buf(),
        };
        let meta_bytes = json::to_vec(&meta).map_err(|e| Error::Internal {
            phase: "new::serialize_meta".to_string(),
            reason: e.to_string(),
        })?;
        fs::write(library_meta_path(library), &meta_bytes)?;
        run_git(&libraries_dir(), &["add", "--", library])?;
        run_git(&libraries_dir(), &["commit", "-m", &format!("new library {library}")])?;
        fs::create_dir_all(sp)?;
        let root_modnu = sp.join("mod.nu");
        if !root_modnu.exists() {
            fs::write(&root_modnu, b"")?;
        }
    } else if source_path.is_some() {
        return Err(Error::LibraryAlreadyRegistered {
            library: library.to_string(),
        });
    }

    let meta = load_meta(library)?;
    let sp = meta.source_path.clone();
    let mut created: Vec<String> = Vec::new();

    // mkdir -p the module-path chain, additively wiring each level into
    // its parent's mod.nu. The terminal MODULE segment is leaf-guarded.
    let mut dir = sp.clone();
    if !module_path.is_empty() {
        let segs: Vec<&str> = module_path.split('/').collect();
        for (i, seg) in segs.iter().enumerate() {
            let parent_modnu = dir.join("mod.nu");
            let child = dir.join(seg);
            let terminal_module = name.is_none() && i == segs.len() - 1;
            if terminal_module && child.exists() {
                return Err(Error::LibraryInvalidName {
                    library: module_path.to_string(),
                    reason: "module already exists; edit it instead of scaffolding over it".to_string(),
                });
            }
            fs::create_dir_all(&child)?;
            let child_modnu = child.join("mod.nu");
            if !child_modnu.exists() {
                fs::write(&child_modnu, b"")?;
            }
            additively_wire_modnu(&parent_modnu, &format!("export module {seg}"))?;
            created.push(child.to_string_lossy().into_owned());
            dir = child;
        }
    }

    if let Some(fn_name) = name {
        let fn_file = dir.join(format!("{fn_name}.nu"));
        if fn_file.exists() {
            return Err(Error::LibraryInvalidName {
                library: fn_name.to_string(),
                reason: "function already exists; edit it instead of scaffolding over it".to_string(),
            });
        }
        fs::write(&fn_file, skeleton_function_source())?;
        additively_wire_modnu(&dir.join("mod.nu"), &format!("export use ./{fn_name}.nu"))?;
        created.push(fn_file.to_string_lossy().into_owned());
    }

    Ok(NewResult {
        source_path: sp.to_string_lossy().into_owned(),
        created,
    })
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
    out.push_str("export def main [args: ");
    out.push_str(args_schema);
    out.push_str("] {\n");
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("}\n\n");
    out.push_str("export def resolve [args: ");
    out.push_str(result_schema);
    out.push_str("] {\n    $args\n}\n");
    out
}

/// What: synthesizes the on-disk function source via
/// `synthesize_function_source` and parse-checks it through the supplied
/// `ParseEngine`, returning any parse errors as `Violation`s with
/// source-relative line numbers.
///
/// Why: `lint_body` reports rule violations but silently returns an empty
/// `Vec` when the wrapper parse fails (`find_def_body_id` early-return).
/// For `define_function` no downstream worker eval runs before disk write,
/// so a syntactically broken body would otherwise be committed to the
/// canonical libraries repo + mirror + signed commit, with the error only
/// surfacing at `call()` time. This helper closes the gap by surfacing
/// parse errors at the agent-facing seam. Mirrors the parse-correctness
/// check `validate_function_file_ast` already performs for
/// `import_library` / `reimport_library`.
///
/// Where: called by `server::tool::NuSh::define_function` after
/// `lint_body` passes but before `library_locks.lookup`. Non-empty result
/// triggers `-32602 invalid_params` with `format_violations`.
pub(crate) fn parse_check_function_source(
    engine: &ParseEngine,
    name: &str,
    args_schema: &str,
    result_schema: &str,
    body: &str,
) -> Vec<Violation> {
    let source = synthesize_function_source(args_schema, result_schema, body);
    let wrapper_name = format!("__pc_{name}");
    let (wrapped, prefix_len) = wrap_as_module(&source, &wrapper_name);
    let engine_state = engine.engine_state();
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    let virtual_name = format!("{name}.nu");
    let _ = nu::parse(&mut working_set, Some(&virtual_name), wrapped.as_bytes(), false);
    let mut violations = Vec::new();
    for err in &working_set.parse_errors {
        let span_start = err.span().start.saturating_sub(prefix_len);
        let (line, _col) = span_to_line_col(&source, span_start);
        violations.push(Violation {
            path: virtual_name.clone(),
            line,
            message: format!("parse error: {err:?}"),
        });
    }
    violations
}

// ============================================================================
// LibraryMeta load helper
// ============================================================================

/// What: reads + parses `<library>/.nushell_mcp_meta.json`, returning
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
) -> Result<(), Error> {
    if !is_valid_ident(library) {
        return Err(Error::LibraryInvalidName {
            library: library.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]*".to_string(),
        });
    }
    if !is_valid_module_path(module_path) {
        return Err(Error::LibraryInvalidModulePath {
            module_path: module_path.to_string(),
            reason: "slash-separated identifier segments; no `..`, no leading/trailing/double slash".to_string(),
        });
    }
    if !is_valid_ident(name) {
        return Err(Error::FunctionInvalidName {
            name: name.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]*".to_string(),
        });
    }
    let lib_root = library_dir(library);
    if !lib_root.exists() {
        return Err(Error::LibraryNotRegistered {
            library: library.to_string(),
        });
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
) -> Result<(), Error> {
    if !is_valid_ident(library) {
        return Err(Error::LibraryInvalidName {
            library: library.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]*".to_string(),
        });
    }
    if !is_valid_module_path(module_path) {
        return Err(Error::LibraryInvalidModulePath {
            module_path: module_path.to_string(),
            reason: "slash-separated identifier segments; no `..`, no leading/trailing/double slash".to_string(),
        });
    }
    if !is_valid_ident(name) {
        return Err(Error::FunctionInvalidName {
            name: name.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]*".to_string(),
        });
    }
    let lib_root = library_dir(library);
    if !lib_root.exists() {
        return Err(Error::LibraryNotRegistered {
            library: library.to_string(),
        });
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
        return Err(Error::FunctionNotDefined {
            library: library.to_string(),
            module_path: module_path.to_string(),
            name: name.to_string(),
        });
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
// Library enumeration (the info() hierarchy)
// ============================================================================

/// What: one callable function in the info() hierarchy - its name
/// plus the schema typedef strings extracted from the canonical
/// function file (`main`'s args record + `resolve`'s result record).
///
/// Why: the agent discovers call() targets live through info()
/// instead of relying on skill docs that go stale; the schemas are
/// the call contract. Structured per the item 21 grammar (the
/// canonical file's verbatim positional type parsed via
/// `schema::nu_to_args_schema` / `nu_to_result_schema`).
///
/// Where: built by `build_module_tree` from each `<name>.nu`;
/// carried in `ModuleInfo::functions` and `LibraryInfo::functions`;
/// serialized inside `InfoEnvelope::libraries`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub struct FunctionInfo {
    pub name: String,
    pub args_schema: mcp::JsonObject,
    pub result_schema: mcp::JsonObject,
}

/// What: one module node in the info() hierarchy. `name` is the
/// single path segment; `submodules` nests recursively; `functions`
/// holds this level's callables. Pure-namespace modules (no
/// functions, only submodules) appear as nodes.
///
/// Why: the envelope mirrors the library -> module -> function
/// hierarchy regardless of internal storage (the_user design lock),
/// and every level is a uniform node so the one-liner-docs followup
/// can attach documentation to libraries, modules, and functions
/// alike. The field reads `submodules` (vs `LibraryInfo::modules`)
/// because the relation differs: a library HAS modules, a module
/// HAS submodules; recursion below the library seam stays
/// single-field either way.
///
/// Where: built recursively by `build_module_tree`; carried in
/// `LibraryInfo::modules`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub struct ModuleInfo {
    pub name: String,
    pub submodules: Vec<ModuleInfo>,
    pub functions: Vec<FunctionInfo>,
}

/// What: one registered library in the info() hierarchy. `path` is
/// the meta sidecar's `source_path` - the agent-actionable directory
/// (the import source for imported libraries, the client mirror for
/// registered ones). The library acts as the root module: top-level
/// `functions` live directly on it.
///
/// Why: surfacing `source_path` (not the canonical repo dir, which
/// is storage internals) tells the agent where editing happens
/// before a reimport; the uniform node shape matches `ModuleInfo`
/// for the per-node-docs followup.
///
/// Where: built by `enumerate_libraries`; serialized as
/// `InfoEnvelope::libraries`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub struct LibraryInfo {
    pub name: String,
    pub path: String,
    pub modules: Vec<ModuleInfo>,
    pub functions: Vec<FunctionInfo>,
}

/// What: scan `source` for `marker` (an `export def <name>`), then
/// capture the full positional TYPE of its `args` parameter -- the
/// text between `args:` and the param list's closing `]`. That type
/// is `record<...>` for a normal function or `nothing` for a
/// void/no-arg one (item 21); typedef text never contains `]`, so the
/// first `]` after the marker closes the param list.
///
/// Why: info() emits the structured schema by parsing this verbatim
/// text via `schema::nu_to_args_schema` / `nu_to_result_schema`; the
/// canonical file is the single source of truth for the call contract.
///
/// Where: called twice per function file by `extract_function_schemas`.
fn positional_type_after(source: &str, marker: &str) -> Option<String> {
    let marker_at = source.find(marker)?;
    let tail = &source[marker_at..];
    let bracket_at = tail.find('[')?;
    let after_bracket = &tail[bracket_at + 1..];
    let close_rel = after_bracket.find(']')?;
    let params = &after_bracket[..close_rel];
    let colon = params.find(':')?;
    Some(params[colon + 1..].trim().to_string())
}

/// What: extract `(args_schema, result_schema)` from a canonical
/// function file's source. Empty strings on a failed scan - which
/// cannot happen for files that passed the strict validator; the
/// degenerate value keeps enumeration total rather than dropping
/// the function silently.
///
/// Why: info()'s FunctionInfo carries the call contract; the
/// canonical file is the single source of truth for it.
///
/// Where: called by `build_module_tree` for every `<name>.nu`.
fn extract_function_schemas(source: &str) -> (String, String) {
    let args = positional_type_after(source, "export def main").unwrap_or_default();
    let result = positional_type_after(source, "export def resolve").unwrap_or_default();
    (args, result)
}

/// What: recursively walk one directory of a canonical library,
/// returning its (sub)modules and functions, both sorted by name.
/// Skips dotfiles and `mod.nu` (cascade plumbing, not surface);
/// only subdirectories carrying a `mod.nu` count as modules.
///
/// Why: the on-disk tree IS the module hierarchy (the mod.nu
/// cascade mirrors it), so a sorted directory walk reproduces the
/// library -> module -> function structure deterministically.
///
/// Where: called by `enumerate_libraries` at each library root and
/// by itself for nested modules.
fn build_module_tree(
    dir: &std::path::Path,
) -> io::Result<(Vec<ModuleInfo>, Vec<FunctionInfo>)> {
    let mut entries: Vec<(String, bool)> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "mod.nu" {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() {
            if entry.path().join("mod.nu").exists() {
                entries.push((name, true));
            }
        } else if ft.is_file() && name.ends_with(".nu") {
            entries.push((name, false));
        }
    }
    entries.sort();
    let mut modules = Vec::new();
    let mut functions = Vec::new();
    for (name, is_dir) in entries {
        if is_dir {
            let (m, f) = build_module_tree(&dir.join(&name))?;
            modules.push(ModuleInfo {
                name,
                submodules: m,
                functions: f,
            });
        } else {
            let source = fs::read_to_string(dir.join(&name))?;
            let stem = name.trim_end_matches(".nu").to_string();
            let (args_type, result_type) = extract_function_schemas(&source);
            let args_schema = nu_to_args_schema(&args_type)
                .map_err(|e| io::Error::other(format!("{stem}: args schema: {e}")))?;
            let result_schema = nu_to_result_schema(&result_type)
                .map_err(|e| io::Error::other(format!("{stem}: result schema: {e}")))?;
            functions.push(FunctionInfo {
                name: stem,
                args_schema,
                result_schema,
            });
        }
    }
    Ok((modules, functions))
}

/// What: enumerate every canonical library into the info()
/// hierarchy. Walks `libraries_dir()` for subdirs carrying the meta
/// sidecar (the same marker `hydrate_from_disk` keys on), takes the
/// per-library READ lock while reading that library's meta +
/// function files (the_user ruling), and builds the recursive node
/// tree. Libraries sorted by name; a library whose meta or tree
/// read fails is skipped with a host-stderr note rather than
/// failing the whole info() call.
///
/// Why: gives the agent a live, always-current view of the call()
/// surface; the read lock means a concurrent define/reimport can't
/// tear an enumeration mid-library.
///
/// Where: called by `server::tool::NuSh::info` to populate
/// `InfoEnvelope::libraries`.
pub(crate) async fn enumerate_libraries(locks: &LibraryLocks) -> Vec<LibraryInfo> {
    let dir = libraries_dir();
    let mut names: Vec<String> = Vec::new();
    if let Ok(read) = fs::read_dir(&dir) {
        for entry in read.flatten() {
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            if !entry.path().join(META_FILE).exists() {
                continue;
            }
            if let Ok(name) = entry.file_name().into_string() {
                names.push(name);
            }
        }
    }
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let lock = locks.lookup(&name).await;
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        let meta = match load_meta(&name) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("nushell_mcp: info enumeration skipped {name}: {e}");
                continue;
            }
        };
        match build_module_tree(&library_dir(&name)) {
            Ok((modules, functions)) => out.push(LibraryInfo {
                name,
                path: meta.source_path.display().to_string(),
                modules,
                functions,
            }),
            Err(e) => {
                eprintln!("nushell_mcp: info enumeration skipped {name}: {e}");
            }
        }
    }
    out
}

// ============================================================================
// Strict library source validator (for import_library / reimport_library)
// ============================================================================

#[derive(Debug, Clone, ser::Serialize, ser::Deserialize, schema::JsonSchema)]
pub struct Violation {
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
}

impl ValidationResult {
    pub(crate) fn is_empty(&self) -> bool {
        self.structural.is_empty()
    }
}

/// Walk every `.nu` under `root`. Apply the strict per-file shape:
/// - `mod.nu`: only `export use ./<file>.nu` or `export module <name>` lines
///   (plus blank lines and `#` comments). Body-lint NOT applied (no agent
///   code lives in mod.nu).
/// - Function files: exactly two exports named `main` and `resolve`;
///   both have `args: record<...>` typed positionals; resolve's body is
///   exactly the expression `$args`. Authored bodies are NOT lint-checked
///   (item 7, the_user 2026-06-12: imports are authored with intent; the
///   AST body-lint covers the on-the-fly run/interact/define path only).
///
/// Each file is parsed through `nu_parser::parse` (in a
/// `module __v_<stem> { ... }` wrapper) so syntax errors land as
/// structural violations with line numbers. Dotfile entries (e.g.
/// `.git`, `.nushell_mcp_meta.json`) are skipped. Returns ALL structural
/// violations -- no bail-on-first; no auto-fix.
pub(crate) fn validate_library_source(
    root: &std::path::Path,
    engine: &ParseEngine,
) -> io::Result<ValidationResult> {
    let mut result = ValidationResult {
        structural: Vec::new(),
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
        );
    }
    // leg 1b: the reserved-terms ban applies to EVERY .nu file -- `call`
    // and `resolve` may appear only as a call-target's exported sentinel.
    scan_reserved_terms(&rel, stem, &source, parent, engine, &mut result.structural);
    Ok(())
}

/// leg 1b: the reserved-terms ban. `call` and `resolve` may appear in a
/// library ONLY as the exported call/resolve trie of a call-target (1a
/// validates that contract). ANY OTHER occurrence as an identifier is a
/// violation -- a private/nested def, a module/dir/file name, a const /
/// alias / let / mut binding, a parameter, a record/table column key, or a
/// cell-path member. Quoted string *values* and command references
/// (`resolve (call $args)`) are not identifiers and pass.
///
/// `nu_parser::flatten_block` tags each token with a `FlatShape`:
/// `call`/`resolve` flag at `VarDecl` (let/mut/const names) and `String`
/// (def/module/alias names, record/table keys, cell-path members,
/// barewords) -- except a `String` token immediately following an
/// `export def` token, which is the call-target's valid sentinel export.
/// Command references parse as `InternalCall`/`External` (skipped), and a
/// quoted `"call"` keeps its quotes in the token content so it never
/// matches `call`. Parameter names hide inside the `Signature` token, so
/// they are walked separately (TODO leg 1b params); module/dir/file names
/// are checked on the path.
fn scan_reserved_terms(
    rel: &str,
    stem: &str,
    source: &str,
    parent: &std::path::Path,
    engine: &ParseEngine,
    violations: &mut Vec<Violation>,
) {
    // 1. Path components: no module / directory / file named call|resolve.
    for comp in rel.split('/') {
        let bare = comp.strip_suffix(".nu").unwrap_or(comp);
        if is_reserved_term(bare) {
            violations.push(Violation {
                path: rel.to_string(),
                line: 0,
                message: format!(
                    "`{bare}` is reserved (the call-target sentinel) and cannot name a module, directory, or file",
                ),
            });
        }
    }

    // 2. Parse + flatten the token stream.
    let wrapper_name = format!("__rt_{stem}");
    let (wrapped, prefix_len) = wrap_as_module(source, &wrapper_name);
    let engine_state = engine.engine_state_for_file(parent);
    let mut working_set = nu::StateWorkingSet::new(&engine_state);
    let block = nu::parse(&mut working_set, Some(rel), wrapped.as_bytes(), false);
    if !working_set.parse_errors.is_empty() {
        // Parse errors are surfaced by the structural validator; don't
        // scan a half-parsed token stream.
        return;
    }

    let mut prev_export_def = false;
    for (span, shape) in nu::flatten_block(&working_set, &block) {
        let content = wrapped.get(span.start..span.end).unwrap_or("");
        let flag = is_reserved_term(content)
            && match &shape {
                nu::FlatShape::VarDecl(_) => true,
                nu::FlatShape::String => !prev_export_def,
                _ => false,
            };
        if flag {
            let src_off = span.start.saturating_sub(prefix_len);
            let (line, _col) = span_to_line_col(source, src_off);
            violations.push(Violation {
                path: rel.to_string(),
                line,
                message: format!(
                    "`{content}` is reserved -- it may appear only as a call-target's exported `call`/`resolve`; rename this identifier",
                ),
            });
        }
        // Parameter names live inside a single Signature token (flatten
        // does not tokenize them individually), so pull the top-level
        // param names out of the signature text.
        if matches!(&shape, nu::FlatShape::Signature) {
            for pname in signature_param_names(content) {
                if is_reserved_term(&pname) {
                    let src_off = span.start.saturating_sub(prefix_len);
                    let (line, _col) = span_to_line_col(source, src_off);
                    violations.push(Violation {
                        path: rel.to_string(),
                        line,
                        message: format!(
                            "`{pname}` is reserved and cannot be a parameter name; rename it",
                        ),
                    });
                }
            }
        }
        prev_export_def =
            matches!(&shape, nu::FlatShape::InternalCall(_)) && content == "export def";
    }
}

/// `call` and `resolve` are the reserved call-target sentinel names.
fn is_reserved_term(s: &str) -> bool {
    s == "call" || s == "resolve"
}

/// Extract the top-level parameter names from a flattened `Signature`
/// token (`[call: int, --flag: string, ...rest]`, or a closure `|call|`),
/// skipping names nested inside type annotations (`record<resolve: int>`)
/// and stripping flag / rest sigils so `--call` and `...call` surface as
/// `call`. A name slot opens at signature start and after each depth<=1
/// comma; `:` and any nested `<([{` close it.
fn signature_param_names(sig: &str) -> Vec<String> {
    let chars: Vec<char> = sig.chars().collect();
    let n = chars.len();
    let mut names = Vec::new();
    let mut depth: i32 = 0;
    let mut at_slot_start = true;
    let mut i = 0;
    while i < n {
        let c = chars[i];
        match c {
            '[' | '<' | '(' | '{' => {
                if depth > 0 {
                    at_slot_start = false;
                }
                depth += 1;
                i += 1;
            }
            ']' | '>' | ')' | '}' => {
                depth -= 1;
                i += 1;
            }
            ',' if depth <= 1 => {
                at_slot_start = true;
                i += 1;
            }
            ':' => {
                at_slot_start = false;
                i += 1;
            }
            c if c.is_whitespace() => {
                i += 1;
            }
            _ if at_slot_start && depth <= 1 => {
                let start = i;
                while i < n {
                    let cc = chars[i];
                    if cc.is_alphanumeric() || cc == '_' || cc == '-' || cc == '.' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let raw: String = chars[start..i].iter().collect();
                let name = raw
                    .trim_start_matches("...")
                    .trim_start_matches("--")
                    .trim_start_matches('-');
                if !name.is_empty() {
                    names.push(name.to_string());
                }
                at_slot_start = false;
            }
            _ => {
                i += 1;
            }
        }
    }
    names
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
            // leg 1: mod.nu carries the cascade (`export use`/`export
            // module`) AND module-level shared utils/consts (`export
            // const`/`export def`).
            if matches!(name, "export use" | "export module" | "export const" | "export def") {
                return;
            }
            violations.push(Violation {
                path: rel.to_string(),
                line,
                message: format!(
                    "mod.nu may only contain `export use`, `export module`, `export const`, or `export def` statements; got call to `{name}`",
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
                    "mod.nu may only contain `export use`, `export module`, `export const`, or `export def` statements; got `{}`",
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

    // 3. Classify by the call / resolve sentinel. nu's Module tracks
    //    `main` separately from `decls` (which holds call / resolve / any
    //    extras). A file exporting `call` or `resolve` is a CALL-TARGET and
    //    must satisfy the full call/resolve/main contract; a file with
    //    neither sentinel is ORGANIZATIONAL (helper defs / export const /
    //    export def) and is left to parse-correctness only. The reserved-
    //    terms ban (leg 1b) makes the sentinel airtight. (leg 1)
    let main_decl = module.main;
    let mut export_names: Vec<(String, nu::DeclId)> = module
        .decls
        .iter()
        .filter(|(name_bytes, _)| name_bytes.as_slice() != b"main")
        .map(|(name_bytes, decl_id)| {
            (String::from_utf8_lossy(name_bytes).into_owned(), *decl_id)
        })
        .collect();
    export_names.sort_by(|a, b| a.0.cmp(&b.0));

    let call_decl = export_names.iter().find(|(n, _)| n == "call").map(|(_, id)| *id);
    let resolve_decl = export_names.iter().find(|(n, _)| n == "resolve").map(|(_, id)| *id);

    if call_decl.is_none() && resolve_decl.is_none() {
        // Organizational file: a plain module (helper defs, export const,
        // export def). No call/resolve sentinel -> no contract to enforce;
        // parse-correctness (checked above) is sufficient here. (leg 1)
        return;
    }

    // 4. Call-target: enforce the full call / resolve / main contract.
    if call_decl.is_none() {
        violations.push(Violation {
            path: rel.to_string(),
            line: 0,
            message: "call-target function file must export `call` (the raw logic): `export def call [args: <T>] { ... }`"
                .to_string(),
        });
    }
    if resolve_decl.is_none() {
        violations.push(Violation {
            path: rel.to_string(),
            line: 0,
            message: "call-target function file must export `resolve` (the result typecheck): `export def resolve [args: <R>] { $args }`"
                .to_string(),
        });
    }
    if main_decl.is_none() {
        violations.push(Violation {
            path: rel.to_string(),
            line: 0,
            message: "call-target function file must export `main` (the validated sugar): `export def main [args: <T>] { resolve (call $args) }`"
                .to_string(),
        });
    }
    for (name, _) in &export_names {
        if name != "call" && name != "resolve" {
            violations.push(Violation {
                path: rel.to_string(),
                line: 0,
                message: format!(
                    "a call-target function file exports only `call`, `resolve`, and `main`; saw `export def {name}` (move helpers to an organizational file and `use ./<file>.nu`)",
                ),
            });
        }
    }

    // 5. Signatures + bodies.
    //    call: typed `args` positional (record<...> with real fields, or
    //    nothing); body is the author's raw logic -- unchecked.
    if let Some(id) = call_decl {
        check_args_record_positional(rel, &working_set, id, "call", source, prefix_len, violations);
    }
    //    resolve: typed `args` positional (the result schema); body exactly
    //    `$args` (the passthrough that forces the runtime result check).
    if let Some(id) = resolve_decl {
        let resolve_args_var =
            check_args_record_positional(rel, &working_set, id, "resolve", source, prefix_len, violations);
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
    //    main: typed `args` positional mirroring call's; body AST-locked to
    //    EXACTLY `resolve (call $args)` -- the generated sugar; the author
    //    owns only main's doc comment, never its body.
    if let Some(id) = main_decl {
        let main_args_var =
            check_args_record_positional(rel, &working_set, id, "main", source, prefix_len, violations);
        check_main_body_is_resolve_call_args(
            rel,
            &working_set,
            id,
            main_args_var,
            source,
            prefix_len,
            violations,
        );
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
            // Accept `record<...>` (normal) or `nothing` (void/no-arg)
            // -- item 21. to_type() maps the positional's SyntaxShape to
            // its value Type, so the check is robust to the exact shape.
            // An empty `record<>` is the unfleshed-skeleton marker --
            // reject it to force real fields (or `nothing`). (leg 1)
            let shape_ok = match p.shape.to_type() {
                nu::Type::Record(fields) => !fields.is_empty(),
                nu::Type::Nothing => true,
                _ => false,
            };
            ((p.name != "args" || !shape_ok), p.var_id)
        }
    };
    if bad {
        let line = decl_line(working_set, decl_id, source, prefix_len);
        violations.push(Violation {
            path: rel.to_string(),
            line,
            message: format!(
                "{fn_name} must take a typed positional `args: record<...>` with real fields (or `args: nothing` for void); an empty `record<>` is the unfleshed skeleton",
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

/// Confirm `main`'s body is exactly `resolve (call $args)` -- one
/// pipeline, one element: a Call to `resolve` whose single positional is
/// the parenthesized `(call $args)` (a Call to `call` taking the `args`
/// positional). The shape is fixed because it is GENERATED and mirrors
/// call()'s composition; the author owns only main's doc comment.
fn check_main_body_is_resolve_call_args(
    rel: &str,
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    main_args_var: Option<nu::VarId>,
    source: &str,
    prefix_len: usize,
    violations: &mut Vec<Violation>,
) {
    let decl = working_set.get_decl(decl_id);
    let line_of_decl = decl_line(working_set, decl_id, source, prefix_len);
    let mut ok = false;
    if let Some(block_id) = decl.block_id() {
        let block = working_set.get_block(block_id);
        if block.pipelines.len() == 1 && block.pipelines[0].elements.len() == 1 {
            ok = is_resolve_of_call_args(
                &block.pipelines[0].elements[0].expr,
                working_set,
                main_args_var,
            );
        }
    }
    if !ok {
        violations.push(Violation {
            path: rel.to_string(),
            line: line_of_decl,
            message: "main's body must be exactly `resolve (call $args)`".to_string(),
        });
    }
}

/// Match `resolve (call $args)`: a Call to `resolve` with one positional
/// argument that is `(call $args)`.
fn is_resolve_of_call_args(
    expr: &nu::Expression,
    working_set: &nu::StateWorkingSet,
    args_var: Option<nu::VarId>,
) -> bool {
    let nu::Expr::Call(outer) = &expr.expr else {
        return false;
    };
    if working_set.get_decl(outer.decl_id).name() != "resolve" {
        return false;
    }
    let mut pos = outer.arguments.iter().filter_map(|a| match a {
        nu::Argument::Positional(e) => Some(e),
        _ => None,
    });
    let (Some(arg), None) = (pos.next(), pos.next()) else {
        return false;
    };
    inner_is_call_args(arg, working_set, args_var)
}

/// Match the `(call $args)` argument: a parenthesized subexpression (or a
/// bare call) wrapping `call $args`.
fn inner_is_call_args(
    expr: &nu::Expression,
    working_set: &nu::StateWorkingSet,
    args_var: Option<nu::VarId>,
) -> bool {
    match &expr.expr {
        nu::Expr::Subexpression(block_id) => {
            block_is_call_args(*block_id, working_set, args_var)
        }
        nu::Expr::FullCellPath(fcp) if fcp.tail.is_empty() => match &fcp.head.expr {
            nu::Expr::Subexpression(block_id) => {
                block_is_call_args(*block_id, working_set, args_var)
            }
            nu::Expr::Call(_) => is_call_args_call(&fcp.head, working_set, args_var),
            _ => false,
        },
        nu::Expr::Call(_) => is_call_args_call(expr, working_set, args_var),
        _ => false,
    }
}

/// A subexpression block holding exactly one `call $args` pipeline element.
fn block_is_call_args(
    block_id: nu::BlockId,
    working_set: &nu::StateWorkingSet,
    args_var: Option<nu::VarId>,
) -> bool {
    let block = working_set.get_block(block_id);
    if block.pipelines.len() != 1 || block.pipelines[0].elements.len() != 1 {
        return false;
    }
    is_call_args_call(&block.pipelines[0].elements[0].expr, working_set, args_var)
}

/// Match `call $args`: a Call to `call` whose one positional is `$args`
/// (main's `args` positional VarId).
fn is_call_args_call(
    expr: &nu::Expression,
    working_set: &nu::StateWorkingSet,
    args_var: Option<nu::VarId>,
) -> bool {
    let nu::Expr::Call(inner) = &expr.expr else {
        return false;
    };
    if working_set.get_decl(inner.decl_id).name() != "call" {
        return false;
    }
    let mut pos = inner.arguments.iter().filter_map(|a| match a {
        nu::Argument::Positional(e) => Some(e),
        _ => None,
    });
    let (Some(arg), None) = (pos.next(), pos.next()) else {
        return false;
    };
    is_args_var(arg, args_var)
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
) -> Result<(), Error> {
    if !is_valid_ident(name) {
        return Err(Error::LibraryInvalidName {
            library: name.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]*".to_string(),
        });
    }
    if build_target().is_test() && !name.ends_with("_test") {
        return Err(Error::LibraryTestSuffixRequired {
            library: name.to_string(),
        });
    }
    if !source_path.exists() || !source_path.is_dir() {
        return Err(Error::LibrarySourceMissing {
            path: source_path.display().to_string(),
        });
    }
    let result = validate_library_source(source_path, engine)?;
    if !result.is_empty() {
        return Err(Error::LibraryViolations {
            structural: result.structural,
            lint: vec![],
        });
    }
    let dest = library_dir(name);
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    copy_dir_recursive(source_path, &dest)?;
    let meta = LibraryMeta {
        kind: LibraryKind::Imported,
        source_path: source_path.to_path_buf(),
    };
    let meta_bytes = json::to_vec(&meta).map_err(|e| Error::Internal {
        phase: "import_library::serialize_meta".to_string(),
        reason: e.to_string(),
    })?;
    fs::write(library_meta_path(name), &meta_bytes)?;
    run_git(&libraries_dir(), &["add", "--", name])?;
    let msg = format!("import library {name} from {}", source_path.display());
    run_git(&libraries_dir(), &["commit", "-m", &msg])?;
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
) -> Result<(), Error> {
    if !is_valid_ident(name) {
        return Err(Error::LibraryInvalidName {
            library: name.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]*".to_string(),
        });
    }
    let lib_root = library_dir(name);
    if !lib_root.exists() {
        return Err(Error::LibraryNotRegistered {
            library: name.to_string(),
        });
    }
    let meta = load_meta(name)?;
    match meta.kind {
        LibraryKind::Imported => {}
        LibraryKind::Registered => {
            return Err(Error::LibraryWrongKind {
                library: name.to_string(),
            });
        }
    }
    let source_path = meta.source_path.clone();
    if !source_path.exists() || !source_path.is_dir() {
        return Err(Error::LibrarySourceMissing {
            path: source_path.display().to_string(),
        });
    }
    let result = validate_library_source(&source_path, engine)?;
    if !result.is_empty() {
        return Err(Error::LibraryViolations {
            structural: result.structural,
            lint: vec![],
        });
    }
    if lib_root.exists() {
        fs::remove_dir_all(&lib_root)?;
    }
    copy_dir_recursive(&source_path, &lib_root)?;
    let meta_bytes = json::to_vec(&meta).map_err(|e| Error::Internal {
        phase: "reimport_library::serialize_meta".to_string(),
        reason: e.to_string(),
    })?;
    fs::write(library_meta_path(name), &meta_bytes)?;
    run_git(&libraries_dir(), &["add", "--", name])?;
    let msg = format!("reimport library {name} from {}", source_path.display());
    run_git(&libraries_dir(), &["commit", "-m", &msg])?;
    Ok(())
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
// Inline tests -- direct unit coverage of parse_check_function_source
// against representative broken-body shapes. Integration coverage via
// tests/library_define.rs exercises the rmcp handler wire-up end-to-end.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> ParseEngine {
        ParseEngine::new_full()
    }

    fn pc(body: &str) -> Vec<Violation> {
        parse_check_function_source(&engine(), "probe", "record<x: int>", "record<out: int>", body)
    }

    #[test]
    fn clean_body_no_violations() {
        let v = pc("{ out: ($args.x * 2) }");
        assert!(v.is_empty(), "expected no violations, got {v:?}");
    }

    #[test]
    fn flags_unclosed_string() {
        let v = pc("\"unclosed");
        assert!(!v.is_empty(), "unclosed string should be a parse error");
    }

    #[test]
    fn flags_unbalanced_braces() {
        let v = pc("}}}");
        assert!(!v.is_empty(), "extra close braces should be a parse error");
    }

    #[test]
    fn flags_shell_and_and() {
        let v = pc("true && false");
        assert!(!v.is_empty(), "&& should be rejected as shell_and_and");
    }

    #[test]
    fn flags_trailing_assignment() {
        let v = pc("let z =");
        assert!(!v.is_empty(), "incomplete let assignment should parse error");
    }

    #[test]
    fn let_without_eq() {
        // Nushell may or may not accept this; document actual behavior.
        let v = pc("let z");
        // Whatever the result, log it.
        eprintln!("let_without_eq: violations={v:?}");
    }
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
