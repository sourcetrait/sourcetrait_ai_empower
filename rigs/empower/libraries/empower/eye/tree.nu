# Compact file tree listing.
#
# ```fs
# /path/dir
# /path/dir/.gitignore
# /path/dir/subdir
# /path/dir/subdir/file1.txt
# /path/dir/subdir/somedir/.file2
# /path/dir/subdir/otherdir
# ```
# ```tree
# /path/dir/
#  .gitignore 0
#  subdir/
#   file1.txt 32mb
#   otherdir/
#   somedir/
#    .file2 40b
# ```
#
# Ignored unless regarded: ['.git']
#
# @ignore Skip the specified globs; deny-list
# @regard Render the specified globs; allow-list, exceptional
export def main [args: record<dir: directory, ignore: list<glob>, regard: list<glob>>]: nothing -> record<tree: string> {
    { tree: (empowered eye tree $args.dir $args.ignore $args.regard) }
}
