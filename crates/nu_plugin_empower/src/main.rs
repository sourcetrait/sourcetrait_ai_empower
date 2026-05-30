fn main() {
    nu_plugin::serve_plugin(
        &nu_plugin_empower::EmpowerPlugin,
        nu_plugin::MsgPackSerializer {}
    );
}
