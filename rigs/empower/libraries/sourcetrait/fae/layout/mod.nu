# AI Repository paths

# > HUMAN
use rig/sourcetrait/empower/fs

# Soak templates for this AI 
export def soak_dir []: nothing -> directory {
    const SOAK_DIR: path = 'templates/fae/soak'
    fs process_dir | path join $SOAK_DIR
}
