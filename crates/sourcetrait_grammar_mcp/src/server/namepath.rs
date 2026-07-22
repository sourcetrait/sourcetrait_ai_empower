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

/// The current-purview pattern. Parsed here so the grammar is complete;
/// RESOLVED by purview, which is the only thing that knows what it names.
pub(crate) const PATTERN_CURRENT: &str = ".";

/// A raw namepath string classified by SHAPE: a trailing `/` or `:`, or the
/// bare `*` / `.`, is a PATTERN; anything else is an exact namepath.
///
/// Classification is NOT validation, and the distinction is the whole point of
/// the split. A bare author (`sourcetrait`) is exact in shape but names nothing
/// addressable, so it classifies as `Namepath` here and then fails
/// `validate` exactly as it does today - the exact parser's strictness is
/// unchanged by the pattern arm existing.
pub(crate) enum NamepathStr {
    Namepath(Namepath),
    Pattern(NamepathPattern),
}

/// A set of namepaths, addressed by the trailing hierarchy character.
///
/// `/` DESCENDS the tree and `:` selects the level BELOW - which is what each
/// separator already means in an exact namepath, so the two module forms differ:
/// `lib:mod/` is that module's whole subtree, `lib:mod:` is only its calls.
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
    /// `.` - the current purview. It must be RESOLVED to that purview's own
    /// namepath patterns before matching (`inspect()` does this); an unresolved
    /// `.` therefore matches NOTHING rather than silently matching everything.
    Current,
}

impl NamepathStr {
    pub(crate) fn parse(raw: &str) -> Result<Self, Error> {
        if is_pattern_shaped(raw) {
            Ok(Self::Pattern(NamepathPattern::parse(raw)?))
        } else {
            Ok(Self::Namepath(Namepath(raw.to_string())))
        }
    }
}

impl NamepathStr {
    /// Does this put `namepath` in view?
    ///
    /// A PATTERN covers a SET; an EXACT namepath covers only itself. Purview
    /// values are allowed to be either - a whole author, or one specific call -
    /// so answering the same question for both is what lets a purview hold a
    /// mixed list without ever branching on which kind it got.
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
    pub(crate) fn validate(&self) -> Result<NamepathRef, Error> {
        fn bad(raw: &str, reason: &str) -> Error {
            Error::NamepathInvalid {
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
    pub(crate) fn parse(raw: &str) -> Result<Self, Error> {
        fn bad(
            raw: &str,
            reason: &str,
        ) -> Error {
            Error::NamepathInvalid {
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
            // `<author>/`. A `/` after the RIG is the wrong separator -
            // everything past a rig is reached with `:`.
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
    ///
    /// The per-SIGNATURE primitive: one namepath, asked whether this pattern
    /// covers it. The renderer walks the index putting exactly that question to
    /// every signature, and purview filtering is the same question over a SET of
    /// namepath patterns - `NamepathStr::covers` wraps it for both, so there is
    /// one answer and not two.
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
///
/// Compared on SEGMENT boundaries, so `a` covers `a/b` but never `ab`.
fn at_or_below(
    path: &str,
    base: &str,
) -> bool {
    path == base
        || path
            .strip_prefix(base)
            .is_some_and(|rest| rest.starts_with('/'))
}
