use crate::*;

// ============================================================================
// Library metadata
// ============================================================================

/// On-disk `<library>/.meta/library.json` shape (big meta): the call()
/// surface INDEX. Git-tracked. Carries `source_path` (the agent's tree the
/// MCP re-reads on `commit` + sanity-checks on `delete`) plus the recursive
/// module -> function tree with schemas, SANS docs (docs live as sibling
/// markdown under `.meta/docs/`). Built at commit during validation; read by
/// info() / call() / inspect() on the hot path instead of re-walking +
/// re-parsing the canonical `.nu` tree. ONLY call-targets are listed as
/// `functions` and ONLY modules carrying a call-target (directly or
/// transitively) appear in `modules` - helper files + helper-only modules are
/// pruned. Replaces the flat pre-big-meta `.nushell_mcp_meta.json` sidecar;
/// `#[serde(default)]` on the tree fields lets an establish-time index
/// (source_path only) deserialize cleanly.
#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct LibraryIndex {
    pub source_path: PathBuf,
    #[serde(default)]
    pub functions: Vec<IndexFunction>,
    #[serde(default)]
    pub modules: Vec<IndexModule>,
}

/// One call-target in the `library.json` index: its name plus the structured
/// schemas extracted from `main`'s PARSED signature (args from the leading
/// positional, result from the output type) at commit time. Mirrors
/// `FunctionInfo` sans the summary (which lives in `.meta/docs/`).
#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct IndexFunction {
    pub name: String,
    pub args_schema: mcp::JsonObject,
    pub result_schema: mcp::JsonObject,
}

/// One module node in the `library.json` index: a single path segment plus its
/// call-target `functions` and (pruned) `modules`. Pure-helper modules (no
/// call-target anywhere below) are absent. Mirrors `ModuleInfo` sans summary.
#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct IndexModule {
    pub name: String,
    #[serde(default)]
    pub functions: Vec<IndexFunction>,
    #[serde(default)]
    pub modules: Vec<IndexModule>,
}

/// Per-library path (relative to the library dir) of the big-meta index
/// sidecar. Doubles as the library-detection marker: a subdir with
/// `.meta/library.json` present IS a registered library (the same key
/// `hydrate_from_disk` + `enumerate_libraries` test). The `.meta/` segment is a
/// dotfile, so `copy_dir_recursive` + `validate_walk` skip it (MCP-generated,
/// never authored, never validated); git still tracks it as part of the signed
/// canonical subtree.
pub(crate) const META_FILE: &str = ".meta/library.json";

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
/// across all lifecycle ops (new / commit / delete); each library is a
/// top-level subdir within it.
///
/// Where: called by every git-aware helper (`new_impl`, `commit_impl`,
/// `delete_impl`, etc.) to compose paths and by `run_git` callers to
/// set the `git -C` directory.
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

/// What: path to a library's big-meta index sidecar
/// (`<library_dir>/.meta/library.json`).
///
/// Why: the index carries `source_path` (re-read on commit, sanity-checked
/// on delete) plus the call() surface tree (read by info / call / inspect).
///
/// Where: written by `new_impl` (establish) + `commit_impl` (rebuild); read
/// by `load_index`.
pub(crate) fn library_meta_path(library: &str) -> PathBuf {
    library_dir(library).join(META_FILE)
}

/// What: a library's `.meta/` dir (`<library_dir>/.meta`), holding the
/// `library.json` index + the `docs/` tree.
///
/// Why: one MCP-generated dotfile dir per library; `commit_impl` recreates it
/// after wiping + copying the authored source (the copy skips dotfiles, so the
/// authored tree never carries one).
///
/// Where: called by `commit_impl` (mkdir + write) and the docs path helper.
pub(crate) fn library_meta_dir(library: &str) -> PathBuf {
    library_dir(library).join(".meta")
}

/// What: a library's doc tree root (`<library_dir>/.meta/docs`). Per-node docs
/// live at `<docs>/<coord>/{summary.md,details.md}` (coord: root = ``, module =
/// `<module_path>`, function = `<module_path>/<name>`).
///
/// Why: prose is split out of `library.json` so the index stays lean; info()
/// overlays the one-liner summary, inspect() returns summary + details.
///
/// Where: written by `commit_impl`; read by `enumerate_libraries` +
/// `inspect_impl`.
pub(crate) fn library_docs_dir(library: &str) -> PathBuf {
    library_meta_dir(library).join("docs")
}

