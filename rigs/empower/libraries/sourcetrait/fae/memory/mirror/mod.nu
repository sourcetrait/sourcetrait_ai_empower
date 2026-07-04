use rig/sourcetrait/fae/memory/classify

# Guarded directional live<->repo memory mirror (explicit direction, no default).
#
# direction is "live_to_repo" (the normal export) or "repo_to_live" (fresh-clone
# hydration). Verify-then-act, all-or-nothing: classifies each file via git into
# add / modify / delete; ABORTS (proceeded=false, no changes) when unsafe -
# unsafe.modify_conflict (a modify whose destination copy is newer) or
# unsafe.mass_delete (deletes while source has under half the files - the
# un-hydrated guard). On a safe run it copies adds + safe modifies and git-rm's
# committed deletes (recoverable via history), returning the lists plus a
# warnings record (resurrected / untracked_stray / mtime_skew) for after-action
# review. The git-synced repo is never edited directly - edit live, mirror down.
export def main [args: record<live: string, repo: string, direction: string>]: nothing -> record<direction: string, proceeded: bool, added: list<string>, modified: list<string>, deleted: list<string>, warnings: record<resurrected: table<file: string, live_mtime: datetime>, untracked_stray: table<file: string, repo_mtime: datetime>, mtime_skew: table<file: string, live_mtime: datetime, repo_mtime: datetime>>, unsafe: record<modify_conflict: table<file: string, live_mtime: datetime, repo_mtime: datetime>, mass_delete: table<file: string, repo_mtime: datetime>>> {
    let dir = $args.direction
    if (($dir != "live_to_repo") and ($dir != "repo_to_live")) {
        error make {msg: $"invalid direction '($dir)': expected live_to_repo or repo_to_live"}
    }
    let a = (classify analyze $args.live $args.repo)
    let to_repo = ($dir == "live_to_repo")
    let src_dir = (if $to_repo { $args.live } else { $args.repo })
    let dst_dir = (if $to_repo { $args.repo } else { $args.live })
    let src_has = {|r| if $to_repo { $r.in_live } else { $r.in_repo } }
    let dst_has = {|r| if $to_repo { $r.in_repo } else { $r.in_live } }
    let src_mt = {|r| if $to_repo { $r.live_mtime } else { $r.repo_mtime } }
    let dst_mt = {|r| if $to_repo { $r.repo_mtime } else { $r.live_mtime } }
    let is_newer = {|r| (((do $dst_mt $r) != null) and ((do $src_mt $r) != null) and ((do $dst_mt $r) > (do $src_mt $r))) }

    let adds = ($a | where {|r| (do $src_has $r) and (not (do $dst_has $r)) })
    let only_dst = ($a | where {|r| (not (do $src_has $r)) and (do $dst_has $r) })
    let mods = ($a | where {|r| (do $src_has $r) and (do $dst_has $r) and (not $r.same) })
    let src_count = ($a | where {|r| do $src_has $r } | length)
    let dst_count = ($a | where {|r| do $dst_has $r } | length)

    let deletes = (if $to_repo { $only_dst | where repo_committed } else { [] })
    let mass = ($to_repo and (not ($deletes | is-empty)) and (($src_count * 2) < $dst_count))

    let w_resurrected = ($adds | where ever_in_repo | each {|r| {file: $r.file, live_mtime: $r.live_mtime} })
    let w_untracked = (if $to_repo { $only_dst | where {|r| not $r.repo_committed } | each {|r| {file: $r.file, repo_mtime: $r.repo_mtime} } } else { [] })
    let w_skew = ($a | where same | where {|r| (($r.live_mtime != null) and ($r.repo_mtime != null) and ((($r.live_mtime - $r.repo_mtime) > 1day) or (($r.repo_mtime - $r.live_mtime) > 1day))) } | each {|r| {file: $r.file, live_mtime: $r.live_mtime, repo_mtime: $r.repo_mtime} })
    let u_conflict = ($mods | where {|r| do $is_newer $r } | each {|r| {file: $r.file, live_mtime: $r.live_mtime, repo_mtime: $r.repo_mtime} })
    let u_mass = (if $mass { $deletes | each {|r| {file: $r.file, repo_mtime: $r.repo_mtime} } } else { [] })

    let warnings = {resurrected: $w_resurrected, untracked_stray: $w_untracked, mtime_skew: $w_skew}
    let unsafe = {modify_conflict: $u_conflict, mass_delete: $u_mass}
    let safe_mods = ($mods | where {|r| not (do $is_newer $r) })

    if ((not ($u_conflict | is-empty)) or (not ($u_mass | is-empty))) {
        {direction: $dir, proceeded: false, added: [], modified: [], deleted: [], warnings: $warnings, unsafe: $unsafe}
    } else {
        for r in $adds {
            cp ($src_dir | path join $r.file) ($dst_dir | path join $r.file)
            if $to_repo { ^git -C $args.repo add $r.file }
        }
        for r in $safe_mods {
            cp ($src_dir | path join $r.file) ($dst_dir | path join $r.file)
            if $to_repo { ^git -C $args.repo add $r.file }
        }
        if $to_repo {
            for r in $deletes { ^git -C $args.repo rm --quiet $r.file }
        }
        {direction: $dir, proceeded: true, added: ($adds | get file), modified: ($safe_mods | get file), deleted: ($deletes | get file), warnings: $warnings, unsafe: $unsafe}
    }
}
