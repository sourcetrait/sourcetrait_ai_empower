# Memory knowledge-base ops for a fae (distributed, parameterized).
#
# The agent-side memory tooling every fae shares: list the memory snakes and
# audit KB hygiene over the single git-backed memory store. All functions
# parameterize the store dir - no ai-repository-specific literals - so the
# module serves any bonded fae.
export module names
export use names
export module audit
export use audit