/// What: path to a library's root `mod.nu` file.
///
/// Why: the root mod.nu is the entry point of a library's cascade --
/// `use <library>` resolves to it. Always exists for an established
/// library (created empty at establish time).
///
/// Where: created empty by `new_impl` (establish); the authored cascade
/// is promoted verbatim by `commit_impl`.
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
/// - Writes (`commit`/`delete`) acquire a WRITE lock on the entry,
///   serializing within a library but not across libraries.
/// - `new` (establish) holds the outer Mutex briefly to insert the
///   entry, then proceeds under the per-library write lock.
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
                map.entry(name)
                    .or_insert_with(|| Arc::new(tk::AsyncRwLock::new(())));
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
    /// Used by the call / commit / delete handlers and by
    /// `enumerate_libraries` to take the per-library guard.
    pub(crate) async fn lookup(&self, name: &str) -> Option<Arc<tk::AsyncRwLock<()>>> {
        let map = self.map.lock().await;
        map.get(name).cloned()
    }

    /// Remove a library's lock entry. Called after `library(uninstall)` drops
    /// the canonical subtree, so the in-memory registry matches on-disk state.
    pub(crate) async fn unregister(&self, name: &str) {
        let mut map = self.map.lock().await;
        map.remove(name);
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
/// Where: returned by `LibraryLocks::register`; matched by `new_impl`
/// to surface `library::already_registered` to the agent.
pub(crate) struct AlreadyRegistered;

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
            .arg("-t")
            .arg("ed25519")
            .arg("-f")
            .arg(&priv_path)
            .arg("-N")
            .arg("")
            .arg("-C")
            .arg(KEY_COMMENT)
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
        run_git(
            &dir,
            &["commit", "--allow-empty", "-m", "init libraries repo"],
        )?;
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
pub(crate) fn run_git(dir: &std::path::Path, args: &[&str]) -> io::Result<()> {
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
// leg 3: new() - the scaffold authoring surface
// ============================================================================

/// The single-`main` skeleton a fresh function file is scaffolded with:
/// `record<>` placeholders on BOTH the args positional and the
/// `: nothing -> record<>` output type (the unfleshed-skeleton marker
/// commit rejects on either side until real fields land, or `nothing` for
/// void). The author owns the whole body.
fn skeleton_function_source() -> String {
    "# one-line summary (<= 80 chars); becomes this function's doc\nexport def main [args: record<>]: nothing -> record<> {\n    # logic here; replace each record<> with real fields (or `nothing` for void)\n    {}\n}\n".to_string()
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

/// Validate the new() coordinate idents + apply the reserved-terms
/// ban (no library / module segment / function named `main`).
fn validate_new_coordinate(
    library: &str,
    module_path: &str,
    name: Option<&str>,
) -> Result<(), Error> {
    if !is_valid_ident(library) || is_reserved_term(library) {
        return Err(Error::LibraryInvalidName {
            library: library.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]* and not be the reserved `main`"
                .to_string(),
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
                reason: "reserved `main` cannot name a module".to_string(),
            });
        }
    }
    if let Some(n) = name {
        if !is_valid_ident(n) || is_reserved_term(n) {
            return Err(Error::LibraryInvalidName {
                library: n.to_string(),
                reason: "invalid or reserved (`main`) function name".to_string(),
            });
        }
    }
    Ok(())
}

/// Establish a fresh library: create the canonical subtree (empty root
/// mod.nu + an establish-time `.meta/library.json` recording `source_path`),
/// land a signed `new library <name>` commit, and seed `source_path/mod.nu`.
///
/// Why: extracted from the old `new_impl` establish path so the `library()`
/// admin tool's `new` + `install` actions share one establishment routine -
/// `new()` itself no longer establishes (it only scaffolds into existing
/// libraries). Errors `LibraryAlreadyRegistered` if the canonical dir already
/// exists (the name is taken).
///
/// Where: called by `tool::library` (the `new` + `install` actions).
pub(crate) fn establish_library(library: &str, source_path: &std::path::Path) -> Result<(), Error> {
    validate_new_coordinate(library, "", None)?;
    let canonical = library_dir(library);
    if canonical.exists() {
        return Err(Error::LibraryAlreadyRegistered {
            library: library.to_string(),
        });
    }
    fs::create_dir_all(&canonical)?;
    fs::write(library_root_modnu_path(library), b"")?;
    // Establish-time index: source_path only, empty tree. commit() rebuilds it
    // from the validated source. Writing it under .meta/ is what makes the
    // library detectable (hydrate / enumerate key on it).
    fs::create_dir_all(library_meta_dir(library))?;
    let index = LibraryIndex {
        source_path: source_path.to_path_buf(),
        functions: Vec::new(),
        modules: Vec::new(),
    };
    let index_bytes = json::to_vec(&index).map_err(|e| Error::Internal {
        phase: "establish::serialize_index".to_string(),
        reason: e.to_string(),
    })?;
    fs::write(library_meta_path(library), &index_bytes)?;
    run_git(&libraries_dir(), &["add", "--", library])?;
    run_git(
        &libraries_dir(),
        &["commit", "-m", &format!("new library {library}")],
    )?;
    fs::create_dir_all(source_path)?;
    let root_modnu = source_path.join("mod.nu");
    if !root_modnu.exists() {
        fs::write(&root_modnu, b"")?;
    }
    Ok(())
}

/// True if the terminal leaf of a scaffold coordinate already exists in the
/// agent source tree: the module directory for a 2-part namepath, the
/// `<name>.nu` file for a 3-part one.
///
/// Why: the batch `new()` pre-checks every requested leaf before scaffolding
/// any, so a single collision aborts the whole batch (nothing scaffolded).
///
/// Where: called by `tool::scaffold` (the new() handler) once per namepath,
/// before the scaffold pass.
pub(crate) fn scaffold_leaf_exists(
    library: &str,
    module_path: &str,
    name: Option<&str>,
) -> Result<bool, Error> {
    let index = load_index(library)?;
    let mut dir = index.source_path.clone();
    for seg in module_path.split('/').filter(|s| !s.is_empty()) {
        dir = dir.join(seg);
    }
    let leaf = match name {
        Some(fn_name) => dir.join(format!("{fn_name}.nu")),
        None => dir,
    };
    Ok(leaf.exists())
}

/// Scaffold one namepath leaf into an already-established library: mkdir -p
/// the module-path chain (additively wiring each level's mod.nu), then write
/// the single-`main` function skeleton for a 3-part coordinate. Returns the
/// created paths. Errors `LibraryNotRegistered` if the library was never
/// established (via `library(new|install)`); LEAF-GUARD refuses a terminal
/// coordinate that already exists.
///
/// Why: `new()` no longer establishes libraries - it batch-scaffolds into
/// existing ones. This is the per-namepath worker the batch handler calls
/// under each library's write lock (after the pre-existence pre-check).
///
/// Where: called by `tool::scaffold` (the new() handler), once per namepath.
pub(crate) fn scaffold_leaf(
    library: &str,
    module_path: &str,
    name: Option<&str>,
) -> Result<Vec<String>, Error> {
    validate_new_coordinate(library, module_path, name)?;
    if !library_dir(library).exists() {
        return Err(Error::LibraryNotRegistered {
            library: library.to_string(),
        });
    }
    let index = load_index(library)?;
    let sp = index.source_path.clone();
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
                    reason: "module already exists; edit it instead of scaffolding over it"
                        .to_string(),
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
                reason: "function already exists; edit it instead of scaffolding over it"
                    .to_string(),
            });
        }
        fs::write(&fn_file, skeleton_function_source())?;
        additively_wire_modnu(&dir.join("mod.nu"), &format!("export use ./{fn_name}.nu"))?;
        created.push(fn_file.to_string_lossy().into_owned());
    }

    Ok(created)
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
// LibraryMeta load helper
// ============================================================================

/// What: reads + parses `<library>/.meta/library.json`, returning the
/// deserialized `LibraryIndex`. Returns an io::Error wrapping the JSON decode
/// error if the file is malformed.
///
/// Why: the index is the source of truth for `source_path` (commit re-read,
/// delete sanity-check) and the call() surface tree (info / call / inspect).
/// Wrapping JSON-decode errors as io::Error keeps the impl signature uniform
/// with other library impls.
///
/// Where: called by `commit_impl` + `delete_impl` + `new_impl` (source_path)
/// and `enumerate_libraries` + `call` + `inspect_impl` (the tree).
pub(crate) fn load_index(library: &str) -> io::Result<LibraryIndex> {
    let bytes = fs::read(library_meta_path(library))?;
    json::from_slice(&bytes)
        .map_err(|e| io::Error::other(format!("decode index for {library}: {e}")))
}

// ============================================================================
// (define_function / undefine_function retired in 0.0.44 - new/commit/delete)
// ============================================================================

/// Resolve `<library>/<module_path>/<name>.nu` into an absolute path
/// under the MCP libraries dir. Returns None if any name component is
/// invalid (path traversal defense). Does NOT verify the file exists;
/// caller checks.
pub(crate) fn call_file_path(library: &str, module_path: &str, name: &str) -> Option<PathBuf> {
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

/// One callable function in the `info()` hierarchy.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub struct FunctionInfo {
    pub name: String,
    pub summary: String,
    pub args_schema: mcp::JsonObject,
    pub result_schema: mcp::JsonObject,
}

