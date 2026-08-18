use crate::*;

#[cereal::derived(Data)]
pub enum ToEngineSys {
    NuRequest(NuRequest),
    NuRemotesRequest(NuRemotesRequest),
}

#[cereal::derived(Data)]
pub enum FromEngineSys {
    NuResponse(NuResponse),
    NuRemotesResponse(NuRemotesResponse),
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

#[cereal::derived(Data, Eq)]
pub struct RemoteAlias {
    pub nom: datum::Nom,
    pub alias: String,
}

#[cereal::derived(Data)]
pub struct NuRequest {
    pub host: Host,
    pub kind: DefKind,
    pub args: nuin::Val,
    pub def: String, 
}

#[cereal::derived(Data, Copy, Eq)]
pub struct NuRemotesRequest;

#[cereal::derived(Data, Eq)]
pub struct NuRemotesResponse {
    pub remotes: Vec<RemoteAlias>,
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
    pub result: NuResult,
    pub nonce: u64,
}
