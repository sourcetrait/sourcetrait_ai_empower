use sourcetrait/empower/fs


# the fae role's asset tree in the empower repo layout (roles/fae), resolved
# from the process working dir.
export def role_asset_dir []: nothing -> directory {
    const ROLE_ASSET_DIR: path = 'sourcetrait/empower/roles/fae'
    fs process_dir | path join $ROLE_ASSET_DIR
}

# the skeleton subtree of the fae role's asset tree.
export def role_skeleton_asset_dir []: nothing -> directory {
    const SKELETON_DIR: path = 'skeleton'
    role_asset_dir | path join $SKELETON_DIR
}