/// One module node in the `info()` hierarchy.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub struct ModuleInfo {
    pub name: String,
    pub summary: String,
    pub submodules: Vec<ModuleInfo>,
    pub functions: Vec<FunctionInfo>,
}

/// One registered library in the `info()` hierarchy.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub struct LibraryInfo {
    pub name: String,
    pub path: String,
    pub summary: String,
    pub modules: Vec<ModuleInfo>,
    pub functions: Vec<FunctionInfo>,
}

/// What: read `.meta/docs/<coord>/<file>` (summary.md / details.md),
/// returning "" when the file is absent. `coord` is "" for the library
/// root, the module path for a module, or `<module_path>/<name>` for a
/// function.
///
/// Why: big meta splits prose out of `library.json` into the docs tree;
/// info() overlays the one-liner summary and inspect() returns summary +
/// details, both via this reader. Absent == undocumented == "".
///
/// Where: called by `index_function_to_info` / `index_module_to_info` /
/// `enumerate_libraries` (summary.md) and `inspect_impl` (both files).
fn read_doc(docs_dir: &std::path::Path, coord: &str, file: &str) -> String {
    let dir = if coord.is_empty() {
        docs_dir.to_path_buf()
    } else {
        docs_dir.join(coord)
    };
    fs::read_to_string(dir.join(file)).unwrap_or_default()
}

/// What: build a `FunctionInfo` (info() node) from an indexed call-target,
/// overlaying its one-liner summary from `.meta/docs/<coord>/summary.md`.
/// `parent` is the accumulated module coordinate ("" at the library root).
///
/// Why: info()'s envelope carries the summary alongside the schemas; the
/// schemas come straight from the index (no re-parse), the summary from
/// the docs tree.
///
/// Where: called by `index_module_to_info` + `enumerate_libraries`.
fn index_function_to_info(
    f: &IndexFunction,
    docs_dir: &std::path::Path,
    parent: &str,
) -> FunctionInfo {
    let coord = if parent.is_empty() {
        f.name.clone()
    } else {
        format!("{parent}/{}", f.name)
    };
    FunctionInfo {
        name: f.name.clone(),
        summary: read_doc(docs_dir, &coord, "summary.md"),
        args_schema: f.args_schema.clone(),
        result_schema: f.result_schema.clone(),
    }
}

/// What: build a `ModuleInfo` (info() node) from an indexed module,
/// recursing into submodules and overlaying each node's summary from the
/// docs tree. `parent` is the accumulated coordinate of this module's
/// PARENT ("" at the library root), so this module's coord is
/// `<parent>/<name>`.
///
/// Why: info() mirrors the index tree; the index is the structural source,
/// the docs tree the prose overlay - no canonical `.nu` walk or parse.
///
/// Where: called by `enumerate_libraries` (top modules) and itself.
fn index_module_to_info(
    m: &IndexModule,
    docs_dir: &std::path::Path,
    parent: &str,
) -> ModuleInfo {
    let coord = if parent.is_empty() {
        m.name.clone()
    } else {
        format!("{parent}/{}", m.name)
    };
    ModuleInfo {
        name: m.name.clone(),
        summary: read_doc(docs_dir, &coord, "summary.md"),
        submodules: m
            .modules
            .iter()
            .map(|s| index_module_to_info(s, docs_dir, &coord))
            .collect(),
        functions: m
            .functions
            .iter()
            .map(|f| index_function_to_info(f, docs_dir, &coord))
            .collect(),
    }
}

/// What: enumerate every canonical library into the info() hierarchy.
/// Walks `libraries_dir()` for subdirs carrying `.meta/library.json` (the
/// same marker `hydrate_from_disk` keys on), takes the per-library READ
/// lock, deserializes the index, and overlays each node's one-liner
/// summary from the docs tree. NO `.nu` walk or parse - the index is the
/// authority (big meta). Libraries sorted by name; one whose index fails
/// to decode is skipped with a host-stderr note rather than failing the
/// whole info() call.
///
/// Why: the hot path reads MCP-generated meta instead of re-deriving the
/// surface from the filesystem on every call - an organizational helper
/// file can no longer drop the whole library (the old enumerate bailed on
/// its schema parse). The read lock means a concurrent commit can't tear
/// an enumeration mid-library.
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
        let index = match load_index(&name) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("nushell_mcp: info enumeration skipped {name}: {e}");
                continue;
            }
        };
        let docs_dir = library_docs_dir(&name);
        out.push(LibraryInfo {
            name: name.clone(),
            path: index.source_path.display().to_string(),
            summary: read_doc(&docs_dir, "", "summary.md"),
            modules: index
                .modules
                .iter()
                .map(|m| index_module_to_info(m, &docs_dir, ""))
                .collect(),
            functions: index
                .functions
                .iter()
                .map(|f| index_function_to_info(f, &docs_dir, ""))
                .collect(),
        });
    }
    out
}

/// Result of `inspect()`: a full single-node descriptor - the coordinate
/// (library, module_path, optional name), the docs (summary + details), and,
/// for a function, the call schemas from the index. `name` + the schemas are
/// None for a module or the library root.
#[derive(Debug, ser::Serialize)]
pub(crate) struct InspectResult {
    pub library: String,
    pub module_path: String,
    pub name: Option<String>,
    pub summary: String,
    pub args_schema: Option<mcp::JsonObject>,
    pub result_schema: Option<mcp::JsonObject>,
    pub details: String,
}

/// What: navigate the index tree to the node at `module_path`, returning its
/// (functions, modules) slices. Empty path -> the library root. None when a
/// path segment names no module in the index.
///
/// Why: call() + inspect() validate a coordinate against the index (is it a
/// registered call-target / module?) instead of touching the filesystem - a
/// helper file (absent from the index) becomes correctly non-addressable.
///
/// Where: called by `inspect_impl` and `server::tool::call::NuSh::call`.
pub(crate) fn index_node<'a>(
    index: &'a LibraryIndex,
    module_path: &str,
) -> Option<(&'a Vec<IndexFunction>, &'a Vec<IndexModule>)> {
    if module_path.is_empty() {
        return Some((&index.functions, &index.modules));
    }
    let mut fns = &index.functions;
    let mut mods = &index.modules;
    for seg in module_path.split('/') {
        let m = mods.iter().find(|m| m.name == seg)?;
        fns = &m.functions;
        mods = &m.modules;
    }
    Some((fns, mods))
}

