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
    /// Hash any `Hash`-able payload via xxh3_64. No counter, no
    /// timestamp -- pure content hash. Callers commonly pass a tuple
    /// of the closure-identifying fields:
    ///
    /// ```ignore
    /// RerunHash::of(&(&args_schema, &result_schema, &closure))
    /// ```
    pub fn of<T: Hash>(payload: &T) -> Self {
        let mut hasher = xxh3::Xxh3::default();
        payload.hash(&mut hasher);
        Self(hasher.finish())
    }

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
