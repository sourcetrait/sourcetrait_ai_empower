use std::path::PathBuf;
use sourcetrait_lib_empower::md;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn finds_h4_headings_in_simple_fixture() {
    let matches = md::find(&fixture("simple.md"), r"^#### ")
        .expect("find should succeed");
    assert_eq!(matches.len(), 3, "expected 3 H4 headings");
    for (offset, length) in &matches {
        assert_eq!(*length, 5, "'#### ' is 5 bytes");
        assert!(*offset > 0);
    }
    // offsets are strictly increasing
    for w in matches.windows(2) {
        assert!(w[0].0 < w[1].0, "offsets must be strictly increasing");
    }
}

#[test]
fn finds_frontmatter_delimiters() {
    let matches = md::find(&fixture("simple.md"), r"^---$")
        .expect("find should succeed");
    assert_eq!(matches.len(), 2, "expected 2 frontmatter delimiters");
    assert_eq!(matches[0].0, 0, "first delimiter at file start");
    assert!(matches[1].0 > matches[0].0);
}

#[test]
fn finds_code_fence_delimiters() {
    let matches = md::find(&fixture("simple.md"), r"^```")
        .expect("find should succeed");
    assert_eq!(matches.len(), 4, "expected 4 code-fence delimiters (2 pairs)");
    assert_eq!(matches.len() % 2, 0, "fences should pair up");
}

#[test]
fn returns_empty_when_no_matches() {
    let matches = md::find(&fixture("simple.md"), r"^xyzzy_no_match$")
        .expect("find should succeed");
    assert!(matches.is_empty(), "expected zero matches");
}

#[test]
fn invalid_pattern_surfaces_invalid_pattern_error() {
    let result = md::find(&fixture("simple.md"), r"[unclosed");
    match result {
        Err(md::MarkdownError::InvalidPattern { pattern, .. }) => {
            assert_eq!(pattern, "[unclosed");
        }
        other => panic!("expected InvalidPattern, got {other:?}"),
    }
}

#[test]
fn missing_file_surfaces_read_file_error() {
    let result = md::find(&fixture("does_not_exist.md"), r".*");
    match result {
        Err(md::MarkdownError::ReadFile { path, .. }) => {
            assert!(path.ends_with("does_not_exist.md"));
        }
        other => panic!("expected ReadFile, got {other:?}"),
    }
}
