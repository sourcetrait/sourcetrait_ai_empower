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

fn function_id(library: &str, module_path: &str, name: &str) -> String {
    if module_path.is_empty() {
        format!("{library}/{name}")
    } else {
        format!("{library}/{module_path}/{name}")
    }
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

/// Walk every `.nu` under `root`. Apply the strict per-file shape:
/// - `mod.nu`: only `export use ./<file>.nu` or `export module <name>` lines
///   (plus blank lines and `#` comments).
/// - Function files: exactly two exports named `main` and `resolve`;
///   both have `args: record<...>` typed positionals; resolve's body is
///   exactly the expression `$args`.
/// Dotfile entries (e.g. `.git`, `.nu_sh_mcp_meta.json`) are skipped.
/// Returns ALL violations -- no bail-on-first; no auto-fix.
pub(crate) fn validate_library_source(root: &std::path::Path) -> io::Result<Vec<Violation>> {
    let mut violations = Vec::new();
    validate_walk(root, root, &mut violations)?;
    Ok(violations)
}

fn validate_walk(
    root: &std::path::Path,
    dir: &std::path::Path,
    violations: &mut Vec<Violation>,
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
            validate_walk(root, &path, violations)?;
        } else if ft.is_file()
            && path.extension().map(|e| e == "nu").unwrap_or(false)
        {
            validate_one_file(root, &path, violations)?;
        }
    }
    Ok(())
}

fn validate_one_file(
    root: &std::path::Path,
    path: &std::path::Path,
    violations: &mut Vec<Violation>,
) -> io::Result<()> {
    let source = fs::read_to_string(path)?;
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    if path.file_name().map(|n| n == "mod.nu").unwrap_or(false) {
        validate_mod_nu(&rel, &source, violations);
    } else {
        validate_function_file(&rel, &source, violations);
    }
    Ok(())
}

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

pub(crate) fn import_library_impl(
    name: &str,
    source_path: &std::path::Path,
) -> Result<(), ImportError> {
    if !is_valid_ident(name) {
        return Err(ImportError::InvalidLibraryName(name.to_string()));
    }
    if !source_path.exists() || !source_path.is_dir() {
        return Err(ImportError::SourceMissing(source_path.to_path_buf()));
    }
    let violations =
        validate_library_source(source_path).map_err(ImportError::Io)?;
    if !violations.is_empty() {
        return Err(ImportError::Violations(violations));
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

pub(crate) fn reimport_library_impl(name: &str) -> Result<(), ImportError> {
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
    let violations =
        validate_library_source(&source_path).map_err(ImportError::Io)?;
    if !violations.is_empty() {
        return Err(ImportError::Violations(violations));
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

#[derive(Debug)]
pub(crate) enum ImportError {
    Io(io::Error),
    InvalidLibraryName(String),
    SourceMissing(PathBuf),
    NotRegistered(String),
    WrongKind,
    Violations(Vec<Violation>),
}

/// Copy a directory tree recursively. Skips dotfile entries at every
/// level (so a client `.git` doesn't bleed into the MCP repo).
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
