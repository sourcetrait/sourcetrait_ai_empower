use crate::*;

#[test]
fn strips_style_block() {
    let src = "# Title\n\n<style>\n.x { color: red; }\n</style>\n\nBody.\n";
    let out = transform_markdown(src);
    assert!(!out.contains("<style>"));
    assert!(!out.contains("color: red"));
    assert!(out.contains("# Title"));
    assert!(out.contains("Body."));
}

#[test]
fn unwraps_div_to_paragraph() {
    let src = "hello\n\n<div class=\"note\">\ninside\n</div>\n\nbye\n";
    let out = transform_markdown(src);
    assert!(!out.contains("<div"));
    assert!(!out.contains("</div>"));
    // `inside` sits as its own paragraph between the neighbours.
    assert!(out.contains("\n\ninside\n\n"));
    assert!(out.starts_with("hello"));
    assert!(out.trim_end().ends_with("bye"));
}

#[test]
fn keeps_table_bare() {
    let src = "<table class=\"t\">\n<tr><td style=\"x\">a</td></tr>\n</table>\n";
    let out = transform_markdown(src);
    assert!(out.contains("<table>"));
    assert!(out.contains("<td>a</td>"));
    assert!(!out.contains("class="));
    assert!(!out.contains("style="));
}

#[test]
fn leaves_html_in_code_fence() {
    let src = "```html\n<div class=\"x\">y</div>\n```\n";
    let out = transform_markdown(src);
    assert!(out.contains("<div class=\"x\">y</div>"));
}

#[test]
fn strips_inline_span_keeping_text() {
    let src = "a <span class=\"k\">b</span> c\n";
    let out = transform_markdown(src);
    assert!(!out.contains("<span"));
    assert!(out.contains('b'));
    assert!(out.contains('a'));
    assert!(out.contains('c'));
}
