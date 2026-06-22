# ai - the fae relay toolkit (host side of the bare-relay flow).
#
# Commands: `ai relay from|to|up`. Run from inside the target repo.
# Per-repo convention: remote `relayed` = the on-box bare; your branch
# `draft/<handle>`; the fae's branch `draft/ai/<handle>`; `dev` on `origin`.
# <handle> is read from git config (portable). Requires the `gstat` plugin.

# principal handle, from git config (your host user.name is your handle)
def handle []: nothing -> string { ^git config user.name | str trim }

# refuse to rebase over a dirty tree (untracked is fine)
def ensure-clean []: nothing -> nothing {
    let s = (gstat)
    let dirty = ($s.conflicts + $s.wt_modified + $s.wt_deleted + $s.idx_added_staged + $s.idx_modified_staged + $s.idx_deleted_staged)
    if $dirty > 0 {
        error make { msg: $"($s.repo_name): working tree not clean - commit or stash first" }
    }
}

# pull my work in: mirror draft/ai/<handle> (no checkout), rebase yours onto it
export def "relay from" []: nothing -> nothing {
    ensure-clean
    let h = (handle)
    ^git fetch relayed
    ^git branch -f $"draft/ai/($h)" $"relayed/draft/ai/($h)"
    ^git rebase $"draft/ai/($h)" $"draft/($h)"
}

# hand your work to the relay so I can rebase onto it
export def "relay to" []: nothing -> nothing {
    let h = (handle)
    ^git push --force-with-lease relayed $"draft/($h)"
}

# publish to GitHub: refresh my mirror, push yours + mine (forced) + dev (plain)
export def "relay up" []: nothing -> nothing {
    let h = (handle)
    ^git fetch relayed
    ^git branch -f $"draft/ai/($h)" $"relayed/draft/ai/($h)"
    ^git fetch origin
    ^git push --force-with-lease origin $"draft/ai/($h)"
    ^git push --force-with-lease origin $"draft/($h)"
    ^git push origin dev
}
