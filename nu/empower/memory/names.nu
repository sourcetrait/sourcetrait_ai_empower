# Memory snakes present in a KB dir (MEMORY.md excluded).
#
# Globs <dir>/*.md, drops the MEMORY.md index, strips the .md extension, returns
# the bare snakes sorted. The source of truth for which memories exist on disk;
# works on any fae's live or repo memory dir via the dir arg.
export def main [args: record<dir: string>]: nothing -> record<names: list<string>> {
    let names = (
        glob ($args.dir | path join "*.md")
        | each {|p| $p | path basename }
        | where {|n| $n != "MEMORY.md" }
        | each {|n| $n | str replace --regex '\.md$' '' }
        | sort
    )
    { names: $names }
}
