# ai - the fae relay toolkit (principal/host side of the bare-relay flow).
#
# First time in a repo: apply the relay .git/config (adds the `relayed` remote +
# branch tracking), then run `ai relay setup` once. After that, from the
# principal branch (draft/<handle>): `ai relay from|to|up`.
#
# Convention: remote `relayed` = the on-box bare; the principal branch is
# `draft/<handle>`; the fae branch is `draft/ai/<handle>`; `dev` on `origin`.
# <handle> is the principal's git handle (the host user.name), read from git
# config (portable). Requires the `gstat` plugin. Assumes the repo is already set
# up against `origin`, with `dev` and `draft/<handle>` tracking it.
#
# INVARIANT - every relay hop to the bare or origin is fast-forward only. Nothing
# here force-pushes: not the bare, and never origin. The local fae mirror is kept
# current by REBASE (a plain ff breaks if the bare's fae branch was rewritten -
# e.g. a recovery; rebase replays the mirror's commits, dropping redundant ones
# as empty, and never blind-forces). A non-ff push/merge or a rebase conflict is
# a STOP - it aborts loudly rather than overwrite history (origin is immutable).

# the principal handle, from git config (the host user.name is the handle)
def handle []: nothing -> string { ^git config user.name | str trim }

# colorize a noun (branch / remote) cyan for the progress lines.
def cy [s: string]: nothing -> string { $"(ansi cyan)($s)(ansi reset)" }

# one progress-reported git hop: blue [relay] label, then green done / red failed
# + the detail. error make on non-zero so the relay STOPS (never forces).
def step [label: string, args: list<string>]: nothing -> nothing {
    print -n $"(ansi blue)[relay](ansi reset) ($label) ... "
    let r = (^git ...$args | complete)
    if $r.exit_code != 0 {
        print $"(ansi red)failed(ansi reset)"
        if not ($r.stderr | is-empty) { print -e ($r.stderr | str trim) }
        if not ($r.stdout | is-empty) { print -e ($r.stdout | str trim) }
        error make { msg: $"relay stopped at: ($label)" }
    }
    print $"(ansi green)done(ansi reset)"
}

# bring the local fae mirror (draft/ai/<h>) to the bare's fae branch. Created if
# missing; otherwise REBASED onto the bare - a plain ff would break when the bare
# was rewritten (a recovery), so rebase replays the mirror's commits, dropping
# now-redundant ones as empty, and never blind-forces. A rebase conflict STOPS.
# Switches in to rebase and back to the principal branch; assumes a clean tree.
def update-fae-mirror [h: string]: nothing -> nothing {
    let fae = $"draft/ai/($h)"
    let principal = $"draft/($h)"
    print -n $"(ansi blue)[relay](ansi reset) updating local (cy $fae) ... "
    if not ($fae in (^git for-each-ref "--format=%(refname:short)" "refs/heads" | lines)) {
        let br = (^git branch $fae $"relayed/($fae)" | complete)
        if $br.exit_code != 0 {
            print $"(ansi red)failed(ansi reset)"
            if not ($br.stderr | is-empty) { print -e ($br.stderr | str trim) }
            error make { msg: $"relay stopped: creating local ($fae)" }
        }
        print $"(ansi green)done(ansi reset)"
        return
    }
    let rb = (^git rebase $"relayed/($fae)" $fae | complete)
    if $rb.exit_code != 0 {
        ^git rebase --abort | complete
        ^git switch $principal | complete
        print $"(ansi red)failed(ansi reset)"
        if not ($rb.stdout | is-empty) { print -e ($rb.stdout | str trim) }
        if not ($rb.stderr | is-empty) { print -e ($rb.stderr | str trim) }
        error make { msg: $"relay stopped: rebasing local ($fae) onto the bare (conflict)" }
    }
    ^git switch $principal | complete
    print $"(ansi green)done(ansi reset)"
}

# relay commands only run on the principal branch; print + abort otherwise
def on-branch []: nothing -> bool {
    let h = (handle)
    let b = (gstat).branch
    if $b == $"draft/($h)" {
        true
    } else {
        print -e $"(ansi red)error:(ansi reset) relay must run on (cy $"draft/($h)")"
        false
    }
}

