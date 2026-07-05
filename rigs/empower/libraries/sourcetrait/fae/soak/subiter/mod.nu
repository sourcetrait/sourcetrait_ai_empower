use rig/sourcetrait/equip/session
use rig/sourcetrait/fae/layout

# > HUMAN

# Generates a new Sub-Iter
# 
# @args.iter Parent Iter key, snake_case
# @args.subiter Sub-iter key, snake_case
export def main [args: record<iter: string, subiter: string>]: nothing -> record<created: directory> {
    const ITER: path = 'iter'
    const SUBITER: path = 'subiter'
    let skeleton_dir = (layout soak_dir | path join $SUBITER)
    let subiter_dir = (session equip_work_dir | path join $ITER $args.iter $SUBITER $args.subiter)
    
    empowered soak $skeleton_dir $subiter_dir $args
    { created: $subiter_dir }
}

