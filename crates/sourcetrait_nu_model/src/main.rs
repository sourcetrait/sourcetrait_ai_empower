fn main() {
    nu_plugin::serve_plugin(
        &sourcetrait_nu_model::NuModelPlugin,
        nu_plugin::MsgPackSerializer {}
    );
}
