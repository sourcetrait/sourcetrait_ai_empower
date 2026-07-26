use rig/sourcetrait/grammar/airepo/store/git/common

# Where the store stands: working branch, pending work, and drift from the bare.
#
# The pre-commit read. `staged` lists what is ALREADY in the index before I add
# anything - a non-empty list in a shared tree is someone else's work and must
# not be swept into my commit. `branch` is HEAD as it actually is, which differs
# from `rev` if I strayed off the working branch. `ahead`/`behind` are measured
# against the bare's copy of the working branch, so behind > 0 means sync first.
export def main [args: nothing]: nothing -> record<repo: string, rev: string, branch: string, staged: list<string>, dirty: int, untracked: int, ahead: int, behind: int> {
    let repo = (common store_repo)
    cd $repo
    common ensure_remote
    let rev = (common current_rev)
    let wt = (common worktree)
    let bare = $"relayed/($rev)"
    let d = if (^git rev-parse --verify --quiet $bare | complete | get exit_code) == 0 {
        common rev_delta $bare $rev
    } else {
        { behind: 0, ahead: 0 }
    }
    {
        repo: $repo,
        rev: $rev,
        branch: (^git rev-parse --abbrev-ref HEAD | str trim),
        staged: $wt.staged,
        dirty: $wt.dirty,
        untracked: $wt.untracked,
        ahead: $d.ahead,
        behind: $d.behind,
    }
}