# refuse to operate over a dirty tree (untracked is fine)
def ensure-clean []: nothing -> nothing {
    let s = (gstat)
    let dirty = ($s.conflicts + $s.wt_modified + $s.wt_deleted + $s.idx_added_staged + $s.idx_modified_staged + $s.idx_deleted_staged)
    if $dirty > 0 {
        error make { msg: $"($s.repo_name): working tree not clean - commit or stash first" }
    }
}

# the closing structured summary: current branch + tip, divergence vs origin, the
# fae branch tip and whether it is merged into the principal branch.
def relay-summary [h: string]: nothing -> nothing {
    let s = (gstat)
    let fae = $"draft/ai/($h)"
    let head = (^git rev-parse --short HEAD | str trim)
    let fae_tip = (^git rev-parse --short $"relayed/($fae)" | complete | get stdout | str trim)
    let merged = ((^git merge-base --is-ancestor $"relayed/($fae)" HEAD | complete).exit_code == 0)
    print ""
    print $"(ansi blue)[relay] summary(ansi reset)"
    print $"  branch     (cy $s.branch) @ ($head)   ahead ($s.ahead), behind ($s.behind) vs origin"
    print $"  fae branch (cy $fae) @ ($fae_tip)   merged in: (if $merged { 'yes' } else { 'no' })"
}

# one-time: create the local checkout-able mirror of the fae branch. Re-run to
# refresh it (the from command keeps it current each run).
export def "relay setup" []: nothing -> nothing {
    if not ("relayed" in (^git remote | lines)) {
        print -e $"(ansi red)error:(ansi reset) no (cy relayed) remote - apply the relay .git/config first"
        return
    }
    ensure-clean
    let h = (handle)
    step $"fetching remote (cy relayed)" ["fetch" "relayed"]
    update-fae-mirror $h
}

# pull the fae's work in: fetch the bare, rebase the local fae mirror onto it,
# then FAST-FORWARD the principal branch onto the fae branch. The fae has already
# rebased its work on top of the principal branch, so the merge is a ff; if not,
# something is wrong upstream - it STOPS (never rebase/force the principal here).
export def "relay from" []: nothing -> nothing {
    if not (on-branch) { return }
    ensure-clean
    let h = (handle)
    let fae = $"draft/ai/($h)"
    let principal = $"draft/($h)"
    step $"fetching remote (cy relayed)" ["fetch" "relayed"]
    update-fae-mirror $h
    step $"fast-forwarding (cy $principal) onto (cy $fae)" ["merge" "--ff-only" $"relayed/($fae)"]
    relay-summary $h
}

# publish the principal's work to the bare: ff-push the principal branch, then
# fast-forward the bare's fae ref up to it so BOTH bare refs converge before `up`
# (Flow 2 - you changed). Both are ff; a non-ff means not synced -> STOP. The fae
# then catches its local branch up on its next sync.
export def "relay to" []: nothing -> nothing {
    if not (on-branch) { return }
    let h = (handle)
    let principal = $"draft/($h)"
    let fae = $"draft/ai/($h)"
    step $"pushing (cy $principal) to remote (cy relayed)" ["push" "relayed" $principal]
    step $"fast-forwarding (cy $fae) to (cy $principal) on (cy relayed)" ["push" "relayed" $"($principal):($fae)"]
    relay-summary $h
}

# publish to GitHub, fast-forward only: the fae branch (off the bare), the
# principal branch, then dev. Any non-ff aborts before it can touch origin.
# (The local fae mirror is maintained by `relay from`; up publishes off the bare.)
export def "relay up" []: nothing -> nothing {
    if not (on-branch) { return }
    let h = (handle)
    let fae = $"draft/ai/($h)"
    let principal = $"draft/($h)"
    step $"fetching remote (cy relayed)" ["fetch" "relayed"]
    step $"fetching remote (cy origin)" ["fetch" "origin"]
    step $"publishing (cy $fae) to remote (cy origin)" ["push" "origin" $"relayed/($fae):($fae)"]
    step $"publishing (cy $principal) to remote (cy origin)" ["push" "origin" $principal]
    step $"publishing (cy dev) to remote (cy origin)" ["push" "origin" "dev"]
    relay-summary $h
}
