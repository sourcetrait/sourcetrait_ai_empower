// Plugin commands are not re-exported; referenced long-hand via `crate::...`.

pub(crate) mod eye {
    pub(crate) mod md {
        pub(crate) mod find;
    }
}
/*pub(crate) mod shm {
    pub(crate) mod dir;
    pub(crate) mod file;
    pub(crate) mod path;
    pub(crate) mod release;
    pub(crate) mod result;
    pub(crate) mod shared;
}*/
pub(crate) mod error;
pub(crate) mod plugin;

pub(crate) use crate::{
    //shm::shared::*,
};

pub(crate) use std::{
    path::{Path, PathBuf}
};

pub(crate) mod nu {
    pub(crate) use nu_plugin::{
        Plugin,
        PluginCommand,
        EngineInterface,
        EvaluatedCall,
        SimplePluginCommand,
    };
    pub(crate) use nu_protocol::{
        Category,
        Example,
        LabeledError,
        Signature,
        SyntaxShape,
        Value,
    };
}

pub(crate) use sourcetrait_ai_lib_empower as lib;

pub use crate::{
    plugin::EmpowerPlugin,
};
