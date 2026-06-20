use crate::*;

/// What: the claudeline entry point - read the Claude Code session JSON on
/// stdin, mirror it to this identity's cache (full status mirror + minimized
/// context artifact, pruning superseded sessions), and render the statusline
/// to stdout.
///
/// Why: replaces scripts/sh/statusline.bash. The render is the primary,
/// must-not-fail output; the cache side-write is best-effort (its errors are
/// swallowed) so a cache hiccup never blanks the statusline.
///
/// Where: called by main().
pub fn run() {
    let mut raw = String::new();
    let _ = io::stdin().read_to_string(&mut raw);

    let input = Input::parse(&raw);
    let render_input = input.as_ref().map(Input::render_input).unwrap_or_default();

    if let Some(input) = input {
        let sid = input.session_id().map(str::to_string);
        let identity = input.identity();
        if let (Some(sid), Some(identity)) = (sid, identity) {
            let _ = persist_session(input, &sid, &identity);
        }
    }

    println!("{}", render(&render_input, LayoutKind::default()));
}
