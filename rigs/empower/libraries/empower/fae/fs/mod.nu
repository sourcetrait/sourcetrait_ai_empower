
export def harness_asset_dir []: nothing -> directory {
    const HARNESS_ASSET_DIR: path = 'sourcetrait/empower/harness/fae'
    empower fs process_dir | path join $HARNESS_ASSET_DIR
}

export def harness_skeleton_asset_dir []: nothing -> directory {
    const SKELETON_DIR: path = 'skeleton'
    harness_asset_dir | path join $SKELETON_DIR
}