pub(crate) fn inspect_impl(
    library: &str,
    module_path: &str,
    name: Option<&str>,
) -> Result<InspectResult, Error> {
    if !is_valid_ident(library) {
        return Err(Error::LibraryInvalidName {
            library: library.to_string(),
            reason: "must match [a-zA-Z_][a-zA-Z0-9_-]*".to_string(),
        });
    }
    if !library_dir(library).exists() {
        return Err(Error::LibraryNotRegistered {
            library: library.to_string(),
        });
    }
    if !is_valid_module_path(module_path) {
        return Err(Error::LibraryInvalidModulePath {
            module_path: module_path.to_string(),
            reason: "invalid module path".to_string(),
        });
    }
    let index = load_index(library)?;
    let docs_dir = library_docs_dir(library);
    // Resolve the coordinate against the index; a function also carries its
    // call schemas. Docs read as "" when the node is undocumented.
    match name {
        Some(fn_name) => {
            let (fns, _) = index_node(&index, module_path).ok_or_else(|| {
                Error::LibraryInvalidModulePath {
                    module_path: module_path.to_string(),
                    reason: "module not found".to_string(),
                }
            })?;
            let f = fns.iter().find(|f| f.name == fn_name).ok_or_else(|| {
                Error::FunctionNotDefined {
                    library: library.to_string(),
                    module_path: module_path.to_string(),
                    name: fn_name.to_string(),
                }
            })?;
            let coord = if module_path.is_empty() {
                fn_name.to_string()
            } else {
                format!("{module_path}/{fn_name}")
            };
            Ok(InspectResult {
                library: library.to_string(),
                module_path: module_path.to_string(),
                name: Some(fn_name.to_string()),
                summary: read_doc(&docs_dir, &coord, "summary.md"),
                details: read_doc(&docs_dir, &coord, "details.md"),
                args_schema: Some(f.args_schema.clone()),
                result_schema: Some(f.result_schema.clone()),
            })
        }
        None => {
            if index_node(&index, module_path).is_none() {
                return Err(Error::LibraryInvalidModulePath {
                    module_path: module_path.to_string(),
                    reason: "module not found".to_string(),
                });
            }
            Ok(InspectResult {
                library: library.to_string(),
                module_path: module_path.to_string(),
                name: None,
                summary: read_doc(&docs_dir, module_path, "summary.md"),
                details: read_doc(&docs_dir, module_path, "details.md"),
                args_schema: None,
                result_schema: None,
            })
        }
    }
}

// ============================================================================
// Strict library source validator (run by commit)
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
/// `commit_impl` (folded into `Error::LibraryViolations` when
/// non-empty).
#[derive(Debug, Clone)]
pub(crate) struct ValidationResult {
    pub structural: Vec<Violation>,
    /// leg 4: true when the structural list was capped at
    /// `LINT_VIOLATION_CAP` and the walk early-stopped (there may be
    /// more). `Error::LibraryViolations.structural_more` mirrors it.
    pub structural_more: bool,
    /// leg 4: doc `summary_length` lint violations (capped at
    /// `LINT_VIOLATION_CAP` + a `More` sentinel). Populates the formerly
    /// dormant `lint` field of `Error::LibraryViolations`.
    pub lint: Vec<LintViolation>,
    /// big meta: the library root's call-target functions, built during the
    /// validation walk. Only meaningful when the result is otherwise clean
    /// (commit_impl writes the index only after `is_empty()` passes).
    pub functions: Vec<IndexFunction>,
    /// big meta: the library root's pruned module tree.
    pub modules: Vec<IndexModule>,
    /// big meta: per-node docs collected during the walk (only nodes with a
    /// non-empty summary or details). commit_impl writes them under
    /// `.meta/docs/<coord>/`.
    pub docs: Vec<DocEntry>,
}

/// big meta: one node's prose, keyed by its docs coordinate ("" = library
/// root, `<module_path>` = module, `<module_path>/<name>` = function).
#[derive(Debug, Clone)]
pub(crate) struct DocEntry {
    pub coord: String,
    pub summary: String,
    pub details: String,
}

impl ValidationResult {
    pub(crate) fn is_empty(&self) -> bool {
        self.structural.is_empty() && self.lint.is_empty()
    }
}

/// leg 4: the doc summary (one-liner) char cap. A longer summary is a
/// `summary_length` lint violation on commit.
const SUMMARY_MAX_CHARS: usize = 80;

/// What: extract a node's doc comment as `(summary, details,
/// summary_line)`. `marker = Some(decl)` takes the consecutive `#`
/// comment lines immediately above the line that starts with `decl`
/// (e.g. `export def main`); `marker = None` takes the leading `#`
/// comment block at the top of the file (a mod.nu). First comment line
/// is the summary, the rest (newline-joined) the details; the `#` marker
/// + one optional space are stripped. Empty + line 0 when unattached.
///
/// Why: leg 4 documents nodes from the comment the author already writes
/// -- functions above `export def main` (nushell attaches it natively),
/// modules/libraries as the mod.nu leading comment (nushell does NOT
/// attach directory-module comments, so we read it ourselves). Text-scan
/// keeps both uniform and lets info()/inspect() read the canonical file.
///
/// Where: `check_summary_length` (commit validation) + the info() /
/// inspect() doc surface (leg 4b / 4c).
fn extract_doc(source: &str, marker: Option<&str>) -> (String, String, usize) {
    let lines: Vec<&str> = source.lines().collect();
    let doc_idxs: Vec<usize> = match marker {
        None => {
            let mut idxs = Vec::new();
            for (i, l) in lines.iter().enumerate() {
                if l.trim_start().starts_with('#') {
                    idxs.push(i);
                } else {
                    break;
                }
            }
            idxs
        }
        Some(m) => match lines.iter().position(|l| l.trim_start().starts_with(m)) {
            None => Vec::new(),
            Some(decl) => {
                let mut idxs = Vec::new();
                let mut i = decl;
                while i > 0 {
                    i -= 1;
                    if lines[i].trim_start().starts_with('#') {
                        idxs.push(i);
                    } else {
                        break;
                    }
                }
                idxs.reverse();
                idxs
            }
        },
    };
    if doc_idxs.is_empty() {
        return (String::new(), String::new(), 0);
    }
    let stripped: Vec<String> = doc_idxs
        .iter()
        .map(|&i| {
            let t = lines[i].trim_start();
            let t = t.strip_prefix('#').unwrap_or(t);
            t.strip_prefix(' ').unwrap_or(t).to_string()
        })
        .collect();
    // nushell's rule (build_desc): join the comment lines, split on the first
    // blank line (an empty `#` line). Before -> summary; after -> details; no
    // blank -> all summary. Mirrors `Signature.description`/`extra_description`.
    let joined = stripped.join("\n");
    let (summary, details) = match joined.split_once("\n\n") {
        Some((s, d)) => (s.to_string(), d.to_string()),
        None => (joined, String::new()),
    };
    (summary, details, doc_idxs[0] + 1)
}

