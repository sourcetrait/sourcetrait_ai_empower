fn main() {
    nu_plugin::serve_plugin(
        &sourcetrait_ai_nu_plugin_empowered::EmpowerPlugin,
        nu_plugin::MsgPackSerializer {}
    );
}
