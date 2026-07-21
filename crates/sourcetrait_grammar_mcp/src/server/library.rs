use crate::*;


#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct LibraryIndex {
    pub source_path: PathBuf,
    #[serde(default)]
    pub functions: Vec<IndexFunction>,
    #[serde(default)]
    pub modules: Vec<IndexModule>,
}

#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct IndexFunction {
    pub name: String,
    pub args_schema: mcp::JsonObject,
    pub result_schema: mcp::JsonObject,
}

#[derive(Debug, Clone, ser::Serialize, ser::Deserialize)]
pub(crate) struct IndexModule {
    pub name: String,
    #[serde(default)]
    pub functions: Vec<IndexFunction>,
    #[serde(default)]
    pub modules: Vec<IndexModule>,
}

pub(crate) const META_FILE: &str = ".meta/library.nuon";

/// The pre-NUON index filename. Nothing writes it any more; it exists so
/// `migrate_meta_to_nuon` can find a store written by an older host and retire it.
const LEGACY_META_FILE: &str = ".meta/library.json";


pub(crate) fn keypair_dir() -> PathBuf {
    data_base_dir().join("keypair")
}

pub(crate) fn private_key_path() -> PathBuf {
    keypair_dir().join("id_grammar")
}

pub(crate) fn public_key_path() -> PathBuf {
    keypair_dir().join("id_grammar.pub")
}

pub(crate) fn allowed_signers_path() -> PathBuf {
    keypair_dir().join("allowed_signers")
}

pub(crate) fn libraries_dir() -> PathBuf {
    data_base_dir().join("libraries")
}

pub(crate) const RIG_TYPE_DIR: &str = "rig";

pub(crate) fn library_dir(library: &str) -> PathBuf {
    libraries_dir().join(RIG_TYPE_DIR).join(library)
}

pub(crate) fn library_store_rel(library: &str) -> String {
    format!("{RIG_TYPE_DIR}/{library}")
}

pub(crate) fn is_valid_library(library: &str) -> bool {
    match library.split_once('/') {
        Some((author, name)) => {
            !name.contains('/')
                && is_valid_ident(author)
                && !is_reserved_term(author)
                && is_valid_ident(name)
                && !is_reserved_term(name)
        }
        None => false,
    }
}

pub(crate) fn library_meta_path(library: &str) -> PathBuf {
    library_dir(library).join(META_FILE)
}

pub(crate) fn library_meta_dir(library: &str) -> PathBuf {
    library_dir(library).join(".meta")
}

pub(crate) fn library_docs_dir(library: &str) -> PathBuf {
    library_meta_dir(library).join("docs")
}


pub(crate) struct LibraryLocks {
    map: tk::AsyncMutex<HashMap<String, Arc<tk::AsyncRwLock<()>>>>,
}

