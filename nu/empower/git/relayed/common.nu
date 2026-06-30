# Shared helpers for the box-side relay tools (empower:git/relayed).
#
# Called via MCP call(); each call-target cd's into the target repo first. These
# operate on the cwd repo, use `gstat` for status, and BREAK ON ERROR: every git
# hop runs through `grun`, which `error make`s (with context + stderr) on a
# non-zero exit so the tool stops rather than force past a non-fast-forward. No
# progress printing - the caller's value is the STRUCTURED RESULT each main
# returns (branches, tips, ahead/behind); errors carry the detail.

# run a git hop; error make (with context + stderr) on a non-zero exit.
export def grun [args: list<string>, what: string]: nothing -> nothing {
    let r = (^git ...$args | complete)
    if $r.exit_code != 0 {
        error make { msg: $"relay stopped at ($what): ($r.stderr | str trim)" }
    }
}

# the principal handle, from the single draft/ai/<h> ref (local or on the bare).
# the handle is the principal's, never mine; error unless exactly one exists.
export def relay_handle []: nothing -> string {
    let hs = (^git for-each-ref "--format=%(refname:short)"
        | lines
        | parse --regex 'draft/ai/(?<h>[^/]+)$'
        | get h
        | uniq)
    if ($hs | length) != 1 {
        error make { msg: $"relay: need exactly one draft/ai/<handle> branch, found: ($hs)" }
    }
    $hs | first
}

# error unless the repo has the `relayed` remote configured.
export def relay_ensure_remote []: nothing -> nothing {
    if not ("relayed" in (^git remote | lines)) {
        error make { msg: "relay: no `relayed` remote configured in this repo" }
    }
}

# error if the working tree is dirty (modified/staged/deleted/conflicted);
# untracked files are fine.
export def relay_ensure_clean []: nothing -> nothing {
    let s = (gstat)
    let dirty = ($s.conflicts + $s.wt_modified + $s.wt_deleted + $s.idx_added_staged + $s.idx_modified_staged + $s.idx_deleted_staged)
    if $dirty > 0 {
        error make { msg: "relay: working tree not clean (modified/staged/conflicts) - commit or revert first" }
    }
}

# true if a ref with this short name exists in the repo.
export def ref_exists [name: string]: nothing -> bool {
    $name in (^git for-each-ref "--format=%(refname:short)" | lines)
}

# ahead/behind counts of <b> relative to <a>: {behind, ahead} (a...b left/right).
export def rev_delta [a: string, b: string]: nothing -> record<behind: int, ahead: int> {
    let c = (^git rev-list --left-right --count $"($a)...($b)" | str trim | split row --regex '\s+')
    {behind: ($c | get 0 | into int), ahead: ($c | get 1 | into int)}
}

# the sync core, shared by sync + submit. cwd = repo, h = handle. fetch the bare,
# get onto my branch, ff-update the local principal branch, rebase mine onto it,
# and assert the result is fast-forward-pushable. returns the resolved structured
# state: branch names, short tips, and how far mine is ahead/behind theirs.
export def relay_sync_core [h: string]: nothing -> record<their_branch: string, mine_branch: string, their_tip: string, mine_tip: string, bare_mine_tip: string, ahead: int, behind: int> {
    let theirs = $"draft/($h)"
    let mine = $"draft/ai/($h)"
    let bare_theirs = $"relayed/($theirs)"
    let bare_mine = $"relayed/($mine)"
    grun ["fetch" "relayed"] "fetch the bare"
    # update the local principal mirror by rebase (robust to a rewritten bare - a
    # plain ff breaks on a recovery; rebase replays, dropping redundant commits as
    # empty, and never blind-forces; a conflict STOPS). create it if missing.
    if not (ref_exists $theirs) {
        grun ["branch" $theirs $bare_theirs] $"create ($theirs) from the bare"
    } else {
        let rbt = (^git rebase $bare_theirs $theirs | complete)
        if $rbt.exit_code != 0 {
            ^git rebase --abort | complete
            error make { msg: $"relay stopped: rebasing local ($theirs) onto the bare (conflict): ($rbt.stdout | str trim)" }
        }
    }
    # get onto my branch (create if missing), rebase it onto the principal branch
    if not (ref_exists $mine) {
        grun ["branch" $mine $bare_mine] $"create ($mine) from the bare"
    }
    grun ["switch" $mine] $"switch to ($mine)"
    let rb = (^git rebase $theirs | complete)
    if $rb.exit_code != 0 {
        ^git rebase --abort | complete
        error make { msg: $"relay stopped: rebasing ($mine) onto ($theirs) conflicted, aborted: ($rb.stdout | str trim)" }
    }
    # ff-push my rebased branch back to the bare so local == bare (synced). a
    # plain push is ff-only - a non-ff is rejected and grun STOPS, never forces.
    grun ["push" "relayed" $mine] $"fast-forward ($mine) to the bare"
    let d = (rev_delta $theirs $mine)
    {
        their_branch: $theirs,
        mine_branch: $mine,
        their_tip: (^git rev-parse --short $theirs | str trim),
        mine_tip: (^git rev-parse --short HEAD | str trim),
        bare_mine_tip: (^git rev-parse --short $bare_mine | str trim),
        ahead: $d.ahead,
        behind: $d.behind,
    }
}
