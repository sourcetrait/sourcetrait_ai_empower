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
    NuRepl(EngineResult<NuReplResponse>),
    NuDef(EngineResult<NuDefResponse>),
    ReNu(EngineResult<ReNuResponse>),
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
pub enum HostKind {
    Think,
    Local,
    Remote,
}

impl HostKind {
    pub const STR_THINK: &'static str = "think";
    pub const STR_LOCAL: &'static str = "local";
    pub const STR_REMOTE: &'static str = "remote";
    
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Think => Self::STR_THINK,
            Self::Local => Self::STR_LOCAL,
            Self::Remote => Self::STR_REMOTE,
        }
    }
}

#[cereal::derived(Data, Copy, Eq)]
pub enum Host {
    /// Only available when a thinkspace harness exists
    Think,
    Local,
    Remote(datum::Nom),
}

impl Host {
    pub const fn kind(&self) -> HostKind {
        match self {
            Self::Think => HostKind::Think,
            Self::Local => HostKind::Local,
            Self::Remote(_) => HostKind::Remote,
        }
    }

    pub const fn into_host_str(self) -> HostStr {
        match self {
            Self::Think => HostStr::Think,
            Self::Local => HostStr::Local,
            Self::Remote(nom) => HostStr::Remote(nom.into_pair()),
        }
    }
}

/// Variation of [Host] that carries a parsed string.
/// Not intended for transmission (use [Host]).
#[cereal::derived(Data, Eq)]
pub enum HostStr {
    Think,
    Local,
    Remote(datum::NomPair),
}

impl HostStr {
    pub const fn kind(&self) -> HostKind {
        match self {
            Self::Think => HostKind::Think,
            Self::Local => HostKind::Local,
            Self::Remote(_) => HostKind::Remote,
        }
    }
    
    pub const fn into_host(self) -> Host {
        match self {
            Self::Think => Host::Think,
            Self::Local => Host::Local,
            Self::Remote(nompair) => Host::Remote(nompair.nom()),
        }
    }
}

impl Display for HostStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Think => f.write_str(HostKind::STR_THINK),
            Self::Local => f.write_str(HostKind::STR_LOCAL),
            Self::Remote(nompair) => {
                f.write_str(HostKind::STR_REMOTE)?;
                f.write_char('/')?;
                f.write_str(nompair.as_str())
            }
        }
    }
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
    pub host: Host,
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

pub type EngineResult<T> = Result<T, EngineError>;

#[cereal::derived(Data)]
pub enum EngineError {
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

#[cereal::derived(Data)]
pub struct ReNuResponse {
    pub nonce: datum::Nonce,
    pub result: nuin::ValResult,
}

#[cereal::derived(Data)]
pub struct NuReplResponse {
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

/*
impl From<NuReplRequest> for ToEngineSys { fn from(v: NuReplRequest) -> Self { Self::NuRepl(v) } }
impl subsys::Request<EngineSystem> for NuReplRequest {
    type ResponseType = nuin::ValResult;
}
*/
impl From<NuReplRequest> for ToEngineSys { fn from(v: NuReplRequest) -> Self { Self::NuRepl(v) } }
impl TryFrom<FromEngineSys> for EngineResult<NuReplResponse> {
    type Error = subsys::SubsysError;
    fn try_from(v: FromEngineSys) -> subsys::SubsysResult<Self> {
        match v {
            FromEngineSys::NuRepl(r) => Ok(r),
            _ => Err(subsys::SubsysError::ResponseType)
        }
    }
}
impl subsys::Request<EngineSystem> for NuReplRequest {
    type ResponseType = EngineResult<NuReplResponse>;
}

impl From<NuDefRequest> for ToEngineSys { fn from(v: NuDefRequest) -> Self { Self::NuDef(v) } }
impl TryFrom<FromEngineSys> for EngineResult<NuDefResponse> {
    type Error = subsys::SubsysError;
    fn try_from(v: FromEngineSys) -> subsys::SubsysResult<Self> {
        match v {
            FromEngineSys::NuDef(r) => Ok(r),
            _ => Err(subsys::SubsysError::ResponseType)
        }
    }
}
impl subsys::Request<EngineSystem> for NuDefRequest {
    type ResponseType = EngineResult<NuDefResponse>;
}

/*
impl From<ReNuRequest> for ToEngineSys { fn from(v: ReNuRequest) -> Self { Self::ReNu(v) } }
impl EngineRequest for ReNuRequest {
    type ResponseType = NuDefResponse;
}
*/
impl From<ReNuRequest> for ToEngineSys { fn from(v: ReNuRequest) -> Self { Self::ReNu(v) } }
impl TryFrom<FromEngineSys> for EngineResult<ReNuResponse> {
    type Error = subsys::SubsysError;
    fn try_from(v: FromEngineSys) -> subsys::SubsysResult<Self> {
        match v {
            FromEngineSys::ReNu(r) => Ok(r),
            _ => Err(subsys::SubsysError::ResponseType)
        }
    }
}
impl subsys::Request<EngineSystem> for ReNuRequest {
    type ResponseType = EngineResult<ReNuResponse>;
}
