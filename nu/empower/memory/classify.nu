# Live<->repo memory mirror analysis (shared helper for verify + mirror).
#
# Returns one row per memory file (union of both sides) with presence, a same
# (content sha equal) flag, both mtimes, and the repo-side git facts
# (repo_committed = tracked in the index/HEAD; ever_in_repo = appears anywhere in
# history) that let a caller classify add / modify / delete and judge safety.
# Pure analysis - performs no file or git mutation. Organizational: pulled into
# verify.nu / mirror.nu via `use`, not a call() target.
export def analyze [live: string, repo: string] {
    let live_files = (glob ($live | path join "*.md") | each {|p| $p | path basename } | sort)
    let repo_files = (glob ($repo | path join "*.md") | each {|p| $p | path basename } | sort)
    let tracked = (^git -C $repo ls-files | lines | each {|p| $p | path basename })
    let ever = (
        ^git -C $repo log --all --pretty=format: --name-only -- .
        | lines
        | where {|l| not ($l | str trim | is-empty) }
        | each {|p| $p | path basename }
        | uniq
    )
    let names = ($live_files | append $repo_files | uniq | sort)
    $names | each {|f|
        let lp = ($live | path join $f)
        let rp = ($repo | path join $f)
        let in_live = ($f in $live_files)
        let in_repo = ($f in $repo_files)
        let lsha = (if $in_live { open --raw $lp | hash sha256 } else { null })
        let rsha = (if $in_repo { open --raw $rp | hash sha256 } else { null })
        {
            file: $f,
            in_live: $in_live,
            in_repo: $in_repo,
            same: ($in_live and $in_repo and ($lsha == $rsha)),
            live_mtime: (if $in_live { ls $lp | get 0.modified } else { null }),
            repo_mtime: (if $in_repo { ls $rp | get 0.modified } else { null }),
            repo_committed: ($f in $tracked),
            ever_in_repo: ($f in $ever)
        }
    }
}
