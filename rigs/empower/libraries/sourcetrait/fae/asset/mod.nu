# Static asset paths for the Empower rig

# > HUMAN 
use rig/sourcetrait/empower/fs

# Base path for Fae assets
export def role_dir []: nothing -> directory {
    const FAE_DIR: path = 'sourcetrait/empower/roles/fae'
    $env.XDGX_ASSET_HOME | path join $FAE_DIR
}

# Soak templates for Fae role
export def soak_dir []: nothing -> directory {
    const SOAK_DIR: path = 'soak'
    role_dir | path join $SOAK_DIR
}
