use rig/sourcetrait/grammar/airepo/store/git/common

# Commit the store's work and fast-forward-push the working branch to the bare.
#
# Reads the message from the shm path (relative to XDGX_SHM_DIR - it travels
# out-of-band, so a long body never rides an argument). Switches to the highest
# rev/rev<N>, stages all, signed-commits if anything is staged, then pushes -
# a plain push, never --force, so a non-fast-forward STOPS. `paths` narrows the
# stage to exactly those pathspecs; empty stages everything, which in a shared
# tree also sweeps in whatever was already staged (see :status first).
export def main [
    args: record<msg_shm: string, paths: list<string>>
]: nothing -> record<repo: string, rev: string, committed: bool, commit: string, tip: string, bare_tip: string, ahead: int> {
    let repo = (common store_repo)
    cd $repo
    common ensure_remote
    let rev = (common current_rev)
    let msg_path = ($env.XDGX_SHM_DIR | path join $args.msg_shm)
    if not ($msg_path | path exists) {
        error make { msg: $"store submit: commit message shm not found: ($args.msg_shm)" }
    }
    if ((open --raw $msg_path | decode | str trim) | is-empty) {
        error make { msg: "store submit: commit message is empty" }
    }
    common grun ["switch" $rev] $"switch to ($rev)"
    if ($args.paths | is-empty) {
        common grun ["add" "-A"] "stage changes"
    } else {
        common grun (["add" "--"] | append $args.paths) "stage the named paths"
    }
    let wt = (common worktree)
    mut committed = false
    if (($wt.staged | length) > 0) {
        common grun ["commit" "-S" "-F" $msg_path] "commit (signed)"
        $committed = true
    }
    common grun ["fetch" "relayed"] "fetch the bare"
    let bare = $"relayed/($rev)"
    common grun ["push" "relayed" $rev] $"fast-forward ($rev) to the bare"
    let d = (common rev_delta $bare $rev)
    {
        repo: $repo,
        rev: $rev,
        committed: $committed,
        commit: (^git rev-parse --short HEAD | str trim),
        tip: (^git rev-parse --short HEAD | str trim),
        bare_tip: (^git rev-parse --short $bare | str trim),
        ahead: $d.ahead,
    }
}
