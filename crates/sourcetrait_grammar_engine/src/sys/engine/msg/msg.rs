use crate::*;

#[cereal::derived(Data)]
pub enum ToEngineSys {
    NuRepl(NuReplRequest),
    NuDef(NuDefRequest),
    NuRig(NuRigRequest),
    NuBed(NuBedRequest),
    ReNu(ReNuRequest),
}

#[cereal::derived(Data)]
pub enum FromEngineSys {
    NuReplResponse(EngineResult<nuin::ValResult>),
    NuDefResponse(EngineResult<NuDefResponse>),
    ReNuResponse(EngineResult<NuDefResponse>),
}

#[cereal::derived(Data, Copy, Eq)]
#[repr(u8)]
pub enum NuDefKind {
    Execute,
    Interact,
}

impl NuDefKind {
    pub const STR_EXECUTE: &'static str  = "execute";
    pub const STR_INTERACT: &'static str = "interact";

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Execute => Self::STR_EXECUTE,
            Self::Interact => Self::STR_INTERACT,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            Self::STR_EXECUTE => Some(Self::Execute),
            Self::STR_INTERACT => Some(Self::Interact),
            _ => None
        }
    }
    
    pub fn from_src(nu_src: String) -> Option<Self> {
        let mut lines = nu_src.lines();
        let mut def = None;
        while let Some(line) = lines.next() && def == None {
            if !line.starts_with('#') {
                def = Some(line);
            }  
        }
    
        let Some(def) = def else { return None };
        let Some(def) = def.strip_prefix("def ") else { return None };
        let Some((def, _)) = def.split_once(' ') else { return None };
        Self::from_str(def)
    }
}

#[cereal::derived(Data, Copy, Eq)]
pub enum Host {
    Local,
    Remote(datum::Nom),
}

#[cereal::derived(Data, Copy, Eq)]
pub enum RemoteHostStatus {
    Connected,
    Disconnected,
}

#[cereal::derived(Data, Copy, Eq)]
pub enum RemoteHostDefined {
    Configuration,
    Adhoc,
}

#[cereal::derived(Data, Eq)]
pub struct RemoteHost {
    pub nom: datum::Nom,
    pub alias: HostAlias,
    pub defined: RemoteHostDefined,
    pub status: RemoteHostStatus,
}

#[cereal::derived(Data, Eq)]
pub struct ChannelRemoteHost {
    pub nom: datum::Nom,
    pub alias: HostAlias,
}

#[cereal::derived(Data, Eq)]
pub struct HostAlias(pub String);

impl HostAlias {
    pub const fn as_str(&self) -> &str { self.0.as_str() }
    pub fn into_string(self) -> String { self.0 }
    pub fn try_new(alias: String) -> GrammarEngineResult<Self> {
        Ok(Self(alias))
    }
}

/// Runs the provided Nu def against the specified args data.
#[cereal::derived(Data)]
pub struct NuDefRequest {
    pub kind: NuDefKind,
    pub host: Host,
    /// Either Table or Record
    pub args: nuin::Val,
    pub def: String, 
}

/// Runs the specified rig call against the provided args.
#[cereal::derived(Data)]
pub struct NuRigRequest {
    pub host: Host,
    pub namepath: String,
    /// Either Table or Record
    pub args: nuin::Val,
}

/// Runs the single specified embedded command against the provided parameters.
#[cereal::derived(Data)]
pub struct NuBedRequest {
    pub host: Host,
    pub command: String,
    /// Only [nuin::Val::List]
    pub parameters: nuin::Val,
}

/// Runs a Nushell REPL in a stateless, system-less, engine state.
/// Useful for evaluations against syntax, data, format, and mathematics.
#[cereal::derived(Data)]
pub struct NuReplRequest {
    pub nu: String, 
}

/// Re-runs a previous [NuDefRequest] with new args data, by its former nonce.
#[cereal::derived(Data)]
pub struct ReNuRequest {
    pub host: Host,
    pub nonce: datum::Nonce,
    /// Either Table or Record
    pub args: nuin::Val,
}

#[cereal::derived(Data, Copy, Eq)]
pub enum RemoteHostKind {
    Channel,
    Nu,
}

/// Retrieves a list of remotes, optionally filtered.
#[cereal::derived(Data, Copy, Eq)]
pub struct RemotesRequest {
    kind: Option<RemoteHostKind>,
    defined: Option<RemoteHostDefined>,
    status: Option<RemoteHostStatus>,
}

#[cereal::derived(Data, Copy, Eq)]
pub struct ChannelRemotesRequest;

#[cereal::derived(Data, Eq)]
pub struct RemotesResponse {
    pub remotes: Vec<RemoteHost>,
}

pub type EngineResult<T> = Result<T, EngineSystemError>;

#[cereal::derived(Data)]
pub enum EngineSystemError {
    Unknown,
}

#[cereal::derived(Data)]
pub enum NuError {
    Unknown,
}

#[cereal::derived(Data)]
pub struct NuDefResponse {
    pub nonce: datum::Nonce,
    pub result: nuin::ValResult,
}

#[cereal::derived(Data, Eq)]
pub struct ChannelOpenResponse {
    pub status: ChannelStatus,
    pub wss: String,
    pub inbox: String,
}

#[cereal::derived(Data, Copy, Eq)]
pub enum ChannelStatus {
    New,
    Existing,
}

impl ChannelStatus {
    pub const STR_NEW: &'static str = "new";
    pub const STR_EXISTING: &'static str = "existing";

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::New => Self::STR_NEW,
            Self::Existing => Self::STR_EXISTING,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            Self::STR_NEW => Some(Self::New),
            Self::STR_EXISTING => Some(Self::Existing),
            _ => None,
        }
    }
}

pub trait EngineRequest: Into<ToEngineSys> {
    type ResponseType;
}

impl From<NuReplRequest> for ToEngineSys { fn from(v: NuReplRequest) -> Self { Self::NuRepl(v) } }
impl EngineRequest for NuReplRequest {
    type ResponseType = nuin::ValResult;
}

impl From<NuDefRequest> for ToEngineSys { fn from(v: NuDefRequest) -> Self { Self::NuDef(v) } }
impl EngineRequest for NuDefRequest {
    type ResponseType = NuDefResponse;
}

impl From<ReNuRequest> for ToEngineSys { fn from(v: ReNuRequest) -> Self { Self::ReNu(v) } }
impl EngineRequest for ReNuRequest {
    type ResponseType = NuDefResponse;
}