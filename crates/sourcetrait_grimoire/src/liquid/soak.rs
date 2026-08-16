use crate::*;

/// Root entries preserved through a Replace and never rendered from the source.
/// `--force` does not override protection.
pub(crate) const PROTECTED_DEFAULT: &[&str] = &[".git", ".repo"];

/// What the caller intends the soak to do at the target. Internal for now
/// (always Replace); the seam for future publish intents.
#[derive(Default)]
enum Intent {
    #[default]
    Replace,
}

/// How a Replace publishes the built tree over the target.
enum ReplaceStrategy {
    /// No protected entry present: move the whole built dir into place.
    Swap,
    /// A protected entry is present: move items inline, preserving protected.
    Inline,
}

/// Replace inputs resolved from the call + caller env.
pub(crate) struct Replace<'a> {
    pub(crate) force: bool,
    pub(crate) tmp_base: &'a Path,
    pub(crate) user: &'a str,
    pub(crate) protected: &'a [&'a str],
}

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = GrimoirePlugin;

    fn name(&self) -> &str {
        "grimoire soak"
    }

    fn description(&self) -> &str {
        "Render a .liquid file, or soak a directory tree of templates, with a record of fill values."
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("grimoire soak")
            .required(
                "from",
                nu::SyntaxShape::Filepath,
                "Source .liquid file or directory tree.",
            )
            .required(
                "to",
                nu::SyntaxShape::Filepath,
                "Destination file or directory.",
            )
            .required(
                "fill",
                nu::SyntaxShape::Record(vec![].into()),
                "Record of values for templating.",
            )
            .switch(
                "force",
                "Replace a target holding non-protected entries (protected paths are still preserved).",
                None,
            )
            .category(nu::Category::FileSystem)
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        vec![nu::Example {
            example: r#"grimoire soak skeleton/ out/ { iter: "mytopic" }"#,
            description: "Soak a directory of .liquid templates into a concrete tree.",
            result: None,
        }]
    }

    fn run(
        &self,
        _plugin: &GrimoirePlugin,
        engine: &nu::EngineInterface,
        call: &nu::EvaluatedCall,
        _input: &nu::Value,
    ) -> Result<nu::Value, nu::LabeledError> {
        let from = path::canonical(engine, &call.req::<PathBuf>(0)?, call.head)?;
        let to = path::expand(engine, &call.req::<PathBuf>(1)?)?;
        let fill: nu::Value = call.req(2)?;
        let force = call.has_flag("force")?;
        let fill_record = fill
            .as_record()
            .map_err(|error| labeled_error(error, call.head))?;

        let tmp_base = tmp_base(engine);
        let user = env_string(engine, "USER").unwrap_or_else(|| "soak".to_string());
        let replace = Replace {
            force,
            tmp_base: &tmp_base,
            user: &user,
            protected: PROTECTED_DEFAULT,
        };

        soak(&from, &to, &fill, fill_record, &replace)
            .map_err(|error| labeled_error(error, call.head))?;
        Ok(nu::Value::nothing(call.head))
    }
}