/// What: push a `LintViolation::SummaryLength` when a node's doc summary
/// (via `extract_doc`) exceeds `SUMMARY_MAX_CHARS`; `position` is the
/// summary line, `source` the file (`WhereSource::Mod`).
///
/// Why: leg 4's only hard doc rule -- the one-liner is length-bounded so
/// info() stays lean (details unconstrained). Lint (capped + More), not
/// structural, so an over-long summary is agent-fixable.
///
/// Where: `validate_function_file_ast` (the `export def main` comment)
/// and `validate_mod_nu_ast` (the mod.nu leading comment).
fn check_summary_length(
    rel: &str,
    source: &str,
    marker: Option<&str>,
    lint: &mut Vec<LintViolation>,
) {
    let (summary, _details, line) = extract_doc(source, marker);
    if summary.chars().count() > SUMMARY_MAX_CHARS {
        lint.push(LintViolation::summary_length(Where {
            position: [line, 1],
            source: Some(WhereSource::Mod(rel.to_string())),
        }));
    }
}

/// Walk every `.nu` under `root`. Apply the strict per-file shape:
/// - `mod.nu`: only `export use ./<file>.nu` or `export module <name>` lines
///   (plus blank lines and `#` comments). Body-lint NOT applied (no agent
///   code lives in mod.nu).
/// - Function files: a single `export def main [args: A]: nothing -> R` (the
///   sole export); args is a non-empty `record<...>` (or `nothing`) and the
///   output type R likewise. Authored bodies are NOT lint-checked
///   (item 7, the_user 2026-06-12: imports are authored with intent; the
///   AST body-lint covers the on-the-fly run/interact/define path only).
///
/// Each file is parsed through `nu_parser::parse` (in a
/// `module __v_<stem> { ... }` wrapper) so syntax errors land as
/// structural violations with line numbers. Dotfile entries (e.g.
/// `.git`, `.meta/`) are skipped. Returns ALL structural violations -- no
/// bail-on-first; no auto-fix. ALSO assembles the big-meta index + docs into
/// `result` (functions / modules / docs) during the same walk (built once at
/// commit, read thereafter by info / call / inspect).
pub(crate) fn validate_library_source(
    root: &std::path::Path,
    engine: &ParseEngine,
) -> io::Result<ValidationResult> {
    let mut result = ValidationResult {
        structural: Vec::new(),
        structural_more: false,
        lint: Vec::new(),
        functions: Vec::new(),
        modules: Vec::new(),
        docs: Vec::new(),
    };
    let (modules, functions) = validate_walk(root, root, engine, "", &mut result)?;
    result.modules = modules;
    result.functions = functions;
    // leg 4: cap both lists at LINT_VIOLATION_CAP. Structural early-stops
    // the walk (see validate_walk) so a hard-broken tree rejects without
    // parsing every file; truncate + flag `structural_more`. The doc
    // `summary_length` lint accumulates across the walk; truncate + append
    // the `More` sentinel (the body-lint truncation discipline).
    if result.structural.len() > LINT_VIOLATION_CAP {
        result.structural.truncate(LINT_VIOLATION_CAP);
        result.structural_more = true;
    }
    if result.lint.len() > LINT_VIOLATION_CAP {
        result.lint.truncate(LINT_VIOLATION_CAP);
        result.lint.push(LintViolation::More);
    }
    Ok(result)
}

/// Recursive validate + index walk over one directory. Validates every `.nu`
/// (mod.nu + function files + the reserved-terms scan) AND assembles the
/// big-meta tree for this level: the dir's own docs (its mod.nu leading
/// comment, keyed at `module_path`), the call-target `functions` here, and the
/// surviving (call-target-bearing) submodules. Returns the dir's
/// `(modules, functions)`; the top call yields the library-root tree. Entries
/// are sorted for a deterministic index order. The structural cap early-stops
/// the walk (commit rejects on violations, so a partial index is harmless).
fn validate_walk(
    root: &std::path::Path,
    dir: &std::path::Path,
    engine: &ParseEngine,
    module_path: &str,
    result: &mut ValidationResult,
) -> io::Result<(Vec<IndexModule>, Vec<IndexFunction>)> {
    // This dir's own docs: its mod.nu leading comment, keyed at `module_path`
    // ("" == the library root). Pushed only when non-empty.
    let modnu = dir.join("mod.nu");
    if modnu.exists() {
        let src = fs::read_to_string(&modnu).unwrap_or_default();
        let (summary, details, _) = extract_doc(&src, None);
        if !summary.is_empty() || !details.is_empty() {
            result.docs.push(DocEntry {
                coord: module_path.to_string(),
                summary,
                details,
            });
        }
    }

    // Collect + sort entries (dotfiles skipped, incl. the MCP `.meta/`).
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            dirs.push((name, path));
        } else if ft.is_file() && path.extension().map(|e| e == "nu").unwrap_or(false) {
            files.push((name, path));
        }
    }
    dirs.sort();
    files.sort();

    let mut functions: Vec<IndexFunction> = Vec::new();
    let mut modules: Vec<IndexModule> = Vec::new();

    // Function (+ mod.nu) files at this level. mod.nu validates but yields no
    // IndexFunction; a call-target yields one plus its docs at the function
    // coordinate.
    for (name, path) in &files {
        if result.structural.len() > LINT_VIOLATION_CAP {
            return Ok((modules, functions));
        }
        let stem = name.trim_end_matches(".nu");
        let coord = if module_path.is_empty() {
            stem.to_string()
        } else {
            format!("{module_path}/{stem}")
        };
        if let Some((idx_fn, summary, details)) = validate_one_file(root, path, engine, result)? {
            if module_path.is_empty() {
                // No root functions: a callable must live in a module (the
                // library root holds only the cascade + helpers, never a
                // call-target). Reject it and skip indexing.
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
                result.structural.push(Violation {
                    path: rel,
                    line: 0,
                    message:
                        "a call-target cannot live at the library root; move it into a module"
                            .to_string(),
                });
                continue;
            }
            if !summary.is_empty() || !details.is_empty() {
                result.docs.push(DocEntry {
                    coord,
                    summary,
                    details,
                });
            }
            functions.push(idx_fn);
        }
    }

    // Subdirectory modules: only dirs carrying mod.nu are modules; only those
    // with a call-target below them survive the prune.
    for (name, path) in &dirs {
        if result.structural.len() > LINT_VIOLATION_CAP {
            return Ok((modules, functions));
        }
        if !path.join("mod.nu").exists() {
            continue;
        }
        let child_path = if module_path.is_empty() {
            name.clone()
        } else {
            format!("{module_path}/{name}")
        };
        let (sub_mods, sub_fns) = validate_walk(root, path, engine, &child_path, result)?;
        if !sub_fns.is_empty() || !sub_mods.is_empty() {
            modules.push(IndexModule {
                name: name.clone(),
                functions: sub_fns,
                modules: sub_mods,
            });
        }
    }

    Ok((modules, functions))
}

