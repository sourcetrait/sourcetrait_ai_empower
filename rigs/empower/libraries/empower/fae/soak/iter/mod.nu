
const FAE_HARNESS_ASSET_DIR: path = 'sourcetrait/empower/harness/fae'
const ITER_SKELETON_DIR: path = 'skeleton/iter'

# Generates a new Iter
# 
# @args.iter Identity, snake_case
# @args.subiters Optional sub-component expertise
# @args.subiters.name Identity, snake_case
# @args.subiters.summary Short, single-line
export def main [args: record<iter: string, subiters: table<name: string, summary: string>>]: nothing -> record<created: directory> {
    let skeleton_dir = (
        $env.XDGX_ASSET_HOME | path join $FAE_HARNESS_ASSET_DIR | $ITER_SKELETON_DIR
    )
    let iter_dir = (pwd | path join 'iter' | path join $args.iter)
    
    empowered soak $skeleton_dir $iter_dir $args
    { created: $iter_dir }
}

