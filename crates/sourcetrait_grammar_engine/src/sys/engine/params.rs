use crate::*;

#[cereal::derived(Data, Eq)]
pub struct EngineSysParams {
    pub equip_id: String,
    pub equip_namespace: String,
}

impl green::Params for EngineSysParams {}

impl EngineSysParams {
    pub const fn equip_id(&self) -> &str { self.equip_id.as_str() }
    pub const fn equip_namespace(&self) -> &str { self.equip_namespace.as_str() }
}
