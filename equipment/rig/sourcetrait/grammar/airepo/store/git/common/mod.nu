# Shared helpers for the store's git tools (grammar:airepo/store/git).
#
# The store is an AIREPO-CLASS repository: it carries no draft pair, and work
# rides the highest `rev/rev<N>` branch directly. That is why the project-side
# relay tools cannot drive it - they resolve a `draft/ai/<handle>` ref that does
# not exist here. Every helper operates on the cwd repo and BREAKS ON ERROR, so
# a non-fast-forward stops the tool rather than tempting a force.

# run a git hop; error make (with context + stderr) on a non-zero exit.
export def grun [args: list<string>, what: string]: nothing -> nothing {
    let r = (^git ...$args | complete)
    if $r.exit_code != 0 {
        error make { msg: $"store git stopped at ($what): ($r.stderr | str trim)" }
    }
}

# the store checkout, reached through the work dir's `store` symlink. the link
# is gitignored and system-specific, so it is resolved at call time and never
# recorded anywhere.
export def store_repo []: nothing -> string {
    let dir = ($env.EQUIP_WORK_DIR | path join "store")
    let kind = (try { $dir | path type } catch { null })
    if $kind == null {
        error make { msg: $"store git: no store at ($dir) - the symlink is missing or dangling" }
    }
    $dir
}

# error unless the repo has the `relayed` remote configured.
export def ensure_remote []: nothing -> nothing {
    if not ("relayed" in (^git remote | lines)) {
        error make { msg: "store git: no `relayed` remote configured in this repo" }
    }
}

# the working branch: the HIGHEST rev/rev<N> that exists locally. a new era adds
# rev/rev<N+1> and every tool follows it without an edit.
export def current_rev []: nothing -> string {
    let revs = (^git for-each-ref "--format=%(refname:short)" "refs/heads/rev"
        | lines
        | parse --regex 'rev/rev(?<n>\d+)$'
        | get n
        | each {|n| $n | into int }
        | sort)
    if ($revs | is-empty) {
        error make { msg: "store git: no rev/rev<N> branch found" }
    }
    $"rev/rev($revs | last)"
}

# ahead/behind counts of <b> relative to <a>: {behind, ahead} (a...b left/right).
export def rev_delta [a: string, b: string]: nothing -> record<behind: int, ahead: int> {
    let c = (^git rev-list --left-right --count $"($a)...($b)" | str trim | split row --regex '\s+')
    { behind: ($c | get 0 | into int), ahead: ($c | get 1 | into int) }
}

# the working tree, split the way a caller acts on it: what is ALREADY staged
# (the shared-tree check - a pre-staged path is not mine to sweep into a commit),
# what is dirty, and what is untracked.
export def worktree []: nothing -> record<staged: list<string>, dirty: int, untracked: int> {
    let rows = (^git status --porcelain | lines)
    let staged = ($rows
        | where {|l| ($l | str substring 0..1) not-in [" " "?"] }
        | each {|l| $l | str substring 3.. })
    let dirty = ($rows | where {|l| ($l | str substring 1..2) in ["M" "D"] } | length)
    let untracked = ($rows | where {|l| $l | str starts-with "??" } | length)
    { staged: $staged, dirty: $dirty, untracked: $untracked }
}
