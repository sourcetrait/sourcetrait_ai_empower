# Memory knowledge-base ops for a fae harness (distributed, parameterized).
#
# The agent-side memory tooling every fae shares: list the memory snakes, verify
# the live<->repo mirror, mirror (guarded, explicit-direction) live<->repo, and
# audit KB hygiene. All functions parameterize the live/repo/dir paths - no
# harness-specific literals - so the module serves any bonded fae.
export use ./names.nu
export use ./audit.nu
export use ./verify.nu
export use ./mirror.nu
