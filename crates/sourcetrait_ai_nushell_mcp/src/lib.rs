pub(crate) mod server {
    pub(crate) mod blocked;
    pub(crate) mod cache;
    pub(crate) mod embed;
    pub(crate) mod error;
    pub(crate) mod executor;
    pub(crate) mod library;
    pub(crate) mod lint;
    pub(crate) mod namepath;
    pub(crate) mod nonce;
    pub(crate) mod oneshot;
    pub(crate) mod parse_engine;
    pub(crate) mod run;
    pub(crate) mod schema;
    pub(crate) mod teardown;
    pub(crate) mod tool {
        pub(crate) mod call;
        pub(crate) mod commit;
        pub(crate) mod common;
        pub(crate) mod handler;
        pub(crate) mod info;
        pub(crate) mod inspect;
        pub(crate) mod interact;
        pub(crate) mod kill;
        pub(crate) mod learn;
        pub(crate) mod library;
        pub(crate) mod new;
        pub(crate) mod processes;
        pub(crate) mod rerun;
        pub(crate) mod run;
    }
    #[cfg(test)]
    mod tests {
        mod lint;
        mod namepath;
        mod schema;
    }
}
pub(crate) mod cli;
pub(crate) mod config;
pub(crate) mod engine;
pub(crate) mod mode;
pub(crate) mod plugins;
pub(crate) mod template;

#[cfg(test)]
mod tests {
    mod template;
}

pub(crate) use crate::{
    cli::CliTool,
    config::{CONFIG, Config, DeniableTool, DenySet, config},
    engine::base_context,
    mcp::ServiceExt,
    mode::Mode,
    nu::FromValue,
    plugins::{list_registered_plugins, load_plugin_decls, registry_mtime},
    server::{
        blocked::shadow_host_fatal_decls,
        cache::{BASE_DIRS, BODY_FILE, CacheKind, cache_dir, data_base_dir, run_body_file},
        embed::{InteractEngine, build_base, eval_stateless},
        error::{Diagnostic, Error, Severity, Source, error_to_call_result},
        executor::Executor,
        library::{
            LibraryInfo, LibraryLocks, ValidationResult, check_library,
            check_source_dir, commit_impl, ensure_substrate, enumerate_libraries, establish_library,
            index_node, inspect_impl, install_impl, is_valid_ident, is_valid_library,
            is_valid_module_path,
            libraries_dir, load_index, scaffold_leaf, scaffold_leaf_exists, uninstall_impl,
        },
        lint::{LINT_VIOLATION_CAP, lint_body},
        namepath::{Namepath, NamepathRef},
        nonce::{Nonce, NonceGen},
        oneshot::run_oneshot,
        parse_engine::{
            ParseEngine, set_lib_dirs_const, span_to_line_col, wrap_as_def_body, wrap_as_module,
        },
        run::{eval_concurrency_cap, run_server},
        schema::{args_schema_to_nu, nu_to_args_schema, nu_to_result_schema, result_schema_to_nu},
        teardown::{install_child_subreaper, make_tracker, tree_kill},
        tool::{
            call::CallParams,
            commit::CommitParams,
            common::{
                CachedRunBody, InFlightKind, NuSh, RunParams, convert_schemas,
                dispatch_interact, dispatch_pooled, envelope_to_structured, lint_run_params,
            },
            info::InfoParams,
            inspect::InspectParams,
            kill::KillParams,
            learn::LearnParams,
            library::LibraryParams,
            new::NewParams,
            processes::ProcessesParams,
            rerun::RerunParams,
        },
    },
    template::{build_call_source, build_interact_source, build_run_source},
};

pub(crate) use std::{
    collections::HashMap,
    fmt::Display,
    fs, io,
    hash::{Hash, Hasher},
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

pub(crate) mod xxh3 {
    pub(crate) use xxhash_rust::xxh3::Xxh3;
}

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
    pub(crate) use nu_engine::command_prelude::Call;
    pub(crate) use nu_json::Value as JsonValue;
    pub(crate) use nu_parser::parse;
    pub(crate) use nu_parser::{FlatShape, flatten_block};
    pub(crate) use nu_path::nu_config_dir;
    pub(crate) use nu_plugin_engine::load_plugin_file;
    pub(crate) use nu_protocol::{
        BlockId, Category, DeclId, FromValue, Module, PipelineData, PluginRegistryFile,
        PluginRegistryItemData, Record, ShellError, Signals, Signature, Span, SyntaxShape, Type,
        Value, VarId,
        ast::{
            Argument, Block, Comparison, Expr, Expression, ExternalArgument, ListItem,
            Operator, Pattern, RecordItem,
        },
        debugger::WithoutDebug,
        engine::{Command, EngineState, Jobs, Mail, Stack, StateWorkingSet, ThreadJob},
    };
    pub(crate) use nu_protocol::shell_error::generic::GenericError;
    pub(crate) use nuon::{ToNuonConfig, from_nuon, to_nuon};
}

pub(crate) mod ser {
    pub(crate) use ::serde::{Deserialize, Serialize};
}

pub(crate) mod schema {
    pub(crate) use schemars::JsonSchema;
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
        spawn,
        sync::{
            Mutex as AsyncMutex, OwnedSemaphorePermit, RwLock as AsyncRwLock, Semaphore, oneshot,
            mpsc::{UnboundedSender, unbounded_channel},
        },
        time::{Duration as TkDuration, timeout},
    };
}

pub(crate) mod json {
    pub(crate) use serde_json::{Value, from_slice, to_value, to_vec};
}

pub use crate::cli::host_main;
