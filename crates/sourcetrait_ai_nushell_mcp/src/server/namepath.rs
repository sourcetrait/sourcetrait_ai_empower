use crate::*;

/// What: the agent-facing addressing string for a library coordinate -
/// `library`, `library:module/path`, or `library:module/path:function`, split
/// on `:`. A newtype over the raw string; `validate()` parses + checks it into
/// the structured `NamepathRef`.
///
/// Why: namepath is sugar over the structured (library, module_path, name)
/// coordinate the call / inspect / new tools take. The internal model stays
/// structured - this is purely the boundary parse, so a malformed namepath
/// fails with a typed `Error::NamepathInvalid` before any dispatch.
///
/// Where: built from the `namepath` param in `tool/call.rs` + `tool/inspect.rs`
/// and from each entry of `tool/new.rs`'s `namepaths` list.
pub(crate) struct Namepath(pub String);

/// The structured coordinate a valid namepath resolves to. Variant arity
/// mirrors the `:`-segment count: 1 = library, 2 = module, 3 = function. A
/// `Function` always carries a non-empty `module_path` - there are no root
/// functions (`library::function` is rejected).
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

impl Namepath {
    /// What: parse + validate the raw namepath into a `NamepathRef`. Splits on
    /// `:` into 1 / 2 / 3 parts (library / module / function); each segment is
    /// checked with the same identifier + module-path rules the library
    /// substrate uses. Returns `Error::NamepathInvalid { namepath, reason }` on
    /// any malformed form.
    ///
    /// Why: one classify-and-check boundary (mirroring the schema converter's
    /// one-error-per-denial discipline) keeps namepath validity in a single
    /// place; the tool handlers then match the returned variant for their
    /// required arity. Denials: empty; leading/trailing `:`; `::` (the banned
    /// root-function form); more than two `:`; an invalid segment; and `main`
    /// as a function name (the reserved call-target sentinel).
    ///
    /// Where: called by `tool/call.rs` (requires `Function`), `tool/inspect.rs`
    /// (any arity), and `tool/new.rs` (requires `Module` or `Function`).
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

/// A library segment must be a valid identifier (and not the reserved `mod`).
fn check_library(library: &str) -> Result<(), &'static str> {
    if is_valid_ident(library) {
        Ok(())
    } else {
        Err("library must match [a-zA-Z_][a-zA-Z0-9_-]* and not be `mod`")
    }
}

/// A module-path segment must be non-empty and a slash-separated chain of
/// valid identifiers (no leading / trailing / double slash).
fn check_module_path(module_path: &str) -> Result<(), &'static str> {
    if module_path.is_empty() {
        Err("module path must not be empty")
    } else if is_valid_module_path(module_path) {
        Ok(())
    } else {
        Err("module path must be slash-separated identifiers (no leading/trailing/double slash)")
    }
}
