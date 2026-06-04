use crate::*;
use ext_serde::*;
use ext_syn::*;
use ext_walkdir::*;
use std::collections::HashMap;

/// What: per-file lex+structure facts walker. Ports rustscan.py
/// functionality to syn: walks the workspace, parses every .rs
/// file via syn::parse_file, extracts impls + traits + types +
/// fns + macros + macro_defs + attrs + derives + uses + mods +
/// type_usages + example_type_usages + seams + doc_count.
///
/// Why: rustscan.py used a regex+char-level lexer with manual
/// brace tracking; syn gives us a proper Rust parse tree, which
/// hardens the lex/structure path against macro / cfg / string
/// edge cases. Output JSON shape mirrors what rustscan.py emits
/// per file, aggregated across the workspace. characterize.py
/// (post-phase-5) reads recon_items.json + buckets per crate via
/// file -> crate longest-prefix-match.
///
/// Where: invoked from run.rs dispatch_scan() on ScanCommand::Items.

const TYPE_USAGE_NOISE_TYPES: &[&str] = &[
    "Vec", "VecDeque", "LinkedList", "BinaryHeap",
    "HashMap", "HashSet", "BTreeMap", "BTreeSet",
    "Box", "Arc", "Rc", "Mutex", "RwLock", "RefCell", "Cell", "Weak",
    "OnceCell", "OnceLock",
    "Option", "Result", "Some", "None", "Ok", "Err",
    "String", "Path", "PathBuf", "Cow", "Pin",
    "OsString", "OsStr", "CString", "CStr",
    "Iter", "IterMut", "IntoIter", "Chain", "Map", "Filter", "Take",
    "Skip", "Zip", "Enumerate", "Peekable",
    "std", "core", "alloc",
    "T", "E", "U", "K", "V",
    "f32", "f64",
    "i8", "i16", "i32", "i64", "i128", "isize",
    "u8", "u16", "u32", "u64", "u128", "usize",
    "char", "bool", "str",
];

const KEYWORDS: &[&str] = &[
    "impl", "trait", "struct", "enum", "union", "type", "fn", "mod", "macro_rules",
    "for", "where", "dyn", "pub", "use", "as", "const", "static", "unsafe", "extern",
    "async", "move", "ref", "mut", "let", "match", "if", "else", "while", "loop",
    "return", "self", "Self", "crate", "super", "in",
];

const NOISE_MACROS: &[&str] = &[
    "vec", "println", "print", "eprintln", "eprint", "format", "write", "writeln",
    "assert", "assert_eq", "assert_ne", "debug_assert", "debug_assert_eq",
    "debug_assert_ne", "panic", "todo", "unimplemented", "unreachable", "dbg",
    "matches", "include_str", "include_bytes", "include", "env", "option_env",
    "concat", "stringify", "format_args", "cfg", "line", "column", "file",
    "compile_error", "if_chain", "try", "await",
];

const NOISE_ATTRS: &[&str] = &[
    "case", "rstest", "expect",
    "rustfmt::skip",
    "serde", "strum", "bitflags", "clap", "arg", "command",
];

const INERT_ATTRS: &[&str] = &[
    "derive", "cfg", "cfg_attr", "allow", "warn", "deny", "forbid", "doc", "must_use",
    "inline", "repr", "non_exhaustive", "automatically_derived", "no_mangle", "used",
    "link_section", "export_name", "test", "ignore", "should_panic", "bench",
    "global_allocator", "panic_handler", "track_caller", "cold", "target_feature",
    "rustfmt", "clippy", "link", "no_link", "path", "macro_use", "macro_export",
    "proc_macro", "proc_macro_derive", "proc_macro_attribute", "stable", "unstable",
];

fn is_noise_macro(name: &str) -> bool {
    if NOISE_MACROS.contains(&name) {
        return true;
    }
    name.starts_with("assert_")
        || name.starts_with("debug_assert_")
        || name.starts_with("async_assert_")
        || name.starts_with("cfg_")
        || name.starts_with("cfg_not_")
}

fn is_noise_attr(path: &str, base: &str) -> bool {
    NOISE_ATTRS.contains(&path) || NOISE_ATTRS.contains(&base)
}

fn is_inert_attr(base: &str) -> bool {
    INERT_ATTRS.contains(&base)
}