/// The tmp base for the isolated build + retire graveyard: the caller's
/// `$XDGX_TMP_HOME` when set, else the system temp dir.
fn tmp_base(engine: &nu::EngineInterface) -> PathBuf {
    env_string(engine, "XDGX_TMP_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// A caller env var as a String, or None when unset / non-string.
fn env_string(engine: &nu::EngineInterface, name: &str) -> Option<String> {
    engine
        .get_env_var(name)
        .ok()
        .flatten()
        .and_then(|value| value.coerce_string().ok())
}

/// FILE mode (a `.liquid` source) renders to `to`. DIR mode validates the fill
/// against an optional `.soak/soak.schema.nutype`, then runs the intent.
fn soak(
    from: &Path,
    to: &Path,
    fill: &nu::Value,
    fill_record: &nu::Record,
    replace: &Replace,
) -> NuPluginGrimoireResult<()> {
    if from.is_file() {
        soak_file(from, to, fill_record)
    } else if from.is_dir() {
        let schema_path = from.join(".soak").join("soak.schema.nutype");
        if schema_path.exists() {
            let schema = read_file(&schema_path)?;
            validate_fill(fill, schema.trim())?;
        }
        match Intent::default() {
            Intent::Replace => soak_dir_replace(from, to, fill_record, replace),
        }
    } else {
        Err(nu_plugin_error(format!(
            "soak source does not exist: {}",
            from.display()
        )))
    }
}

/// Render a single `.liquid` source file to the output path `to`.
fn soak_file(from: &Path, to: &Path, fill_record: &nu::Record) -> NuPluginGrimoireResult<()> {
    if from.extension().and_then(|ext| ext.to_str()) != Some("liquid") {
        return Err(nu_plugin_error(format!(
            "soak file source must end in .liquid: {}",
            from.display()
        )));
    }
    let rendered = render_template(&read_file(from)?, fill_record)?;
    write_file(to, &rendered)
}

/// DIR-mode Replace: build the tree in isolation, then publish it over `to`.
///
/// Precondition: `to` is replacable (absent, empty, or holds only protected
/// entries) unless `--force`. The publish strategy comes from the target -
/// Inline (item moves preserving protected entries) when a protected entry is
/// present, else Swap (whole-dir move). A build failure leaves `to` untouched.
pub(crate) fn soak_dir_replace(
    from: &Path,
    to: &Path,
    fill_record: &nu::Record,
    replace: &Replace,
) -> NuPluginGrimoireResult<()> {
    let target = analyze_target(to, replace.protected)?;
    if !target.replacable() && !replace.force {
        return Err(nu_plugin_error(format!(
            "soak target holds non-protected entries; pass --force to replace it \
             ({} are preserved regardless): {}",
            replace.protected.join(", "),
            to.display()
        )));
    }

    // Build in isolation first; on a build error the tempdir is removed on drop
    // and `to` is never touched.
    let build = new_build_dir(replace)?;
    soak_dir(from, build.path(), fill_record, replace.protected)?;

    let strategy = if target.has_protected() {
        ReplaceStrategy::Inline
    } else {
        ReplaceStrategy::Swap
    };
    match strategy {
        ReplaceStrategy::Swap => publish_swap(build, to, &target, replace),
        ReplaceStrategy::Inline => publish_inline(build, to, &target, replace),
    }
}

/// The analyzed root state of a Replace target: which entries are protected vs
/// not (absent / empty read as both lists empty).
struct Target {
    exists: bool,
    protected: Vec<PathBuf>,
    non_protected: Vec<PathBuf>,
}

impl Target {
    /// Replacable without `--force`: absent, empty, or only protected entries.
    fn replacable(&self) -> bool {
        self.non_protected.is_empty()
    }

    fn has_protected(&self) -> bool {
        !self.protected.is_empty()
    }
}

/// Classify the target's root entries into protected / non-protected. A target
/// that exists but is not a directory is an error.
fn analyze_target(to: &Path, protected: &[&str]) -> NuPluginGrimoireResult<Target> {
    if !to.exists() {
        return Ok(Target { exists: false, protected: Vec::new(), non_protected: Vec::new() });
    }
    if !to.is_dir() {
        return Err(nu_plugin_error(format!(
            "soak target exists and is not a directory: {}",
            to.display()
        )));
    }
    let mut prot = Vec::new();
    let mut non = Vec::new();
    for entry in fs::read_dir(to).map_err(|error| {
        nu_plugin_error(format!("could not read target {}: {error}", to.display()))
    })? {
        let entry = entry
            .map_err(|error| nu_plugin_error(format!("could not read a target entry: {error}")))?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if protected.contains(&name.as_ref()) {
            prot.push(entry.path());
        } else {
            non.push(entry.path());
        }
    }
    Ok(Target { exists: true, protected: prot, non_protected: non })
}

/// A fresh isolated build dir under the tmp base, prefixed with the caller user.
fn new_build_dir(replace: &Replace) -> NuPluginGrimoireResult<tempfile::TempDir> {
    fs::create_dir_all(replace.tmp_base).map_err(|error| {
        nu_plugin_error(format!(
            "could not create tmp base {}: {error}",
            replace.tmp_base.display()
        ))
    })?;
    tempfile::Builder::new()
        .prefix(&format!("{}.", replace.user))
        .tempdir_in(replace.tmp_base)
        .map_err(|error| {
            nu_plugin_error(format!(
                "could not create build dir under {}: {error}",
                replace.tmp_base.display()
            ))
        })
}

/// A fresh, persisted retire dir under `<tmp_base>/retired/soak/`, named after
/// the target with a unique suffix. Kept (not auto-removed) so retired content
/// survives for recovery.
fn new_retire_dir(to: &Path, replace: &Replace) -> NuPluginGrimoireResult<PathBuf> {
    let graveyard = replace.tmp_base.join("retired").join("soak");
    fs::create_dir_all(&graveyard).map_err(|error| {
        nu_plugin_error(format!("could not create retire dir {}: {error}", graveyard.display()))
    })?;
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "target".to_string());
    let dir = tempfile::Builder::new()
        .prefix(&format!("{name}."))
        .tempdir_in(&graveyard)
        .map_err(|error| {
            nu_plugin_error(format!(
                "could not create retire dir under {}: {error}",
                graveyard.display()
            ))
        })?;
    Ok(dir.keep())
}

/// Swap: no protected entry at the target. Clear it (nothing if absent, rmdir if
/// empty, retire the whole dir if it holds `--force`'d non-protected content),
/// then move the built dir into place.
fn publish_swap(
    build: tempfile::TempDir,
    to: &Path,
    target: &Target,
    replace: &Replace,
) -> NuPluginGrimoireResult<()> {
    if target.exists {
        if target.non_protected.is_empty() {
            fs::remove_dir(to).map_err(|error| {
                nu_plugin_error(format!("could not remove empty target {}: {error}", to.display()))
            })?;
        } else {
            let retire = new_retire_dir(to, replace)?;
            move_path(to, &retire)?;
        }
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            nu_plugin_error(format!("could not create parent of {}: {error}", to.display()))
        })?;
    }
    let built = build.keep();
    move_path(&built, to)
}

