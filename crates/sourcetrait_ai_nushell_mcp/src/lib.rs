pub(crate) mod server {
    pub(crate) mod cache;
    pub(crate) mod error;
    pub(crate) mod library;
    pub(crate) mod lint;
    pub(crate) mod parse_engine;
    pub(crate) mod pool;
    pub(crate) mod run;
    pub(crate) mod schema;
    pub(crate) mod tool {
        pub(crate) mod call;
        pub(crate) mod commit;
        pub(crate) mod common;
        pub(crate) mod delete;
        pub(crate) mod handler;
        pub(crate) mod info;
        pub(crate) mod inspect;
        pub(crate) mod interact;
        pub(crate) mod kill;
        pub(crate) mod learn;
        pub(crate) mod new;
        pub(crate) mod processes;
        pub(crate) mod rerun;
        pub(crate) mod run;
    }
    pub(crate) mod worker_handle;
}
pub(crate) mod worker {
    pub(crate) mod base;
    pub(crate) mod request_loop;
    pub(crate) mod run;
}
pub(crate) mod ipc {
    pub(crate) mod framing;
}
pub(crate) mod build_target;
pub(crate) mod cli;
pub(crate) mod mode;
pub(crate) mod plugins;
pub(crate) mod template;
pub(crate) mod wire;

pub(crate) use crate::{
    build_target::build_target,
    cli::parse_worker_mode,
    ipc::framing::{read_frame, read_frame_async, write_frame, write_frame_async},
    mcp::ServiceExt,
    nu::FromValue,
    plugins::list_registered_plugins,
    server::{
        cache::{CacheKind, cache_dir, closure_cache_file, data_base_dir},
        error::{Error, Where, WhereSource, error_to_call_result},
        library::{
            LibraryInfo, LibraryLocks, Violation, call_file_path, commit_impl, delete_impl,
            ensure_substrate, enumerate_libraries, index_node, inspect_impl, load_index, new_impl,
        },
        lint::{LINT_VIOLATION_CAP, LintViolation, lint_body},
        parse_engine::{ParseEngine, span_to_line_col, wrap_as_def_body, wrap_as_module},
        pool::Pool,
        schema::{args_schema_to_nu, nu_to_args_schema, nu_to_result_schema, result_schema_to_nu},
        tool::common::{
            ClosureCacheBody, InFlightKind, NuSh, RunParams, convert_schemas, dispatch_interact,
            dispatch_pooled, envelope_to_structured, lint_run_params,
        },
        worker_handle::{WorkerHandle, kill_worker_pid},
    },
    template::{build_call_source, build_interact_source, build_run_source},
    wire::{Hello, PROTOCOL_VERSION, RunRequest, RunResponse},
    worker::base::WarmBase,
};

pub(crate) use std::{
    collections::HashMap,
    fs, io,
    io::{Read, Write},
    ops::ControlFlow,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    process,
    sync::{
        Arc, LazyLock, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) use clap::Parser;

pub(crate) use sourcetrait_ai_lib_empower as lib_empower;

pub(crate) mod dirs {
    pub(crate) use directories::BaseDirs;
}

pub(crate) mod nu {
    pub(crate) use nu_cmd_lang::create_default_context;
    pub(crate) use nu_cmd_extra::add_extra_command_context;
    pub(crate) use nu_cmd_plugin::add_plugin_command_context;
    pub(crate) use nu_command::add_shell_command_context;
    pub(crate) use nu_command::tls::CRYPTO_PROVIDER;
    pub(crate) use nu_engine::eval_block;
    pub(crate) use nu_json::Value as JsonValue;
    pub(crate) use nu_parser::parse;
    pub(crate) use nu_parser::{FlatShape, flatten_block};
    pub(crate) use nu_path::nu_config_dir;
    pub(crate) use nu_plugin_engine::load_plugin_file;
    pub(crate) use nu_protocol::{
        BlockId, DeclId, FromValue, Module, PipelineData, PluginRegistryFile,
        PluginRegistryItemData, Record, Signals, Span, Type, Value, VarId,
        ast::{
            Argument, Block, Comparison, Expr, Expression, ExternalArgument, ListItem, Operator,
            Pattern, RecordItem,
        },
        debugger::WithoutDebug,
        engine::{EngineState, Stack, StateWorkingSet},
    };
    pub(crate) use nuon::{ToNuonConfig, to_nuon};
}

pub(crate) mod sys {
    pub(crate) use nix::sys::signal::{Signal, kill};
    pub(crate) use nix::unistd::{Pid, setsid};
}

pub(crate) mod ser {
    pub(crate) use ::serde::{Deserialize, Serialize, Serializer};
}

pub(crate) mod schema {
    pub(crate) use schemars::JsonSchema;
}

pub(crate) mod msgpack {
    pub(crate) use rmp_serde::{from_slice, to_vec_named};
}

pub(crate) mod mcp {
    pub(crate) use rmcp::{
        ErrorData, ServerHandler, ServiceExt,
        handler::server::router::tool::ToolRouter,
        handler::server::tool::schema_for_type,
        handler::server::wrapper::Parameters,
        model::{CallToolResult, Implementation, JsonObject, ServerCapabilities, ServerInfo},
        tool, tool_handler, tool_router,
        transport::stdio,
    };
}

pub(crate) mod tk {
    pub(crate) use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        process::{Child, ChildStdin, ChildStdout, Command},
        runtime::Runtime,
        spawn,
        sync::{Mutex as AsyncMutex, OwnedSemaphorePermit, RwLock as AsyncRwLock, Semaphore},
        time::{Duration as TkDuration, interval, timeout},
    };
}

pub(crate) mod json {
    pub(crate) use serde_json::{Value, from_slice, to_value, to_vec};
}

pub use crate::{
    build_target::BuildTarget, mode::Mode, server::run::run_server, worker::run::worker_main,
};
