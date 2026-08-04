pub(crate) mod server {
    pub(crate) mod blocked;
    pub(crate) mod cache;
    pub(crate) mod cert;
    pub(crate) mod channel {
        pub(crate) mod hub;
        pub(crate) mod state;
    }
    pub(crate) mod cycle;
    pub(crate) mod embed;
    pub(crate) mod emergency;
    pub(crate) mod error;
    pub(crate) mod executor;
    pub(crate) mod rig;
    pub(crate) mod lint;
    pub(crate) mod liveness;
    pub(crate) mod namepath;
    pub(crate) mod nonce;
    pub(crate) mod oneshot;
    pub(crate) mod parse_engine;
    pub(crate) mod pin;
    pub(crate) mod purview;
    pub(crate) mod remote {
        pub(crate) mod codec;
        pub(crate) mod link;
        pub(crate) mod verify;
    }
    pub(crate) mod run;
    pub(crate) mod schema;
    pub(crate) mod teardown;
    pub(crate) mod watchdog;
    #[cfg(feature = "test-hooks")]
    pub(crate) mod test_hooks;
    pub(crate) mod tool {
        pub(crate) mod call;
        pub(crate) mod channel_close;
        pub(crate) mod channel_open;
        pub(crate) mod channel_verified;
        pub(crate) mod commit;
        pub(crate) mod config_channel;
        pub(crate) mod common;
        pub(crate) mod handler;
        pub(crate) mod info;
        pub(crate) mod inspect;
        pub(crate) mod interact;
        pub(crate) mod kill;
        pub(crate) mod learn;
        pub(crate) mod rig;
        pub(crate) mod new;
        pub(crate) mod processes;
        pub(crate) mod purview_configure;
        pub(crate) mod purview_extend;
        pub(crate) mod purviews;
        pub(crate) mod purview;
        pub(crate) mod remote_channel_open;
        pub(crate) mod remote_channel_close;
        pub(crate) mod remote_channels;
        pub(crate) mod rerun;
        pub(crate) mod run;
    }
    #[cfg(test)]
    mod tests {
        mod channel;
        mod emergency;
        mod remote;
        mod rig;
        mod namepath;
        mod pin;
        mod schema;
        mod teardown;
        mod watchdog;
    }
}
pub(crate) mod nuapi {
    pub(crate) mod grimm {
        pub(crate) mod channel_send;
        pub(crate) mod common;
        pub(crate) mod config;
        pub(crate) mod dbg;
        pub(crate) mod remote_send;
    }
}
#[cfg(test)]
mod tests {
    mod config;
}
pub(crate) mod cli;
pub(crate) mod config;
pub(crate) mod engine;
pub(crate) mod mode;
pub(crate) mod plugins;
pub(crate) mod template;
pub mod guts;