/// Inline: a protected entry is present, so `to` stays put. Move its
/// non-protected entries to a retire dir, then move the built output's contents
/// in. Protected entries are untouched; the build skips protected names, so no
/// built entry clashes with one.
fn publish_inline(
    build: tempfile::TempDir,
    to: &Path,
    target: &Target,
    replace: &Replace,
) -> NuPluginGrimoireResult<()> {
    if !target.non_protected.is_empty() {
        let retire = new_retire_dir(to, replace)?;
        for entry in &target.non_protected {
            let name = entry
                .file_name()
                .ok_or_else(|| nu_plugin_error("target entry has no name".to_string()))?;
            move_path(entry, &retire.join(name))?;
        }
    }
    for entry in fs::read_dir(build.path()).map_err(|error| {
        nu_plugin_error(format!("could not read build dir {}: {error}", build.path().display()))
    })? {
        let entry = entry
            .map_err(|error| nu_plugin_error(format!("could not read a build entry: {error}")))?;
        move_path(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

/// Move a path (file or dir) to `dest` via rename - a move within the filesystem.
fn move_path(src: &Path, dest: &Path) -> NuPluginGrimoireResult<()> {
    fs::rename(src, dest).map_err(|error| {
        nu_plugin_error(format!("could not move {} -> {}: {error}", src.display(), dest.display()))
    })
}

/// Build `from` into `to` recursively: `.liquid` files render (extension
/// dropped), other files copy as-is. Protected names and the `.soak` config dir
/// are skipped (never rendered into the output).
fn soak_dir(
    from: &Path,
    to: &Path,
    fill_record: &nu::Record,
    protected: &[&str],
) -> NuPluginGrimoireResult<()> {
    mkdir(to)?;
    let entries = fs::read_dir(from).map_err(|error| {
        nu_plugin_error(format!("could not read dir {}: {error}", from.display()))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            nu_plugin_error(format!("could not read an entry in {}: {error}", from.display()))
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".soak" || protected.contains(&name.as_ref()) {
            continue;
        }
        let src = entry.path();
        if src.is_dir() {
            soak_dir(&src, &to.join(name.as_ref()), fill_record, protected)?;
        } else if let Some(stem) = name.strip_suffix(".liquid") {
            let rendered = render_template(&read_file(&src)?, fill_record)?;
            write_file(&to.join(stem), &rendered)?;
        } else {
            copy_file(&src, &to.join(name.as_ref()))?;
        }
    }
    Ok(())
}

fn mkdir(path: &Path) -> NuPluginGrimoireResult<()> {
    fs::create_dir_all(path)
        .map_err(|error| nu_plugin_error(format!("could not create dir {}: {error}", path.display())))
}

fn read_file(path: &Path) -> NuPluginGrimoireResult<String> {
    fs::read_to_string(path)
        .map_err(|error| nu_plugin_error(format!("could not read {}: {error}", path.display())))
}

fn write_file(path: &Path, content: &str) -> NuPluginGrimoireResult<()> {
    if let Some(parent) = path.parent() {
        mkdir(parent)?;
    }
    fs::write(path, content)
        .map_err(|error| nu_plugin_error(format!("could not write {}: {error}", path.display())))
}

fn copy_file(from: &Path, to: &Path) -> NuPluginGrimoireResult<()> {
    if let Some(parent) = to.parent() {
        mkdir(parent)?;
    }
    fs::copy(from, to).map(|_| ()).map_err(|error| {
        nu_plugin_error(format!("could not copy {} -> {}: {error}", from.display(), to.display()))
    })
}
