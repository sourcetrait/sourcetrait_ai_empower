use crate::*;

/// What: parse S5.1..5.6 pick lists out of an `orientation.md` body.
/// Returns a PicksDict whose six lists carry the backtick-quoted
/// `kind:name` strings from the bullet lines in each sub-section.
///
/// Why: measure_overlap.py's `parse_picks` (lines 35-90). The picker
/// output is human-readable markdown; this parse extracts the
/// machine-checkable pick lists for ground-truth scoring without
/// requiring the picker to dump a parallel JSON.
///
/// Where: called by `crate::measure_overlap::score::score_target` per
/// target's orientation.md when scoring against the ground-truth list.
pub fn parse_picks(orientation_text: &str) -> PicksDict {
    let pick_line_re = regex::Regex::new(r"^-\s+`([^`]+)`").unwrap();
    let mut sections = PicksDict::default();
    let mut current_section: Option<&str> = None;
    let mut in_s5 = false;
    for line in orientation_text.lines() {
        let stripped = line.trim_end();
        if stripped.starts_with("## 5.") {
            in_s5 = true;
        }
        if in_s5 && stripped.starts_with("## ") && !stripped.starts_with("## 5.") {
            in_s5 = false;
            current_section = None;
            continue;
        }
        if !in_s5 {
            continue;
        }
        if stripped.starts_with("### 5.1") {
            current_section = Some("architecture");
            continue;
        }
        if stripped.starts_with("### 5.2") {
            current_section = Some("public");
            continue;
        }
        if stripped.starts_with("### 5.3") {
            current_section = Some("inter_crate");
            continue;
        }
        if stripped.starts_with("### 5.4") {
            current_section = Some("internals");
            continue;
        }
        if stripped.starts_with("### 5.5") {
            current_section = Some("intra_crate");
            continue;
        }
        if stripped.starts_with("### 5.6") {
            current_section = Some("inner_crate");
            continue;
        }
        if stripped.starts_with("**[AGENT]") {
            current_section = None;
            continue;
        }
        let section = match current_section {
            Some(s) => s,
            None => continue,
        };
        if let Some(captures) = pick_line_re.captures(stripped) {
            let pick = captures.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            match section {
                "architecture" => sections.architecture.push(pick),
                "public" => sections.public.push(pick),
                "inter_crate" => sections.inter_crate.push(pick),
                "internals" => sections.internals.push(pick),
                "intra_crate" => sections.intra_crate.push(pick),
                "inner_crate" => sections.inner_crate.push(pick),
                _ => {}
            }
        }
    }
    sections
}
