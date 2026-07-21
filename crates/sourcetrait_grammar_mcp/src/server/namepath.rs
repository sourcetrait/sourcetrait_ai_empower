use crate::*;

pub(crate) struct Namepath(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NamepathRef {
    Library {
        library: String,
    },
    Module {
        library: String,
        module_path: String,
    },
    Function {
        library: String,
        module_path: String,
        name: String,
    },
}

/// The whole-store pattern.
pub(crate) const PATTERN_ALL: &str = "*";

/// The current-purview pattern. Parsed here so the grammar is complete;
/// RESOLVED by purview, which is the only thing that knows what it names.
pub(crate) const PATTERN_CURRENT: &str = ".";

/// A raw namepath string classified by SHAPE: a trailing `/` or `:`, or the
/// bare `*` / `.`, is a PATTERN; anything else is an exact namepath.
///
/// Classification is NOT validation, and the distinction is the whole point of
/// the split. A bare author (`sourcetrait`) is exact in shape but names no
/// addressable coordinate, so it classifies as `Namepath` here and then fails
/// `validate` exactly as it does today - the exact parser's strictness is
/// unchanged by the pattern arm existing.
pub(crate) enum NamepathStr {
    Namepath(Namepath),
    Pattern(NamepathPattern),
}

/// A set of coordinates, addressed by the trailing hierarchy character.
///
/// `/` DESCENDS the tree and `:` selects the level BELOW - which is what each
/// separator already means in an exact namepath, so the two module forms differ:
/// `lib:mod/` is that module's whole subtree, `lib:mod:` is only its calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NamepathPattern {
    /// `<author>/` - everything that author published.
    Author { author: String },
    /// `<author>/<name>:` - everything in that library.
    Library { library: String },
    /// `<author>/<name>:<module_path>/` - that module and everything below it.
    ModuleTree {
        library: String,
        module_path: String,
    },
    /// `<author>/<name>:<module_path>:` - the calls in that module, no deeper.
    ModuleCalls {
        library: String,
        module_path: String,
    },
    /// `*` - the whole store.
    All,
    /// `.` - the current purview, a STUB until purview lands. It must be
    /// RESOLVED to that purview's own patterns before matching; an unresolved
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

fn is_pattern_shaped(raw: &str) -> bool {
    raw == PATTERN_ALL || raw == PATTERN_CURRENT || raw.ends_with('/') || raw.ends_with(':')
}

impl NamepathRef {
    /// The compound `<author>/<name>` every coordinate carries.
    pub(crate) fn library(&self) -> &str {
        match self {
            Self::Library { library }
            | Self::Module { library, .. }
            | Self::Function { library, .. } => library,
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
            [library] => {
                check_library(library).map_err(|e| bad(raw, e))?;
                Ok(NamepathRef::Library {
                    library: (*library).to_string(),
                })
            }
            [library, module_path] => {
                check_library(library).map_err(|e| bad(raw, e))?;
                check_module_path(module_path).map_err(|e| bad(raw, e))?;
                Ok(NamepathRef::Module {
                    library: (*library).to_string(),
                    module_path: (*module_path).to_string(),
                })
            }
            [library, module_path, name] => {
                check_library(library).map_err(|e| bad(raw, e))?;
                if module_path.is_empty() {
                    return Err(bad(
                        raw,
                        "a function needs a parent module; `library::function` (a root function) is not allowed",
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
                    library: (*library).to_string(),
                    module_path: (*module_path).to_string(),
                    name: (*name).to_string(),
                })
            }
            _ => Err(bad(
                raw,
                "too many `:` segments; the most specific form is library:module/path:function",
            )),
        }
    }
}

fn check_library(library: &str) -> Result<(), &'static str> {
    if is_valid_library(library) {
        Ok(())
    } else {
        Err("library must be the compound `<author>/<name>` (e.g. `sourcetrait/grammar`)")
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
            // `<author>/`. A `/` after the LIBRARY is the wrong separator -
            // everything past a library is reached with `:`.
            [single] if descends => {
                if single.contains('/') {
                    return Err(bad(raw, "after a library the separator is `:`, not `/`"));
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
            [library] => {
                check_library(library).map_err(|e| bad(raw, e))?;
                Ok(Self::Library {
                    library: (*library).to_string(),
                })
            }
            [library, module_path] => {
                check_library(library).map_err(|e| bad(raw, e))?;
                check_module_path(module_path).map_err(|e| bad(raw, e))?;
                let library = (*library).to_string();
                let module_path = (*module_path).to_string();
                Ok(if descends {
                    Self::ModuleTree {
                        library,
                        module_path,
                    }
                } else {
                    Self::ModuleCalls {
                        library,
                        module_path,
                    }
                })
            }
            _ => Err(bad(
                raw,
                "too many `:` segments; the deepest pattern is library:module/path:",
            )),
        }
    }

    /// Does `node` fall within this pattern?
    ///
    /// The per-NODE primitive, for a caller holding ONE coordinate and asking
    /// whether a pattern covers it. That is purview filtering's shape, and its
    /// remaining consumer.
    ///
    /// The signature renderer deliberately does NOT use it. Rendering a subtree
    /// wants the pattern's ROOT rather than a per-node predicate, because the
    /// ancestor lines above that root must still be emitted for structure even
    /// though the pattern does not match them (server/library.rs
    /// `pattern_root`). Tree-walking belongs with the index either way.
    #[allow(dead_code)] // purview is the consumer; the renderer roots instead
    pub(crate) fn matches(
        &self,
        node: &NamepathRef,
    ) -> bool {
        match self {
            Self::All => true,
            Self::Current => false,
            Self::Author { author } => node
                .library()
                .split_once('/')
                .is_some_and(|(node_author, _)| node_author == author),
            Self::Library { library } => node.library() == library,
            Self::ModuleTree {
                library,
                module_path,
            } => {
                node.library() == library
                    && match node {
                        NamepathRef::Library { .. } => false,
                        NamepathRef::Module {
                            module_path: path, ..
                        }
                        | NamepathRef::Function {
                            module_path: path, ..
                        } => at_or_below(path, module_path),
                    }
            }
            Self::ModuleCalls {
                library,
                module_path,
            } => matches!(
                node,
                NamepathRef::Function {
                    library: node_library,
                    module_path: path,
                    ..
                } if node_library == library && path == module_path
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
