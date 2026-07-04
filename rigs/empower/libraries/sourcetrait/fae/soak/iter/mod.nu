use rig/sourcetrait/empower/fs
use rig/sourcetrait/fae/layout


# Generates a new Iter
# 
# @args.iter Identity, snake_case
# @args.subiters Optional sub-component expertise
# @args.subiters.name Identity, snake_case
# @args.subiters.summary Short, single-line
export def main [args: record<iter: string, subiters: table<name: string, summary: string>>]: nothing -> record<created: directory> {
    const ITER: path = 'iter'
    let skeleton_dir = (layout soak_dir | path join $ITER)
    let iter_dir = (fs process_dir | path join 'iter' | path join $args.iter)
    
    empowered soak $skeleton_dir $iter_dir $args
    { created: $iter_dir }
}

