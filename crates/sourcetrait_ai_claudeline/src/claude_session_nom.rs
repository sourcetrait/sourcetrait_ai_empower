use crate::*;

/// What: deterministic base62 "nom" (the workspace term for a base62-hashed
/// identifier / key) derived from a Claude Code session id (SID) by
/// xxh3-hashing the SID string. Wraps the resulting u64 and renders base62
/// via the shared `base62::fmt_base62` formatter.
///
/// Why: distinct from `Nonce` (per-call: counter + time + payload, used
/// once) - a `ClaudeSessionNom` is REPRODUCED on every render of the same
/// session, so it can name that session's statusline artifact and a stable
/// pointer to it without storing a SID->name map. Same mechanism as
/// `RerunHash` (a content hash) but a separate domain type.
///
/// Where: produced by `ClaudeSessionNom::from(sid)` in
/// `sourcetrait_ai_claudeline`; rendered into the YAML `session_nom` field
/// and the `<cache>/statusline/{nom}.yaml` filename via `Display`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ClaudeSessionNom(u64);

impl From<&str> for ClaudeSessionNom {
    /// What: xxh3_64-hashes the session id string and wraps the result.
    /// No counter, no timestamp - the same SID always produces the same
    /// nom.
    ///
    /// Why: determinism is the whole point. claudeline writes a session's
    /// statusline payload to `{nom}.yaml` and points `latest.yaml` +
    /// `{sid}.yaml` symlinks at it; the nom must be stable across renders
    /// for those pointers to stay coherent.
    ///
    /// Where: called by `sourcetrait_ai_claudeline` once it reads the SID
    /// out of the session JSON on stdin.
    fn from(sid: &str) -> Self {
        let mut hasher = xxh3::Xxh3::default();
        sid.hash(&mut hasher);
        Self(hasher.finish())
    }
}

impl Display for ClaudeSessionNom {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        lib::base62::fmt_base62(self.0, f)
    }
}