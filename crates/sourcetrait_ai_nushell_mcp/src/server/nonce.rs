use crate::*;

/// What: u64-backed per-call identifier that mixes a payload, a
/// process-local counter, and the system time so collisions are
/// statistically nil; displays as a base62 string via the shared
/// `base62::fmt_base62` formatter.
///
/// Why: tool calls need a stable, agent-readable id that the
/// MCP can attach to per-call artifacts (log directories, response
/// envelopes) so the agent can later correlate stdout/stderr files
/// with the original call. Random + counter + time guards against
/// silent collisions even if the agent submits identical payloads
/// back-to-back.
///
/// Where: produced by `NonceGen::next`, rendered into MCP envelopes
/// by `sourcetrait_ai_nushell_mcp::server::tool::dispatch_to_worker`, and used by
/// `sourcetrait_ai_nushell_mcp::server::cache::cache_dir` as the leaf path segment
/// for per-call log dirs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Nonce(u64);

impl Display for Nonce {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        lib_empower::base62::fmt_base62(self.0, f)
    }
}

/// What: per-process counter that produces unique `Nonce` values when
/// combined with a payload and the system time inside `next`.
///
/// Why: holding the counter on an instance rather than a global lets
/// each MCP server own its own counter, which makes test isolation
/// easier (each test spawns a fresh NuSh with a fresh NonceGen) and
/// lets the counter reset cleanly on server restart without bleed
/// from a prior process.
///
/// Where: constructed once in `sourcetrait_ai_nushell_mcp::server::run::run_server`,
/// wrapped in `Arc<NonceGen>`, and shared across all rmcp tool
/// handlers in `NuSh`. Each handler calls `next` once per
/// dispatched worker round-trip.
pub(crate) struct NonceGen {
    counter: AtomicUsize,
}

impl Default for NonceGen {
    fn default() -> Self {
        Self::new()
    }
}

impl NonceGen {
    /// What: constructs a fresh `NonceGen` with the internal counter
    /// initialized to zero.
    ///
    /// Why: zero is a fine starting point because the next() function
    /// hashes counter + payload + time together, so even nonce #0
    /// produces a high-entropy output. No need for randomized seeds.
    ///
    /// Where: called once during `sourcetrait_ai_nushell_mcp::server::run::run_server`'s
    /// startup, before workers are spawned. Tests construct their own
    /// instances per-Host.
    pub(crate) fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }

    /// What: produces a `Nonce` by xxh3-hashing the payload, then
    /// mixing in a monotonically incrementing counter and the current
    /// nanosecond timestamp. Returns the finalized 64-bit hash wrapped
    /// in `Nonce`. Side effect: the internal counter increments by
    /// exactly one via `SeqCst` fetch-add (so concurrent callers each
    /// get a unique counter value).
    ///
    /// Why: hashing payload + counter + time guarantees three things
    /// at once: same payload twice still produces different nonces
    /// (because counter + time differ); payloads with the same shape
    /// across server restarts also differ (because time differs); and
    /// the output space is the full u64 so collisions across a
    /// realistic session count are statistically zero.
    ///
    /// Where: called by `sourcetrait_ai_nushell_mcp::server::tool::dispatch_to_worker`
    /// at the start of every run/interact/rerun/call. The returned
    /// `Nonce` becomes the per-call log dir name and the envelope's
    /// `nonce` field.
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