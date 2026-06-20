/// What: the render-ready fields for one statusline render, extracted from
/// the payload (see `Input::render_input`). Every field is optional - a
/// segment is omitted when its source field is absent.
///
/// Why: decouples the layout (pure string assembly) from the JSON shape
/// and the ALT_TZ time math. Default = everything absent (the blank render
/// used when stdin did not parse).
///
/// Where: produced by input.rs, consumed by `render`.
#[derive(Debug, Default)]
pub(crate) struct RenderInput {
    pub(crate) project: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) ctx: Option<String>,
    pub(crate) five_hour: Option<RateWindow>,
    pub(crate) seven_day: Option<i64>,
}

/// What: a rate-limit window's rendered pieces - the floored used percent
/// and the optional reset clock time (HHMM).
///
/// Why: backs the `[<used>% <HHMM>]` five-hour segment. Where: RenderInput.
#[derive(Debug)]
pub(crate) struct RateWindow {
    pub(crate) used_pct: i64,
    pub(crate) hhmm: Option<String>,
}

/// What: which statusline layout to render.
///
/// Why: the renderer is a match over this enum so new prompt styles are
/// added as variants without touching call sites. Where: passed to
/// `render`; `FaeOne` (the current line) is the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LayoutKind {
    #[default]
    FaeOne,
}

/// What: render the statusline line for `input` under `layout`.
///
/// Why: single dispatch point so layouts stay isolated. Where: run(),
/// once per invocation.
pub(crate) fn render(input: &RenderInput, layout: LayoutKind) -> String {
    match layout {
        LayoutKind::FaeOne => fae_one(input),
    }
}

/// What: the current line - `proj: model (effort) ctx% [5h% HHMM] {7d%}`,
/// each segment present only when its field is. A byte-faithful port of
/// scripts/sh/statusline.bash's jq render.
///
/// Why: the one layout shipped today. Where: `render` for FaeOne.
fn fae_one(input: &RenderInput) -> String {
    let mut segs: Vec<String> = Vec::new();
    if let Some(m) = &input.model {
        segs.push(m.clone());
    }
    if let Some(e) = &input.effort {
        segs.push(format!("({e})"));
    }
    if let Some(c) = &input.ctx {
        segs.push(format!("{c}%"));
    }
    if let Some(fh) = &input.five_hour {
        let mut s = format!("[{}%", fh.used_pct);
        if let Some(t) = &fh.hhmm {
            s.push(' ');
            s.push_str(t);
        }
        s.push(']');
        segs.push(s);
    }
    if let Some(sd) = input.seven_day {
        segs.push(format!("{{{sd}%}}"));
    }
    let body = segs.join(" ");
    match &input.project {
        Some(p) => format!("{p}: {body}"),
        None => body,
    }
}