fn validate_one_file(
    root: &std::path::Path,
    path: &std::path::Path,
    engine: &ParseEngine,
    result: &mut ValidationResult,
) -> io::Result<Option<(IndexFunction, String, String)>> {
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
    // mod.nu validates but is no call-target (None); a function file yields the
    // IndexFunction + (summary, details) when it is a clean call-target.
    let extracted = if is_mod {
        validate_mod_nu_ast(
            &rel,
            stem,
            &source,
            parent,
            engine,
            &mut result.structural,
            &mut result.lint,
        );
        None
    } else {
        validate_function_file_ast(
            &rel,
            stem,
            &source,
            parent,
            engine,
            &mut result.structural,
            &mut result.lint,
        )
    };
    // the reserved-terms ban applies to EVERY .nu file -- `main` may appear
    // only as a call-target's exported sentinel.
    scan_reserved_terms(&rel, stem, &source, parent, engine, &mut result.structural);
    Ok(extracted)
}

/// The reserved-terms ban. `main` may appear in a library ONLY as the
/// exported call-target sentinel (`export def main`, the 1-def contract).
/// ANY OTHER occurrence as an identifier is a violation -- a private/nested
/// def, a module/dir/file name, a const / alias / let / mut binding, a
/// parameter, a record/table column key, or a cell-path member. Quoted
/// string *values* and command references are not identifiers and pass.
///
/// `nu_parser::flatten_block` tags each token with a `FlatShape`: `main`
/// flags at `VarDecl` (let/mut/const names) and `String` (def/module/alias
/// names, record/table keys, cell-path members, barewords) -- except a
/// `String` token immediately following an `export def` token, which is the
/// call-target's valid sentinel export. Command references parse as
/// `InternalCall`/`External` (skipped), and a quoted `"main"` keeps its
/// quotes in the token content so it never matches `main`. Parameter names
/// hide inside the `Signature` token, so they are walked separately;
/// module/dir/file names are checked on the path.
fn scan_reserved_terms(
    rel: &str,
    stem: &str,
    source: &str,
    parent: &std::path::Path,
    engine: &ParseEngine,
    violations: &mut Vec<Violation>,
) {
    // 1. Path components: no module / directory / file named main.
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
                    "`{content}` is reserved -- it may appear only as a call-target's exported `main`; rename this identifier",
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

/// `main` is the reserved call-target sentinel name.
fn is_reserved_term(s: &str) -> bool {
    s == "main"
}

/// Extract the top-level parameter names from a flattened `Signature`
/// token (`[call: int, --flag: string, ...rest]`, or a closure `|call|`),
/// skipping names nested inside type annotations (`record<main: int>`)
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
    lint: &mut Vec<LintViolation>,
) {
    // leg 4: the module/library summary (the mod.nu leading comment's
    // first line) must be <= 80 chars. Lint, not structural.
    check_summary_length(rel, source, None, lint);
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
            check_mod_nu_pipeline_element(
                rel,
                &elem.expr,
                &working_set,
                source,
                prefix_len,
                violations,
            );
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
            if matches!(
                name,
                "export use" | "export module" | "export const" | "export def"
            ) {
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

/// AST-based function file validator (the 1-def `main` contract). Wraps
/// `source` in `module __v_<stem> { ... }`, runs nu_parser (with `$env.PWD =
/// parent` so any sibling `use` resolves cleanly), then walks the resulting
/// `Module` to enforce:
///   - parse cleanly (no syntax errors)
///   - the sole export is `main` (no extra exports)
///   - main has a typed `args: record<...>` (or `nothing`) positional
///   - main declares a `: nothing -> R` output whose R is a non-empty
///     `record<...>` (or `nothing` for void)
/// A file WITHOUT `main` is ORGANIZATIONAL (helper `export def` /
/// `export const`, unconstrained): parse-correctness only, no contract.
/// `main` IS the call-target -- the author owns the whole body (no
/// resolve / main-body AST-lock).
fn validate_function_file_ast(
    rel: &str,
    stem: &str,
    source: &str,
    parent: &std::path::Path,
    engine: &ParseEngine,
    violations: &mut Vec<Violation>,
    lint: &mut Vec<LintViolation>,
) -> Option<(IndexFunction, String, String)> {
    // leg 4: the function summary (the doc one-liner above `export def
    // main`) must be <= 80 chars. Lint (capped + More), not structural.
    check_summary_length(rel, source, Some("export def main"), lint);
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
        return None;
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
            return None;
        }
    };
    let module: &nu::Module = working_set.get_module(module_id);

    // 3. Classify by the `main` sentinel. nu's Module tracks `main`
    //    separately from `decls`. A file exporting `main` is a CALL-TARGET
    //    (the 1-def contract: main IS the logic); a file WITHOUT it is
    //    ORGANIZATIONAL (helper `export def` / `export const`,
    //    unconstrained) and is left to parse-correctness only. The
    //    reserved-terms ban (scan_reserved_terms) keeps the sentinel
    //    airtight.
    let main_decl = module.main;
    let Some(main_id) = main_decl else {
        // Organizational file: no `main` sentinel -> no contract to enforce;
        // parse-correctness (checked above) is sufficient here.
        return None;
    };

    // 4. A call-target file MAY also export helpers (and define private defs)
    //    alongside `main` -- only `main` is the indexed / callable target, so
    //    there is no export-set restriction. The reserved-terms ban still
    //    keeps `main` itself sacrosanct.

    // 5. main's signature: a typed `args: record<...>` (real fields) or
    //    `args: nothing` positional, AND a `: nothing -> R` output whose R is
    //    a non-empty `record<...>` or `nothing`. The author owns the body --
    //    no AST-lock, since main IS the logic now.
    check_args_record_positional(rel, &working_set, main_id, "main", source, prefix_len, violations);
    let out_type = check_main_output_type(rel, &working_set, main_id, source, prefix_len, &wrapped, violations);

    // big meta: extract the IndexFunction + the function docs (main's native
    // description / extra_description). args come from main's positional
    // SyntaxShape; the result comes from main's OUTPUT annotation read as
    // SOURCE TEXT (not the parsed Type, which collapses path/directory fields
    // to string) -- so both sides keep full fidelity across all 14 scalars.
    // Any miss returns None (the structural checks above recorded the defect;
    // commit rejects before the index is written).
    let main_sig = working_set.get_decl(main_id).signature();
    let summary = main_sig.description.clone();
    let details = main_sig.extra_description.clone();
    let args_str = main_sig.required_positional.first()?.shape.to_string();
    let result_str = out_type?;
    let args_schema = nu_to_args_schema(&args_str).ok()?;
    let result_schema = nu_to_result_schema(&result_str).ok()?;
    Some((
        IndexFunction {
            name: stem.to_string(),
            args_schema,
            result_schema,
        },
        summary,
        details,
    ))
}

