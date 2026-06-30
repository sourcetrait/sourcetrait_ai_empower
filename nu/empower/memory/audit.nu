# KB integrity audit over the token-form ragref memory store (p1-p5 envelope).
#
# p1 frontmatter (name==snake, description present + no triple-dash, meta
# complete), p2 refs (dangling = a `## ref` token whose base memory is missing;
# orphans = memories no ref points at), p3 index (MEMORY.md first-bullet token
# coverage), p4 mirror (live<->repo sha drift), p5 MEMORY.md size vs cap. Parses
# the {implied:/adhoc:} token forms (shards + granularity); the retired wikilink
# / mem: / MEMORY_*_INDEX machinery is gone.
export def main [args: record<live: string, repo: string>]: nothing -> record<counts: record<memories: int, indexed: int>, p1_frontmatter: record<bad_frontmatter: list<string>, name_missing: list<string>, name_mismatch: table<file: string, name: string, snake: string>, desc_missing: list<string>, desc_tripledash: list<string>, meta_incomplete: list<string>>, p2_refs: record<dangling: table<from: string, base: string>, orphan_count: int, orphans: list<string>>, p3_index: record<unindexed: list<string>, broken: table<idx: string, missing: string>>, p4_mirror: record<only_live: list<string>, only_repo: list<string>, sha_mismatch: table<file: string>>, p5_sizes: record<memory_md_bytes: int, over_cap: bool>> {
    const DEFAULT_MEMORY_CAP = 24576

    def mem_files [dir: string] {
        glob ($dir | path join "*.md")
        | each {|p| $p | path basename }
        | where {|x| $x != "MEMORY.md" }
        | sort
    }

    # Implied/adhoc ragref tokens from a doc's `## ref` bullets only (the
    # canonical cross-refs). Scoping to the bullets between `## ref` and the next
    # `## ` header excludes body example tokens and granularity self-anchors.
    def ref_tokens [raw: string] {
        let lns = ($raw | lines)
        let ref_idx = ($lns | enumerate | where {|r| ($r.item | str trim) == "## ref" } | get index.0?)
        if $ref_idx == null {
            []
        } else {
            let after = ($lns | enumerate | where {|r| $r.index > $ref_idx })
            let end_idx = ($after | where {|r| ($r.item | str trim | str starts-with "## ") } | get index.0?)
            let section = (if $end_idx == null { $after } else { $after | where {|r| $r.index < $end_idx } })
            let bullets = ($section | where {|r| ($r.item | str trim | str starts-with "- ") } | get item)
            let m = (($bullets | str join (char nl)) | parse --regex '\{(?<tok>(?:implied|adhoc):[a-z0-9_:]+)\}')
            if ($m | is-empty) { [] } else { $m | get tok | uniq }
        }
    }

    # Ragref token inner (e.g. "adhoc:rule:nu:not_transactional") -> base memory
    # snake. Handles `::` shard (-> `__`) before `:` split; drops granularity.
    def token_base [inner: string] {
        let parts = ($inner | split row "::")
        if (($parts | length) > 1) {
            let base = ($parts | first | split row ":" | first 3 | str join "_")
            let shard = ($parts | get 1 | split row ":" | first)
            [$base $shard] | str join "__"
        } else {
            $inner | split row ":" | first 3 | str join "_"
        }
    }

    def parse_memory [dir: string, file: string] {
        let raw = (open --raw ([$dir $file] | path join) | decode)
        let lns = ($raw | lines)
        let fences = ($lns | enumerate | where {|r| ($r.item | str trim) == "---" } | get index)
        let fm_ok = (($fences | length) >= 2 and ($fences | first) == 0)
        let fm = (if $fm_ok {
            $lns | enumerate
            | where {|r| $r.index > 0 and $r.index < ($fences | get 1) }
            | get item | each {|l| $l | str trim }
        } else { [] })
        let getf = {|key|
            $fm | where {|l| $l | str starts-with $key } | get 0?
            | default "" | str replace $key "" | str trim
        }
        let name = (do $getf "name:")
        let desc = (do $getf "description:")
        {
            file: $file,
            snake: ($file | str replace --regex '\.md$' ''),
            name: $name,
            fm_ok: $fm_ok,
            has_name: (not ($name | is-empty)),
            has_desc: (not ($desc | is-empty)),
            desc_tripledash: ($desc =~ '-{3}'),
            meta_complete: (["node_type:" "type:" "revision:" "date:"]
                | all {|k| not ((do $getf $k) | is-empty) }),
            ref_bases: ((ref_tokens $raw) | each {|t| token_base $t } | uniq),
            sha: ($raw | hash sha256)
        }
    }

    let files = (mem_files $args.live)
    let parsed = ($files | each {|f| parse_memory $args.live $f })
    let snakes = ($parsed | get snake)

    let all_refs = ($parsed | each {|p| $p.ref_bases | each {|b| {from: $p.file, base: $b} } } | flatten)
    let dangling = (if ($all_refs | is-empty) { [] } else {
        $all_refs | where {|e| ($e.base + ".md") not-in $files } | uniq-by base | select from base
    })
    let inbound = (if ($all_refs | is-empty) { [] } else { $all_refs | get base | uniq })
    let orphans = ($parsed | where {|p| $p.snake not-in $inbound } | get file)

    let mem_raw = (open --raw ($args.live | path join "MEMORY.md") | decode)
    let idx_bases = ((ref_tokens $mem_raw) | each {|t| token_base $t } | uniq)
    let indexed_files = ($idx_bases | each {|b| $b + ".md" })
    let unindexed = ($files | where {|f| $f not-in $indexed_files })
    let broken = ($idx_bases | where {|b| ($b + ".md") not-in $files } | each {|b| {idx: "MEMORY.md", missing: ($b + ".md")} })

    let repo_files = (mem_files $args.repo)
    let common = ($files | where {|f| $f in $repo_files })
    let only_live = ($files | where {|f| $f not-in $repo_files })
    let only_repo = ($repo_files | where {|f| $f not-in $files })
    let sha_mismatch = ($common | each {|f|
        let l = (open --raw ([$args.live $f] | path join) | hash sha256)
        let r = (open --raw ([$args.repo $f] | path join) | hash sha256)
        if $l != $r { {file: $f} } else { null }
    } | compact)

    let mem_bytes = (open --raw ($args.live | path join "MEMORY.md") | into binary | bytes length)

    {
        counts: { memories: ($files | length), indexed: ($idx_bases | where {|b| ($b + ".md") in $files } | length) }
        p1_frontmatter: {
            bad_frontmatter: ($parsed | where not fm_ok | get file)
            name_missing: ($parsed | where not has_name | get file)
            name_mismatch: ($parsed | where {|p| $p.has_name and $p.name != $p.snake } | select file name snake)
            desc_missing: ($parsed | where not has_desc | get file)
            desc_tripledash: ($parsed | where desc_tripledash | get file)
            meta_incomplete: ($parsed | where not meta_complete | get file)
        }
        p2_refs: {
            dangling: $dangling
            orphan_count: ($orphans | length)
            orphans: $orphans
        }
        p3_index: {
            unindexed: $unindexed
            broken: $broken
        }
        p4_mirror: {
            only_live: $only_live
            only_repo: $only_repo
            sha_mismatch: $sha_mismatch
        }
        p5_sizes: {
            memory_md_bytes: $mem_bytes
            over_cap: ($mem_bytes > $DEFAULT_MEMORY_CAP)
        }
    }
}
