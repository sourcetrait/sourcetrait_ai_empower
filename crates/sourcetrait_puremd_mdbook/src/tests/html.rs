use crate::*;

#[test]
fn drops_style_subtree() {
    assert_eq!(clean_html("<style>.x{}</style>"), "");
}

#[test]
fn drops_comment() {
    assert_eq!(clean_html("<!-- hi -->"), "");
}

#[test]
fn keeps_table_tags_bare() {
    assert_eq!(
        clean_html("<table class=\"t\"><tr><td>a</td></tr></table>"),
        "<table><tr><td>a</td></tr></table>",
    );
}

#[test]
fn unwraps_div_keeping_text() {
    assert_eq!(clean_html("<div class=\"x\">hi</div>"), "hi");
}

#[test]
fn strips_inline_and_void_tags() {
    assert_eq!(clean_html("<span>x</span>"), "x");
    assert_eq!(clean_html("<br>"), "");
    assert_eq!(clean_html("<br/>"), "");
}
