/// Crate-internal base62 alphabet (`[0-9a-zA-Z]`). Shared by `Nonce` and
/// `RerunHash` so their string forms are interchangeable as filename
/// fragments / URL components.
pub(crate) const ALPHABET: &[u8; 62] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Format a u64 into a Formatter using the shared base62 alphabet.
/// Zero renders as `"0"`; otherwise as the minimal digit sequence
/// (most significant first). A u64's worst case is 11 ASCII chars.
pub(crate) fn fmt_base62(
    mut n: u64,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    if n == 0 {
        return f.write_str("0");
    }
    let mut buf = [0u8; 11];
    let mut i = 0;
    while n > 0 {
        buf[i] = ALPHABET[(n % 62) as usize];
        n /= 62;
        i += 1;
    }
    buf[..i].reverse();
    let s = std::str::from_utf8(&buf[..i]).expect("ASCII");
    f.write_str(s)
}

/// Predicate: is the string a valid base62 token (non-empty, only
/// `[0-9a-zA-Z]`)? Used by callers that need to validate untrusted
/// rerun_id strings before joining them onto a filesystem path
/// (rejects `/`, `..`, and friends).
pub fn is_base62(s: &str) -> bool {
    !s.is_empty()
        && s.as_bytes()
            .iter()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z'))
}