impl LibraryLocks {
    pub(crate) fn new() -> Self {
        Self {
            map: tk::AsyncMutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn hydrate_from_disk(&self) -> io::Result<()> {
        let dir = libraries_dir().join(RIG_TYPE_DIR);
        if !dir.exists() {
            return Ok(());
        }
        let mut map = self.map.lock().await;
        for author_entry in fs::read_dir(&dir)? {
            let author_entry = author_entry?;
            if !author_entry.file_type()?.is_dir() {
                continue;
            }
            let author = match author_entry.file_name().into_string() {
                Ok(s) => s,
                Err(_) => continue,
            };
            for lib_entry in fs::read_dir(author_entry.path())? {
                let lib_entry = lib_entry?;
                if !lib_entry.file_type()?.is_dir() {
                    continue;
                }
                let name = match lib_entry.file_name().into_string() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if lib_entry.path().join(META_FILE).exists() {
                    map.entry(format!("{author}/{name}"))
                        .or_insert_with(|| Arc::new(tk::AsyncRwLock::new(())));
                }
            }
        }
        Ok(())
    }

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

    pub(crate) async fn lookup(&self, name: &str) -> Option<Arc<tk::AsyncRwLock<()>>> {
        let map = self.map.lock().await;
        map.get(name).cloned()
    }

    pub(crate) async fn unregister(&self, name: &str) {
        let mut map = self.map.lock().await;
        map.remove(name);
    }
}

pub(crate) struct AlreadyRegistered;


const KEY_COMMENT: &str = "grammar@localhost";

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
    let pub_key = fs::read_to_string(public_key_path())?;
    let allowed = format!("{} {}", KEY_COMMENT, pub_key.trim());
    fs::write(allowed_signers_path(), allowed.as_bytes())?;
    Ok(())
}

pub(crate) fn ensure_libraries_repo() -> io::Result<()> {
    let dir = libraries_dir();
    fs::create_dir_all(&dir)?;
    let git_dir = dir.join(".git");
    if !git_dir.exists() {
        run_git(&dir, &["init", "-b", "main"])?;
        configure_repo(&dir)?;
        run_git(
            &dir,
            &["commit", "--allow-empty", "-m", "init libraries repo"],
        )?;
    } else {
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
        ("user.name", "grammar"),
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


fn skeleton_function_source() -> String {
    "# one-line summary (<= 80 chars); becomes this function's doc\nexport def main [args: record<>]: nothing -> record<> {\n    # logic here; replace each record<> with real fields (or `nothing` for void)\n    {}\n}\n".to_string()
}

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

fn validate_new_coordinate(
    library: &str,
    module_path: &str,
    name: Option<&str>,
) -> Result<(), Error> {
    let (author, lib_name) = match library.split_once('/') {
        Some((a, n)) if !n.contains('/') => (a, n),
        _ => {
            return Err(Error::LibraryInvalidName {
                library: library.to_string(),
                reason: "library must be the compound `<author>/<name>`".to_string(),
            });
        }
    };
    for seg in [author, lib_name] {
        if !is_valid_ident(seg) || is_reserved_term(seg) {
            return Err(Error::LibraryInvalidName {
                library: seg.to_string(),
                reason: "author/name must match [a-zA-Z_][a-zA-Z0-9_-]* and not be the reserved `main`"
                    .to_string(),
            });
        }
    }
    if NAME_DENYLIST.contains(&lib_name) {
        return Err(Error::LibraryNameDenied {
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

pub(crate) fn establish_library(library: &str, source_path: &std::path::Path) -> Result<(), Error> {
    validate_new_coordinate(library, "", None)?;
    let canonical = library_dir(library);
    if canonical.exists() {
        return Err(Error::LibraryAlreadyRegistered {
            library: library.to_string(),
        });
    }
    let meta_dir = canonical.join(".meta");
    fs::create_dir_all(&canonical)?;
    fs::write(canonical.join("mod.nu"), b"")?;
    fs::create_dir_all(&meta_dir)?;
    let index = LibraryIndex {
        source_path: source_path.to_path_buf(),
        functions: Vec::new(),
        modules: Vec::new(),
    };
    let index_nuon = index_to_nuon(&index).map_err(|reason| Error::Internal {
        phase: "establish::serialize_index".to_string(),
        reason,
    })?;
    fs::write(canonical.join(META_FILE), index_nuon.as_bytes())?;
    let rel = library_store_rel(library);
    run_git(&libraries_dir(), &["add", "--", &rel])?;
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
        Some(fn_name) => dir.join(fn_name),
        None => dir,
    };
    Ok(leaf.exists())
}

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
        let fn_dir = dir.join(fn_name);
        if fn_dir.exists() {
            return Err(Error::LibraryInvalidName {
                library: fn_name.to_string(),
                reason: "function already exists; edit it instead of scaffolding over it"
                    .to_string(),
            });
        }
        fs::create_dir_all(&fn_dir)?;
        let fn_modnu = fn_dir.join("mod.nu");
        fs::write(&fn_modnu, skeleton_function_source())?;
        let parent_modnu = dir.join("mod.nu");
        additively_wire_modnu(&parent_modnu, &format!("export module {fn_name}"))?;
        additively_wire_modnu(&parent_modnu, &format!("export use {fn_name}"))?;
        created.push(fn_modnu.to_string_lossy().into_owned());
    }

    Ok(created)
}


pub(crate) fn is_valid_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
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

pub(crate) fn is_valid_module_path(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if s.starts_with('/') || s.ends_with('/') || s.contains("//") {
        return false;
    }
    s.split('/').all(is_valid_ident)
}


/// Render the index as NUON - the house format for anything we persist.
///
/// Routed through serde's Value rather than hand-mapping every field: the index types
/// already derive Serialize/Deserialize and the schemas they carry are `JsonObject`s,
/// so ONE bridge at the Value layer covers the whole tree and cannot drift from the
/// structs as they change.
pub(crate) fn index_to_nuon(index: &LibraryIndex) -> Result<String, String> {
    let json = json::to_value(index).map_err(|e| e.to_string())?;
    let value = json_value_to_nu_value(&json);
    nu::to_nuon(&nu::EngineState::new(), &value, nu::ToNuonConfig::default())
        .map_err(|e| e.to_string())
}

/// Parse an index back out of NUON, the mirror of `index_to_nuon`.
pub(crate) fn index_from_nuon(text: &str) -> Result<LibraryIndex, String> {
    let value = nu::from_nuon(text, None).map_err(|e| e.to_string())?;
    // `nu_json` is the FRIENDLY converter (what `to json` emits); serde's own
    // Serialize on a nu Value would hand back the internal tagged form with spans.
    let json_compat = nu::JsonValue::from_value(value).map_err(|e| e.to_string())?;
    let json = json::to_value(&json_compat).map_err(|e| e.to_string())?;
    json::from_value(json).map_err(|e| e.to_string())
}

pub(crate) fn load_index(library: &str) -> io::Result<LibraryIndex> {
    let text = fs::read_to_string(library_meta_path(library))?;
    index_from_nuon(&text)
        .map_err(|e| io::Error::other(format!("decode index for {library}: {e}")))
}



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

fn read_doc(docs_dir: &std::path::Path, coord: &str, file: &str) -> String {
    let dir = if coord.is_empty() {
        docs_dir.to_path_buf()
    } else {
        docs_dir.join(coord)
    };
    fs::read_to_string(dir.join(file)).unwrap_or_default()
}

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

pub(crate) async fn enumerate_libraries(locks: &LibraryLocks) -> Vec<LibraryInfo> {
    let dir = libraries_dir().join(RIG_TYPE_DIR);
    let mut names: Vec<String> = Vec::new();
    if let Ok(read) = fs::read_dir(&dir) {
        for author_entry in read.flatten() {
            if !author_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let Ok(author) = author_entry.file_name().into_string() else {
                continue;
            };
            let Ok(libs) = fs::read_dir(author_entry.path()) else {
                continue;
            };
            for lib_entry in libs.flatten() {
                if !lib_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                if !lib_entry.path().join(META_FILE).exists() {
                    continue;
                }
                if let Ok(name) = lib_entry.file_name().into_string() {
                    names.push(format!("{author}/{name}"));
                }
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
                eprintln!("grammar: info enumeration skipped {name}: {e}");
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
    if !is_valid_library(library) {
        return Err(Error::LibraryInvalidName {
            library: library.to_string(),
            reason: "library must be the compound `<author>/<name>`".to_string(),
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


fn error_count(diagnostics: &[Diagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count()
}

fn cap_diagnostics(diagnostics: Vec<Diagnostic>, cap: usize) -> Vec<Diagnostic> {
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut out = Vec::new();
    for d in diagnostics {
        match d.severity {
            Severity::Error => {
                if errors < cap {
                    out.push(d);
                    errors += 1;
                }
            }
            Severity::Warning => {
                if warnings < cap {
                    out.push(d);
                    warnings += 1;
                }
            }
        }
    }
    out
}

fn prefix_diagnostic_paths(result: &mut ValidationResult, library: &str) {
    for d in &mut result.diagnostics {
        if let Some(src) = &mut d.source {
            if let Some(path) = &src.path {
                src.path = Some(format!("{library}/{path}"));
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ValidationResult {
    pub diagnostics: Vec<Diagnostic>,
    pub functions: Vec<IndexFunction>,
    pub modules: Vec<IndexModule>,
    pub docs: Vec<DocEntry>,
}

#[derive(Debug, Clone)]
pub(crate) struct DocEntry {
    pub coord: String,
    pub summary: String,
    pub details: String,
}

impl ValidationResult {
    pub(crate) fn is_empty(&self) -> bool {
        error_count(&self.diagnostics) == 0
    }
}

const SUMMARY_MAX_CHARS: usize = 80;

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
    let joined = stripped.join("\n");
    let (summary, details) = match joined.split_once("\n\n") {
        Some((s, d)) => (s.to_string(), d.to_string()),
        None => (joined, String::new()),
    };
    (summary, details, doc_idxs[0] + 1)
}

fn check_summary_length(
    rel: &str,
    source: &str,
    marker: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (summary, _details, line) = extract_doc(source, marker);
    if summary.chars().count() > SUMMARY_MAX_CHARS {
        diagnostics.push(Diagnostic::warning(
            "lint::summary_length",
            Some(Source {
                path: Some(rel.to_string()),
                position: [line, 1],
            }),
            "doc summary line exceeds 80 characters",
        ));
    }
}


const NAME_DENYLIST: &[&str] = &["docs", "tools", "bin", "target"];

const ASSET_EXT_DENYLIST: &[&str] = &[
    "nu", "sh", "bash", "zsh", "fish", "ksh", "py", "rb", "pl", "js", "ts", "lua", "ps1", "bat",
    "cmd", "com", "exe",
];

#[derive(Copy, Clone)]
enum Zone {
    Source,
    Doc,
    Asset,
}

impl Zone {
    fn ext_kind(self) -> &'static str {
        match self {
            Zone::Source => "library::source_extension_denied",
            Zone::Doc => "library::doc_extension_denied",
            Zone::Asset => "library::asset_extension_denied",
        }
    }

    fn exec_kind(self) -> &'static str {
        match self {
            Zone::Source => "library::source_executable_denied",
            Zone::Doc => "library::doc_executable_denied",
            Zone::Asset => "library::asset_executable_denied",
        }
    }
}

fn is_executable(path: &std::path::Path) -> bool {
    fs::metadata(path)
        .map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0)
        .unwrap_or(false)
}

fn ext_lower(path: &std::path::Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
}

fn is_sanctioned_root_file(name: &str) -> bool {
    matches!(
        name,
        "library.rig.toml" | "README.md" | "LEGAL.md" | "LICENSE.txt"
    ) || (name.starts_with("LICENSE-") && name.ends_with(".txt"))
}

fn push_ext_denied(
    root: &std::path::Path,
    path: &std::path::Path,
    zone: Zone,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    diagnostics.push(Diagnostic::error(
        zone.ext_kind(),
        Some(Source {
            path: Some(rel),
            position: [0, 0],
        }),
        message.to_string(),
    ));
}

fn check_not_executable(
    root: &std::path::Path,
    path: &std::path::Path,
    zone: Zone,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_executable(path) {
        return;
    }
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    diagnostics.push(Diagnostic::error(
        zone.exec_kind(),
        Some(Source {
            path: Some(rel),
            position: [0, 0],
        }),
        "library files must not be executable (+x); clear the executable bit",
    ));
}

fn validate_assets_tree(
    root: &std::path::Path,
    dir: &std::path::Path,
    result: &mut ValidationResult,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        if error_count(&result.diagnostics) > LINT_VIOLATION_CAP {
            return Ok(());
        }
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            validate_assets_tree(root, &path, result)?;
        } else if ft.is_file() {
            check_not_executable(root, &path, Zone::Asset, &mut result.diagnostics);
            if let Some(ext) = ext_lower(&path)
                && ASSET_EXT_DENYLIST.contains(&ext.as_str())
            {
                push_ext_denied(
                    root,
                    &path,
                    Zone::Asset,
                    ".assets/ must not contain executable/script file types (.sh, .py, .nu, ...)",
                    &mut result.diagnostics,
                );
            }
        }
    }
    Ok(())
}

fn validate_docs_tree(
    root: &std::path::Path,
    dir: &std::path::Path,
    result: &mut ValidationResult,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        if error_count(&result.diagnostics) > LINT_VIOLATION_CAP {
            return Ok(());
        }
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            validate_docs_tree(root, &path, result)?;
        } else if ft.is_file() {
            check_not_executable(root, &path, Zone::Doc, &mut result.diagnostics);
            let allowed = name == ".gitignore"
                || matches!(ext_lower(&path).as_deref(), Some("md") | Some("txt"));
            if !allowed {
                push_ext_denied(
                    root,
                    &path,
                    Zone::Doc,
                    ".docs/ accepts only .md and .txt files (and .gitignore)",
                    &mut result.diagnostics,
                );
            }
        }
    }
    Ok(())
}

struct SelfView {
    base: PathBuf,
}

static SELF_VIEW_SEQ: AtomicU64 = AtomicU64::new(0);

impl SelfView {
    fn new(library: &str, source: &std::path::Path) -> Option<Self> {
        if !is_valid_library(library) {
            return None;
        }
        let base = std::env::temp_dir().join(format!(
            "grammar_selfview_{}_{}",
            process::id(),
            SELF_VIEW_SEQ.fetch_add(1, Ordering::Relaxed),
        ));
        let link = base.join(RIG_TYPE_DIR).join(library);
        fs::create_dir_all(link.parent()?).ok()?;
        std::os::unix::fs::symlink(source, &link).ok()?;
        Some(Self { base })
    }
}

impl Drop for SelfView {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

pub(crate) fn validate_library_source(
    library: &str,
    root: &std::path::Path,
    engine: &ParseEngine,
) -> io::Result<ValidationResult> {
    let mut result = ValidationResult {
        diagnostics: Vec::new(),
        functions: Vec::new(),
        modules: Vec::new(),
        docs: Vec::new(),
    };
    let self_view = SelfView::new(library, root);
    let scoped;
    let walk_engine = match &self_view {
        Some(view) => {
            scoped = engine.with_extra_lib_dir(view.base.clone());
            &scoped
        }
        None => engine,
    };
    let (modules, functions) = validate_walk(root, root, walk_engine, "", &mut result)?;
    result.modules = modules;
    result.functions = functions;
    result.diagnostics = cap_diagnostics(result.diagnostics, LINT_VIOLATION_CAP);
    Ok(result)
}

fn validate_walk(
    root: &std::path::Path,
    dir: &std::path::Path,
    engine: &ParseEngine,
    module_path: &str,
    result: &mut ValidationResult,
) -> io::Result<(Vec<IndexModule>, Vec<IndexFunction>)> {
    let modnu = dir.join("mod.nu");
    let modnu_src = if modnu.exists() {
        fs::read_to_string(&modnu).unwrap_or_default()
    } else {
        String::new()
    };
    if modnu.exists() {
        let (summary, details, _) = extract_doc(&modnu_src, None);
        if !summary.is_empty() || !details.is_empty() {
            result.docs.push(DocEntry {
                coord: module_path.to_string(),
                summary,
                details,
            });
        }
        let rel = modnu
            .strip_prefix(root)
            .unwrap_or(&modnu)
            .to_string_lossy()
            .into_owned();
        let fname = modnu.to_string_lossy().into_owned();
        if module_path.is_empty()
            && parse_module_has_main(&modnu, "root", &modnu_src, dir, engine)
        {
            result.diagnostics.push(Diagnostic::error(
                "library::root_function",
                Some(Source {
                    path: Some(rel.clone()),
                    position: [0, 0],
                }),
                "a call-target cannot live at the library root; move it into a module",
            ));
        }
        validate_mod_nu_ast(&rel, &fname, "mod", &modnu_src, dir, engine, &mut result.diagnostics);
        scan_reserved_terms(&rel, &fname, "mod", &modnu_src, dir, engine, &mut result.diagnostics);
    }
    let edges = extract_module_edges(&modnu_src, &modnu, dir, engine);

    let is_root = module_path.is_empty();
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            if is_root && name == ".assets" {
                validate_assets_tree(root, &path, result)?;
            } else if is_root && name == ".docs" {
                validate_docs_tree(root, &path, result)?;
            } else if name.starts_with('.') {
            } else {
                dirs.push((name, path));
            }
        } else if ft.is_file() {
            let is_nu = path.extension().map(|e| e == "nu").unwrap_or(false);
            if is_nu {
                check_not_executable(root, &path, Zone::Source, &mut result.diagnostics);
                files.push((name, path));
            } else if name == ".gitignore" {
                check_not_executable(root, &path, Zone::Source, &mut result.diagnostics);
            } else if name.starts_with('.') {
            } else {
                check_not_executable(root, &path, Zone::Source, &mut result.diagnostics);
                if !(is_root && is_sanctioned_root_file(&name)) {
                    push_ext_denied(
                        root,
                        &path,
                        Zone::Source,
                        "only .nu files are allowed in the library tree (plus root README.md / LEGAL.md / LICENSE.txt / LICENSE-*.txt / library.rig.toml, .gitignore, and the .assets/ + .docs/ dirs)",
                        &mut result.diagnostics,
                    );
                }
            }
        }
    }
    dirs.sort();
    files.sort();

    let mut functions: Vec<IndexFunction> = Vec::new();
    let mut modules: Vec<IndexModule> = Vec::new();

    for (name, path) in &files {
        if name == "mod.nu" {
            continue;
        }
        if error_count(&result.diagnostics) > LINT_VIOLATION_CAP {
            return Ok((modules, functions));
        }
        validate_flat_file(root, path, engine, result)?;
        let stem = name.trim_end_matches(".nu");
        if !edges.iter().any(|(_, n)| n == stem) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            result.diagnostics.push(Diagnostic::error(
                "library::orphan",
                Some(Source {
                    path: Some(rel),
                    position: [0, 0],
                }),
                format!(
                    "`{name}` is not reached through the module graph; `export use`/`use` it from mod.nu or remove it",
                ),
            ));
        }
    }

    for (name, path) in &dirs {
        if error_count(&result.diagnostics) > LINT_VIOLATION_CAP {
            return Ok((modules, functions));
        }
        let child_modnu = path.join("mod.nu");
        if !child_modnu.exists() {
            continue;
        }
        let child_src = fs::read_to_string(&child_modnu).unwrap_or_default();
        let rel = child_modnu
            .strip_prefix(root)
            .unwrap_or(&child_modnu)
            .to_string_lossy()
            .into_owned();
        let fname = child_modnu.to_string_lossy().into_owned();
        let edge_kinds: Vec<EdgeKind> = edges
            .iter()
            .filter(|(_, n)| n == name)
            .map(|(k, _)| *k)
            .collect();
        let child_has_main = parse_module_has_main(&child_modnu, name, &child_src, path, engine);

        if child_has_main {
            if module_path.is_empty() {
                result.diagnostics.push(Diagnostic::error(
                    "library::root_function",
                    Some(Source {
                        path: Some(rel),
                        position: [0, 0],
                    }),
                    "a call-target cannot live at the library root; move it into a module",
                ));
                continue;
            }
            check_not_executable(root, &child_modnu, Zone::Source, &mut result.diagnostics);
            if edge_kinds.is_empty() {
                result.diagnostics.push(Diagnostic::error(
                    "library::orphan",
                    Some(Source {
                        path: Some(rel.clone()),
                        position: [0, 0],
                    }),
                    format!("call `{name}` is not wired into mod.nu; add `export use {name}`"),
                ));
            } else if !edge_kinds.contains(&EdgeKind::ExportUse) {
                result.diagnostics.push(Diagnostic::error(
                    "library::call_wiring",
                    Some(Source {
                        path: Some(rel.clone()),
                        position: [0, 0],
                    }),
                    format!(
                        "a call must be wired into its parent via `export use {name}`, not `export module`",
                    ),
                ));
            }
            if has_child_module_dir(path) {
                result.diagnostics.push(Diagnostic::error(
                    "library::call_leaf",
                    Some(Source {
                        path: Some(rel.clone()),
                        position: [0, 0],
                    }),
                    format!(
                        "a call target is an edge module and cannot contain submodules; `{name}` has one",
                    ),
                ));
            }
            let extracted = validate_function_file_ast(
                &rel,
                &fname,
                name,
                &child_src,
                path,
                engine,
                &mut result.diagnostics,
            );
            scan_reserved_terms(&rel, &fname, name, &child_src, path, engine, &mut result.diagnostics);
            if let Some((idx_fn, summary, details)) = extracted {
                let coord = format!("{module_path}/{name}");
                if !summary.is_empty() || !details.is_empty() {
                    result.docs.push(DocEntry {
                        coord,
                        summary,
                        details,
                    });
                }
                functions.push(idx_fn);
            }
        } else {
            if edge_kinds.is_empty() {
                result.diagnostics.push(Diagnostic::error(
                    "library::orphan",
                    Some(Source {
                        path: Some(rel),
                        position: [0, 0],
                    }),
                    format!("`{name}` is not wired into mod.nu; add `export module {name}` (or remove it)"),
                ));
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
    }

    Ok((modules, functions))
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum EdgeKind {
    ExportModule,
    ExportUse,
    Use,
}

fn parse_module_has_main(
    modnu: &std::path::Path,
    name: &str,
    source: &str,
    parent: &std::path::Path,
    engine: &ParseEngine,
) -> bool {
    let fname = modnu.to_string_lossy().into_owned();
    let wrapper_name = format!("__cls_{name}");
    let (wrapped, _) = wrap_as_module(source, &wrapper_name);
    let engine_state = engine.engine_state_for_file(parent);
    let mut ws = nu::StateWorkingSet::new(&engine_state);
    let _ = ws.files.push(PathBuf::from(&fname), nu::Span::unknown());
    let _ = nu::parse(&mut ws, Some(&fname), wrapped.as_bytes(), false);
    if !ws.parse_errors.is_empty() {
        return false;
    }
    match ws.find_module(wrapper_name.as_bytes()) {
        Some(id) => ws.get_module(id).main.is_some(),
        None => false,
    }
}

fn extract_module_edges(
    source: &str,
    modnu: &std::path::Path,
    parent: &std::path::Path,
    engine: &ParseEngine,
) -> Vec<(EdgeKind, String)> {
    let mut edges = Vec::new();
    if source.is_empty() {
        return edges;
    }
    let fname = modnu.to_string_lossy().into_owned();
    let wrapper_name = "__edges";
    let (wrapped, _) = wrap_as_module(source, wrapper_name);
    let engine_state = engine.engine_state_for_file(parent);
    let mut ws = nu::StateWorkingSet::new(&engine_state);
    let _ = ws.files.push(PathBuf::from(&fname), nu::Span::unknown());
    let block = nu::parse(&mut ws, Some(&fname), wrapped.as_bytes(), false);
    if !ws.parse_errors.is_empty() {
        return edges;
    }
    let body_id = block
        .pipelines
        .first()
        .and_then(|p| p.elements.first())
        .and_then(|elem| match &elem.expr.expr {
            nu::Expr::Call(call) => call.arguments.iter().find_map(|arg| {
                if let nu::Argument::Positional(e) = arg {
                    if let nu::Expr::Block(id) = &e.expr {
                        return Some(*id);
                    }
                }
                None
            }),
            _ => None,
        });
    let Some(body_id) = body_id else {
        return edges;
    };
    let body = ws.get_block(body_id);
    for pipeline in &body.pipelines {
        for elem in &pipeline.elements {
            if let nu::Expr::Call(call) = &elem.expr.expr {
                let decl = ws.get_decl(call.decl_id);
                let kind = match decl.name() {
                    "export module" => EdgeKind::ExportModule,
                    "export use" => EdgeKind::ExportUse,
                    "use" | "overlay use" => EdgeKind::Use,
                    _ => continue,
                };
                if let Some(nu::Argument::Positional(e)) = call.arguments.first() {
                    if let Some(n) = edge_name_from_expr(&e.expr) {
                        edges.push((kind, n));
                    }
                }
            }
        }
    }
    edges
}

fn edge_name_from_expr(expr: &nu::Expr) -> Option<String> {
    let raw = match expr {
        nu::Expr::String(s)
        | nu::Expr::RawString(s)
        | nu::Expr::GlobPattern(s, _)
        | nu::Expr::Filepath(s, _)
        | nu::Expr::Directory(s, _) => s.clone(),
        _ => return None,
    };
    let stem = std::path::Path::new(&raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&raw)
        .to_string();
    if stem.is_empty() { None } else { Some(stem) }
}

fn has_child_module_dir(dir: &std::path::Path) -> bool {
    let Ok(read) = fs::read_dir(dir) else {
        return false;
    };
    for entry in read.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if entry.path().join("mod.nu").exists() {
            return true;
        }
    }
    false
}

fn validate_flat_file(
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
    let fname = path.to_string_lossy().into_owned();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    let wrapper_name = format!("__of_{stem}");
    let (wrapped, prefix_len) = wrap_as_module(&source, &wrapper_name);
    let engine_state = engine.engine_state_for_file(parent);
    let mut ws = nu::StateWorkingSet::new(&engine_state);
    let _ = ws.files.push(PathBuf::from(&fname), nu::Span::unknown());
    let _ = nu::parse(&mut ws, Some(&fname), wrapped.as_bytes(), false);
    for err in &ws.parse_errors {
        let span_start = err.span().start.saturating_sub(prefix_len);
        let (line, _col) = span_to_line_col(&source, span_start);
        result.diagnostics.push(Diagnostic::error(
            "library::parse_error",
            Some(Source {
                path: Some(rel.clone()),
                position: [line, 0],
            }),
            format!("parse error: {err:?}"),
        ));
    }
    if ws.parse_errors.is_empty()
        && let Some(id) = ws.find_module(wrapper_name.as_bytes())
        && ws.get_module(id).main.is_some()
    {
        result.diagnostics.push(Diagnostic::error(
            "library::main_in_flat_file",
            Some(Source {
                path: Some(rel.clone()),
                position: [0, 0],
            }),
            "`export def main` is reserved for a call target and must live in `<call>/mod.nu`, not a flat file",
        ));
    }
    scan_reserved_terms(&rel, &fname, stem, &source, parent, engine, &mut result.diagnostics);
    Ok(())
}

fn scan_reserved_terms(
    rel: &str,
    fname: &str,
    stem: &str,
    source: &str,
    parent: &std::path::Path,
    engine: &ParseEngine,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for comp in rel.split('/') {
        let bare = comp.strip_suffix(".nu").unwrap_or(comp);
        if is_reserved_term(bare) {
            diagnostics.push(Diagnostic::error(
                "library::reserved",
                Some(Source {
                    path: Some(rel.to_string()),
                    position: [0, 0],
                }),
                format!(
                    "`{bare}` is reserved (the call-target sentinel) and cannot name a module, directory, or file",
                ),
            ));
        }
    }

    let wrapper_name = format!("__rt_{stem}");
    let (wrapped, prefix_len) = wrap_as_module(source, &wrapper_name);
    let engine_state = engine.engine_state_for_file(parent);
    let mut working_set = nu::StateWorkingSet::new(&engine_state);
    let _ = working_set.files.push(PathBuf::from(fname), nu::Span::unknown());
    let block = nu::parse(&mut working_set, Some(fname), wrapped.as_bytes(), false);
    if !working_set.parse_errors.is_empty() {
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
            diagnostics.push(Diagnostic::error(
                "library::reserved",
                Some(Source {
                    path: Some(rel.to_string()),
                    position: [line, 0],
                }),
                format!(
                    "`{content}` is reserved -- it may appear only as a call-target's exported `main`; rename this identifier",
                ),
            ));
        }
        if matches!(&shape, nu::FlatShape::Signature) {
            for pname in signature_param_names(content) {
                if is_reserved_term(&pname) {
                    let src_off = span.start.saturating_sub(prefix_len);
                    let (line, _col) = span_to_line_col(source, src_off);
                    diagnostics.push(Diagnostic::error(
                        "library::reserved",
                        Some(Source {
                            path: Some(rel.to_string()),
                            position: [line, 0],
                        }),
                        format!(
                            "`{pname}` is reserved and cannot be a parameter name; rename it",
                        ),
                    ));
                }
            }
        }
        prev_export_def =
            matches!(&shape, nu::FlatShape::InternalCall(_)) && content == "export def";
    }
}

pub(crate) fn is_reserved_term(s: &str) -> bool {
    s == "main"
}

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

fn validate_mod_nu_ast(
    rel: &str,
    fname: &str,
    stem: &str,
    source: &str,
    parent: &std::path::Path,
    engine: &ParseEngine,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_summary_length(rel, source, None, diagnostics);
    let wrapper_name = format!("__v_{stem}");
    let (wrapped, prefix_len) = wrap_as_module(source, &wrapper_name);
    let engine_state = engine.engine_state_for_file(parent);
    let mut working_set = nu::StateWorkingSet::new(&engine_state);
    let _ = working_set.files.push(PathBuf::from(fname), nu::Span::unknown());
    let outer_block = nu::parse(&mut working_set, Some(fname), wrapped.as_bytes(), false);

    for err in &working_set.parse_errors {
        let span_start = err.span().start.saturating_sub(prefix_len);
        let (line, _col) = span_to_line_col(source, span_start);
        diagnostics.push(Diagnostic::error(
            "library::parse_error",
            Some(Source {
                path: Some(rel.to_string()),
                position: [line, 0],
            }),
            format!("parse error: {err:?}"),
        ));
    }

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
            return;
        }
    };

    let body = working_set.get_block(body_block_id);
    for pipeline in &body.pipelines {
        for elem in &pipeline.elements {
            check_mod_nu_pipeline_element(
                rel,
                &elem.expr,
                &working_set,
                source,
                prefix_len,
                diagnostics,
            );
        }
    }
}

fn check_mod_nu_pipeline_element(
    rel: &str,
    expr: &nu::Expression,
    working_set: &nu::StateWorkingSet,
    source: &str,
    prefix_len: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let span_start = expr.span.start.saturating_sub(prefix_len);
    let (line, _col) = span_to_line_col(source, span_start);
    match &expr.expr {
        nu::Expr::Call(call) => {
            let decl = working_set.get_decl(call.decl_id);
            let name = decl.name();
            if matches!(
                name,
                "export use"
                    | "export module"
                    | "export const"
                    | "export def"
                    | "use"
                    | "overlay use"
                    | "def"
                    | "const"
                    | "alias"
            ) {
                return;
            }
            diagnostics.push(Diagnostic::error(
                "library::mod_nu",
                Some(Source {
                    path: Some(rel.to_string()),
                    position: [line, 0],
                }),
                format!(
                    "mod.nu may only contain the module cascade (`export module`/`export use`), exports (`export def`/`export const`), or private `def`/`const`/`use`/`alias` declarations; got call to `{name}`",
                ),
            ));
        }
        nu::Expr::Garbage => {
        }
        nu::Expr::ImportPattern(_) | nu::Expr::Overlay(_) => {}
        other => {
            diagnostics.push(Diagnostic::error(
                "library::mod_nu",
                Some(Source {
                    path: Some(rel.to_string()),
                    position: [line, 0],
                }),
                format!(
                    "mod.nu may only contain the module cascade, exports, or private declarations; got `{}`",
                    short_expr_label(other),
                ),
            ));
        }
    }
}

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

fn validate_function_file_ast(
    rel: &str,
    fname: &str,
    stem: &str,
    source: &str,
    parent: &std::path::Path,
    engine: &ParseEngine,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(IndexFunction, String, String)> {
    check_summary_length(rel, source, Some("export def main"), diagnostics);
    let wrapper_name = format!("__v_{stem}");
    let (wrapped, prefix_len) = wrap_as_module(source, &wrapper_name);
    let engine_state = engine.engine_state_for_file(parent);
    let mut working_set = nu::StateWorkingSet::new(&engine_state);
    let _ = working_set.files.push(PathBuf::from(fname), nu::Span::unknown());
    let _ = nu::parse(&mut working_set, Some(fname), wrapped.as_bytes(), false);

    for err in &working_set.parse_errors {
        let span_start = err.span().start.saturating_sub(prefix_len);
        let (line, _col) = span_to_line_col(source, span_start);
        diagnostics.push(Diagnostic::error(
            "library::parse_error",
            Some(Source {
                path: Some(rel.to_string()),
                position: [line, 0],
            }),
            format!("parse error: {err:?}"),
        ));
    }
    if !working_set.parse_errors.is_empty() {
        return None;
    }

    let wrapper_name_bytes = wrapper_name.as_bytes();
    let module_id = match working_set.find_module(wrapper_name_bytes) {
        Some(id) => id,
        None => {
            diagnostics.push(Diagnostic::error(
                "library::internal",
                Some(Source {
                    path: Some(rel.to_string()),
                    position: [0, 0],
                }),
                "internal: wrapper module not found after parse",
            ));
            return None;
        }
    };
    let module: &nu::Module = working_set.get_module(module_id);

    let main_decl = module.main;
    let Some(main_id) = main_decl else {
        return None;
    };


    check_args_record_positional(rel, &working_set, main_id, "main", source, prefix_len, diagnostics);
    let out_type =
        check_main_output_type(rel, &working_set, main_id, source, prefix_len, &wrapped, diagnostics);

    let main_sig = working_set.get_decl(main_id).signature();
    let summary = main_sig.description.clone();
    let details = main_sig.extra_description.clone();
    let args_str = extract_main_args_text(&working_set, main_id, &wrapped)?;
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

fn check_args_record_positional(
    rel: &str,
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    fn_name: &str,
    source: &str,
    prefix_len: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<nu::VarId> {
    let decl = working_set.get_decl(decl_id);
    let sig = decl.signature();
    let positional = sig.required_positional.first();
    let (bad, var_id) = match positional {
        None => (true, None),
        Some(p) => {
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
        diagnostics.push(Diagnostic::error(
            "library::args",
            Some(Source {
                path: Some(rel.to_string()),
                position: [line, 0],
            }),
            format!(
                "{fn_name} must take a typed positional `args: record<...>` with real fields (or `args: nothing` for void); an empty `record<>` is the unfleshed skeleton",
            ),
        ));
    }
    var_id
}

fn check_main_output_type(
    rel: &str,
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    source: &str,
    prefix_len: usize,
    wrapped: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let sig = working_set.get_decl(decl_id).signature();
    let line = decl_line(working_set, decl_id, source, prefix_len);
    let output = sig
        .input_output_types
        .iter()
        .find(|(input, _)| matches!(input, nu::Type::Nothing))
        .map(|(_, out)| out.clone());
    let Some(output) = output else {
        diagnostics.push(Diagnostic::error(
            "library::output",
            Some(Source {
                path: Some(rel.to_string()),
                position: [line, 0],
            }),
            "main must declare a `: nothing -> <record<...>|nothing>` output type",
        ));
        return None;
    };
    match &output {
        nu::Type::Nothing => Some("nothing".to_string()),
        nu::Type::Record(fields) if !fields.is_empty() => {
            Some(
                extract_main_output_text(working_set, decl_id, wrapped)
                    .unwrap_or_else(|| output.to_string()),
            )
        }
        nu::Type::Record(_) => {
            diagnostics.push(Diagnostic::error(
                "library::skeleton",
                Some(Source {
                    path: Some(rel.to_string()),
                    position: [line, 0],
                }),
                "main's output `record<>` is the unfleshed skeleton; give it real fields (or `nothing` for void)",
            ));
            None
        }
        other => {
            diagnostics.push(Diagnostic::error(
                "library::output",
                Some(Source {
                    path: Some(rel.to_string()),
                    position: [line, 0],
                }),
                format!(
                    "main's output type must be a non-empty `record<...>` or `nothing`; got `{other}`",
                ),
            ));
            None
        }
    }
}

fn extract_main_output_text(
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    wrapped: &str,
) -> Option<String> {
    let block_id = working_set.get_decl(decl_id).block_id()?;
    let body_start = working_set.get_block(block_id).span?.start;
    let before = wrapped.get(..body_start)?;
    let arrow = before.rfind("->")?;
    let after = &before[arrow + 2..];
    let r = after.split('{').next().unwrap_or(after).trim();
    if r.is_empty() {
        None
    } else {
        Some(r.to_string())
    }
}

fn extract_main_args_text(
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    wrapped: &str,
) -> Option<String> {
    let block_id = working_set.get_decl(decl_id).block_id()?;
    let body_start = working_set.get_block(block_id).span?.start;
    let before = wrapped.get(..body_start)?;
    let arrow = before.rfind("->")?;
    let head = &before[..arrow];
    let close = head.rfind(']')?;
    let open = head[..close].rfind('[')?;
    let params = &head[open + 1..close];
    let colon = params.find(':')?;
    let type_text = params[colon + 1..].trim();
    if type_text.is_empty() {
        None
    } else {
        Some(type_text.to_string())
    }
}

fn decl_line(
    working_set: &nu::StateWorkingSet,
    decl_id: nu::DeclId,
    source: &str,
    prefix_len: usize,
) -> usize {
    let decl = working_set.get_decl(decl_id);
    let span = decl.signature().name.is_empty();
    let _ = span;
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

fn write_meta(
    name: &str,
    source_path: &std::path::Path,
    result: &ValidationResult,
) -> Result<(), Error> {
    let canonical = library_dir(name);
    let meta_dir = canonical.join(".meta");
    fs::create_dir_all(&meta_dir)?;
    let index = LibraryIndex {
        source_path: source_path.to_path_buf(),
        functions: result.functions.clone(),
        modules: result.modules.clone(),
    };
    let index_nuon = index_to_nuon(&index).map_err(|reason| Error::Internal {
        phase: "commit::serialize_index".to_string(),
        reason,
    })?;
    fs::write(canonical.join(META_FILE), index_nuon.as_bytes())?;
    let docs_dir = meta_dir.join("docs");
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

pub(crate) fn commit_impl(name: &str, engine: &ParseEngine) -> Result<CommitResult, Error> {
    if !is_valid_library(name) {
        return Err(Error::LibraryInvalidName {
            library: name.to_string(),
            reason: "library must be the compound `<author>/<name>`".to_string(),
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
    let mut result = validate_library_source(name, &source_path, engine)?;
    prefix_diagnostic_paths(&mut result, name);
    if !result.is_empty() {
        return Err(Error::LibraryViolations {
            diagnostics: result.diagnostics,
        });
    }
    if lib_root.exists() {
        fs::remove_dir_all(&lib_root)?;
    }
    if let Some(parent) = lib_root.parent() {
        fs::create_dir_all(parent)?;
    }
    copy_dir_recursive(&source_path, &lib_root)?;
    write_meta(name, &source_path, &result)?;
    let rel = library_store_rel(name);
    run_git(&libraries_dir(), &["add", "--", &rel])?;
    let porcelain = run_git_output(&libraries_dir(), &["status", "--porcelain", "--", &rel])?;
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

pub(crate) fn install_impl(
    library: &str,
    source_dir: &std::path::Path,
    engine: &ParseEngine,
) -> Result<CommitResult, Error> {
    establish_library(library, source_dir)?;
    match commit_impl(library, engine) {
        Ok(result) => Ok(result),
        Err(e) => {
            let lib_root = library_dir(library);
            if lib_root.exists() {
                let _ = fs::remove_dir_all(&lib_root);
                let rel = library_store_rel(library);
                let _ = run_git(&libraries_dir(), &["add", "--", &rel]);
                let _ = run_git(
                    &libraries_dir(),
                    &["commit", "-m", &format!("rollback failed install {library}")],
                );
            }
            Err(e)
        }
    }
}

pub(crate) fn uninstall_impl(library: &str) -> Result<(), Error> {
    let lib_root = library_dir(library);
    if !lib_root.exists() {
        return Ok(());
    }
    fs::remove_dir_all(&lib_root)?;
    let rel = library_store_rel(library);
    run_git(&libraries_dir(), &["add", "--", &rel])?;
    run_git(
        &libraries_dir(),
        &["commit", "-m", &format!("uninstall library {library}")],
    )?;
    Ok(())
}

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
    let mut result = validate_library_source(library, &source_path, engine)?;
    prefix_diagnostic_paths(&mut result, library);
    Ok(result)
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> io::Result<()> {
    copy_library_tree(src, dst, true)
}

fn copy_library_tree(src: &std::path::Path, dst: &std::path::Path, is_root: bool) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            if is_root && (name == ".assets" || name == ".docs") {
                copy_tree_all(&src_path, &dst_path)?;
            } else if name.starts_with('.') {
                continue;
            } else {
                copy_library_tree(&src_path, &dst_path, false)?;
            }
        } else if ft.is_file() && (name == ".gitignore" || !name.starts_with('.')) {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn copy_tree_all(src: &std::path::Path, dst: &std::path::Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_tree_all(&src_path, &dst_path)?;
        } else if ft.is_file() {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}


/// One-time migration: `.meta/library.json` -> `.meta/library.nuon`.
///
/// The index was the last JSON we persisted, against the house rule that anything we
/// persist is NUON. Changing the FORMAT without migrating existing STORES would be a
/// silent break rather than a fix: library DETECTION keys on the meta file, so an
/// unmigrated library simply stops existing - no error, just an empty `info()` and a
/// `not_registered` on every call.
///
/// Idempotent. A store with no legacy file is a no-op; a library that somehow carries
/// both keeps the `.nuon` it already has and drops the stale `.json`. Returns how many
/// libraries were touched.
fn migrate_meta_to_nuon() -> io::Result<usize> {
    let root = libraries_dir().join(RIG_TYPE_DIR);
    if !root.exists() {
        return Ok(0);
    }
    let mut migrated = 0usize;
    for author in fs::read_dir(&root)? {
        let author = author?;
        if !author.file_type()?.is_dir() {
            continue;
        }
        for lib in fs::read_dir(author.path())? {
            let lib = lib?;
            if !lib.file_type()?.is_dir() {
                continue;
            }
            let legacy = lib.path().join(LEGACY_META_FILE);
            if !legacy.exists() {
                continue;
            }
            let target = lib.path().join(META_FILE);
            if !target.exists() {
                let bytes = fs::read(&legacy)?;
                let index: LibraryIndex = json::from_slice(&bytes)
                    .map_err(|e| io::Error::other(format!("decode {}: {e}", legacy.display())))?;
                let nuon = index_to_nuon(&index)
                    .map_err(|e| io::Error::other(format!("render {}: {e}", target.display())))?;
                fs::write(&target, nuon.as_bytes())?;
            }
            fs::remove_file(&legacy)?;
            migrated += 1;
        }
    }
    Ok(migrated)
}

pub(crate) async fn ensure_substrate() -> io::Result<Arc<LibraryLocks>> {
    ensure_keypair()?;
    ensure_libraries_repo()?;
    // BEFORE hydration, or an unmigrated store hydrates as empty.
    let migrated = migrate_meta_to_nuon()?;
    if migrated > 0 {
        eprintln!("grammar: migrated {migrated} library index file(s) to NUON");
        // Hygiene rather than correctness - the files on disk are already right. A
        // failure here leaves the signed repo dirty until the next commit sweeps it,
        // which is worth saying out loud but not worth refusing to start over.
        let dir = libraries_dir();
        if let Err(e) = run_git(&dir, &["add", "--", RIG_TYPE_DIR])
            .and_then(|()| run_git(&dir, &["commit", "-m", "migrate library index to NUON"]))
        {
            eprintln!("grammar: index migration not committed: {e}");
        }
    }
    let locks = Arc::new(LibraryLocks::new());
    locks.hydrate_from_disk().await?;
    Ok(locks)
}
