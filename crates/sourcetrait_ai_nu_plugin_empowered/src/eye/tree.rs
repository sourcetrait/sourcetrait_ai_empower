use crate::*;

/// Render a compact file tree of `dir` as a newline-joined string.
///
/// Files precede directories within each level (each group name-sorted by raw
/// bytes, case-sensitive); directories recurse depth-first, one space of indent
/// per level. Sizes are 1024-based with an integer floor (`0` bare for an empty
/// file). An `ignore` glob is skipped unless a `regard` glob rematches it;
/// `**/.git` is ignored by default. Symlinks are leaves (never followed). Globs
/// match each entry path relative to `dir` with nu_glob's default options.
pub(crate) fn tree(dir: &Path, ignore: &[String], regard: &[String]) -> TreeResult<String> {
    let mut ignore_globs = vec![compile("**/.git")?];
    for pattern in ignore {
        ignore_globs.push(compile(pattern)?);
    }
    let mut regard_globs = Vec::with_capacity(regard.len());
    for pattern in regard {
        regard_globs.push(compile(pattern)?);
    }

    let root = dir.display().to_string();
    let mut lines = vec![format!("{}/", root.trim_end_matches('/'))];
    walk(dir, dir, 1, &ignore_globs, &regard_globs, &mut lines)?;
    Ok(lines.join("\n"))
}

/// Compile one ignore/regard pattern, mapping a syntax error to `TreeError::Glob`.
fn compile(pattern: &str) -> TreeResult<nu_glob::Pattern> {
    nu_glob::Pattern::new(pattern).context(GlobSnafu { pattern: pattern.to_string() })
}

/// A directory entry retained for rendering, pre-classified by kind.
struct Entry {
    name: String,
    path: PathBuf,
    kind: EntryKind,
}

enum EntryKind {
    File { size: u64 },
    Dir,
    Symlink,
}

/// Append the lines for `dir`'s entries (at `depth`) to `lines`, recursing into
/// child directories depth-first. Files and symlinks emit before directories.
fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    ignore: &[nu_glob::Pattern],
    regard: &[nu_glob::Pattern],
    lines: &mut Vec<String>,
) -> TreeResult<()> {
    let mut files: Vec<Entry> = Vec::new();
    let mut dirs: Vec<Entry> = Vec::new();

    for entry in fs::read_dir(dir).context(ReadSnafu { path: dir.to_path_buf() })? {
        let entry = entry.context(ReadSnafu { path: dir.to_path_buf() })?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if ignored(relative, ignore, regard) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let metadata = fs::symlink_metadata(&path).context(ReadSnafu { path: path.clone() })?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            files.push(Entry { name, path, kind: EntryKind::Symlink });
        } else if file_type.is_dir() {
            dirs.push(Entry { name, path, kind: EntryKind::Dir });
        } else {
            files.push(Entry { name, path, kind: EntryKind::File { size: metadata.len() } });
        }
    }

    files.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    dirs.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));

    let indent = " ".repeat(depth);
    for entry in &files {
        match entry.kind {
            EntryKind::File { size } => {
                lines.push(format!("{indent}{} {}", wrap(&entry.name), size_label(size)));
            }
            EntryKind::Symlink => {
                lines.push(format!("{indent}{}", symlink_label(&entry.name, &entry.path)));
            }
            EntryKind::Dir => unreachable!("directories are collected separately"),
        }
    }
    for entry in &dirs {
        lines.push(format!("{indent}{}/", wrap(&entry.name)));
        walk(root, &entry.path, depth + 1, ignore, regard, lines)?;
    }
    Ok(())
}

/// Whether `relative` is hidden: an ignore glob matches it and no regard glob does.
fn ignored(relative: &Path, ignore: &[nu_glob::Pattern], regard: &[nu_glob::Pattern]) -> bool {
    if !ignore.iter().any(|glob| glob.matches_path(relative)) {
        return false;
    }
    !regard.iter().any(|glob| glob.matches_path(relative))
}

/// The size suffix: the largest 1024-based unit >= 1, integer-floored, lowercase
/// (`b`/`kb`/`mb`/`gb`/`tb`); `0` (bare, no unit) for an empty file.
pub(crate) fn size_label(bytes: u64) -> String {
    const KB: u64 = 1 << 10;
    const MB: u64 = 1 << 20;
    const GB: u64 = 1 << 30;
    const TB: u64 = 1 << 40;
    if bytes == 0 {
        "0".to_string()
    } else if bytes < KB {
        format!("{bytes}b")
    } else if bytes < MB {
        format!("{}kb", bytes / KB)
    } else if bytes < GB {
        format!("{}mb", bytes / MB)
    } else if bytes < TB {
        format!("{}gb", bytes / GB)
    } else {
        format!("{}tb", bytes / TB)
    }
}

