
export def "report info" [who: string]: string -> nothing {
    print $"(ansi blue)[($who)](ansi reset) ($in)"
}

export def "report ok" [who: string]: string -> nothing {
    print $"(ansi green)[($who)](ansi reset) ($in)"
}

export def "report warn" [who: string]: string -> nothing {
    print $"(ansi yellow)[($who)](ansi reset) ($in)"
}

export def "ask yes" [who: string]: string -> bool {
    let prompt: string = $in
    let ok: string = input $"(ansi yellow)<($who)>(ansi reset) ($prompt)? [yes/(ansi d)no(ansi rst_d)]: " | str downcase
    $ok == "yes"
}

export def abort [who: string]: nothing -> nothing {
    print $"(ansi yellow)[($who)](ansi reset) (ansi bo)aborted(ansi rst_bo)"
    exit 1
}

export def linkdir [from: directory, to: directory]: nothing -> nothing {
    match $nu.os-info.name {
        "windows" => { ^mklink /D $from $to }
        _ => { ^ln -s $from $to }
    }
}

