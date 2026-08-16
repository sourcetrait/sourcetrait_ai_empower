fn main() {
    nu_plugin::serve_plugin(
        &sourcetrait_grimoire::GrimoirePlugin,
        nu_plugin::MsgPackSerializer {}
    );
}