/// Backtick-wrap a name that contains a space; otherwise return it unchanged.
pub(crate) fn wrap(name: &str) -> String {
    if name.contains(' ') {
        format!("`{name}`")
    } else {
        name.to_string()
    }
}

/// The `<name> -> <target>[ <size>]` line for a symlink (never followed): the
/// canonical target with a trailing `/` for a directory (no size), or a space +
/// size for a file; a broken link shows its raw readlink target with no size.
fn symlink_label(name: &str, path: &Path) -> String {
    let name = wrap(name);
    match fs::canonicalize(path) {
        Ok(target) => match fs::metadata(&target) {
            Ok(metadata) if metadata.is_dir() => format!("{name} -> {}/", target.display()),
            Ok(metadata) => {
                format!("{name} -> {} {}", target.display(), size_label(metadata.len()))
            }
            Err(_) => broken_symlink_label(&name, path),
        },
        Err(_) => broken_symlink_label(&name, path),
    }
}

/// The `<name> -> <readlink target>` line (no size) for a broken symlink; `name`
/// is already space-wrapped.
fn broken_symlink_label(name: &str, path: &Path) -> String {
    match fs::read_link(path) {
        Ok(target) => format!("{name} -> {}", target.display()),
        Err(_) => format!("{name} ->"),
    }
}

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = EmpowerPlugin;

    fn name(&self) -> &str {
        "empowered eye tree"
    }

    fn description(&self) -> &str {
        "Compact file tree listing."
    }

    fn extra_description(&self) -> &str {
        // An fs listing and the tree it renders, then the ignore / regard notes.
        // Left-aligned (column 0) so the help text carries it verbatim.
        r#"```fs
/path/dir
/path/dir/.gitignore
/path/dir/subdir
/path/dir/subdir/file1.txt
/path/dir/subdir/somedir/.file2
/path/dir/subdir/otherdir
```
```tree
/path/dir/
 .gitignore 0
 subdir/
  file1.txt 32mb
  otherdir/
  somedir/
   .file2 40b
```

Ignored unless regarded: ['.git']

@ignore Skip the specified globs; deny-list
@regard Render the specified globs; allow-list, exceptional"#
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("empowered eye tree")
            .required("dir", nu::SyntaxShape::Directory, "Root directory to render.")
            .optional(
                "ignore",
                nu::SyntaxShape::List(Box::new(nu::SyntaxShape::GlobPattern)),
                "Globs to skip; deny-list (extends the default `**/.git`).",
            )
            .optional(
                "regard",
                nu::SyntaxShape::List(Box::new(nu::SyntaxShape::GlobPattern)),
                "Globs to render even when ignored; allow-list, exceptional.",
            )
            .category(nu::Category::FileSystem)
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        vec![nu::Example {
            example: "empowered eye tree /path/to/dir",
            description: "Render a compact file tree of a directory.",
            result: None,
        }]
    }

    fn run(
        &self,
        _plugin: &EmpowerPlugin,
        _engine: &nu::EngineInterface,
        call: &nu::EvaluatedCall,
        _input: &nu::Value,
    ) -> Result<nu::Value, nu::LabeledError> {
        let dir: PathBuf = call.req(0)?;
        let ignore = globs(call, 1)?;
        let regard = globs(call, 2)?;
        let rendered =
            tree(&dir, &ignore, &regard).map_err(|error| labeled_error(error, call.head))?;
        Ok(nu::Value::string(rendered, call.head))
    }
}

/// The glob-pattern strings at optional positional `pos` (an empty vec if absent).
fn globs(call: &nu::EvaluatedCall, pos: usize) -> Result<Vec<String>, nu::LabeledError> {
    let Some(value): Option<nu::Value> = call.opt(pos)? else {
        return Ok(Vec::new());
    };
    let nu::Value::List { vals, .. } = &value else {
        return Err(labeled_error("expected a list of globs", call.head));
    };
    vals.iter()
        .map(|item| item.coerce_string().map_err(|error| labeled_error(error, call.head)))
        .collect()
}
