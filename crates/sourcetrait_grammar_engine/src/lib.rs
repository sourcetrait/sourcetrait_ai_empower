pub(crate) mod sys {
    pub(crate) mod engine {
        pub(crate) mod config;
        pub(crate) mod params;
        pub(crate) mod msg;
        pub(crate) mod paths;
        pub(crate) mod system;
    }
}
pub(crate) mod error;

pub use crate::{
    sys::engine::{
        config::EngineSysConfig,
        msg::{ToEngineSys, FromEngineSys},
        params::EngineSysParams,
        paths::EngineSysPaths,
        system::EngineSystem,
    },
};

pub mod prelude {
    pub use crate::{
    };
}

pub(crate) use std::{
    path::PathBuf,
};

pub(crate) use sourcetrait_agnostic::{self as agnostic, prelude::*};
pub(crate) use sourcetrait_cereal::{self as cereal, prelude::*};
pub(crate) use sourcetrait_sysgreen::{self as green, prelude::*};
pub(crate) use sourcetrait_tomlx::{self as tomlx, prelude::*};
