use crate::*;

/// What: deterministic base62 id derived from a Claude Code session id
/// (SID) by xxh3-hashing the SID string. Wraps the resulting u64 and
/// renders base62 via the shared `base62::fmt_base62` formatter.
///
/// Why: distinct from `Nonce` (per-call: counter + time + payload, used
/// once) - a `ClaudeSessionHash` is REPRODUCED on every render of the
/// same session, so it can name that session's statusline artifact and a
/// stable pointer to it without storing a SID->name map. Same mechanism
/// as `RerunHash` (a content hash) but a separate domain type so the two
/// uses do not get conflated.
///
/// Where: produced by `ClaudeSessionHash::from(sid)` in
/// `sourcetrait_ai_claudeline`; rendered into the YAML `session_hash`
/// field and the `<cache>/statusline/{hash}.yaml` filename via `Display`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClaudeSessionHash(u64);

impl ClaudeSessionHash {
    /// What: returns the underlying u64 value, bypassing the base62
    /// `Display` impl.
    ///
    /// Why: mirrors `Nonce::to_u64` / `RerunHash::to_u64` for the few
    /// callers that want the raw bits (comparing by integer value,
    /// hashing into a larger key, asserting exact values in tests).
    ///
    /// Where: not used in claudeline's hot path; reserved for tests and
    /// future internal helpers.
    pub fn to_u64(self) -> u64 {
        self.0
    }
}

impl From<&str> for ClaudeSessionHash {
    /// What: xxh3_64-hashes the session id string and wraps the result.
    /// No counter, no timestamp - the same SID always produces the same
    /// hash.
    ///
    /// Why: determinism is the whole point. claudeline writes a session's
    /// statusline payload to `{hash}.yaml` and points `latest.yaml` +
    /// `{sid}.yaml` symlinks at it; the hash must be stable across renders
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

impl Display for ClaudeSessionHash {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        base62::fmt_base62(self.0, f)
    }
}