pub(crate) use crate::{
    nuapi::grimm::{
        channel_send::GrimmChannelSend,
        common::{NuapiCall, data_shape, register_nuapi, require_record_or_table},
        config::{GrimmGetConfig, GrimmGetConfigAll, GrimmPinConfig},
        dbg::GrimmDbg,
        remote_send::{GrimmRemoteChannelSend, GrimmRemoteChannelSendWith},
    },
    cli::CliTool,
    config::{
        CONFIG, Config, ConfigToml, DEFAULT_NAMESPACE, DeniableTool, DenySet, RemoteEntry,
        RemoteRole, SpamThresholds, SupervisorConfig, TEST_NAMESPACE, config, default_id,
        default_work_dir, expand_path, fraction_field,
    },
    engine::base_context,
    mcp::ServiceExt,
    mode::Mode,
    nu::CallExt,
    nu::FromValue,
    plugins::{list_registered_plugins, load_plugin_decls, registry_mtime},
    server::{
        blocked::shadow_host_fatal_decls,
        cache::{
            BASE_DIRS, BODY_FILE, CacheKind, cache_base_dir, cache_dir, data_base_dir,
            run_body_file,
        },
        cert::ensure_cert_profile,
        channel::{
            hub::{FROM_MCP, open_packet, start as start_channel_hub},
            state::{
                ChannelHandle, ChannelPhase, ChannelSendError, ChannelVerifyError,
                CloseSignal as ChannelCloseSignal, MAX_FRAME_BYTES, MCP_RESERVED_PREFIX,
                SpamVerdict, channel_handle, inbox_dir, mint_msg_id, render_nuon, render_packet,
            },
        },
        cycle::detect_import_cycle,
        embed::{EVAL_STACK_SIZE, InteractEngine, build_base, eval_stateless},
        emergency::{
            BackgroundJobsWarningEmergency, ChannelSpamErrorEmergency,
            ChannelSpamWarningEmergency, CpuWarningEmergency, CriticalEmergency,
            DiskWarningEmergency, Emergency, EmergencyTx, HungEngineThreadEmergency,
            RamWarningEmergency, VramWarningEmergency, append_line,
            spawn_emergency_responder,
        },
        error::{Diagnostic, Error, Severity, Source, error_to_call_result},
        executor::Executor,
        rig::{
            RigLocks, ValidationResult, check_rig,
            check_source_dir, commit_impl, ensure_substrate, establish_rig,
            render_signatures_within, SignaturesDoc,
            InspectDoc, index_node, inspect_impl, install_impl, is_reserved_term, is_valid_ident,
            is_valid_rig,
            is_valid_module_path,
            rigs_dir, load_index, registered_rig_names, scaffold_leaf,
            scaffold_leaf_exists, uninstall_impl,
        },
        lint::{LINT_VIOLATION_CAP, lint_body},
        liveness::acquire as acquire_host_lock,
        namepath::{Namepath, NamepathPattern, NamepathRef, NamepathStr},
        nonce::{McpNom, Nonce, NonceGen},
        oneshot::run_oneshot,
        parse_engine::{
            LintEngine, ParseEngine, set_lib_dirs_const, span_to_line_col, wrap_as_def_body,
            wrap_as_module,
        },
        pin::{clear_pins, effective_supervisor, pin, reap_pins},
        purview::{
            CurrentPurview, PURVIEW_ALL, PURVIEW_DEFAULT, PurviewRow,
            PurviewView, is_derived_purview, is_valid_purview_id, load_purviews,
            ensure_default_purview, expand_values, is_nameable_purview, is_valid_purview_ref,
            parse_patterns, pattern_delta, prune_dangling, purview_ref, purview_views,
            purviews_path, resolve_patterns, save_purviews,
        },
        remote::{
            codec::{
                AcceptorToInitiator, BitcodeCodec, DeliveryResult, FileFrame, InitiatorToAcceptor,
                MsgFrame, RemoteStream,
            },
            link::{RemoteLinkHandle, find_link_send, open_remote, remote_links, safe_dest},
            verify::EntityPin,
        },
        run::{eval_concurrency_cap, run_server},
        schema::{
            args_schema_to_nu, args_schema_to_signature, nu_to_args_schema, nu_to_result_schema,
            result_schema_to_nu, result_schema_to_signature,
        },
        teardown::{
            OrphanReaper, install_child_subreaper, kill_plugin_subprocesses, make_tracker,
            process_start_time, tree_kill,
        },
        watchdog::{
            HungRegistry, HungWatch, Lane, WatchdogDeps, register_hung, spawn_watchdog,
        },
        tool::{
            call::CallParams,
            channel_close::ChannelCloseParams,
            channel_verified::ChannelVerifiedParams,
            commit::CommitParams,
            config_channel::ConfigChannelParams,
            common::{
                CachedRunBody, InFlightEntry, InFlightKind, NuSh, RemoteLinkEntry, RunParams,
                convert_schemas, dispatch_interact, dispatch_pooled, envelope_to_structured,
                lint_run_params, now_millis, teardown_all_in_flight,
            },
            info::InfoParams,
            inspect::InspectParams,
            kill::KillParams,
            learn::LearnParams,
            rig::RigParams,
            new::NewParams,
            processes::ProcessesParams,
            purview_configure::PurviewConfigureParams,
            purview_extend::PurviewExtendParams,
            purviews::PurviewsParams,
            purview::PurviewParams,
            rerun::RerunParams,
        },
    },
    template::{
        build_call_source, build_interact_source, build_run_source, json_value_to_nu_value,
    },
};

