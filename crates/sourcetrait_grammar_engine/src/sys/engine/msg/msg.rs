use crate::*;

#[cereal::derived(Data)]
pub enum ToEngineSys {
    DefRequest(NuRequest),
}

#[cereal::derived(Data)]
pub enum FromEngineSys {
    DefResponse(NuResponse),
}

#[cereal::derived(Data, Copy, Eq)]
#[repr(u8)]
pub enum DefKind {
    Execute,
    Call,
    Interact,
}

#[cereal::derived(Data, Copy, Eq)]
pub enum Host {
    Local,
    Remote(datum::Nom),
}

#[cereal::derived(Data)]
pub struct NuRequest {
    host: Host,
    kind: DefKind,
    args: nuin::Val,
    def: String, 
}

pub type NuResult = Result<nuin::Val, EngineError>;

#[cereal::derived(Data)]
pub enum EngineError {
    Nu(NuError),
}

#[cereal::derived(Data)]
pub enum NuError {
    Unknown,
}

#[cereal::derived(Data)]
pub struct NuResponse {
    result: NuResult,
    nonce: u64,
}
