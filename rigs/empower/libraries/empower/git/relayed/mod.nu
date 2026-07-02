# Box-side relay tools: sync prepares a repo, submit commits + ff-pushes.
#
# Fast-forward only and break-on-error; named apart from the user's `ai relay`
# host toolkit on purpose. Call targets: empower:git/relayed:{sync,submit};
# shared helpers live in common.
export module sync
export module submit
export module common
