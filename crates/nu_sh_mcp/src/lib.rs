pub(crate) mod server {
    pub(crate) mod cache;
    pub(crate) mod library;
    pub(crate) mod lint;
    pub(crate) mod parse_engine;
    pub(crate) mod pool;
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
pub(crate) mod plugins;

pub(crate) use crate::{
    ipc::framing::{
        read_frame,
        read_frame_async,
        write_frame,
        write_frame_async,
    },
    mcp::ServiceExt,
    nu::FromValue,
    plugins::list_registered_plugins,
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
            ValidationResult,
            Violation,
            call_file_path,
            define_function_impl,
            ensure_substrate,
            import_library_impl,
            parse_check_function_source,
            register_library_impl,
            reimport_library_impl,
            undefine_function_impl,
            unregister_library_impl,
        },
        lint::{
            LintViolation,
            format_lint_violations,
            lint_block,
            lint_body,
        },
        parse_engine::{
            ParseEngine,
            span_to_line_col,
            wrap_as_def_body,
            wrap_as_module,
        },
        pool::Pool,
        tool::{
            NuSh,
            RunParams,
        },
        worker_handle::{
            WorkerHandle,
            kill_worker_pid,
        },
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
            AtomicUsize,
            Ordering,
        },
    },
    time::{
        SystemTime,
        UNIX_EPOCH,
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
    pub(crate) use nu_path::nu_config_dir;
    pub(crate) use nu_plugin_engine::load_plugin_file;
    pub(crate) use nu_protocol::{
        BlockId,
        DeclId,
        FromValue,
        Module,
        PipelineData,
        PluginRegistryFile,
        PluginRegistryItemData,
        Record,
        Signals,
        Span,
        SyntaxShape,
        Value,
        VarId,
        ast::{
            Argument,
            Block,
            Comparison,
            Expr,
            Expression,
            ExternalArgument,
            ListItem,
            Operator,
            Pattern,
            RecordItem,
        },
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
    pub(crate) use nix::sys::signal::{
        Signal,
        kill,
    };
    pub(crate) use nix::unistd::{
        Pid,
        setsid,
    };
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
        handler::server::tool::schema_for_type,
        handler::server::wrapper::Parameters,
        model::{
            CallToolResult,
            ErrorCode,
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
        spawn,
        sync::{
            Mutex as AsyncMutex,
            OwnedSemaphorePermit,
            RwLock as AsyncRwLock,
            Semaphore,
        },
        time::{
            Duration as TkDuration,
            interval,
            timeout,
        },
    };
}

pub(crate) mod json {
    pub(crate) use serde_json::{
        Value,
        from_slice,
        to_string as to_string_json,
        to_value,
        to_vec,
    };
}

pub use crate::{
    mode::Mode,
    server::run::run_server,
    worker::run::run_worker,
};
