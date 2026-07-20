fn main() {
    nu_plugin::serve_plugin(
        &sourcetrait_nu_plugin_grimoire::GrimoirePlugin,
        nu_plugin::MsgPackSerializer {}
    );
}
