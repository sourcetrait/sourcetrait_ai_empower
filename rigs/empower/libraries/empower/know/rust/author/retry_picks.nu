export def main [args: record<picks_path: string, failed_patterns: list<string>, out_dir: string, attempt: int>]: nothing -> record<retry_picks_path: string, retry_count: int> {
    let all_picks = (open --raw $args.picks_path | decode utf-8 | from json)
    let failed_set = $args.failed_patterns
    let retry_picks = ($all_picks | where {|p| $p.pattern in $failed_set})
    let retry_path = ($args.out_dir | path join $"picks-retry-($args.attempt).json")
    $retry_picks | to json | save -f $retry_path
    {
        retry_picks_path: $retry_path
        retry_count: ($retry_picks | length)
    }
}
