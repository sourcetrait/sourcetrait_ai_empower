use crate::*;

/// What: liquid render context for the orientation_header.liquid
/// prompt. Carries the provenance banner string (commit + rustc +
/// tool_version + rustdoc_overlay_present) for interpolation at
/// `{{ provenance }}` inside the triple-backtick block.
///
/// Why: the_user 2026-06-05 phase 3e directive moved the orientation
/// document's intro prose to liquid so wording can be edited without
/// recompiling. Provenance stays the only interpolated value at the
/// top of the artifact.
///
/// Where: built by `crate::emit::orientation::render_orientation`
/// before invoking `Templates::render_prompt("orientation_header",
/// &ctx)`.
#[derive(serde::Serialize)]
pub struct OrientationHeaderContext {
    pub provenance: String,
}

/// What: liquid render context for the orientation_s7_authoring.liquid
/// prompt. Carries the classification-tag prefix string (either
/// `Workspace classification: **<label>**. ` when use_label is
/// present, or empty otherwise).
///
/// Why: the S7 authoring guide bakes the workspace classification
/// label inline so the agent reads it in context. Liquid handles the
/// substitution at render time.
///
/// Where: built by `crate::emit::orientation::render_orientation` for
/// the S7 prompt render call.
#[derive(serde::Serialize)]
pub struct OrientationS7Context {
    pub classification_tag: String,
}

/// What: liquid render context for the container_intro.liquid prompt.
/// Carries the provenance banner string for the container-routing
/// artifact's intro block.
///
/// Why: the container-routing artifact has the same provenance shape
/// as orientation; the_user can iterate on the routing intro prose
/// without recompiling.
///
/// Where: built by
/// `crate::emit::container_routing::render_container_routing` before
/// invoking `Templates::render_prompt("container_intro", &ctx)`.
#[derive(serde::Serialize)]
pub struct ContainerIntroContext {
    pub provenance: String,
}

/// What: liquid render context for prompts with no interpolated
/// values. An empty struct serializes to an empty liquid object so the
/// template renders its static prose verbatim.
///
/// Why: several phase 3e prompts (S4 dataflow, S5 tier guidance, S5
/// authoring guidance, S5 use_tier_* paragraphs, container_agent) are
/// pure static prose; they still pass through the liquid pipeline so
/// the `-t` runtime override picks up edited files when present.
///
/// Where: passed to `Templates::render_prompt` for the static-prose
/// prompts in `orientation` + `container_routing`.
#[derive(serde::Serialize)]
pub struct EmptyContext {}
