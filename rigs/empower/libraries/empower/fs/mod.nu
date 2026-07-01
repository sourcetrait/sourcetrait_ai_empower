
export def process_dir []: nothing -> directory {
    (^readlink /proc/self/cwd | str trim)
}