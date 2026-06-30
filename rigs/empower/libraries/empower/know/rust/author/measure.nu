export def main [args: record<out_dir: string, picks_path: string>]: nothing -> record<valid_picks: int, failed_picks: int, total_kp_chars: int, per_set: table<set: string, count: int, kp_chars: int>, per_group: table<group: string, count: int, kp_chars: int>> {
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

    let entries = ($all_picks | each {|p|
        let dir = ($args.out_dir | path join (slug $p.pattern))
        # Final kp = stage_d.md when Stage D reduced; else stage_c.md
        # (Stage B unknown -> Stage D no-op -> the Stage C draft IS final).
        let stage_d = ($dir | path join "stage_d.md")
        let final = (if ($stage_d | path exists) { $stage_d } else { ($dir | path join "stage_c.md") })
        let exists = ($final | path exists)
        let text = if $exists { (open --raw $final | decode utf-8) } else { "" }
        let valid = (if $exists { (is_valid_kp $text) } else { false })
        {pattern: $p.pattern, set_label: $p.set_label, group: $p.group, valid: $valid, kp_chars: ($text | str length)}
    })

    let valid_entries = ($entries | where valid)
    let failed_entries = ($entries | where not valid)
    let total_kp_chars = ($valid_entries | get kp_chars | math sum)

    let per_set = ($valid_entries | group-by set_label | items {|k v|
        {set: $k, count: ($v | length), kp_chars: ($v | get kp_chars | math sum)}
    })
    let per_group = ($valid_entries | group-by group | items {|k v|
        {group: $k, count: ($v | length), kp_chars: ($v | get kp_chars | math sum)}
    })

    {
        valid_picks: ($valid_entries | length)
        failed_picks: ($failed_entries | length)
        total_kp_chars: $total_kp_chars
        per_set: $per_set
        per_group: $per_group
    }
}
