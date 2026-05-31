pub(crate) mod server {
    pub(crate) mod cache;
    pub(crate) mod library;
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
pub(crate) mod mode;
pub(crate) mod wire;
pub(crate) mod template;

pub(crate) use crate::{
    ipc::framing::{
        read_frame,
        read_frame_async,
        write_frame,
        write_frame_async,
    },
    mcp::ServiceExt,
    nu::FromValue,
    server::{
        cache::{
            CacheKind,
            cache_dir,
            closure_cache_file,
            data_base_dir,
        },
        library::{
            ImportError,
            LibraryLocks,
            Violation,
            define_function_impl,
            ensure_substrate,
            import_library_impl,
            register_library_impl,
            reimport_library_impl,
            undefine_function_impl,
            unregister_library_impl,
        },
        tool::{
            NuSh,
            RunParams,
        },
        worker_handle::WorkerHandle,
    },
    template::{
        build_interact_source,
        build_run_source,
    },
    wire::{
        Hello,
        PROTOCOL_VERSION,
        RunRequest,
        RunResponse,
    },
    worker::base::WarmBase,
};

pub(crate) use std::{
    collections::HashMap,
    fs,
    io,
    io::{
        Read,
        Write,
    },
    panic::{
        AssertUnwindSafe,
        catch_unwind,
    },
    path::PathBuf,
    process,
    sync::{
        Arc,
        LazyLock,
        atomic::{
            AtomicBool,
            AtomicU64,
            Ordering,
        },
    },
};

pub(crate) use sourcetrait_lib_empower as lib_empower;

pub(crate) mod dirs {
    pub(crate) use directories::BaseDirs;
}

pub(crate) mod nu {
    pub(crate) use nu_cmd_lang::create_default_context;
    pub(crate) use nu_command::add_shell_command_context;
    pub(crate) use nu_engine::eval_block;
    pub(crate) use nu_json::Value as JsonValue;
    pub(crate) use nu_parser::parse;
    pub(crate) use nu_protocol::{
        FromValue,
        PipelineData,
        Signals,
        Span,
        Value,
        debugger::WithoutDebug,
        engine::{
            EngineState,
            Stack,
            StateWorkingSet,
        },
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
        model::{
            Implementation,
            JsonObject,
            ServerCapabilities,
            ServerInfo,
        },
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
        sync::{
            Mutex as AsyncMutex,
            RwLock as AsyncRwLock,
        },
        try_join,
    };
}

pub(crate) mod json {
    pub(crate) use serde_json::{
        Value,
        from_slice,
        json,
        to_string as to_string_json,
        to_vec,
    };
}

pub use crate::{
    mode::Mode,
    server::run::run_server,
    worker::run::run_worker,
};
