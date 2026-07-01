use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = EmpowerPlugin;

    fn name(&self) -> &str {
        "empowered soak"
    }

    fn description(&self) -> &str {
        "Render a .liquid file, or soak a directory tree of templates, with a record of fill values."
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("empowered soak")
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
                nu::SyntaxShape::Record(vec![]),
                "Record of values for templating.",
            )
            .category(nu::Category::FileSystem)
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        vec![nu::Example {
            example: r#"empowered soak skeleton/ out/ { iter: "mytopic" }"#,
            description: "Soak a directory of .liquid templates into a concrete tree.",
            result: None,
        }]
    }

    fn run(
        &self,
        _plugin: &EmpowerPlugin,
        engine: &nu::EngineInterface,
        call: &nu::EvaluatedCall,
        _input: &nu::Value,
    ) -> Result<nu::Value, nu::LabeledError> {
        let from = path::canonical(engine, &call.req::<PathBuf>(0)?, call.head)?;
        let to = path::expand(engine, &call.req::<PathBuf>(1)?)?;
        let fill: nu::Value = call.req(2)?;
        let fill_record = fill
            .as_record()
            .map_err(|error| labeled_error(error, call.head))?;
        soak(&from, &to, &fill, fill_record).map_err(|error| labeled_error(error, call.head))?;
        Ok(nu::Value::nothing(call.head))
    }
}

/// FILE mode (a `.liquid` source) renders to `to`. DIR mode reads soak config
/// from a `.soak` dir at the root of `from`: it validates the fill against an
/// optional `.soak/soak.schema.nutype`, then walks the tree (the whole `.soak`
/// dir is omitted from the output).
fn soak(
    from: &Path,
    to: &Path,
    fill: &nu::Value,
    fill_record: &nu::Record,
) -> NuPluginEmpowerResult<()> {
    if from.is_file() {
        soak_file(from, to, fill_record)
    } else if from.is_dir() {
        let schema_path = from.join(".soak").join("soak.schema.nutype");
        if schema_path.exists() {
            let schema = read_file(&schema_path)?;
            validate_fill(fill, schema.trim())?;
        }
        soak_dir(from, to, fill_record)
    } else {
        Err(nu_plugin_error(format!(
            "soak source does not exist: {}",
            from.display()
        )))
    }
}

/// Render a single `.liquid` source file to the output path `to`.
fn soak_file(from: &Path, to: &Path, fill_record: &nu::Record) -> NuPluginEmpowerResult<()> {
    if from.extension().and_then(|ext| ext.to_str()) != Some("liquid") {
        return Err(nu_plugin_error(format!(
            "soak file source must end in .liquid: {}",
            from.display()
        )));
    }
    let rendered = render_template(&read_file(from)?, fill_record)?;
    write_file(to, &rendered)
}

/// Walk `from` recursively, mirroring into `to`: `.liquid` files render (the
/// extension dropped), other files copy as-is; `.git` and the `.soak` config
/// dir (the soak meta home, holding `soak.schema.nutype`) are skipped.
fn soak_dir(from: &Path, to: &Path, fill_record: &nu::Record) -> NuPluginEmpowerResult<()> {
    mkdir(to)?;
    let entries = fs::read_dir(from)
        .map_err(|error| nu_plugin_error(format!("could not read dir {}: {error}", from.display())))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            nu_plugin_error(format!("could not read an entry in {}: {error}", from.display()))
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".git" || name == ".soak" {
            continue;
        }
        let src = entry.path();
        if src.is_dir() {
            soak_dir(&src, &to.join(name.as_ref()), fill_record)?;
        } else if let Some(stem) = name.strip_suffix(".liquid") {
            let rendered = render_template(&read_file(&src)?, fill_record)?;
            write_file(&to.join(stem), &rendered)?;
        } else {
            copy_file(&src, &to.join(name.as_ref()))?;
        }
    }
    Ok(())
}

fn mkdir(path: &Path) -> NuPluginEmpowerResult<()> {
    fs::create_dir_all(path)
        .map_err(|error| nu_plugin_error(format!("could not create dir {}: {error}", path.display())))
}

fn read_file(path: &Path) -> NuPluginEmpowerResult<String> {
    fs::read_to_string(path)
        .map_err(|error| nu_plugin_error(format!("could not read {}: {error}", path.display())))
}

fn write_file(path: &Path, content: &str) -> NuPluginEmpowerResult<()> {
    if let Some(parent) = path.parent() {
        mkdir(parent)?;
    }
    fs::write(path, content)
        .map_err(|error| nu_plugin_error(format!("could not write {}: {error}", path.display())))
}

fn copy_file(from: &Path, to: &Path) -> NuPluginEmpowerResult<()> {
    if let Some(parent) = to.parent() {
        mkdir(parent)?;
    }
    fs::copy(from, to).map(|_| ()).map_err(|error| {
        nu_plugin_error(format!(
            "could not copy {} -> {}: {error}",
            from.display(),
            to.display()
        ))
    })
}
