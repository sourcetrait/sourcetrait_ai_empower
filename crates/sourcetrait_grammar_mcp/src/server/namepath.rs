use crate::*;

pub(crate) struct Namepath(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NamepathRef {
    Rig {
        rig: String,
    },
    Module {
        rig: String,
        module_path: String,
    },
    Function {
        rig: String,
        module_path: String,
        name: String,
    },
}

/// The whole-namespace pattern.
pub(crate) const PATTERN_ALL: &str = "*";

/// The current-purview pattern.
pub(crate) const PATTERN_CURRENT: &str = ".";

/// A raw namepath string classified by SHAPE: a pattern, or an exact namepath.
pub(crate) enum NamepathStr {
    Namepath(Namepath),
    Pattern(NamepathPattern),
}

/// A set of namepaths, addressed by the trailing hierarchy character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NamepathPattern {
    /// `<author>/` - everything that author published.
    Author { author: String },
    /// `<author>/<name>:` - everything in that rig.
    Rig { rig: String },
    /// `<author>/<name>:<module_path>/` - that module and everything below it.
    ModuleTree {
        rig: String,
        module_path: String,
    },
    /// `<author>/<name>:<module_path>:` - the calls in that module, no deeper.
    ModuleCalls {
        rig: String,
        module_path: String,
    },
    /// `*` - the whole namespace.
    All,
    /// `.` - the current purview; it must be RESOLVED before matching.
    Current,
}

impl NamepathStr {
    pub(crate) fn parse(raw: &str) -> Result<Self, GrammarMcpError> {
        if is_pattern_shaped(raw) {
            Ok(Self::Pattern(NamepathPattern::parse(raw)?))
        } else {
            Ok(Self::Namepath(Namepath(raw.to_string())))
        }
    }
}

impl NamepathStr {
    /// Does this put `namepath` in view?
    pub(crate) fn covers(
        &self,
        namepath: &NamepathRef,
    ) -> bool {
        match self {
            Self::Pattern(pattern) => pattern.matches(namepath),
            Self::Namepath(exact) => exact.validate().is_ok_and(|v| &v == namepath),
        }
    }
}

fn is_pattern_shaped(raw: &str) -> bool {
    raw == PATTERN_ALL || raw == PATTERN_CURRENT || raw.ends_with('/') || raw.ends_with(':')
}

impl NamepathRef {
    /// The compound `<author>/<name>` every namepath carries.
    pub(crate) fn rig(&self) -> &str {
        match self {
            Self::Rig { rig }
            | Self::Module { rig, .. }
            | Self::Function { rig, .. } => rig,
        }
    }
}

impl Namepath {
    pub(crate) fn validate(&self) -> Result<NamepathRef, GrammarMcpError> {
        fn bad(raw: &str, reason: &str) -> GrammarMcpError {
            GrammarMcpError::NamepathInvalid {
                namepath: raw.to_string(),
                reason: reason.to_string(),
            }
        }
        let raw = self.0.as_str();
        if raw.is_empty() {
            return Err(bad(raw, "empty namepath"));
        }
        let parts: Vec<&str> = raw.split(':').collect();
        match parts.as_slice() {
            [rig] => {
                check_rig(rig).map_err(|e| bad(raw, e))?;
                Ok(NamepathRef::Rig {
                    rig: (*rig).to_string(),
                })
            }
            [rig, module_path] => {
                check_rig(rig).map_err(|e| bad(raw, e))?;
                check_module_path(module_path).map_err(|e| bad(raw, e))?;
                Ok(NamepathRef::Module {
                    rig: (*rig).to_string(),
                    module_path: (*module_path).to_string(),
                })
            }
            [rig, module_path, name] => {
                check_rig(rig).map_err(|e| bad(raw, e))?;
                if module_path.is_empty() {
                    return Err(bad(
                        raw,
                        "a function needs a parent module; `rig::function` (a root function) is not allowed",
                    ));
                }
                check_module_path(module_path).map_err(|e| bad(raw, e))?;
                if !is_valid_ident(name) {
                    return Err(bad(raw, "function name must match [a-zA-Z_][a-zA-Z0-9_-]*"));
                }
                if *name == "main" {
                    return Err(bad(raw, "`main` is reserved and cannot be a function name"));
                }
                Ok(NamepathRef::Function {
                    rig: (*rig).to_string(),
                    module_path: (*module_path).to_string(),
                    name: (*name).to_string(),
                })
            }
            _ => Err(bad(
                raw,
                "too many `:` segments; the most specific form is rig:module/path:function",
            )),
        }
    }
}

