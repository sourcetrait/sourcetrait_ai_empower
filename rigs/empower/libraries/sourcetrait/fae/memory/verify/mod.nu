use rig/sourcetrait/fae/memory/classify

# Read-only live<->repo memory mirror drift report (the bootstrap check).
#
# match=true means the dirs are identical (same files + content). Otherwise
# only_live / only_repo list one-sided files and content_diff lists files on both
# sides whose content differs (with both mtimes + whether the repo copy is
# git-committed). Performs no copy; {implied:bootstrap} step 2 wants match=true.
export def main [args: record<live: string, repo: string>]: nothing -> record<match: bool, only_live: list<string>, only_repo: list<string>, content_diff: table<file: string, live_mtime: datetime, repo_mtime: datetime, repo_committed: bool>> {
    let a = (classify analyze $args.live $args.repo)
    let only_live = ($a | where {|r| $r.in_live and (not $r.in_repo) } | get file)
    let only_repo = ($a | where {|r| (not $r.in_live) and $r.in_repo } | get file)
    let content_diff = (
        $a
        | where {|r| $r.in_live and $r.in_repo and (not $r.same) }
        | each {|r| {file: $r.file, live_mtime: $r.live_mtime, repo_mtime: $r.repo_mtime, repo_committed: $r.repo_committed} }
    )
    {
        match: (($only_live | is-empty) and ($only_repo | is-empty) and ($content_diff | is-empty)),
        only_live: $only_live,
        only_repo: $only_repo,
        content_diff: $content_diff
    }
}
