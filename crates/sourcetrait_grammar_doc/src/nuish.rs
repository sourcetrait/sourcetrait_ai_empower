use crate::*;

pub(crate) fn into_string_nuon(value: nu::Value) -> String {
    let cfg = nu::ToNuonConfig::default()
        .style(nuon::ToStyle::Spaces(2));
    nu::to_nuon(
        &nu::EngineState::new(),
        &value,
        cfg
    )
    .unwrap()
}