# Generically generates file trees based on re-usable liquid skeletons
export def main [args: record<from_dir: directory, to_dir: directory, fill: record<>>]: nothing -> record<created: directory> {
    empowered soak $args.from_dir $args.to_dir $args.fill
    { created: $args.to_dir }
}