export def main [args: record<out_dir: string, picks_path: string, orientation_out_path: string>]: nothing -> record<written_path: string, picks_included: int, picks_missing: list<string>, byte_count: int> {
    let all_picks = (open --raw $args.picks_path | decode utf-8 | from json)

    def slug [pattern: string] {
        $pattern | str replace --all --regex '[^A-Za-z0-9_]' '_'
    }

    def is_valid_kp [text: string] {
        if ($text | str length) < 60 { return false }
        let lower = ($text | str downcase)
        if ($lower | str contains "api error") or ($lower | str contains "rate limited") or ($lower | str contains "temporarily limiting") { return false }
        ($text | str trim | str starts-with "### ")
    }

    let set_order = ["arch" "public" "inter" "clique" "intra" "inner"]
    let set_titles = {
        arch: "5.1 Architecture significance (workspace-wide cross-crate AND public-by-example)"
        public: "5.2 Public significance (workspace-wide public-by-example only)"
        inter: "5.3 Inter-crate significance (workspace-wide cross-crate flow only)"
        clique: "5.4 Clique significance (workspace-wide STV over per-crate intra ballots)"
        intra: "5.5 Intra-crate significance (per crate)"
        inner: "5.6 Inner-crate significance (per crate)"
    }

    mut sections = {}
    for s in $set_order { $sections = ($sections | upsert $s []) }
    mut missing = []

    for p in $all_picks {
        let dir = ($args.out_dir | path join (slug $p.pattern))
        # Final kp = stage_d.md when Stage D reduced; else stage_c.md
        # (Stage B unknown -> Stage D no-op -> the Stage C draft IS final).
        let stage_d = ($dir | path join "stage_d.md")
        let final = (if ($stage_d | path exists) { $stage_d } else { ($dir | path join "stage_c.md") })
        if not ($final | path exists) {
            $missing = ($missing | append $p.pattern)
            continue
        }
        let text = (open --raw $final | decode utf-8)
        if not (is_valid_kp $text) {
            $missing = ($missing | append $p.pattern)
            continue
        }
        let s = $p.set_label
        let prev = ($sections | get $s)
        $sections = ($sections | upsert $s ($prev | append $text))
    }

    mut out_lines = ["# Orientation"]
    $out_lines = ($out_lines | append "")
    for s in $set_order {
        let title = ($set_titles | get $s)
        let entries = ($sections | get $s)
        if ($entries | length) == 0 { continue }
        $out_lines = ($out_lines | append $"## ($title)")
        $out_lines = ($out_lines | append "")
        for e in $entries {
            $out_lines = ($out_lines | append ($e | str trim))
            $out_lines = ($out_lines | append "")
        }
    }

    let final_text = ($out_lines | str join "\n")
    $final_text | save -f $args.orientation_out_path

    {
        written_path: $args.orientation_out_path
        picks_included: (($all_picks | length) - ($missing | length))
        picks_missing: $missing
        byte_count: ($final_text | str length)
    }
}