/// Confirm the decl's first required positional is named `args` with a
/// `record<...>` shape (real fields) or `nothing` (void); pushes a violation
/// otherwise. (Returns the positional's VarId; vestigial, now unused.)
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

/// Confirm `main`'s output type (the `-> R` of `: nothing -> R`) and return
/// the result type as a nu type STRING for schema extraction. The contract: a
/// `nothing -> R` input/output pair (args arrive via the positional, never
/// the pipe) whose R is `nothing` (void) or a NON-EMPTY `record<...>`. An
/// empty `record<>` is the unfleshed-skeleton marker on the result side; a
/// missing pair or a non-record/non-nothing output is a violation.
///
/// Structure is validated against the parsed output `Type`, but for a record
/// the returned string is read from the OUTPUT ANNOTATION SOURCE TEXT (via
/// `extract_main_output_text`), because the parsed `Type` collapses
/// `path`/`directory` fields to `string` -- the source keeps every scalar
/// (`path`, `directory`, `cell-path`, `glob`, ...) verbatim, matching the
/// args-side SyntaxShape fidelity. Void returns `"nothing"`. Pushes a
/// violation + returns None on any miss.
fn check_main_output_type(
    rel: &str,
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    source: &str,
    prefix_len: usize,
    wrapped: &str,
    violations: &mut Vec<Violation>,
) -> Option<String> {
    let sig = working_set.get_decl(decl_id).signature();
    let line = decl_line(working_set, decl_id, source, prefix_len);
    // The contract's pipeline signature is `nothing -> R`: find the pair
    // whose input is `nothing`. Its output carries the result schema.
    let output = sig
        .input_output_types
        .iter()
        .find(|(input, _)| matches!(input, nu::Type::Nothing))
        .map(|(_, out)| out.clone());
    let Some(output) = output else {
        violations.push(Violation {
            path: rel.to_string(),
            line,
            message: "main must declare a `: nothing -> <record<...>|nothing>` output type"
                .to_string(),
        });
        return None;
    };
    match &output {
        nu::Type::Nothing => Some("nothing".to_string()),
        nu::Type::Record(fields) if !fields.is_empty() => {
            // Structurally valid; read the precise type string from the source
            // annotation so path/directory fields survive (the parsed Type
            // collapsed them). Fall back to the lossy Type render only if the
            // source extraction unexpectedly fails.
            Some(
                extract_main_output_text(working_set, decl_id, wrapped)
                    .unwrap_or_else(|| output.to_string()),
            )
        }
        nu::Type::Record(_) => {
            violations.push(Violation {
                path: rel.to_string(),
                line,
                message: "main's output `record<>` is the unfleshed skeleton; give it real fields (or `nothing` for void)".to_string(),
            });
            None
        }
        other => {
            violations.push(Violation {
                path: rel.to_string(),
                line,
                message: format!(
                    "main's output type must be a non-empty `record<...>` or `nothing`; got `{other}`",
                ),
            });
            None
        }
    }
}

/// Read `main`'s output type annotation (the `R` of `: nothing -> R`) verbatim
/// from the wrapped source, so path/directory fields survive (the parsed
/// `Type` collapses them to string). Slices the wrapped source up to main's
/// body block and takes the text after the LAST `->` before it -- main's own
/// output arrow (the caller gates this on the parsed Type confirming main HAS
/// a `nothing`-input pair, so a helper's earlier arrow can't be mistaken). The
/// grammar has no other `->`, and a type carries no `{`, so the slice is
/// exactly `R`. None if the body span or arrow can't be located.
fn extract_main_output_text(
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    wrapped: &str,
) -> Option<String> {
    let block_id = working_set.get_decl(decl_id).block_id()?;
    let body_start = working_set.get_block(block_id).span?.start;
    let before = wrapped.get(..body_start)?;
    let arrow = before.rfind("->")?;
    // R ends at the body's opening `{`; a type carries no `{`, so split there
    // (robust whether the body span starts at or just past the `{`).
    let after = &before[arrow + 2..];
    let r = after.split('{').next().unwrap_or(after).trim();
    if r.is_empty() {
        None
    } else {
        Some(r.to_string())
    }
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
    let resolve_line = exports
        .iter()
        .find(|(_, n)| n == "resolve")
        .map(|(l, _)| *l);

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
            message:
                "function file must contain `export def resolve [args: record<...>] { $args }`"
                    .to_string(),
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
                    message: "resolve's body could not be located (parens/brackets imbalanced?)"
                        .to_string(),
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
// leg 3: commit() - the validate-and-promote upsert
// ============================================================================

/// Result of commit(): paths grouped by change kind (all lists empty =
/// idempotent no-op). Grouped lists (vs a flat `[{path, kind}]`) keep the
/// envelope terse - no repeated `path` / `kind` keys per entry.
#[derive(Debug, ser::Serialize)]
pub(crate) struct CommitResult {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}

impl CommitResult {
    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.removed.is_empty()
    }
}

/// Like `run_git` but returns the command's stdout (for `status
/// --porcelain` and other read-back queries).
fn run_git_output(dir: &std::path::Path, args: &[&str]) -> io::Result<String> {
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
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Parse staged changes out of `git status --porcelain`. The first
/// column is the staged status after `git add` (A/M/D/R/C); renames
/// render as `old -> new` (keep the new path).
fn parse_git_changes(porcelain: &str) -> CommitResult {
    let mut result = CommitResult {
        added: Vec::new(),
        modified: Vec::new(),
        removed: Vec::new(),
    };
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let raw = line[3..].trim();
        let path = raw.rsplit(" -> ").next().unwrap_or(raw).trim().to_string();
        match line.as_bytes()[0] as char {
            'A' | 'C' => result.added.push(path),
            'M' | 'R' => result.modified.push(path),
            'D' => result.removed.push(path),
            _ => {}
        }
    }
    result
}

