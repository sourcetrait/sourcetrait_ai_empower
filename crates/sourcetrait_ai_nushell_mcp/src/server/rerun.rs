use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RerunHash(u64);

impl RerunHash {
    pub(crate) fn of<T: Hash>(payload: &T) -> Self {
        let mut hasher = xxh3::Xxh3::default();
        payload.hash(&mut hasher);
        Self(hasher.finish())
    }
}

impl Display for RerunHash {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        lib_empower::base62::fmt_base62(self.0, f)
    }
}
