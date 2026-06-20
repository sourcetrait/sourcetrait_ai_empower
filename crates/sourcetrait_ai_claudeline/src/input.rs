use crate::*;

/// What: the parsed statusline stdin payload - the full JSON value plus
/// typed accessors for the session id and the fields the renderer needs.
///
/// Why: claudeline mirrors the ENTIRE payload to YAML (so the agent can
/// introspect any field), and separately pulls a small set of fields for
/// the rendered line; holding the whole Value keeps the mirror lossless
/// while the accessors isolate the render-relevant bits.
///
/// Where: built by run() from stdin via `Input::parse`; the Value feeds
/// store::persist (YAML), the accessors feed RenderInput.
pub(crate) struct Input {
    pub(crate) value: serde_json::Value,
}

impl Input {
    /// What: parse raw stdin text as JSON; None if it is not valid JSON.
    ///
    /// Why: a malformed payload should degrade to a blank render, not a
    /// crash. Where: run(), once per invocation.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let value = serde_json::from_str(raw).ok()?;
        Some(Self { value })
    }

    /// What: the Claude session id from `.session_id`, falling back to
    /// `.session.id`.
    ///
    /// Why: the SID seeds ClaudeSessionHash and the `{sid}.yaml`
    /// correlation symlink. Where: run(), store::persist.
    pub(crate) fn session_id(&self) -> Option<&str> {
        self.value
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                self.value
                    .get("session")
                    .and_then(|s| s.get("id"))
                    .and_then(serde_json::Value::as_str)
            })
    }

    /// What: the agent identity that scopes the cache tree - the basename
    /// of `workspace.project_dir` (e.g. `emptwo` for `/home/box/ai/emptwo`).
    ///
    /// Why: claudeline writes per-identity so multiple harnesses on one box
    /// never collide. No path enforcement - just the leaf segment.
    ///
    /// Where: run(), to build this render's status + context dirs.
    pub(crate) fn identity(&self) -> Option<String> {
        self.value
            .get("workspace")
            .and_then(|w| w.get("project_dir"))
            .and_then(serde_json::Value::as_str)
            .map(basename)
    }

    /// What: pull the render-relevant fields out of the payload into a
    /// RenderInput (project basename, model, effort, context %, the
    /// five-hour window, the seven-day %).
    ///
    /// Why: keeps the layout code free of JSON-poking and ALT_TZ logic.
    /// Where: run(), before render().
    pub(crate) fn render_input(&self) -> RenderInput {
        let v = &self.value;
        let project = v
            .get("workspace")
            .and_then(|w| w.get("project_dir"))
            .and_then(serde_json::Value::as_str)
            .map(basename);
        let model = v
            .get("model")
            .and_then(|m| m.get("display_name"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let effort = v
            .get("effort")
            .and_then(|e| e.get("level"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let ctx = v
            .get("context_window")
            .and_then(|c| c.get("used_percentage"))
            .and_then(serde_json::Value::as_f64)
            .map(fmt_pct);
        let five_hour = v
            .get("rate_limits")
            .and_then(|r| r.get("five_hour"))
            .and_then(rate_window);
        let seven_day = v
            .get("rate_limits")
            .and_then(|r| r.get("seven_day"))
            .and_then(|s| s.get("used_percentage"))
            .and_then(serde_json::Value::as_f64)
            .map(|p| p.floor() as i64);
        RenderInput {
            project,
            model,
            effort,
            ctx,
            five_hour,
            seven_day,
        }
    }
}

/// What: last path segment of a project dir (jq `split("/") | last`).
/// Why: the statusline prefix shows the project's leaf name. Where:
/// render_input.
fn basename(p: &str) -> String {
    Path::new(p)
        .file_name()
        .map_or_else(|| p.to_string(), |n| n.to_string_lossy().into_owned())
}

/// What: render-ready pieces of a rate-limit window (used % floored, plus
/// the reset time as HHMM when present).
///
/// Why: matches the bash `[<used>% <HHMM>]` segment. Where: render_input.
fn rate_window(v: &serde_json::Value) -> Option<RateWindow> {
    let used_pct = v
        .get("used_percentage")
        .and_then(serde_json::Value::as_f64)?
        .floor() as i64;
    let hhmm = v
        .get("resets_at")
        .and_then(serde_json::Value::as_i64)
        .map(fmt_hhmm);
    Some(RateWindow { used_pct, hhmm })
}

/// What: format a percentage like jq interpolation - integer if integral,
/// otherwise the plain float. Why: context % is printed raw (unfloored) by
/// the bash. Where: render_input.
fn fmt_pct(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// What: a unix-epoch (seconds) reset time formatted as HHMM in the ALT_TZ
/// zone (default UTC), matching `TZ=$ALT_TZ jq localtime | strftime`.
///
/// Why: the five-hour segment shows the window's reset clock time. Where:
/// rate_window. An unparseable ALT_TZ or out-of-range epoch falls back to
/// UTC / empty.
fn fmt_hhmm(epoch: i64) -> String {
    let tz: chrono_tz::Tz = env::var("ALT_TZ")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(chrono_tz::UTC);
    chrono::DateTime::from_timestamp(epoch, 0)
        .map(|dt| dt.with_timezone(&tz).format("%H%M").to_string())
        .unwrap_or_default()
}
