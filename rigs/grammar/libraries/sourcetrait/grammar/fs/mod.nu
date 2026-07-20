
export def process_dir []: nothing -> directory {
    ^pwdx $nu.pid | parse '{pid}: {dir}' | get dir.0
}