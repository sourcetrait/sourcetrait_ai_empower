export def main [args: record<out_dir: string, picks_batch_path: string>]: nothing -> record<valid_count: int, failed_count: int, valid_patterns: list<string>, failed_patterns: list<string>, failed_details: table<pattern: string, reason: string, bytes: int>> {
    let picks = (open --raw $args.picks_batch_path | decode utf-8 | from json)

    def slug [pattern: string] {
        $pattern | str replace --all --regex '[^A-Za-z0-9_]' '_'
    }

    def classify [args: record<text: string, exists: bool>] {
        if not $args.exists {
            return "missing_file"
        }
        if ($args.text | str length) < 60 {
            return "too_small"
        }
        let lower = ($args.text | str downcase)
        if ($lower | str contains "api error") or ($lower | str contains "rate limited") or ($lower | str contains "temporarily limiting") or ($lower | str contains "rate_limited") {
            return "rate_limited"
        }
        if not ($args.text | str trim | str starts-with "### ") {
            return "no_header"
        }
        "valid"
    }

    let classified = ($picks | each {|p|
        let dir = ($args.out_dir | path join (slug $p.pattern))
        # Final kp = stage_d.md when Stage D reduced; else stage_c.md
        # (Stage B unknown -> Stage D no-op -> the Stage C draft IS final).
        let stage_d = ($dir | path join "stage_d.md")
        let final = (if ($stage_d | path exists) { $stage_d } else { ($dir | path join "stage_c.md") })
        let exists = ($final | path exists)
        let text = if $exists { (open --raw $final | decode utf-8) } else { "" }
        let reason = (classify {text: $text, exists: $exists})
        {pattern: $p.pattern, reason: $reason, valid: ($reason == "valid"), bytes: ($text | str length)}
    })

    let valid = ($classified | where valid)
    let failed = ($classified | where not valid)

    {
        valid_count: ($valid | length)
        failed_count: ($failed | length)
        valid_patterns: ($valid | get pattern)
        failed_patterns: ($failed | get pattern)
        failed_details: ($failed | each {|f| {pattern: $f.pattern, reason: $f.reason, bytes: $f.bytes}})
    }
}
