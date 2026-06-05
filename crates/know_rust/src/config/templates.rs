use crate::*;

/// What: liquid template engine wrapper resolving a named prompt or
/// non-prompt template from either an `include_str!`-bundled default
/// or the runtime-loaded user override directory (the global `-t
/// <dir>` CLI flag, expected to contain `prompts/` and `templates/`
/// subdirectories).
///
/// Why: prompts and templates are data the_user iterates on without
/// recompiling the binary. Per the_user 2026-06-05: `assets/prompts/`
/// holds liquid prompts (agent-directed text), `assets/templates/`
/// holds non-prompt liquid (mechanical prose, picker tables, the
/// reference index); the directory convention IS the liquid signal.
/// Render call returns a fully composed String for the caller to
/// concatenate / write.
///
/// Where: instantiated once from `crate::run::run` and threaded into
/// the `characterize` + `emit` subcommands when they land. Phase 1
/// foundation: parser builds; per-template defaults + contexts get
/// filled in across phases 2 + 3.
pub struct Templates {
    parser: liquid::Parser,
    custom_prompts_dir: Option<PathBuf>,
    custom_templates_dir: Option<PathBuf>,
}

impl Templates {
    /// What: build the liquid parser + record the user-override
    /// directory layout (when `custom_root` is Some, prompts/ and
    /// templates/ children are searched for runtime overrides).
    ///
    /// Why: the parser is shareable across all template renders; the
    /// override directory layout mirrors the embedded asset layout so
    /// users see a 1:1 schema between defaults and their overrides.
    ///
    /// Where: called from `crate::run::run` once per invocation.
    pub fn new(custom_root: Option<PathBuf>) -> Self {
        let parser = liquid::ParserBuilder::with_stdlib()
            .build()
            .expect("liquid stdlib parser construction is infallible");
        let custom_prompts_dir = custom_root.as_ref().map(|p| p.join("prompts"));
        let custom_templates_dir = custom_root.as_ref().map(|p| p.join("templates"));
        Self {
            parser,
            custom_prompts_dir,
            custom_templates_dir,
        }
    }

    /// What: render the named prompt with the supplied serde-
    /// serializable context.
    ///
    /// Why: prompts are agent-directed liquid templates that get
    /// edited as the_user iterates on the orientation pipeline; the
    /// data-passing contract keeps the call site agnostic of where
    /// the template text comes from.
    ///
    /// Where: called from `crate::emit` section renderers (phase 3
    /// fills in concrete prompt names + contexts).
    pub fn render_prompt<C: serde::Serialize>(
        &self,
        name: &str,
        context: &C,
    ) -> std::result::Result<String, Error> {
        let text = self.load_prompt(name)?;
        self.render(name, &text, context)
    }

    /// What: render the named non-prompt template with the supplied
    /// serde-serializable context.
    ///
    /// Why: non-prompt templates are mechanical liquid (section
    /// scaffolding, picker tables, the reference.md index) that
    /// shares the same iteration ergonomics as prompts but isn't
    /// directed at an agent.
    ///
    /// Where: called from `crate::emit` section renderers (phase 3
    /// fills in concrete template names + contexts).
    pub fn render_template<C: serde::Serialize>(
        &self,
        name: &str,
        context: &C,
    ) -> std::result::Result<String, Error> {
        let text = self.load_template(name)?;
        self.render(name, &text, context)
    }

    fn render<C: serde::Serialize>(
        &self,
        name: &str,
        text: &str,
        context: &C,
    ) -> std::result::Result<String, Error> {
        let template = self.parser.parse(text).map_err(|source| Error::Liquid {
            name: name.to_string(),
            source,
        })?;
        let globals = liquid::to_object(context).map_err(|source| Error::Liquid {
            name: name.to_string(),
            source,
        })?;
        template.render(&globals).map_err(|source| Error::Liquid {
            name: name.to_string(),
            source,
        })
    }

    fn load_prompt(&self, name: &str) -> std::result::Result<String, Error> {
        if let Some(dir) = &self.custom_prompts_dir {
            let path = dir.join(name);
            if path.is_file() {
                return fs::read_to_string(&path).map_err(|source| Error::Read {
                    path,
                    source,
                });
            }
        }
        default_prompt(name)
            .map(|s| s.to_string())
            .ok_or_else(|| Error::TemplateNotFound {
                name: name.to_string(),
            })
    }

    fn load_template(&self, name: &str) -> std::result::Result<String, Error> {
        if let Some(dir) = &self.custom_templates_dir {
            let path = dir.join(name);
            if path.is_file() {
                return fs::read_to_string(&path).map_err(|source| Error::Read {
                    path,
                    source,
                });
            }
        }
        default_template(name)
            .map(|s| s.to_string())
            .ok_or_else(|| Error::TemplateNotFound {
                name: name.to_string(),
            })
    }
}

/// What: registry of embedded prompt defaults. Each entry maps a
/// `<prompt_snake>` name (no extension; the directory convention
/// signals liquid) to its compile-time-bundled text.
///
/// Why: phase 1 foundation has no entries yet; phase 3 fills in as
/// the emit subcommand's prompt blocks get authored. Centralizing the
/// registry in one match arm keeps the inventory legible.
///
/// Where: called from `Templates::load_prompt` when no user override
/// is configured or when the override directory lacks the requested
/// file.
fn default_prompt(_name: &str) -> Option<&'static str> {
    None
}

/// What: registry of embedded non-prompt template defaults. Same
/// shape as `default_prompt` for the `assets/templates/` subset.
///
/// Why: phase 1 foundation has no entries yet; phase 3 fills in as
/// the emit subcommand's mechanical-prose template blocks get
/// authored.
///
/// Where: called from `Templates::load_template`.
fn default_template(_name: &str) -> Option<&'static str> {
    None
}
