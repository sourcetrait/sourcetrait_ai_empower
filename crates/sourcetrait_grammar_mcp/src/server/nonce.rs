use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Nonce(u64);

impl Display for Nonce {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        lib_grammar::base62::fmt_base62(self.0, f)
    }
}

/// This host PROCESS's own identity, minted once at startup from its pid.
///
/// Distinct from `Nonce` in lifetime, not in shape: a Nonce names one eval, this names
/// the host that ran it. It namespaces per-process artifacts (the emergency log) so two
/// hosts sharing a store coordinate never clobber each other, and `info()` reports it so
/// an agent can tell a RESTARTED host from the one it was talking to - which a pid alone
/// cannot do, since pids are reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct McpNom(Nonce);

impl McpNom {
    pub(crate) fn mint(nonce_gen: &NonceGen) -> Self {
        Self(nonce_gen.next(&process::id()))
    }
}

impl Display for McpNom {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

pub(crate) struct NonceGen {
    counter: AtomicUsize,
}

impl Default for NonceGen {
    fn default() -> Self {
        Self::new()
    }
}

impl NonceGen {
    pub(crate) fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }

    pub(crate) fn next<T: Hash>(
        &self,
        payload: &T,
    ) -> Nonce {
        let mut hasher = xxh3::Xxh3::default();
        payload.hash(&mut hasher);
        self.counter
            .fetch_add(1, Ordering::SeqCst)
            .hash(&mut hasher);
        let time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        time_ns.hash(&mut hasher);
        Nonce(hasher.finish())
    }
}
