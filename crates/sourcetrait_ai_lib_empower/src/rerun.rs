use crate::*;

/// Content hash of a re-executable closure. Distinct from `Nonce`: a
/// nonce is a per-call random id (counter + time + payload), while a
/// `RerunHash` is a deterministic content hash -- same input always
/// produces the same hash. Stored in the closure cache as a filename
/// (`closures/<rerun_hash>.json`); surfaced to the agent as the
/// `rerun_id` field in run()'s envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RerunHash(u64);

impl RerunHash {
    /// What: hash any `Hash`-able payload via xxh3_64 and wrap the
    /// resulting u64 in `RerunHash`. No counter, no timestamp --
    /// pure content hash, so identical payloads always produce
    /// identical hashes. Callers commonly pass a tuple of the
    /// closure-identifying fields:
    ///
    /// ```ignore
    /// RerunHash::of(&(&args_schema, &result_schema, &closure))
    /// ```
    ///
    /// Why: determinism is the whole point of `RerunHash`. The
    /// `rerun()` MCP tool re-evaluates a previously-run closure by
    /// looking up the cache file named after this hash; same closure
    /// shape -> same id -> agent can rerun by id without re-sending
    /// the full source.
    ///
    /// Where: called by `sourcetrait_ai_nushell_mcp::server::tool::NuSh::run` after
    /// successful evaluation, hashing the closure's args_schema +
    /// result_schema + body. The resulting hash becomes the envelope
    /// `rerun_id` field and the name of the
    /// `closures/<rerun_id>.json` cache file.
    pub fn of<T: Hash>(payload: &T) -> Self {
        let mut hasher = xxh3::Xxh3::default();
        payload.hash(&mut hasher);
        Self(hasher.finish())
    }

    /// What: returns the underlying u64 value, bypassing the base62
    /// `Display` impl.
    ///
    /// Why: mirrors `Nonce::to_u64` so any code that needs to compare
    /// two `RerunHash` instances by integer value, hash a hash into a
    /// larger key, or assert exact bit patterns in tests can do so
    /// without parsing the base62 form.
    ///
    /// Where: not used in sourcetrait_ai_nushell_mcp's hot path; reserved for tests
    /// and future internal helpers.
    pub fn to_u64(self) -> u64 {
        self.0
    }
}

impl Display for RerunHash {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        base62::fmt_base62(self.0, f)
    }
}
