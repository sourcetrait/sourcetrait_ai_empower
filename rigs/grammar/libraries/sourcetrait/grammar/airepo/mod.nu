# Tooling for a fae's own repositories (the AIREPO class).
#
# An airepo-class repository is the agent's own: the ai repository itself and
# the store beside it. They differ from the project repositories in one way that
# reaches every tool - they carry NO `draft/ai/<handle>` + `draft/<handle>` pair,
# so the relay tools under grammar:git/relayed cannot drive them at all. Work
# rides a single line, the fae pushes it to the bare, and the_user pulls from
# there and publishes. Subjects get their own subtree, concerns nest beneath:
# grammar:airepo/store/git.
export module store
