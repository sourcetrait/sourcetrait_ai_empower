pub(crate) mod server {
    pub(crate) mod run;
    pub(crate) mod tool;
    pub(crate) mod worker_handle;
}
pub(crate) mod worker {
    pub(crate) mod run;
    pub(crate) mod base;
    pub(crate) mod request_loop;
}
pub(crate) mod ipc {
    pub(crate) mod framing;
}
pub(crate) mod wire;
pub(crate) mod template;

pub(crate) use crate::{
    ipc::framing::{
        read_frame,
        read_frame_async,
        write_frame,
        write_frame_async,
    },
    server::{
        tool::{
            NuSh,
            RunParams,
        },
        worker_handle::WorkerHandle,
    },
    template::build_run_source,
    wire::{
        Hello,
        PROTOCOL_VERSION,
        RunRequest,
        RunResponse,
    },
    worker::base::WarmBase,
};

pub(crate) use std::{
    io,
    io::{
        Read,
        Write,
    },
    path::PathBuf,
    process,
    sync::Arc,
};

pub(crate) mod nu {
    pub(crate) use nu_cmd_lang::create_default_context;
    pub(crate) use nu_command::add_shell_command_context;
    pub(crate) use nu_engine::eval_block;
    pub(crate) use nu_parser::parse;
    pub(crate) use nu_protocol::{
        PipelineData,
        Signals,
        Span,
        debugger::WithoutDebug,
        engine::{
            EngineState,
            Stack,
            StateWorkingSet,
        },
    };
    pub(crate) use nuon::{
        ToNuonConfig,
        to_nuon,
    };
}

pub(crate) mod sys {
    pub(crate) use nix::unistd::setsid;
}

pub(crate) mod ser {
    pub(crate) use ::serde::{
        Deserialize,
        Serialize,
    };
}

pub(crate) mod schema {
    pub(crate) use schemars::JsonSchema;
}

pub(crate) mod msgpack {
    pub(crate) use rmp_serde::{
        from_slice,
        to_vec_named,
    };
}

pub(crate) mod mcp {
    pub(crate) use rmcp::{
        ErrorData,
        ServerHandler,
        ServiceExt,
        handler::server::router::tool::ToolRouter,
        handler::server::wrapper::Parameters,
        tool,
        tool_handler,
        tool_router,
        transport::stdio,
    };
}

pub(crate) mod tk {
    pub(crate) use tokio::{
        io::{
            AsyncReadExt,
            AsyncWriteExt,
        },
        process::{
            Child,
            ChildStdin,
            ChildStdout,
            Command,
        },
        runtime::Runtime,
        sync::Mutex as AsyncMutex,
    };
}

pub(crate) mod json {
    pub(crate) use serde_json::{
        Value,
        json,
        to_string as to_string_json,
    };
}

pub use crate::{
    server::run::run_server,
    worker::run::run_worker,
};
