// Basic HTML elements kept in the output (bare - every attribute stripped),
// for idiomatic table usage. Markdown's own tables are weaker, so tables are
// the one HTML shape worth keeping.
pub(crate) const TABLE_TAGS: &[&str] = &[
    "table", "thead", "tbody", "tfoot", "tr", "th", "td", "caption",
];

// Elements whose entire subtree (tag AND content) is dropped: the content is
// not prose a reader wants (css, scripts).
pub(crate) const DROP_SUBTREE_TAGS: &[&str] = &["style", "script"];

// Clean one run of raw HTML per the puremd rules, returning the replacement:
// - a table tag survives, stripped to its bare `<tag>` / `</tag>` form
// - `style` / `script` and comments are dropped whole (tag and content)
// - every other tag is unwrapped: the tag is removed, its inner text kept
pub(crate) fn clean_html(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;

    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        rest = &rest[lt..];

        // An HTML comment: drop through its terminator.
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(end) => rest = &rest[end + 3..],
                None => rest = "",
            }
            continue;
        }

        let Some(gt) = rest.find('>') else {
            // A stray `<` with no closing `>`: keep it as literal text.
            out.push_str(rest);
            return out;
        };
        let tag = &rest[..=gt];
        let after = &rest[gt + 1..];
        let name = tag_name(tag);

        if DROP_SUBTREE_TAGS.contains(&name.as_str()) && !is_closing(tag) {
            // Skip everything up to and including the matching close tag.
            let close = format!("</{name}");
            match find_ci(after, &close) {
                Some(pos) => {
                    let tail = &after[pos..];
                    rest = match tail.find('>') {
                        Some(g) => &tail[g + 1..],
                        None => "",
                    };
                }
                None => rest = "",
            }
            continue;
        }

        if TABLE_TAGS.contains(&name.as_str()) {
            if is_closing(tag) {
                out.push_str(&format!("</{name}>"));
            } else {
                out.push_str(&format!("<{name}>"));
            }
        }
        // Any other tag is unwrapped: emit nothing, keep the surrounding text.

        rest = after;
    }

    out.push_str(rest);
    out
}

// Whether a tag token is a closing tag (`</div>`).
fn is_closing(tag: &str) -> bool {
    tag.starts_with("</")
}

// The lowercased element name from a tag token (`<div class=..>` -> `div`,
// `</div>` -> `div`, `<br/>` -> `br`).
fn tag_name(tag: &str) -> String {
    let body = tag
        .trim_start_matches('<')
        .trim_start_matches('/')
        .trim_end_matches('>')
        .trim_end_matches('/');
    let end = body
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(body.len());
    body[..end].to_ascii_lowercase()
}

// Case-insensitive search for an ASCII-lowercased needle. ASCII lowercasing
// preserves byte length, so the returned index is valid in the original too.
fn find_ci(haystack: &str, needle_lower: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(needle_lower)
}
