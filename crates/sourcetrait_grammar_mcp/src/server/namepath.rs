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
