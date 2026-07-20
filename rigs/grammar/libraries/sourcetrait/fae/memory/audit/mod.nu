# KB integrity audit over the token-form ragref memory store (p1-p4 envelope).
#
# p1 frontmatter (name==snake, description present + no triple-dash, meta
# complete), p2 refs (dangling = a `## ref` token whose base memory is missing;
# orphans = memories no ref points at), p3 index (MEMORY.md coverage - one bare
# token per line under the `# MEMORY.md` H1; malformed = lines that are neither
# the H1, blank, nor exactly one token), p4 MEMORY.md size vs cap. Parses the
# {implied:/adhoc:} token forms (shards + granularity).
export def main [args: record<dir: string>]: nothing -> record<counts: record<memories: int, indexed: int>, p1_frontmatter: record<bad_frontmatter: list<string>, name_missing: list<string>, name_mismatch: table<file: string, name: string, snake: string>, desc_missing: list<string>, desc_tripledash: list<string>, meta_incomplete: list<string>>, p2_refs: record<dangling: table<from: string, base: string>, orphan_count: int, orphans: list<string>>, p3_index: record<unindexed: list<string>, broken: table<idx: string, missing: string>, malformed: list<string>>, p4_sizes: record<memory_md_bytes: int, over_cap: bool>> {
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

    let files = (mem_files $args.dir)
    let parsed = ($files | each {|f| parse_memory $args.dir $f })
    let snakes = ($parsed | get snake)

    let all_refs = ($parsed | each {|p| $p.ref_bases | each {|b| {from: $p.file, base: $b} } } | flatten)
    let dangling = (if ($all_refs | is-empty) { [] } else {
        $all_refs | where {|e| ($e.base + ".md") not-in $files } | uniq-by base | select from base
    })
    let inbound = (if ($all_refs | is-empty) { [] } else { $all_refs | get base | uniq })
    let orphans = ($parsed | where {|p| $p.snake not-in $inbound } | get file)

    # The MEMORY.md index is bare tokens, one per line, under a `# MEMORY.md`
    # H1 - no bullets, no prose. Tokens are read from the whole file; format
    # violations land in `malformed`.
    let mem_raw = (open --raw ($args.dir | path join "MEMORY.md") | decode)
    let idx_m = ($mem_raw | parse --regex '\{(?<tok>(?:implied|adhoc):[a-z0-9_:]+)\}')
    let idx_bases = (if ($idx_m | is-empty) { [] } else { $idx_m | get tok | each {|t| token_base $t } | uniq })
    let indexed_files = ($idx_bases | each {|b| $b + ".md" })
    let unindexed = ($files | where {|f| $f not-in $indexed_files })
    let broken = ($idx_bases | where {|b| ($b + ".md") not-in $files } | each {|b| {idx: "MEMORY.md", missing: ($b + ".md")} })
    let malformed = ($mem_raw | lines | each {|l| $l | str trim } | where {|t|
        if ($t | is-empty) {
            false
        } else if ($t == "# MEMORY.md") {
            false
        } else {
            not ($t =~ '^\{(?:implied|adhoc):[a-z0-9_:]+\}$')
        }
    })

    let mem_bytes = ($mem_raw | into binary | bytes length)

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
            malformed: $malformed
        }
        p4_sizes: {
            memory_md_bytes: $mem_bytes
            over_cap: ($mem_bytes > $DEFAULT_MEMORY_CAP)
        }
    }
}
