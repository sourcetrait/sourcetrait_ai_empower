use nu_plugin::{MsgPackSerializer, serve_plugin};
use nu_plugin_empower::EmpowerPlugin;

fn main() {
    serve_plugin(&EmpowerPlugin, MsgPackSerializer {});
}
