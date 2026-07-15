use crate::*;

#[derive(clap::Parser)]
#[command(name = "arxivmd", about = "Fetch an arXiv paper and convert it to markdown")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Fetch a paper and write its markdown to a file.
    Get {
        /// arXiv id or URL (e.g. 1706.03762 or https://arxiv.org/abs/1706.03762).
        source: String,
        /// Output file, or a directory (then `<id>.md` is written into it).
        out: std::path::PathBuf,
    },
    /// Print a paper's metadata as compact NUON to stdout.
    Info {
        /// arXiv id or URL.
        source: String,
    },
}

/// CLI entry point.
pub fn run() -> Result<()> {
    match <Cli as clap::Parser>::parse().command {
        Command::Get { source, out } => get(&source, &out),
        Command::Info { source } => info(&source),
    }
}

fn get(source: &str, out: &std::path::Path) -> Result<()> {
    let id = arxiv::parse_id(source);
    let client = arxiv::Client::new()?;
    let metadata = client.metadata(&id)?;

    let body = match client.source(&id)? {
        arxiv::Source::Archive(tar) => convert::latex_to_markdown(&tar)?,
        arxiv::Source::PdfOnly => {
            let pdf = client.pdf(&id)?;
            convert::pdf_to_markdown(&pdf).map_err(|e| ArxivmdError::PdfFallback {
                id: id.clone(),
                message: e.to_string(),
            })?
        }
    };

    let document = format!(
        "# {}\n\n## Abstract\n\n{}\n\n{}\n",
        metadata.title.trim(),
        metadata.summary.trim(),
        body.trim(),
    );

    let path = resolve_out(out, &id);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ArxivmdError::Io {
                context: format!("create {}", parent.display()),
                source: e,
            })?;
        }
    }
    std::fs::write(&path, document).map_err(|e| ArxivmdError::Io {
        context: format!("write {}", path.display()),
        source: e,
    })?;
    eprintln!("arxivmd: wrote {}", path.display());
    Ok(())
}

// A directory target gets `<id>.md`; anything else is treated as a file path.
fn resolve_out(out: &std::path::Path, id: &str) -> std::path::PathBuf {
    if out.is_dir() {
        out.join(format!("{}.md", id.replace('/', "_")))
    } else {
        out.to_path_buf()
    }
}

fn info(source: &str) -> Result<()> {
    let id = arxiv::parse_id(source);
    let metadata = arxiv::Client::new()?.metadata(&id)?;
    println!("{}", metadata_to_nuon(&metadata)?);
    Ok(())
}

// Serialize the metadata as compact NUON via the nuon crate (single line; the
// user can `| to nuon --indent` to prettify).
fn metadata_to_nuon(m: &arxiv::Metadata) -> Result<String> {
    let span = Span::unknown();
    let text = |s: &str| Value::string(s, span);
    let list = |values: &[String]| {
        Value::list(
            values.iter().map(|s| Value::string(s.as_str(), span)).collect(),
            span,
        )
    };
    let fields = record! {
        "id" => text(&m.id),
        "title" => text(&m.title),
        "authors" => list(&m.authors),
        "abstract" => text(&m.summary),
        "published" => text(&m.published),
        "updated" => text(&m.updated),
        "primary_category" => text(&m.primary_category),
        "categories" => list(&m.categories),
        "doi" => text(&m.doi),
        "comment" => text(&m.comment),
        "journal_ref" => text(&m.journal_ref),
        "abs_url" => text(&m.abs_url),
        "pdf_url" => text(&m.pdf_url),
    };
    let value = Value::record(fields, span);
    to_nuon(&EngineState::new(), &value, nuon::ToNuonConfig::default())
        .map_err(|e| ArxivmdError::Nuon { message: e.to_string() })
}