fn check_rig(rig: &str) -> Result<(), &'static str> {
    if is_valid_rig(rig) {
        Ok(())
    } else {
        Err("rig must be the compound `<author>/<name>` (e.g. `sourcetrait/grammar`)")
    }
}

fn check_module_path(module_path: &str) -> Result<(), &'static str> {
    if module_path.is_empty() {
        Err("module path must not be empty")
    } else if is_valid_module_path(module_path) {
        Ok(())
    } else {
        Err("module path must be slash-separated identifiers (no leading/trailing/double slash)")
    }
}

impl NamepathPattern {
    pub(crate) fn parse(raw: &str) -> Result<Self, GrammarMcpError> {
        fn bad(
            raw: &str,
            reason: &str,
        ) -> GrammarMcpError {
            GrammarMcpError::NamepathInvalid {
                namepath: raw.to_string(),
                reason: reason.to_string(),
            }
        }
        if raw == PATTERN_ALL {
            return Ok(Self::All);
        }
        if raw == PATTERN_CURRENT {
            return Ok(Self::Current);
        }
        let descends = raw.ends_with('/');
        let Some(body) = raw.strip_suffix('/').or_else(|| raw.strip_suffix(':')) else {
            return Err(bad(
                raw,
                "not a pattern; a pattern ends in `/` or `:`, or is `*` or `.`",
            ));
        };
        let parts: Vec<&str> = body.split(':').collect();
        match parts.as_slice() {
            [single] if descends => {
                if single.contains('/') {
                    return Err(bad(raw, "after a rig the separator is `:`, not `/`"));
                }
                if !is_valid_ident(single) || is_reserved_term(single) {
                    return Err(bad(
                        raw,
                        "author must match [a-zA-Z_][a-zA-Z0-9_-]* and not be the reserved `main`",
                    ));
                }
                Ok(Self::Author {
                    author: (*single).to_string(),
                })
            }
            [rig] => {
                check_rig(rig).map_err(|e| bad(raw, e))?;
                Ok(Self::Rig {
                    rig: (*rig).to_string(),
                })
            }
            [rig, module_path] => {
                check_rig(rig).map_err(|e| bad(raw, e))?;
                check_module_path(module_path).map_err(|e| bad(raw, e))?;
                let rig = (*rig).to_string();
                let module_path = (*module_path).to_string();
                Ok(if descends {
                    Self::ModuleTree {
                        rig,
                        module_path,
                    }
                } else {
                    Self::ModuleCalls {
                        rig,
                        module_path,
                    }
                })
            }
            _ => Err(bad(
                raw,
                "too many `:` segments; the deepest pattern is rig:module/path:",
            )),
        }
    }

    /// Does `namepath` fall within this pattern?
    pub(crate) fn matches(
        &self,
        namepath: &NamepathRef,
    ) -> bool {
        match self {
            Self::All => true,
            Self::Current => false,
            Self::Author { author } => namepath
                .rig()
                .split_once('/')
                .is_some_and(|(their_author, _)| their_author == author),
            Self::Rig { rig } => namepath.rig() == rig,
            Self::ModuleTree {
                rig,
                module_path,
            } => {
                namepath.rig() == rig
                    && match namepath {
                        NamepathRef::Rig { .. } => false,
                        NamepathRef::Module {
                            module_path: path, ..
                        }
                        | NamepathRef::Function {
                            module_path: path, ..
                        } => at_or_below(path, module_path),
                    }
            }
            Self::ModuleCalls {
                rig,
                module_path,
            } => matches!(
                namepath,
                NamepathRef::Function {
                    rig: their_rig,
                    module_path: path,
                    ..
                } if their_rig == rig && path == module_path
            ),
        }
    }
}

/// Is `path` the module `base`, or one below it?
fn at_or_below(
    path: &str,
    base: &str,
) -> bool {
    path == base
        || path
            .strip_prefix(base)
            .is_some_and(|rest| rest.starts_with('/'))
}
