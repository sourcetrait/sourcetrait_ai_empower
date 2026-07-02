//! Unit tests for `crate::server::namepath`.

use crate::*;

fn validate(s: &str) -> Result<NamepathRef, Error> {
    Namepath(s.to_string()).validate()
}

#[test]
fn library_only() {
    assert_eq!(
        validate("sourcetrait/calc").unwrap(),
        NamepathRef::Library {
            library: "sourcetrait/calc".into()
        }
    );
}

#[test]
fn module_one_and_nested() {
    assert_eq!(
        validate("sourcetrait/calc:math").unwrap(),
        NamepathRef::Module {
            library: "sourcetrait/calc".into(),
            module_path: "math".into()
        }
    );
    assert_eq!(
        validate("sourcetrait/calc:math/trig").unwrap(),
        NamepathRef::Module {
            library: "sourcetrait/calc".into(),
            module_path: "math/trig".into()
        }
    );
}

#[test]
fn function_one_and_nested() {
    assert_eq!(
        validate("sourcetrait/calc:math:double").unwrap(),
        NamepathRef::Function {
            library: "sourcetrait/calc".into(),
            module_path: "math".into(),
            name: "double".into()
        }
    );
    assert_eq!(
        validate("sourcetrait/calc:math/trig:sin").unwrap(),
        NamepathRef::Function {
            library: "sourcetrait/calc".into(),
            module_path: "math/trig".into(),
            name: "sin".into()
        }
    );
}

#[test]
fn deny_bare_library() {
    // Hard cutover: a library must be the compound `<author>/<name>`; a bare
    // name (no slash) is rejected at every arity.
    assert!(validate("calc").is_err());
    assert!(validate("calc:math").is_err());
    assert!(validate("calc:math:double").is_err());
    // More than one slash in the library segment is rejected too.
    assert!(validate("a/b/c:math:double").is_err());
}

#[test]
fn deny_empty() {
    assert!(validate("").is_err());
}

#[test]
fn deny_root_function() {
    // `<author>/<name>::function` - the empty-module form is the banned root function
    assert!(validate("sourcetrait/calc::double").is_err());
}

#[test]
fn deny_leading_colon() {
    assert!(validate(":math:double").is_err());
    assert!(validate(":sourcetrait/calc").is_err());
}

#[test]
fn deny_trailing_colon() {
    assert!(validate("sourcetrait/calc:math:").is_err());
    assert!(validate("sourcetrait/calc:").is_err());
}

#[test]
fn deny_too_many_segments() {
    assert!(validate("sourcetrait/calc:math:double:extra").is_err());
}

#[test]
fn deny_main_function_name() {
    assert!(validate("sourcetrait/calc:math:main").is_err());
}

#[test]
fn deny_bad_module_path() {
    assert!(validate("sourcetrait/calc:a//b:f").is_err());
    assert!(validate("sourcetrait/calc:a/:f").is_err());
    assert!(validate("sourcetrait/calc:/b:f").is_err());
}

#[test]
fn deny_bad_idents() {
    assert!(validate("sourcetrait/1calc:math:double").is_err());
    assert!(validate("sourcetrait/calc:math:1double").is_err());
    // A bad author segment too.
    assert!(validate("1author/calc:math:double").is_err());
}
