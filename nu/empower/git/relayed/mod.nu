# Box-side relay tools: sync prepares a repo, submit commits + ff-pushes.
#
# Fast-forward only and break-on-error; named apart from the user's `ai relay`
# host toolkit on purpose. Call targets: empower:git/relayed:{sync,submit};
# shared helpers live in common.nu.
export use ./sync.nu
export use ./submit.nu
