use crate::*;

/// What: the claudeline entry point - read the Claude Code session JSON on
/// stdin, mirror it to the session's YAML artifact (plus the latest
/// pointer), and render the statusline to stdout.
///
/// Why: replaces scripts/sh/statusline.bash. The render is the primary,
/// must-not-fail output; the YAML side-write is best-effort (its errors are
/// swallowed) so a cache hiccup never blanks the statusline.
///
/// Where: called by main().
pub fn run() {
    let mut raw = String::new();
    let _ = io::stdin().read_to_string(&mut raw);

    let input = Input::parse(&raw);

    if let Some(input) = &input {
        let _ = match input.session_id() {
            Some(sid) => persist(input, sid),
            None => clear_latest(),
        };
    }

    let render_input = input.as_ref().map(Input::render_input).unwrap_or_default();
    println!("{}", render(&render_input, LayoutKind::default()));
}