/// What: write a library's `.meta/` from a clean validation result -
/// `.meta/library.json` (source_path + the index tree built during the walk)
/// + `.meta/docs/<coord>/{summary,details}.md` (only the non-empty parts).
///
/// Why: big meta makes `.meta/` the authority for the call() surface; commit
/// is the only place it is (re)built, right after the canonical subtree is
/// rebuilt from the authored source.
///
/// Where: called by `commit_impl` after `copy_dir_recursive`, before the git
/// add / commit.
fn write_meta(
    name: &str,
    source_path: &std::path::Path,
    result: &ValidationResult,
) -> Result<(), Error> {
    fs::create_dir_all(library_meta_dir(name))?;
    let index = LibraryIndex {
        source_path: source_path.to_path_buf(),
        functions: result.functions.clone(),
        modules: result.modules.clone(),
    };
    let index_bytes = json::to_vec(&index).map_err(|e| Error::Internal {
        phase: "commit::serialize_index".to_string(),
        reason: e.to_string(),
    })?;
    fs::write(library_meta_path(name), &index_bytes)?;
    let docs_dir = library_docs_dir(name);
    for doc in &result.docs {
        let dir = if doc.coord.is_empty() {
            docs_dir.clone()
        } else {
            docs_dir.join(&doc.coord)
        };
        fs::create_dir_all(&dir)?;
        if !doc.summary.is_empty() {
            fs::write(dir.join("summary.md"), doc.summary.as_bytes())?;
        }
        if !doc.details.is_empty() {
            fs::write(dir.join("details.md"), doc.details.as_bytes())?;
        }
    }
    Ok(())
}

/// Implementation for `commit(library)` - the validate-and-promote
/// UPSERT (leg 3): re-reads the source tree from the meta's source_path,
/// runs the strict structural + single-`main` contract validator,
/// and rebuilds the canonical signed subtree from it. NO kind gate (one
/// authored kind). IDEMPOTENT on a no-change resync (no empty commit);
/// RETURNS the changed paths.
pub(crate) fn commit_impl(name: &str, engine: &ParseEngine) -> Result<CommitResult, Error> {
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
    let index = load_index(name)?;
    let source_path = index.source_path.clone();
    if !source_path.exists() || !source_path.is_dir() {
        return Err(Error::LibrarySourceMissing {
            path: source_path.display().to_string(),
        });
    }
    let result = validate_library_source(&source_path, engine)?;
    if !result.is_empty() {
        return Err(Error::LibraryViolations {
            structural: result.structural,
            structural_more: result.structural_more,
            lint: result.lint,
        });
    }
    // Rebuild the canonical subtree from the validated source. copy_dir_
    // recursive skips dotfiles, so the prior .meta/ is gone with the wipe and
    // the authored source never carries one; write_meta then lays the fresh
    // big-meta index + docs back down.
    fs::remove_dir_all(&lib_root)?;
    copy_dir_recursive(&source_path, &lib_root)?;
    write_meta(name, &source_path, &result)?;
    // Stage, then diff. Idempotent: nothing staged -> no commit (item 10).
    run_git(&libraries_dir(), &["add", "--", name])?;
    let porcelain = run_git_output(&libraries_dir(), &["status", "--porcelain", "--", name])?;
    let changed = parse_git_changes(&porcelain);
    if changed.is_empty() {
        return Ok(changed);
    }
    run_git(
        &libraries_dir(),
        &["commit", "-m", &format!("commit library {name}")],
    )?;
    Ok(changed)
}

// ============================================================================
// library() admin: install / uninstall / check (+ new via establish_library)
// ============================================================================

/// The `library()` "are you sure" cross-check: the passed source_dir must
/// equal the registered source_path by PLAIN STRING EQUALITY (never
/// canonicalized - a realpath compare could follow a symlink). Used by the
/// actions that operate on an already-registered library (check, uninstall).
pub(crate) fn check_source_dir(library: &str, source_dir: &str) -> Result<(), Error> {
    let index = load_index(library)?;
    let registered = index.source_path.to_string_lossy().into_owned();
    if registered != source_dir {
        return Err(Error::LibrarySourcePathMismatch {
            library: library.to_string(),
            passed: source_dir.to_string(),
            registered,
        });
    }
    Ok(())
}

/// Implementation for `library(install)`: bring a shipped/complete library
/// source into the mcp - establish it at `source_dir`, then run the first
/// `commit_impl`. ATOMIC: if the commit (validation) fails, the freshly-built
/// canonical subtree is wiped so nothing stays registered. Returns the first
/// commit's changed paths.
pub(crate) fn install_impl(
    library: &str,
    source_dir: &std::path::Path,
    engine: &ParseEngine,
) -> Result<CommitResult, Error> {
    establish_library(library, source_dir)?;
    match commit_impl(library, engine) {
        Ok(result) => Ok(result),
        Err(e) => {
            // Fresh install whose first commit failed: wipe what we built so
            // nothing remains registered, and record the removal in git.
            let lib_root = library_dir(library);
            if lib_root.exists() {
                let _ = fs::remove_dir_all(&lib_root);
                let _ = run_git(&libraries_dir(), &["add", "--", library]);
                let _ = run_git(
                    &libraries_dir(),
                    &["commit", "-m", &format!("rollback failed install {library}")],
                );
            }
            Err(e)
        }
    }
}

/// Implementation for `library(uninstall)`: remove the library from the mcp
/// (drop the canonical subtree + a signed commit). The agent's source_dir is
/// NEVER touched. Idempotent: an absent canonical subtree is success. The
/// source_dir sanity check + the lock-registry removal are the caller's
/// (tool::library) responsibility.
pub(crate) fn uninstall_impl(library: &str) -> Result<(), Error> {
    let lib_root = library_dir(library);
    if !lib_root.exists() {
        return Ok(());
    }
    fs::remove_dir_all(&lib_root)?;
    run_git(&libraries_dir(), &["add", "--", library])?;
    run_git(
        &libraries_dir(),
        &["commit", "-m", &format!("uninstall library {library}")],
    )?;
    Ok(())
}

/// Implementation for `library(check)`: validate the registered library's
/// in-source tree (its `cargo test`) WITHOUT committing or mutating anything.
/// Returns the `ValidationResult` (structural + lint findings) for the caller
/// to bucket into errors / warnings. Requires the library registered.
pub(crate) fn check_library(
    library: &str,
    engine: &ParseEngine,
) -> Result<ValidationResult, Error> {
    let lib_root = library_dir(library);
    if !lib_root.exists() {
        return Err(Error::LibraryNotRegistered {
            library: library.to_string(),
        });
    }
    let index = load_index(library)?;
    let source_path = index.source_path.clone();
    if !source_path.exists() || !source_path.is_dir() {
        return Err(Error::LibrarySourceMissing {
            path: source_path.display().to_string(),
        });
    }
    Ok(validate_library_source(&source_path, engine)?)
}

/// What: copies a directory tree recursively. Skips dotfile entries
/// at every level (so a client `.git` doesn't bleed into the MCP
/// repo). Creates `dst` and all parents if needed.
///
/// Why: commit copies the authored source tree into the canonical
/// repo; dotfile skipping is mandatory to keep client-side VCS
/// metadata out of our git history. Recursive walk handles arbitrary
/// nesting without bookkeeping.
///
/// Where: called by `commit_impl` after validation succeeds and the
/// pre-existing subtree has been wiped.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> io::Result<()> {
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