fn is_example_file(rel_path: &str) -> bool {
    let p = rel_path.replace('\\', "/");
    p.contains("/examples/")
        || p.starts_with("examples/")
        || p.contains("/tests/")
        || p.starts_with("tests/")
        || p.contains("/benches/")
        || p.starts_with("benches/")
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub(crate) struct ItemsFacts {
    pub(crate) tool_version: String,
    pub(crate) files_scanned: usize,
    pub(crate) files_parse_failed: usize,
    pub(crate) impls: Vec<ImplEntry>,
    pub(crate) traits: Vec<TraitEntry>,
    pub(crate) types: Vec<TypeEntry>,
    pub(crate) fns: Vec<FnEntry>,
    pub(crate) mods: Vec<ModEntry>,
    pub(crate) uses: Vec<UseEntry>,
    pub(crate) macros: Vec<MacroEntry>,
    pub(crate) macro_defs: Vec<MacroDefEntry>,
    pub(crate) attrs: Vec<AttrEntry>,
    pub(crate) derives: Vec<DeriveEntry>,
    pub(crate) type_usages: Vec<TypeUsageEntry>,
    pub(crate) example_type_usages: Vec<TypeUsageEntry>,
    pub(crate) seams: HashMap<String, usize>,
    pub(crate) doc_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ImplEntry {
    pub(crate) file: String,
    #[serde(rename = "trait")]
    pub(crate) trait_name: Option<String>,
    #[serde(rename = "type")]
    pub(crate) type_name: Option<String>,
    pub(crate) line: usize,
    pub(crate) end_line: usize,
    pub(crate) cfg_gated: bool,
    pub(crate) cfg: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TraitEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) cfg_gated: bool,
    pub(crate) doc: String,
    pub(crate) visibility: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TypeEntry {
    pub(crate) file: String,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) cfg_gated: bool,
    pub(crate) doc: String,
    pub(crate) visibility: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct FnEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) brace_depth: usize,
    pub(crate) doc: String,
    pub(crate) visibility: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ModEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) visibility: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct UseEntry {
    pub(crate) file: String,
    pub(crate) reexport: bool,
    pub(crate) path: String,
    pub(crate) line: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MacroEntry {
    pub(crate) file: String,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) expansion_unverified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) args_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) arg_idents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) brace_depth: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MacroDefEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) visibility: String,
    pub(crate) macro_exported: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AttrEntry {
    pub(crate) file: String,
    pub(crate) path: String,
    pub(crate) base: String,
    pub(crate) args: String,
    pub(crate) inner: bool,
    pub(crate) line: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DeriveEntry {
    pub(crate) file: String,
    #[serde(rename = "trait")]
    pub(crate) trait_name: String,
    pub(crate) line: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TypeUsageEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) kind_hint: String,
    pub(crate) line: usize,
    pub(crate) brace_depth: usize,
    pub(crate) expansion_unverified: bool,
}

struct FileWalker {
    file: String,
    is_example: bool,
    file_facts: FileLevelFacts,
}

#[derive(Default)]
struct FileLevelFacts {
    impls: Vec<ImplEntry>,
    traits: Vec<TraitEntry>,
    types: Vec<TypeEntry>,
    fns: Vec<FnEntry>,
    mods: Vec<ModEntry>,
    uses: Vec<UseEntry>,
    macros: Vec<MacroEntry>,
    macro_defs: Vec<MacroDefEntry>,
    attrs: Vec<AttrEntry>,
    derives: Vec<DeriveEntry>,
    type_usages: Vec<TypeUsageEntry>,
    example_type_usages: Vec<TypeUsageEntry>,
    doc_count: usize,
    seams_extern: usize,
    seams_no_std: usize,
    seams_dyn: usize,
    seams_process_spawn: usize,
    seams_libc: usize,
    seams_syscall: usize,
    seams_serialize: usize,
    seams_stdin_stdout: usize,
    seams_unsafe: usize,
}

