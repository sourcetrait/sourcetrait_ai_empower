#[allow(unused_imports)]
use crate::*;

/// What: per-classification tier modifier prose - returns the
/// classification-aware paragraph that surfaces the workspace's
/// consumership-axis implications inline with the existing S5 coverage
/// tier guidance. `None` if the label has no modifier entry.
///
/// Why: emit.py's `_USE_TIER_MODIFIERS` constant + the `if use_label
/// in _USE_TIER_MODIFIERS` block in `emit_orientation` (lines 1193-
/// 1243 + 1581-1583). dev_use and end_with_dev_use have empirical
/// anchors in the 10-target reference set; dev_with_end_use and
/// end_use are speculative until representative targets probe through.
///
/// Where: called by `crate::emit::orientation::render_orientation`
/// after the S5 tier listing if a workspace_use_classification is
/// present.
pub fn use_tier_modifier(label: &str) -> Option<&'static str> {
    match label {
        "dev_use" => Some(
            "Workspace is a **dev_use library** consumed by other developers. The ARCHITECTURE \
             set (5.1) is the workspace's doubly-strong external API surface; the PUBLIC set \
             (5.2) is the broader public-by-example face; the INTER-CRATE set (5.3) is the \
             library's internal composition flow; the CLIQUE set (5.4) is shared infrastructure \
             broadly supported across the library's crates via STV vote. INNER-CRATE (5.6) \
             per-crate shows where each library crate's own architecture lives."
        ),
        "end_with_dev_use" => Some(
            "Workspace ships an **end-user product** (end_with_dev_use) with internal libraries \
             composing it. The INTER-CRATE set (5.3) captures cross-crate flow that makes the \
             product work; the PUBLIC set (5.2) is the (often narrower) external face the \
             product offers to embedders or extension authors; the CLIQUE set (5.4) is the \
             product's shared infrastructure by broad-consensus election. INTRA-CRATE (5.5) \
             within the product's primary crate shows what it consumes from the internal \
             libraries (minus clique); INNER-CRATE (5.6) shows each library crate's own \
             architecture."
        ),
        "dev_with_end_use" => Some(
            "Workspace's primary deliverable is a **library with an auxiliary CLI** \
             (dev_with_end_use, gitoxide pattern). The PUBLIC set (5.2) is the library API; \
             the INTER-CRATE set (5.3) is the cross-crate flow within the lib; the CLIQUE set \
             (5.4) is shared infrastructure across the lib's crates by broad-consensus election. \
             The CLI binary is part of the public face but should be treated as a thin wrapper \
             around the lib unless its own complexity warrants attention. Note: this bucket \
             has no empirical anchor in the current 10-target reference set; guidance is \
             speculative until a gitoxide-class target probes through (see notes/know_rust/\
             next-phase-agent-augmentation.md for the related 5th-bucket arbitration hatch \
             debt)."
        ),
        "end_use" => Some(
            "Workspace is a pure **end-user product** (end_use). Architecture / public / \
             inter-crate / clique sets are the user-facing entry points and the cross-crate \
             flows that compose the product's behavior. No external library face to track. \
             Note: this bucket has no empirical anchor in the current 10-target reference set; \
             guidance is speculative."
        ),
        _ => None,
    }
}
