use crate::*;

// Apply the puremd HTML touch-ups to a chapter's markdown, returning cleaned
// pure markdown. HTML is located via a CommonMark parse - so HTML inside code
// fences is never touched - then edited in place on the source text.
pub fn transform_markdown(src: &str) -> String {
    let options = pd::Options::ENABLE_TABLES
        | pd::Options::ENABLE_FOOTNOTES
        | pd::Options::ENABLE_STRIKETHROUGH
        | pd::Options::ENABLE_TASKLISTS
        | pd::Options::ENABLE_HEADING_ATTRIBUTES;

    // pulldown emits a block of HTML one event PER LINE, so a multi-line element
    // (`<style>..</style>`, `<div>..</div>`) arrives as several contiguous Html
    // events. Coalesce a run of contiguous block-HTML events into one string
    // before cleaning, so `style`/`script` subtrees and an unwrapped block's
    // outer tags are seen whole. Inline HTML is handled per event.
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut block: Option<(usize, usize, String)> = None;
    for (event, range) in pd::Parser::new_ext(src, options).into_offset_iter() {
        match event {
            pd::Event::Html(html) => {
                if let Some((_, end, text)) = &mut block {
                    if *end == range.start {
                        text.push_str(&html);
                        *end = range.end;
                        continue;
                    }
                }
                flush_block(&mut block, &mut edits);
                block = Some((range.start, range.end, html.to_string()));
            }
            pd::Event::InlineHtml(html) => {
                flush_block(&mut block, &mut edits);
                // Inline HTML: strip the tag in place, keeping surrounding text.
                edits.push((range, clean_html(&html)));
            }
            _ => flush_block(&mut block, &mut edits),
        }
    }
    flush_block(&mut block, &mut edits);

    // Splice from the end so earlier byte ranges stay valid.
    edits.sort_by(|a, b| b.0.start.cmp(&a.0.start));
    let mut out = src.to_string();
    for (range, replacement) in edits {
        out.replace_range(range, &replacement);
    }

    collapse_blank_lines(&out)
}

// Emit the accumulated block-HTML run (if any) as one edit: unwrapped to its own
// paragraph, or dropped when it cleans to nothing.
fn flush_block(
    block: &mut Option<(usize, usize, String)>,
    edits: &mut Vec<(Range<usize>, String)>,
) {
    if let Some((start, end, text)) = block.take() {
        let cleaned = clean_html(&text);
        let replacement = if cleaned.trim().is_empty() {
            "\n\n".to_string()
        } else {
            format!("\n\n{}\n\n", cleaned.trim())
        };
        edits.push((start..end, replacement));
    }
}

// Collapse runs of blank lines outside fenced code to a single blank line, and
// trim leading and trailing blank lines. Blank lines inside code fences stay.
fn collapse_blank_lines(text: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut in_fence = false;
    let mut prev_blank = false;

    for line in text.lines() {
        let trimmed = line.trim();
        let opens =
            trimmed.starts_with("```") || trimmed.starts_with("~~~");
        let inline_span = opens
            && trimmed.len() > 3
            && (trimmed.ends_with("```") || trimmed.ends_with("~~~"));
        if opens && !inline_span {
            in_fence = !in_fence;
            kept.push(line);
            prev_blank = false;
            continue;
        }

        let blank = !in_fence && trimmed.is_empty();
        if blank && prev_blank {
            continue;
        }
        kept.push(line);
        prev_blank = blank;
    }

    while kept.first().is_some_and(|l| l.trim().is_empty()) {
        kept.remove(0);
    }
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }

    let mut result = kept.join("\n");
    result.push('\n');
    result
}
