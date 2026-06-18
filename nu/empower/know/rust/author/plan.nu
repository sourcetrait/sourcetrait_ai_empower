export def main [args: record<picks_path: string, batch_size: int, out_dir: string>]: nothing -> record<batch_paths: list<string>, total_picks: int, batches: int> {
    let all_picks = (open --raw $args.picks_path | decode utf-8 | from json)
    let total = ($all_picks | length)
    mkdir $args.out_dir
    let batches = ($all_picks | chunks $args.batch_size)
    let batch_count = ($batches | length)
    let batch_paths = ($batches | enumerate | each {|r|
        let path = ($args.out_dir | path join $"picks-batch-($r.index).json")
        $r.item | to json | save -f $path
        $path
    })
    {
        batch_paths: $batch_paths
        total_picks: $total
        batches: $batch_count
    }
}