pub(crate) use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    fs,
    io::{self, Write},
    hash::{Hash, Hasher},
    marker::PhantomData,
    os::unix::fs::OpenOptionsExt,
    ops::ControlFlow,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    process,
    sync::{
        Arc, LazyLock, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) use clap::Parser;

pub(crate) use sourcetrait_cert_lib as lib_cert;
pub(crate) use sourcetrait_grammar_lib as lib_grammar;

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
    pub(crate) use nu_engine::CallExt;
    pub(crate) use nu_engine::command_prelude::Call;
    pub(crate) use nu_json::Value as JsonValue;
    pub(crate) use nu_parser::parse;
    pub(crate) use nu_parser::{FlatShape, flatten_block};
    pub(crate) use nu_path::nu_config_dir;
    pub(crate) use nu_plugin_engine::load_plugin_file;
    pub(crate) use nu_protocol::{
        BlockId, Category, CollectionColumns, DeclId, FromValue, Module, PipelineData,
        PluginRegistryFile,
        PluginRegistryItemData, Record, ShellError, Signals, Signature, Span, SyntaxShape, Type,
        Value, VarId,
        ast::{
            Argument, Block, Comparison, Expr, Expression, ExternalArgument, ListItem,
            Operator, Pattern, RecordItem,
        },
        debugger::WithoutDebug,
        engine::{Command, EngineState, Job, Jobs, Mail, Stack, StateWorkingSet, ThreadJob},
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

pub(crate) mod tls {
    pub(crate) use tokio_rustls::{TlsAcceptor, TlsConnector, client, server};
    pub(crate) use tokio_rustls::rustls::{ClientConfig, ServerConfig};
    pub(crate) use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
}

pub(crate) mod rv {
    pub(crate) use rustls::{CertificateError, DigitallySignedStruct, DistinguishedName, SignatureScheme};
    pub(crate) use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    pub(crate) use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
    pub(crate) use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
    pub(crate) use rustls::crypto::ring::default_provider;
    pub(crate) use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
}

pub(crate) mod ws {
    pub(crate) use tokio_tungstenite::accept_async;
    pub(crate) use tokio_tungstenite::tungstenite::Message;
    pub(crate) use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    pub(crate) use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
}

pub(crate) use futures_util::{SinkExt, StreamExt};
pub(crate) use tokio_rustls::rustls::pki_types::pem::PemObject;

pub(crate) mod tk {
    pub(crate) use tokio::{
        spawn,
        io::{ReadHalf, WriteHalf, split},
        net::{TcpListener, TcpStream},
        signal::unix::{SignalKind, signal},
        task::{JoinHandle, spawn_blocking},
        sync::{
            Mutex as AsyncMutex, OwnedSemaphorePermit, RwLock as AsyncRwLock, Semaphore, oneshot,
            mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
        },
        time::{Duration as TkDuration, sleep, timeout},
    };
}

pub(crate) mod tku {
    pub(crate) use tokio_util::codec::{Decoder, Encoder, FramedRead, FramedWrite, LengthDelimitedCodec};
    pub(crate) use tokio_util::bytes::BytesMut;
    pub(crate) use tokio_util::sync::CancellationToken;
}

pub(crate) mod json {
    pub(crate) use serde_json::{Value, from_slice, from_value, to_value, to_vec};
}

pub use crate::cli::host_main;
