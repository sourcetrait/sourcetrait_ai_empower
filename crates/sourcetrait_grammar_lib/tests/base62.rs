use sourcetrait_grammar_lib::base62::{fmt_base62, is_base62};

/// Display wrapper so the integration test can exercise `fmt_base62` (which
/// writes into a `Formatter`) through `to_string()`.
struct B62(u64);

impl std::fmt::Display for B62 {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        fmt_base62(self.0, f)
    }
}

fn b62(n: u64) -> String {
    B62(n).to_string()
}

#[test]
fn fmt_base62_renders_zero_and_alphabet_boundaries() {
    assert_eq!(b62(0), "0");
    assert_eq!(b62(1), "1");
    assert_eq!(b62(9), "9");
    assert_eq!(b62(10), "a");
    assert_eq!(b62(35), "z");
    assert_eq!(b62(36), "A");
    assert_eq!(b62(61), "Z");
    assert_eq!(b62(62), "10");
}

#[test]
fn fmt_base62_output_is_always_base62() {
    for n in [0u64, 1, 61, 62, 12345, u64::MAX] {
        assert!(is_base62(&b62(n)), "rendered {n} -> non-base62 string");
    }
}

#[test]
fn is_base62_accepts_valid_tokens() {
    assert!(is_base62("0"));
    assert!(is_base62("abcXYZ09"));
    assert!(is_base62("Z"));
}

#[test]
fn is_base62_rejects_empty_and_non_alphabet() {
    assert!(!is_base62(""));
    assert!(!is_base62(".."));
    assert!(!is_base62("a/b"));
    assert!(!is_base62("has space"));
    assert!(!is_base62("under_score"));
}
