//! Unit tests for `crate::server::namepath`.

use crate::*;

fn validate(s: &str) -> Result<NamepathRef, Error> {
    Namepath(s.to_string()).validate()
}

#[test]
fn library_only() {
    assert_eq!(
        validate("calc").unwrap(),
        NamepathRef::Library {
            library: "calc".into()
        }
    );
}

#[test]
fn module_one_and_nested() {
    assert_eq!(
        validate("calc:math").unwrap(),
        NamepathRef::Module {
            library: "calc".into(),
            module_path: "math".into()
        }
    );
    assert_eq!(
        validate("calc:math/trig").unwrap(),
        NamepathRef::Module {
            library: "calc".into(),
            module_path: "math/trig".into()
        }
    );
}

#[test]
fn function_one_and_nested() {
    assert_eq!(
        validate("calc:math:double").unwrap(),
        NamepathRef::Function {
            library: "calc".into(),
            module_path: "math".into(),
            name: "double".into()
        }
    );
    assert_eq!(
        validate("calc:math/trig:sin").unwrap(),
        NamepathRef::Function {
            library: "calc".into(),
            module_path: "math/trig".into(),
            name: "sin".into()
        }
    );
}

#[test]
fn deny_empty() {
    assert!(validate("").is_err());
}

#[test]
fn deny_root_function() {
    // `library::function` - the empty-module form is the banned root function
    assert!(validate("calc::double").is_err());
}

#[test]
fn deny_leading_colon() {
    assert!(validate(":math:double").is_err());
    assert!(validate(":calc").is_err());
}

#[test]
fn deny_trailing_colon() {
    assert!(validate("calc:math:").is_err());
    assert!(validate("calc:").is_err());
}

#[test]
fn deny_too_many_segments() {
    assert!(validate("calc:math:double:extra").is_err());
}

#[test]
fn deny_main_function_name() {
    assert!(validate("calc:math:main").is_err());
}

#[test]
fn deny_bad_module_path() {
    assert!(validate("calc:a//b:f").is_err());
    assert!(validate("calc:a/:f").is_err());
    assert!(validate("calc:/b:f").is_err());
}

#[test]
fn deny_bad_idents() {
    assert!(validate("1calc:math:double").is_err());
    assert!(validate("calc:math:1double").is_err());
}
