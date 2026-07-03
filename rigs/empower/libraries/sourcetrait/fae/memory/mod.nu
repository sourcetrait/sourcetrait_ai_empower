# Memory knowledge-base ops for a fae (distributed, parameterized).
#
# The agent-side memory tooling every fae shares: list the memory snakes, verify
# the live<->repo mirror, mirror (guarded, explicit-direction) live<->repo, and
# audit KB hygiene. All functions parameterize the live/repo/dir paths - no
# ai-repository-specific literals - so the module serves any bonded fae.
export module names
export module audit
export module verify
export module mirror
export module classify
