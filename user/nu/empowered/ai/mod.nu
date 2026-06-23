# ai - the fae relay toolkit (host side of the bare-relay flow).
#
# First time in a repo: apply the relay .git/config (adds the `relayed` remote +
# branch tracking), then run `ai relay setup` once. After that, from your own
# branch (draft/<handle>): `ai relay from|to|up`.
#
# Convention: remote `relayed` = the on-box bare; your branch `draft/<handle>`;
# the fae's branch `draft/ai/<handle>`; `dev` on `origin`. <handle> is read from
# git config (portable). Requires the `gstat` plugin. Assumes the repo is already
# set up against `origin`, with `dev` and `draft/<handle>` tracking it.
#
# INVARIANT - every relay hop is fast-forward only. Nothing here force-pushes:
# not the bare, and never origin. The fae rebases its work onto your branch
# before pushing to the bare, so your side only ever fast-forwards. A non-ff hop
# is a STOP - it aborts loudly rather than overwrite history (origin is
# immutable). If a relay stops, reconcile by hand; never add --force here.

# principal handle, from git config (your host user.name is your handle)
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

# relay commands only run from your own branch; print + abort otherwise
def on-branch []: nothing -> bool {
    let h = (handle)
    let b = (gstat).branch
    if $b == $"draft/($h)" {
        true
    } else {
        print -e $"(ansi red)error:(ansi reset) you must switch to (cy $"draft/($h)") to relay"
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
# fae branch tip and whether it is merged into your branch.
def relay-summary [h: string]: nothing -> nothing {
    let s = (gstat)
    let mine = $"draft/ai/($h)"
    let head = (^git rev-parse --short HEAD | str trim)
    let fae = (^git rev-parse --short $"relayed/($mine)" | complete | get stdout | str trim)
    let merged = ((^git merge-base --is-ancestor $"relayed/($mine)" HEAD | complete).exit_code == 0)
    print ""
    print $"(ansi blue)[relay] summary(ansi reset)"
    print $"  branch     (cy $s.branch) @ ($head)   ahead ($s.ahead), behind ($s.behind) vs origin"
    print $"  ai branch  (cy $mine) @ ($fae)   merged in: (if $merged { 'yes' } else { 'no' })"
}

# one-time: create the local checkout-able mirror of the fae's branch. Re-run to
# refresh that local snapshot (the from/up commands always use the live ref).
export def "relay setup" []: nothing -> nothing {
    if not ("relayed" in (^git remote | lines)) {
        print -e $"(ansi red)error:(ansi reset) no (cy relayed) remote - apply the relay .git/config first"
        return
    }
    let h = (handle)
    ^git fetch relayed
    # a local read-only mirror that tracks the bare; it never carries your own
    # commits and is never pushed, so refreshing it with --force is safe.
    ^git branch --force $"draft/ai/($h)" $"relayed/draft/ai/($h)"
}

# pull the fae's work in: fetch the bare, then FAST-FORWARD your branch onto the
# fae's. The fae has already rebased its work on top of yours, so this is a ff;
# if not, something is wrong upstream - it STOPS (never rebase/force here).
export def "relay from" []: nothing -> nothing {
    if not (on-branch) { return }
    ensure-clean
    let h = (handle)
    let mine = $"draft/ai/($h)"
    let theirs = $"draft/($h)"
    step $"fetching remote (cy relayed)" ["fetch" "relayed"]
    step $"fast-forwarding (cy $theirs) onto (cy $mine)" ["merge" "--ff-only" $"relayed/($mine)"]
    relay-summary $h
}

# hand your work to the relay: a plain (fast-forward) push of your branch to the
# bare, so the fae can rebase its next work on top of it.
export def "relay to" []: nothing -> nothing {
    if not (on-branch) { return }
    let h = (handle)
    let theirs = $"draft/($h)"
    step $"pushing (cy $theirs) to remote (cy relayed)" ["push" "relayed" $theirs]
    relay-summary $h
}

# publish to GitHub, fast-forward only: the fae's branch (off the bare), your
# branch, then dev. Any non-ff aborts before it can touch origin's history.
export def "relay up" []: nothing -> nothing {
    if not (on-branch) { return }
    let h = (handle)
    let mine = $"draft/ai/($h)"
    let theirs = $"draft/($h)"
    step $"fetching remote (cy relayed)" ["fetch" "relayed"]
    step $"fetching remote (cy origin)" ["fetch" "origin"]
    step $"publishing (cy $mine) to remote (cy origin)" ["push" "origin" $"relayed/($mine):($mine)"]
    step $"publishing (cy $theirs) to remote (cy origin)" ["push" "origin" $theirs]
    step $"publishing (cy dev) to remote (cy origin)" ["push" "origin" "dev"]
    relay-summary $h
}
