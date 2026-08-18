use crate::*;

#[cereal::derived(Data)]
pub enum ToEngineSys {
    DefRequest(DefRequest),
}

#[cereal::derived(Data)]
pub enum FromEngineSys {
    DefResponse(DefResponse),
}

#[cereal::derived(Data, Copy, Eq)]
#[repr(u8)]
pub enum DefKind {
    Execute,
    Call,
    Interact,
}

#[cereal::derived(Data)]
pub struct DefRequest {
    kind: DefKind,
    remote: Option<datum::Nom>,
    args: vocab::Val,
    def: String, 
}

pub type DefResult = Result<vocab::Val, EngineError>;

#[cereal::derived(Data)]
pub enum EngineError {
    Def(DefError),
}

#[cereal::derived(Data)]
pub enum DefError {
    Unknown,
}

#[cereal::derived(Data)]
pub struct DefResponse {
    result: DefResult,
    nonce: u64,
}
