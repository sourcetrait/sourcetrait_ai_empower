
use crate::*;

fn validate(s: &str) -> Result<NamepathRef, GrammarMcpError> {
    Namepath(s.to_string()).validate()
}

#[test]
fn rig_only() {
    assert_eq!(
        validate("sourcetrait/calc").unwrap(),
        NamepathRef::Rig {
            rig: "sourcetrait/calc".into()
        }
    );
}

#[test]
fn module_one_and_nested() {
    assert_eq!(
        validate("sourcetrait/calc:math").unwrap(),
        NamepathRef::Module {
            rig: "sourcetrait/calc".into(),
            module_path: "math".into()
        }
    );
    assert_eq!(
        validate("sourcetrait/calc:math/trig").unwrap(),
        NamepathRef::Module {
            rig: "sourcetrait/calc".into(),
            module_path: "math/trig".into()
        }
    );
}

#[test]
fn function_one_and_nested() {
    assert_eq!(
        validate("sourcetrait/calc:math:double").unwrap(),
        NamepathRef::Function {
            rig: "sourcetrait/calc".into(),
            module_path: "math".into(),
            name: "double".into()
        }
    );
    assert_eq!(
        validate("sourcetrait/calc:math/trig:sin").unwrap(),
        NamepathRef::Function {
            rig: "sourcetrait/calc".into(),
            module_path: "math/trig".into(),
            name: "sin".into()
        }
    );
}

#[test]
fn deny_bare_rig() {
    assert!(validate("calc").is_err());
    assert!(validate("calc:math").is_err());
    assert!(validate("calc:math:double").is_err());
    assert!(validate("a/b/c:math:double").is_err());
}

#[test]
fn deny_empty() {
    assert!(validate("").is_err());
}

#[test]
fn deny_root_function() {
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
    assert!(validate("1author/calc:math:double").is_err());
}

fn classify(s: &str) -> NamepathStr {
    NamepathStr::parse(s).unwrap_or_else(|e| panic!("classify {s:?}: {e:?}"))
}

fn pat(s: &str) -> NamepathPattern {
    match classify(s) {
        NamepathStr::Pattern(p) => p,
        NamepathStr::Namepath(n) => {
            panic!("{s:?} classified exact ({}), expected a pattern", n.0)
        }
    }
}

fn lib_ref(rig: &str) -> NamepathRef {
    NamepathRef::Rig {
        rig: rig.into(),
    }
}

fn mod_ref(
    rig: &str,
    module_path: &str,
) -> NamepathRef {
    NamepathRef::Module {
        rig: rig.into(),
        module_path: module_path.into(),
    }
}

fn call_ref(
    rig: &str,
    module_path: &str,
    name: &str,
) -> NamepathRef {
    NamepathRef::Function {
        rig: rig.into(),
        module_path: module_path.into(),
        name: name.into(),
    }
}

#[test]
fn exact_shapes_classify_as_namepaths() {
    for s in [
        "sourcetrait",
        "sourcetrait/calc",
        "sourcetrait/calc:math",
        "sourcetrait/calc:math/trig",
        "sourcetrait/calc:math:double",
    ] {
        assert!(
            matches!(classify(s), NamepathStr::Namepath(_)),
            "{s:?} should classify exact",
        );
    }
}

#[test]
fn a_bare_author_is_exact_in_shape_and_still_invalid() {
    // Classification is by SHAPE; validity is a separate, unchanged question. An
    // author names nothing addressable, so it classifies exact and errors -
    // it does NOT become a pattern and does not gain a descriptor.
    let NamepathStr::Namepath(np) = classify("sourcetrait") else {
        panic!("a bare author must classify as exact, not as a pattern");
    };
    assert!(
        np.validate().is_err(),
        "an author is exact in shape but is not a valid exact namepath",
    );
}

#[test]
fn trailing_hierarchy_characters_classify_as_patterns() {
    for s in [
        "sourcetrait/",
        "sourcetrait/calc:",
        "sourcetrait/calc:math/",
        "sourcetrait/calc:math:",
        "*",
        ".",
    ] {
        assert!(
            matches!(classify(s), NamepathStr::Pattern(_)),
            "{s:?} should classify as a pattern",
        );
    }
}

#[test]
fn each_pattern_form_parses_to_its_variant() {
    assert_eq!(
        pat("sourcetrait/"),
        NamepathPattern::Author {
            author: "sourcetrait".into(),
        },
    );
    assert_eq!(
        pat("sourcetrait/calc:"),
        NamepathPattern::Rig {
            rig: "sourcetrait/calc".into(),
        },
    );
    assert_eq!(
        pat("sourcetrait/calc:math/"),
        NamepathPattern::ModuleTree {
            rig: "sourcetrait/calc".into(),
            module_path: "math".into(),
        },
    );
    assert_eq!(
        pat("sourcetrait/calc:math:"),
        NamepathPattern::ModuleCalls {
            rig: "sourcetrait/calc".into(),
            module_path: "math".into(),
        },
    );
    assert_eq!(pat("*"), NamepathPattern::All);
    assert_eq!(pat("."), NamepathPattern::Current);
}

