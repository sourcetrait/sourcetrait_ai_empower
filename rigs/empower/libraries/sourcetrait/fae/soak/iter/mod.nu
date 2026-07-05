use rig/sourcetrait/equip/session
use rig/sourcetrait/fae/layout

# > HUMAN

# Generates a new Iter
# 
# @args.iter Key, snake_case
# @args.subiters Optional sub-component expertise
# @args.subiters.name Key, snake_case
# @args.subiters.summary Short, single-line
export def main [args: record<iter: string, subiters: table<name: string, summary: string>>]: nothing -> record<created: directory> {
    const ITER: path = 'iter'
    let skeleton_dir = (layout soak_dir | path join $ITER)
    let iter_dir = (session equip_work_dir | path join $ITER $args.iter)
    
    empowered soak $skeleton_dir $iter_dir $args
    { created: $iter_dir }
}

