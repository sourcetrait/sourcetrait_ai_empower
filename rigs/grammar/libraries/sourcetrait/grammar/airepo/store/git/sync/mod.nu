use rig/sourcetrait/grammar/airepo/store/git/common

# Get onto the working branch and fast-forward it from the bare.
#
# Call before working. Requires the `relayed` remote and a clean tree, fetches
# the bare, switches to the highest rev/rev<N>, and fast-forwards it - ff-only,
# so a divergence STOPS rather than being rebased or forced past. Returns the
# resolved branch, its tip, the bare's tip, and how far the two sit apart.
export def main [args: nothing]: nothing -> record<repo: string, rev: string, tip: string, bare_tip: string, ahead: int, behind: int> {
    let repo = (common store_repo)
    cd $repo
    common ensure_remote
    let wt = (common worktree)
    if (($wt.staged | length) > 0) or ($wt.dirty > 0) {
        error make { msg: "store sync: working tree not clean - commit or revert first" }
    }
    let rev = (common current_rev)
    common grun ["fetch" "relayed"] "fetch the bare"
    common grun ["switch" $rev] $"switch to ($rev)"
    let bare = $"relayed/($rev)"
    if (^git rev-parse --verify --quiet $bare | complete | get exit_code) == 0 {
        common grun ["merge" "--ff-only" $bare] $"fast-forward ($rev) from the bare"
    }
    let d = (common rev_delta $bare $rev)
    {
        repo: $repo,
        rev: $rev,
        tip: (^git rev-parse --short HEAD | str trim),
        bare_tip: (^git rev-parse --short $bare | str trim),
        ahead: $d.ahead,
        behind: $d.behind,
    }
}
