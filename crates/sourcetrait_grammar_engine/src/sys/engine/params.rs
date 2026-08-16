use crate::*;

#[cereal::derived(Eq, Data)]
pub struct EngineSysParams {
}

impl green::Params for EngineSysParams {}
