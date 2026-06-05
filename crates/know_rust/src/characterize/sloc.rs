#[allow(unused_imports)]
use crate::*;

/// What: remove inline `#[cfg(test)] <item>` regions from the given
/// rust source text via brace-matched item-body walk. Single-line
/// items terminated by `;` are stripped to the semicolon.
///
/// Why: characterize.py the_user 2026-06-04 directive: tests are
/// meaningless to know_rust; their bodies must not pollute the
/// SLOC count + the python rustscan would have emitted facts for
/// items inside them. Mirrors `_strip_cfg_test` byte-for-byte
/// (allowed pragmatic limitations: ignores string/comment context;
/// only handles bare `#[cfg(test)]`, not `cfg_attr(test, ...)`).
///
/// Where: called from `compute_sloc` before the comment-strip +
/// blank-line filter.
pub fn strip_cfg_test(src: &str) -> String {
    let needle = "#[cfg(test)]";
    let bytes = src.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    loop {
        let idx_opt = find_substr(src, i, needle);
        let Some(idx) = idx_opt else {
            out.push_str(&src[i..]);
            break;
        };
        out.push_str(&src[i..idx]);
        let mut j = idx + needle.len();
        loop {
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r') {
                j += 1;
            }
            if j + 1 < bytes.len() && bytes[j] == b'#' && bytes[j + 1] == b'[' {
                let mut depth = 0i32;
                let mut k = j;
                while k < bytes.len() {
                    if bytes[k] == b'[' {
                        depth += 1;
                    } else if bytes[k] == b']' {
                        depth -= 1;
                        if depth == 0 {
                            k += 1;
                            break;
                        }
                    }
                    k += 1;
                }
                j = k;
                continue;
            }
            break;
        }
        let brace = find_byte(bytes, j, b'{');
        let semi = find_byte(bytes, j, b';');
        if brace.is_none() && semi.is_none() {
            break;
        }
        match (brace, semi) {
            (Some(_), Some(s)) if brace.unwrap() > s => {
                i = s + 1;
                continue;
            }
            (None, Some(s)) => {
                i = s + 1;
                continue;
            }
            (Some(b), _) => {
                let mut depth = 0i32;
                let mut k = b;
                while k < bytes.len() {
                    match bytes[k] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                k += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                i = k;
            }
            _ => break,
        }
    }
    out
}

fn find_substr(haystack: &str, from: usize, needle: &str) -> Option<usize> {
    haystack[from..].find(needle).map(|n| from + n)
}

fn find_byte(bytes: &[u8], from: usize, b: u8) -> Option<usize> {
    bytes[from..].iter().position(|&x| x == b).map(|n| from + n)
}

/// What: count source lines of code in the supplied rust source
/// after stripping block comments + line/doc comments + blank lines.
///
/// Why: characterize.py `_compute_sloc` is what feeds the per-crate
/// SLOC + workspace-wide total that drives emit's top-N cap formula
/// (max(floor, round(floor + multiplier * log2(sloc / divisor)))).
/// Pragmatic: does not honor `//` inside string literals (rare).
///
/// Where: called from `crate::characterize::scan_crate::scan_crate`
/// per source file after `strip_cfg_test` removes test blocks.
pub fn compute_sloc(src: &str) -> usize {
    let s = strip_block_comments(src);
    let s = strip_line_comments(&s);
    s.lines().filter(|l| !l.trim().is_empty()).count()
}

fn strip_block_comments(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let mut j = i + 2;
            while j + 1 < bytes.len() {
                if bytes[j] == b'*' && bytes[j + 1] == b'/' {
                    j += 2;
                    break;
                }
                j += 1;
            }
            if j + 1 >= bytes.len() && !(bytes.len() >= 2 && bytes[bytes.len() - 2] == b'*' && bytes[bytes.len() - 1] == b'/') {
                j = bytes.len();
            }
            i = j;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn strip_line_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.split_inclusive('\n') {
        let stripped = match line.find("//") {
            Some(idx) => {
                let trailing_newline = line.ends_with('\n');
                let mut kept = line[..idx].to_string();
                if trailing_newline {
                    kept.push('\n');
                }
                kept
            }
            None => line.to_string(),
        };
        out.push_str(&stripped);
    }
    out
}