pub(crate) fn scan_workspace(
    workspace_root: &Path,
    out_dir: &Path,
) -> std::result::Result<(), Error> {
    let mut facts = ItemsFacts {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    };
    let mut rs_files = Vec::new();
    for entry in WalkDir::new(workspace_root)
        .into_iter()
        .filter_entry(|e| !is_target_dir(e.path()))
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if p.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let rel = match p.strip_prefix(workspace_root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        let parts: Vec<&str> = rel.split('/').collect();
        if parts.iter().any(|s| *s == "tests" || *s == "benches") {
            continue;
        }
        rs_files.push((p.to_path_buf(), rel));
    }
    rs_files.sort_by(|a, b| a.1.cmp(&b.1));
    let mut seam_extern = 0usize;
    let mut seam_no_std = 0usize;
    let mut seam_dyn = 0usize;
    let mut seam_process = 0usize;
    let mut seam_libc = 0usize;
    let mut seam_syscall = 0usize;
    let mut seam_serialize = 0usize;
    let mut seam_stdin_stdout = 0usize;
    let mut seam_unsafe = 0usize;
    for (p, rel) in &rs_files {
        let src = match fs::read_to_string(p) {
            Ok(s) => s,
            Err(_) => {
                continue;
            }
        };
        let file: RsFile = match syn::parse_str(&src) {
            Ok(f) => f,
            Err(_) => {
                facts.files_parse_failed += 1;
                continue;
            }
        };
        facts.files_scanned += 1;
        let mut walker = FileWalker {
            file: rel.clone(),
            is_example: is_example_file(rel),
            file_facts: FileLevelFacts::default(),
        };
        walker.walk_file(&file);
        let f = walker.file_facts;
        facts.impls.extend(f.impls);
        facts.traits.extend(f.traits);
        facts.types.extend(f.types);
        facts.fns.extend(f.fns);
        facts.mods.extend(f.mods);
        facts.uses.extend(f.uses);
        facts.macros.extend(f.macros);
        facts.macro_defs.extend(f.macro_defs);
        facts.attrs.extend(f.attrs);
        facts.derives.extend(f.derives);
        facts.type_usages.extend(f.type_usages);
        facts.example_type_usages.extend(f.example_type_usages);
        facts.doc_count += f.doc_count;
        seam_extern += f.seams_extern;
        seam_no_std += f.seams_no_std;
        seam_dyn += f.seams_dyn;
        seam_process += f.seams_process_spawn;
        seam_libc += f.seams_libc;
        seam_syscall += f.seams_syscall;
        seam_serialize += f.seams_serialize;
        seam_stdin_stdout += f.seams_stdin_stdout;
        seam_unsafe += f.seams_unsafe;
    }
    if seam_extern > 0 {
        facts.seams.insert("extern".to_string(), seam_extern);
    }
    if seam_no_std > 0 {
        facts.seams.insert("no_std".to_string(), seam_no_std);
    }
    if seam_dyn > 0 {
        facts.seams.insert("dyn_trait_object".to_string(), seam_dyn);
    }
    if seam_process > 0 {
        facts.seams.insert("process_spawn".to_string(), seam_process);
    }
    let syscall_total = seam_libc + seam_syscall;
    if syscall_total > 0 {
        facts.seams.insert("syscall_libc".to_string(), syscall_total);
    }
    if seam_serialize > 0 {
        facts.seams.insert("serde_serialize".to_string(), seam_serialize);
    }
    if seam_stdin_stdout > 0 {
        facts.seams.insert("std_io_stream".to_string(), seam_stdin_stdout);
    }
    if seam_unsafe > 0 {
        facts.seams.insert("unsafe".to_string(), seam_unsafe);
    }
    let out_path = out_dir.join("recon_items.json");
    let json = serde_json::to_string_pretty(&facts)
        .map_err(|source| Error::Serialize { source })?;
    let write_path = out_path.clone();
    fs::write(&out_path, json).map_err(|source| Error::Write {
        path: write_path,
        source,
    })?;
    eprintln!(
        "[rust_recon scan items] {} files scanned, {} parse failed, wrote {}",
        facts.files_scanned,
        facts.files_parse_failed,
        out_path.display()
    );
    Ok(())
}

fn is_target_dir(p: &Path) -> bool {
    p.components().any(|c| {
        c.as_os_str().to_str() == Some("target")
    })
}

impl FileWalker {
    fn walk_file(&mut self, file: &RsFile) {
        for attr in &file.attrs {
            self.record_attribute(attr, true);
        }
        self.walk_items(&file.items, 0);
    }

    fn walk_items(&mut self, items: &[Item], brace_depth: usize) {
        for item in items {
            self.walk_item(item, brace_depth);
        }
    }

    fn walk_item(&mut self, item: &Item, brace_depth: usize) {
        let attrs = item_attrs(item);
        let mut cfg_gated = false;
        let mut cfg_expr = String::new();
        let mut has_macro_export = false;
        if let Some(attrs) = attrs {
            for attr in attrs {
                let path_str = attribute_path_string(attr);
                let base = last_segment(&path_str);
                if base == "cfg" {
                    cfg_gated = true;
                    cfg_expr = attribute_args_string(attr);
                    if cfg_expr.trim() == "test" {
                        return;
                    }
                }
                if base == "macro_export" {
                    has_macro_export = true;
                }
                self.record_attribute(attr, false);
            }
        }
        match item {
            Item::Impl(i) => self.walk_impl(i, cfg_gated, &cfg_expr, brace_depth),
            Item::Trait(t) => self.walk_trait(t, cfg_gated, brace_depth),
            Item::Struct(s) => self.walk_struct(s, cfg_gated, brace_depth),
            Item::Enum(e) => self.walk_enum(e, cfg_gated, brace_depth),
            Item::Union(u) => self.walk_union(u, cfg_gated, brace_depth),
            Item::Type(ta) => self.walk_type_alias(ta),
            Item::Fn(f) => self.walk_fn(f, brace_depth),
            Item::Mod(m) => self.walk_mod(m, brace_depth),
            Item::Macro(mc) => self.walk_macro_item(mc, has_macro_export, brace_depth),
            Item::Use(u) => self.walk_use(u),
            Item::ExternCrate(_) => {
                self.file_facts.seams_extern += 1;
            }
            Item::ForeignMod(fm) => {
                self.file_facts.seams_extern += 1;
                for it in &fm.items {
                    if let syn::ForeignItem::Fn(ff) = it {
                        let line = ff.sig.ident.span().start().line;
                        self.file_facts.fns.push(FnEntry {
                            file: self.file.clone(),
                            name: ff.sig.ident.to_string(),
                            line,
                            brace_depth,
                            doc: extract_doc(&ff.attrs),
                            visibility: visibility_string(&ff.vis),
                        });
                    }
                }
            }
            Item::Const(c) => {
                self.walk_expr(&c.expr, brace_depth);
            }
            Item::Static(s) => {
                self.walk_expr(&s.expr, brace_depth);
            }
            _ => {}
        }
    }

