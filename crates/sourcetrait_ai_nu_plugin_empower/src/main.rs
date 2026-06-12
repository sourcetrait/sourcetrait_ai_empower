fn main() {
    nu_plugin::serve_plugin(
        &sourcetrait_ai_nu_plugin_empower::EmpowerPlugin,
        nu_plugin::MsgPackSerializer {}
    );
}
