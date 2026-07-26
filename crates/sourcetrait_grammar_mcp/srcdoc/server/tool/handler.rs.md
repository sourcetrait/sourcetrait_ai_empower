# handler.rs

## fn NuSh::tool_router
DENY IS ENFORCED AT ROUTER ASSEMBLY, not at call time, and that is the whole design: a denied
tool is ABSENT from `tools/list`, so its schema never enters the agent's context and there is
no denied-error loop to fall into. A call anyway fails at the rmcp layer as an unknown tool.

rmcp routers compose with `+` at runtime, which is what makes conditional assembly possible
at all rather than requiring a compile-time feature per tool.

The four core tools are added unconditionally and are not deniable. There is NO implication
between tokens either - denying `run` does not deny `rerun` - so an operator's config lists
exactly what it means.

## fn get_info
`ServerCapabilities::builder().enable_tools()` is REQUIRED. `default()` advertises no tool
capability and Claude Code then skips `tools/list` entirely, which presents as a server with
no tools rather than as an error.

`env!("CARGO_PKG_VERSION")` must expand in THIS crate, so the version cannot be threaded in
from elsewhere.

The TITLE suffixes a non-default namespace so co-running namespace variants stay
tellable-apart in the client's own listing.

`instructions` renders the cache-path surface LIVE from the resolved config rather than as a
static string, so what it names is what this host actually uses. The agent-facing operational
craft beyond those paths lives in the `/nu` skill, not here.