    fn walk_impl(
        &mut self,
        i: &ItemImpl,
        cfg_gated: bool,
        cfg_expr: &str,
        brace_depth: usize,
    ) {
        let line = i.impl_token.span.start().line;
        let end_line = i.brace_token.span.close().start().line;
        let trait_name = i
            .trait_
            .as_ref()
            .and_then(|(_, path, _)| path.segments.last().map(|s| s.ident.to_string()));
        let type_name = type_base_name(&i.self_ty);
        self.file_facts.impls.push(ImplEntry {
            file: self.file.clone(),
            trait_name,
            type_name: Some(type_name),
            line,
            end_line,
            cfg_gated,
            cfg: cfg_expr.to_string(),
        });
        if i.unsafety.is_some() {
            self.file_facts.seams_unsafe += 1;
        }
        for item in &i.items {
            match item {
                ImplItem::Fn(f) => {
                    for attr in &f.attrs {
                        self.record_attribute(attr, false);
                    }
                    let line = f.sig.ident.span().start().line;
                    self.file_facts.fns.push(FnEntry {
                        file: self.file.clone(),
                        name: f.sig.ident.to_string(),
                        line,
                        brace_depth: brace_depth + 1,
                        doc: extract_doc(&f.attrs),
                        visibility: visibility_string(&f.vis),
                    });
                    if f.sig.unsafety.is_some() {
                        self.file_facts.seams_unsafe += 1;
                    }
                    self.walk_block(&f.block, brace_depth + 1);
                }
                ImplItem::Type(ty) => {
                    for attr in &ty.attrs {
                        self.record_attribute(attr, false);
                    }
                    let line = ty.ident.span().start().line;
                    self.file_facts.types.push(TypeEntry {
                        file: self.file.clone(),
                        kind: "type".to_string(),
                        name: ty.ident.to_string(),
                        line,
                        cfg_gated: false,
                        doc: String::new(),
                        visibility: visibility_string(&ty.vis),
                    });
                }
                ImplItem::Const(c) => {
                    for attr in &c.attrs {
                        self.record_attribute(attr, false);
                    }
                    self.walk_expr(&c.expr, brace_depth + 1);
                }
                _ => {}
            }
        }
    }

