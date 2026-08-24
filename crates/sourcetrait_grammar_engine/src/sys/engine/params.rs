use crate::*;

#[cereal::derived(Data, Eq)]
pub struct EngineSysParams {
    pub equip_id: String,
    pub equip_namespace: String,
}

impl subsys::Params for EngineSysParams {}

impl EngineSysParams {
    pub(crate) const fn equip_id(&self) -> &str { self.equip_id.as_str() }
    pub(crate) const fn equip_namespace(&self) -> &str { self.equip_namespace.as_str() }
}

impl EngineSysParams {
    pub(crate) fn try_from_face(v: EngineParameters) -> GrammarEngineResult<Self> {
        Ok(Self {
            equip_id: v.equip_id
                .ok_or_else(|| GrammarEngineError::EngineParameter { parameter: EngineParameterKind::EquipId })?,
            equip_namespace: v.equip_namespace
                .ok_or_else(|| GrammarEngineError::EngineParameter { parameter: EngineParameterKind::EquipNamespace })?,
        })
    }
}

impl TryFrom<EngineParameters> for EngineSysParams {
    type Error = GrammarEngineError;
    fn try_from(v: EngineParameters) -> GrammarEngineResult<Self> { Self::try_from_face(v) }
}