#[test]
fn deny_invalid_pattern_bodies() {
    for s in [
        "sourcetrait/calc/",              // past a rig the separator is `:`
        "1author/",                       // bad ident
        "main/",                          // reserved
        "/",                              // empty author
        ":",                              // empty rig
        "calc:",                          // bare rig, no author
        "sourcetrait/calc:a//b:",         // bad module path
        "sourcetrait/calc:math:double:",  // there is no level below a call
    ] {
        assert!(NamepathStr::parse(s).is_err(), "{s:?} should be rejected");
    }
}

#[test]
fn author_matches_on_the_whole_segment() {
    let p = pat("sourcetrait/");
    assert!(p.matches(&lib_ref("sourcetrait/calc")));
    assert!(p.matches(&mod_ref("sourcetrait/calc", "math")));
    assert!(p.matches(&call_ref("sourcetrait/calc", "math", "double")));
    assert!(
        !p.matches(&lib_ref("sourcetraitx/calc")),
        "a prefix is not an author",
    );
    assert!(!p.matches(&lib_ref("bob/calc")));
}

#[test]
fn rig_covers_everything_in_it_and_nothing_outside() {
    let p = pat("sourcetrait/calc:");
    assert!(p.matches(&lib_ref("sourcetrait/calc")));
    assert!(p.matches(&mod_ref("sourcetrait/calc", "math")));
    assert!(p.matches(&call_ref("sourcetrait/calc", "math/trig", "sin")));
    assert!(!p.matches(&lib_ref("sourcetrait/other")));
    assert!(!p.matches(&call_ref("sourcetrait/other", "math", "sin")));
}

#[test]
fn module_tree_compares_on_segment_boundaries() {
    let p = pat("sourcetrait/calc:math/");
    assert!(
        p.matches(&mod_ref("sourcetrait/calc", "math")),
        "the module itself is at the root of its own subtree",
    );
    assert!(p.matches(&mod_ref("sourcetrait/calc", "math/trig")));
    assert!(p.matches(&call_ref("sourcetrait/calc", "math/trig", "sin")));
    assert!(
        !p.matches(&mod_ref("sourcetrait/calc", "mathematics")),
        "`math` must not cover `mathematics` - a prefix is not a parent",
    );
    assert!(
        !p.matches(&lib_ref("sourcetrait/calc")),
        "the rig sits ABOVE the module, so it is not below the pattern",
    );
}

#[test]
fn module_calls_selects_only_direct_calls() {
    let p = pat("sourcetrait/calc:math:");
    assert!(p.matches(&call_ref("sourcetrait/calc", "math", "double")));
    assert!(
        !p.matches(&mod_ref("sourcetrait/calc", "math")),
        "a module is not a call",
    );
    assert!(
        !p.matches(&call_ref("sourcetrait/calc", "math/trig", "sin")),
        "`:` selects the call level, it does not descend",
    );
    assert!(!p.matches(&call_ref("sourcetrait/other", "math", "double")));
}

#[test]
fn the_two_module_forms_differ_by_separator() {
    // `/` DESCENDS the module tree; `:` selects the CALL level - the same thing
    // each separator already means in an exact namepath.
    let tree = pat("sourcetrait/calc:math/");
    let calls = pat("sourcetrait/calc:math:");
    let submodule = mod_ref("sourcetrait/calc", "math/trig");
    assert!(tree.matches(&submodule));
    assert!(!calls.matches(&submodule));
}

#[test]
fn all_matches_every_kind() {
    let p = pat("*");
    assert!(p.matches(&lib_ref("bob/burgers")));
    assert!(p.matches(&mod_ref("bob/burgers", "fries")));
    assert!(p.matches(&call_ref("bob/burgers", "fries", "cook")));
}

#[test]
fn an_unresolved_current_matches_nothing() {
    // `.` is a STUB until purview resolves it to that purview's own patterns.
    // Matching NOTHING is the safe unresolved reading - matching everything
    // would silently expose the whole namespace wherever a `.` was left unresolved.
    let p = pat(".");
    assert!(!p.matches(&lib_ref("bob/burgers")));
    assert!(!p.matches(&mod_ref("bob/burgers", "fries")));
    assert!(!p.matches(&call_ref("bob/burgers", "fries", "cook")));
}