    fn walk_trait(&mut self, t: &ItemTrait, cfg_gated: bool, brace_depth: usize) {
        let line = t.ident.span().start().line;
        self.file_facts.traits.push(TraitEntry {
            file: self.file.clone(),
            name: t.ident.to_string(),
            line,
            cfg_gated,
            doc: extract_doc(&t.attrs),
            visibility: visibility_string(&t.vis),
        });
        if t.unsafety.is_some() {
            self.file_facts.seams_unsafe += 1;
        }
        for item in &t.items {
            match item {
                TraitItem::Fn(f) => {
                    for attr in &f.attrs {
                        self.record_attribute(attr, false);
                    }
                    let line = f.sig.ident.span().start().line;
                    self.file_facts.fns.push(FnEntry {
                        file: self.file.clone(),
                        name: f.sig.ident.to_string(),
                        line,
                        brace_depth: brace_depth + 1,
                        doc: extract_doc(&f.attrs),
                        visibility: visibility_string(&t.vis),
                    });
                    if f.sig.unsafety.is_some() {
                        self.file_facts.seams_unsafe += 1;
                    }
                    if let Some(b) = &f.default {
                        self.walk_block(b, brace_depth + 1);
                    }
                }
                TraitItem::Type(ty) => {
                    for attr in &ty.attrs {
                        self.record_attribute(attr, false);
                    }
                    let line = ty.ident.span().start().line;
                    self.file_facts.types.push(TypeEntry {
                        file: self.file.clone(),
                        kind: "type".to_string(),
                        name: ty.ident.to_string(),
                        line,
                        cfg_gated: false,
                        doc: String::new(),
                        visibility: visibility_string(&t.vis),
                    });
                }
                TraitItem::Const(c) => {
                    for attr in &c.attrs {
                        self.record_attribute(attr, false);
                    }
                    if let Some((_, expr)) = &c.default {
                        self.walk_expr(expr, brace_depth + 1);
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_struct(&mut self, s: &ItemStruct, cfg_gated: bool, _brace_depth: usize) {
        let line = s.ident.span().start().line;
        self.file_facts.types.push(TypeEntry {
            file: self.file.clone(),
            kind: "struct".to_string(),
            name: s.ident.to_string(),
            line,
            cfg_gated,
            doc: extract_doc(&s.attrs),
            visibility: visibility_string(&s.vis),
        });
        self.walk_fields(&s.fields);
    }

    fn walk_enum(&mut self, e: &ItemEnum, cfg_gated: bool, _brace_depth: usize) {
        let line = e.ident.span().start().line;
        self.file_facts.types.push(TypeEntry {
            file: self.file.clone(),
            kind: "enum".to_string(),
            name: e.ident.to_string(),
            line,
            cfg_gated,
            doc: extract_doc(&e.attrs),
            visibility: visibility_string(&e.vis),
        });
        for v in &e.variants {
            for attr in &v.attrs {
                self.record_attribute(attr, false);
            }
            self.walk_fields(&v.fields);
            if let Some((_, expr)) = &v.discriminant {
                self.walk_expr(expr, 0);
            }
        }
    }

    fn walk_union(&mut self, u: &ItemUnion, cfg_gated: bool, _brace_depth: usize) {
        let line = u.ident.span().start().line;
        self.file_facts.types.push(TypeEntry {
            file: self.file.clone(),
            kind: "union".to_string(),
            name: u.ident.to_string(),
            line,
            cfg_gated,
            doc: extract_doc(&u.attrs),
            visibility: visibility_string(&u.vis),
        });
        for f in &u.fields.named {
            for attr in &f.attrs {
                self.record_attribute(attr, false);
            }
        }
    }

    fn walk_fields(&mut self, fields: &Fields) {
        match fields {
            Fields::Named(named) => {
                for f in &named.named {
                    for attr in &f.attrs {
                        self.record_attribute(attr, false);
                    }
                }
            }
            Fields::Unnamed(unnamed) => {
                for f in &unnamed.unnamed {
                    for attr in &f.attrs {
                        self.record_attribute(attr, false);
                    }
                }
            }
            Fields::Unit => {}
        }
    }

    fn walk_type_alias(&mut self, ta: &ItemType) {
        let line = ta.ident.span().start().line;
        self.file_facts.types.push(TypeEntry {
            file: self.file.clone(),
            kind: "type".to_string(),
            name: ta.ident.to_string(),
            line,
            cfg_gated: false,
            doc: String::new(),
            visibility: visibility_string(&ta.vis),
        });
    }

    fn walk_fn(&mut self, f: &ItemFn, brace_depth: usize) {
        let line = f.sig.ident.span().start().line;
        self.file_facts.fns.push(FnEntry {
            file: self.file.clone(),
            name: f.sig.ident.to_string(),
            line,
            brace_depth,
            doc: extract_doc(&f.attrs),
            visibility: visibility_string(&f.vis),
        });
        if f.sig.unsafety.is_some() {
            self.file_facts.seams_unsafe += 1;
        }
        self.walk_block(&f.block, brace_depth + 1);
    }

    fn walk_mod(&mut self, m: &ItemMod, brace_depth: usize) {
        let line = m.ident.span().start().line;
        self.file_facts.mods.push(ModEntry {
            file: self.file.clone(),
            name: m.ident.to_string(),
            line,
            visibility: visibility_string(&m.vis),
        });
        if let Some((_, items)) = &m.content {
            self.walk_items(items, brace_depth + 1);
        }
    }

    fn walk_macro_item(
        &mut self,
        mc: &syn::ItemMacro,
        has_macro_export: bool,
        brace_depth: usize,
    ) {
        let name_segs: Vec<String> = mc
            .mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let name = name_segs.join("::");
        if name == "macro_rules" || mc.ident.is_some() {
            if let Some(ident) = &mc.ident {
                let line = ident.span().start().line;
                self.file_facts.macro_defs.push(MacroDefEntry {
                    file: self.file.clone(),
                    name: ident.to_string(),
                    line,
                    visibility: String::new(),
                    macro_exported: has_macro_export,
                });
            }
        } else {
            let line = mc.mac.path.segments.last().map(|s| s.ident.span().start().line).unwrap_or(0);
            let arg_idents = extract_macro_arg_idents(&mc.mac.tokens);
            let args_count = count_top_commas_plus_one(&mc.mac.tokens);
            if !is_noise_macro(&name) {
                self.file_facts.macros.push(MacroEntry {
                    file: self.file.clone(),
                    kind: "macro_invocation".to_string(),
                    name,
                    line,
                    expansion_unverified: true,
                    args_count: Some(args_count),
                    arg_idents: Some(arg_idents.into_iter().take(64).collect()),
                    brace_depth: Some(brace_depth),
                });
            }
        }
    }

    fn walk_use(&mut self, u: &syn::ItemUse) {
        let line = u.use_token.span.start().line;
        let reexport = matches!(u.vis, Visibility::Public(_));
        let path = flatten_use_tree(&u.tree);
        self.file_facts.uses.push(UseEntry {
            file: self.file.clone(),
            reexport,
            path,
            line,
        });
    }

    fn walk_block(&mut self, block: &Block, brace_depth: usize) {
        for stmt in &block.stmts {
            self.walk_stmt(stmt, brace_depth);
        }
    }

    fn walk_stmt(&mut self, stmt: &Stmt, brace_depth: usize) {
        match stmt {
            Stmt::Local(l) => {
                if let Some(init) = &l.init {
                    self.walk_expr(&init.expr, brace_depth);
                    if let Some((_, diverge)) = &init.diverge {
                        self.walk_expr(diverge, brace_depth);
                    }
                }
            }
            Stmt::Item(i) => self.walk_item(i, brace_depth),
            Stmt::Expr(e, _) => self.walk_expr(e, brace_depth),
            Stmt::Macro(sm) => {
                let name_segs: Vec<String> = sm
                    .mac
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                let name = name_segs.last().cloned().unwrap_or_default();
                let line = sm.mac.path.segments.last().map(|s| s.ident.span().start().line).unwrap_or(0);
                let arg_idents = extract_macro_arg_idents(&sm.mac.tokens);
                let args_count = count_top_commas_plus_one(&sm.mac.tokens);
                if !is_noise_macro(&name) {
                    self.file_facts.macros.push(MacroEntry {
                        file: self.file.clone(),
                        kind: "macro_invocation".to_string(),
                        name,
                        line,
                        expansion_unverified: true,
                        args_count: Some(args_count),
                        arg_idents: Some(arg_idents.into_iter().take(64).collect()),
                        brace_depth: Some(brace_depth),
                    });
                }
            }
        }
    }

    fn walk_expr(&mut self, expr: &Expr, brace_depth: usize) {
        match expr {
            Expr::Call(c) => {
                if let Expr::Path(p) = &*c.func {
                    self.maybe_record_type_usage(&p.path, brace_depth);
                }
                self.walk_expr(&c.func, brace_depth);
                for a in &c.args {
                    self.walk_expr(a, brace_depth);
                }
            }
            Expr::MethodCall(mc) => {
                if mc.method == "spawn" {
                    self.file_facts.seams_process_spawn += 1;
                }
                self.walk_expr(&mc.receiver, brace_depth);
                for a in &mc.args {
                    self.walk_expr(a, brace_depth);
                }
            }
            Expr::Path(p) => {
                self.scan_path_for_seams(&p.path);
            }
            Expr::Closure(cl) => self.walk_expr(&cl.body, brace_depth),
            Expr::Block(b) => self.walk_block(&b.block, brace_depth + 1),
            Expr::If(i) => {
                self.walk_expr(&i.cond, brace_depth);
                self.walk_block(&i.then_branch, brace_depth + 1);
                if let Some((_, else_branch)) = &i.else_branch {
                    self.walk_expr(else_branch, brace_depth);
                }
            }
            Expr::Match(m) => {
                self.walk_expr(&m.expr, brace_depth);
                for arm in &m.arms {
                    if let Some((_, g)) = &arm.guard {
                        self.walk_expr(g, brace_depth);
                    }
                    self.walk_expr(&arm.body, brace_depth);
                }
            }
            Expr::Loop(l) => self.walk_block(&l.body, brace_depth + 1),
            Expr::While(w) => {
                self.walk_expr(&w.cond, brace_depth);
                self.walk_block(&w.body, brace_depth + 1);
            }
            Expr::ForLoop(fl) => {
                self.walk_expr(&fl.expr, brace_depth);
                self.walk_block(&fl.body, brace_depth + 1);
            }
            Expr::Return(r) => {
                if let Some(e) = &r.expr {
                    self.walk_expr(e, brace_depth);
                }
            }
            Expr::Tuple(t) => {
                for e in &t.elems {
                    self.walk_expr(e, brace_depth);
                }
            }
            Expr::Array(a) => {
                for e in &a.elems {
                    self.walk_expr(e, brace_depth);
                }
            }
            Expr::Binary(b) => {
                self.walk_expr(&b.left, brace_depth);
                self.walk_expr(&b.right, brace_depth);
            }
            Expr::Unary(u) => self.walk_expr(&u.expr, brace_depth),
            Expr::Reference(r) => self.walk_expr(&r.expr, brace_depth),
            Expr::Paren(p) => self.walk_expr(&p.expr, brace_depth),
            Expr::Group(g) => self.walk_expr(&g.expr, brace_depth),
            Expr::Cast(c) => self.walk_expr(&c.expr, brace_depth),
            Expr::Field(f) => self.walk_expr(&f.base, brace_depth),
            Expr::Index(i) => {
                self.walk_expr(&i.expr, brace_depth);
                self.walk_expr(&i.index, brace_depth);
            }
            Expr::Range(r) => {
                if let Some(s) = &r.start {
                    self.walk_expr(s, brace_depth);
                }
                if let Some(e) = &r.end {
                    self.walk_expr(e, brace_depth);
                }
            }
            Expr::Try(t) => self.walk_expr(&t.expr, brace_depth),
            Expr::Await(a) => self.walk_expr(&a.base, brace_depth),
            Expr::Assign(a) => {
                self.walk_expr(&a.left, brace_depth);
                self.walk_expr(&a.right, brace_depth);
            }
            Expr::Let(l) => self.walk_expr(&l.expr, brace_depth),
            Expr::Async(a) => self.walk_block(&a.block, brace_depth + 1),
            Expr::Unsafe(u) => {
                self.file_facts.seams_unsafe += 1;
                self.walk_block(&u.block, brace_depth + 1);
            }
            Expr::TryBlock(t) => self.walk_block(&t.block, brace_depth + 1),
            Expr::Struct(s) => {
                for fv in &s.fields {
                    self.walk_expr(&fv.expr, brace_depth);
                }
                if let Some(rest) = &s.rest {
                    self.walk_expr(rest, brace_depth);
                }
            }
            Expr::Repeat(r) => {
                self.walk_expr(&r.expr, brace_depth);
                self.walk_expr(&r.len, brace_depth);
            }
            Expr::Macro(m) => {
                let name_segs: Vec<String> = m
                    .mac
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                let name = name_segs.last().cloned().unwrap_or_default();
                let line = m.mac.path.segments.last().map(|s| s.ident.span().start().line).unwrap_or(0);
                let arg_idents = extract_macro_arg_idents(&m.mac.tokens);
                let args_count = count_top_commas_plus_one(&m.mac.tokens);
                if !is_noise_macro(&name) {
                    self.file_facts.macros.push(MacroEntry {
                        file: self.file.clone(),
                        kind: "macro_invocation".to_string(),
                        name,
                        line,
                        expansion_unverified: true,
                        args_count: Some(args_count),
                        arg_idents: Some(arg_idents.into_iter().take(64).collect()),
                        brace_depth: Some(brace_depth),
                    });
                }
            }
            _ => {}
        }
    }

    fn maybe_record_type_usage(&mut self, path: &syn::Path, brace_depth: usize) {
        let segments: Vec<&syn::PathSegment> = path.segments.iter().collect();
        if segments.len() < 2 {
            return;
        }
        let inner_seg = segments[segments.len() - 1];
        let outer_seg = segments[segments.len() - 2];
        let outer = outer_seg.ident.to_string();
        let inner = inner_seg.ident.to_string();
        if KEYWORDS.contains(&outer.as_str()) || KEYWORDS.contains(&inner.as_str()) {
            return;
        }
        if TYPE_USAGE_NOISE_TYPES.contains(&outer.as_str()) {
            return;
        }
        let line = outer_seg.ident.span().start().line;
        let entry = TypeUsageEntry {
            file: self.file.clone(),
            name: format!("{}::{}", outer, inner),
            kind_hint: "factory_call".to_string(),
            line,
            brace_depth,
            expansion_unverified: false,
        };
        if self.is_example {
            self.file_facts.example_type_usages.push(entry);
        } else {
            self.file_facts.type_usages.push(entry);
        }
    }

    fn scan_path_for_seams(&mut self, path: &syn::Path) {
        for seg in &path.segments {
            let name = seg.ident.to_string();
            match name.as_str() {
                "libc" => self.file_facts.seams_libc += 1,
                "syscall" => self.file_facts.seams_syscall += 1,
                "Serialize" | "Deserialize" => self.file_facts.seams_serialize += 1,
                "stdin" | "stdout" => self.file_facts.seams_stdin_stdout += 1,
                "Command" => self.file_facts.seams_process_spawn += 1,
                _ => {}
            }
        }
    }

    fn record_attribute(&mut self, attr: &syn::Attribute, is_inner: bool) {
        let path_str = attribute_path_string(attr);
        let base = last_segment(&path_str);
        let args = attribute_args_string(attr);
        let line = attr_line(attr);
        if path_str == "doc" {
            self.file_facts.doc_count += 1;
        }
        self.file_facts.attrs.push(AttrEntry {
            file: self.file.clone(),
            path: path_str.clone(),
            base: base.clone(),
            args: args.clone(),
            inner: is_inner,
            line,
        });
        if base == "no_std" {
            self.file_facts.seams_no_std = 1;
        }
        if base == "derive" {
            for piece in split_top_commas(&args) {
                let cleaned = last_segment(piece.trim());
                if !cleaned.is_empty() {
                    self.file_facts.derives.push(DeriveEntry {
                        file: self.file.clone(),
                        trait_name: cleaned,
                        line,
                    });
                }
            }
        } else if !is_inert_attr(&base) && !is_noise_attr(&path_str, &base) {
            let args_count = split_top_commas(&args).iter().filter(|s| !s.trim().is_empty()).count();
            self.file_facts.macros.push(MacroEntry {
                file: self.file.clone(),
                kind: "attr_macro".to_string(),
                name: path_str.clone(),
                line,
                expansion_unverified: true,
                args_count: Some(args_count),
                arg_idents: None,
                brace_depth: None,
            });
        }
    }

}

fn item_attrs(item: &Item) -> Option<&Vec<syn::Attribute>> {
    match item {
        Item::Fn(i) => Some(&i.attrs),
        Item::Impl(i) => Some(&i.attrs),
        Item::Trait(i) => Some(&i.attrs),
        Item::Struct(i) => Some(&i.attrs),
        Item::Enum(i) => Some(&i.attrs),
        Item::Union(i) => Some(&i.attrs),
        Item::Type(i) => Some(&i.attrs),
        Item::Mod(i) => Some(&i.attrs),
        Item::Macro(i) => Some(&i.attrs),
        Item::Use(i) => Some(&i.attrs),
        Item::Const(i) => Some(&i.attrs),
        Item::Static(i) => Some(&i.attrs),
        Item::ExternCrate(i) => Some(&i.attrs),
        Item::ForeignMod(i) => Some(&i.attrs),
        _ => None,
    }
}

fn attribute_path_string(attr: &syn::Attribute) -> String {
    let path = attr.path();
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn last_segment(path: &str) -> String {
    path.rsplit("::").next().unwrap_or(path).trim().to_string()
}

fn attribute_args_string(attr: &syn::Attribute) -> String {
    match &attr.meta {
        syn::Meta::Path(_) => String::new(),
        syn::Meta::List(list) => list.tokens.to_string(),
        syn::Meta::NameValue(_) => String::new(),
    }
}

fn attr_line(attr: &syn::Attribute) -> usize {
    attr.pound_token.span.start().line
}

fn extract_doc(attrs: &[syn::Attribute]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for attr in attrs {
        if attribute_path_string(attr) != "doc" {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(lit) = &nv.value {
                if let syn::Lit::Str(s) = &lit.lit {
                    parts.push(clean_doc_line(&s.value()));
                }
            }
        }
    }
    parts
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn clean_doc_line(s: &str) -> String {
    s.trim().trim_start_matches('*').trim().to_string()
}

fn visibility_string(vis: &Visibility) -> String {
    match vis {
        Visibility::Public(_) => "pub".to_string(),
        Visibility::Restricted(r) => {
            let path = r
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            format!("pub({})", path)
        }
        Visibility::Inherited => String::new(),
    }
}

fn type_base_name(ty: &Type) -> String {
    match ty {
        Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        Type::Reference(r) => type_base_name(&r.elem),
        Type::Paren(p) => type_base_name(&p.elem),
        Type::Group(g) => type_base_name(&g.elem),
        _ => String::new(),
    }
}

fn split_top_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' | '<' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' | '}' | '>' => {
                depth = (depth - 1).max(0);
                cur.push(ch);
            }
            ',' if depth == 0 => {
                if !cur.trim().is_empty() {
                    out.push(cur.clone());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn count_top_commas_plus_one(tokens: &proc_macro2::TokenStream) -> usize {
    let s = tokens.to_string();
    if s.trim().is_empty() {
        return 0;
    }
    split_top_commas(&s).len().max(1)
}

fn extract_macro_arg_idents(tokens: &proc_macro2::TokenStream) -> Vec<String> {
    let s = tokens.to_string();
    let pieces = split_top_commas(&s);
    let mut out = Vec::new();
    for piece in pieces {
        let trimmed = piece.trim();
        let last = trimmed
            .rsplit("::")
            .next()
            .unwrap_or(trimmed)
            .split('<')
            .next()
            .unwrap_or(trimmed)
            .trim()
            .to_string();
        if is_ident(&last) {
            out.push(last);
        }
    }
    out
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {
            chars.all(|c| c.is_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

fn flatten_use_tree(tree: &syn::UseTree) -> String {
    let mut buf = String::new();
    flatten_use_tree_inner(tree, &mut buf);
    buf
}

fn flatten_use_tree_inner(tree: &syn::UseTree, buf: &mut String) {
    match tree {
        syn::UseTree::Path(p) => {
            buf.push_str(&p.ident.to_string());
            buf.push_str("::");
            flatten_use_tree_inner(&p.tree, buf);
        }
        syn::UseTree::Name(n) => {
            buf.push_str(&n.ident.to_string());
        }
        syn::UseTree::Rename(r) => {
            buf.push_str(&r.ident.to_string());
            buf.push_str(" as ");
            buf.push_str(&r.rename.to_string());
        }
        syn::UseTree::Glob(_) => {
            buf.push('*');
        }
        syn::UseTree::Group(g) => {
            buf.push('{');
            let mut first = true;
            for item in &g.items {
                if !first {
                    buf.push_str(", ");
                }
                first = false;
                flatten_use_tree_inner(item, buf);
            }
            buf.push('}');
        }
    }
}
