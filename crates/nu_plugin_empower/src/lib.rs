pub(crate) mod peek {
    pub(crate) mod md_find;
}
pub(crate) mod error;
pub(crate) mod plugin;

pub(crate) use crate::{
    peek::{
        md_find::MdFind,
    },
};

pub(crate) use std::{
    path::PathBuf
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

pub(crate) use sourcetrait_libcli_empower as libcli;

pub use crate::{
    plugin::EmpowerPlugin,
};
