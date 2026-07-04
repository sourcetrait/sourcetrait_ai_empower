use rig/sourcetrait/empower/git/relayed/common

# Commit my work and fast-forward-push my branch to the bare.
#
# cd into the repo; read the commit message from the shm path (relative to
# XDGX_SHM_DIR - the message travels out-of-band); stage all, signed-commit if
# anything is staged; run the sync core (rebase onto the principal's latest);
# then a plain `git push relayed draft/ai/<h>` - never --force, so a non-ff push
# STOPS. Returns the structured outcome: whether it committed, the new tip, the
# principal's tip, and how many of my commits now sit on the bare ahead of theirs.
export def main [args: record<repo: directory, msg_shm: string>]: nothing -> record<handle: string, committed: bool, commit: string, mine_tip: string, their_tip: string, ahead: int> {
    cd $args.repo
    common relay_ensure_remote
    let h = (common relay_handle)
    let mine = $"draft/ai/($h)"
    let msg_path = ($env.XDGX_SHM_DIR | path join $args.msg_shm)
    if not ($msg_path | path exists) {
        error make { msg: $"relay submit: commit message shm not found: ($args.msg_shm)" }
    }
    if ((open --raw $msg_path | decode | str trim) | is-empty) {
        error make { msg: "relay submit: commit message is empty" }
    }
    common grun ["switch" $mine] $"switch to ($mine)"
    common grun ["add" "-A"] "stage changes"
    let s = (gstat)
    let staged = ($s.idx_added_staged + $s.idx_modified_staged + $s.idx_deleted_staged)
    mut committed = false
    if $staged > 0 {
        common grun ["commit" "-S" "-F" $msg_path] "commit (signed)"
        $committed = true
    }
    let t = (common relay_sync_core $h)
    {
        handle: $h,
        committed: $committed,
        commit: (^git rev-parse --short HEAD | str trim),
        mine_tip: $t.mine_tip,
        their_tip: $t.their_tip,
        ahead: $t.ahead,
    }
}
