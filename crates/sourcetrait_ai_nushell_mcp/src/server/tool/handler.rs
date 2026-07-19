use crate::*;

impl NuSh {
    pub(crate) fn tool_router() -> mcp::ToolRouter<Self> {
        let deny = &config().deny;
        let mut router = Self::processes_router()
            + Self::kill_router()
            + Self::info_router()
            + Self::inspect_router();
        if !deny.denies(DeniableTool::Run) {
            router = router + Self::run_router();
        }
        if !deny.denies(DeniableTool::Rerun) {
            router = router + Self::rerun_router();
        }
        if !deny.denies(DeniableTool::Interact) {
            router = router + Self::interact_router();
        }
        if !deny.denies(DeniableTool::Call) {
            router = router + Self::call_router();
        }
        if !deny.denies(DeniableTool::Learn) {
            router = router + Self::learn_router();
        }
        if !deny.denies(DeniableTool::New) {
            router = router + Self::new_router();
        }
        if !deny.denies(DeniableTool::Commit) {
            router = router + Self::commit_router();
        }
        if !deny.denies(DeniableTool::Library) {
            router = router + Self::library_router();
        }
        router
    }
}

#[mcp::tool_handler(router = self.tool_router)]
impl mcp::ServerHandler for NuSh {
    fn get_info(&self) -> mcp::ServerInfo {
        let id = &config().id;
        let namespace = &config().namespace;
        let title = if namespace == "default" {
            "nushell".to_string()
        } else {
            format!("nushell ({namespace})")
        };
        let mut info = mcp::ServerInfo::default();
        info.capabilities = mcp::ServerCapabilities::builder().enable_tools().build();
        info.server_info = mcp::Implementation::new(
            lib_empower::consts::NUSHELL_MCP,
            env!("CARGO_PKG_VERSION"),
        )
        .with_title(title);
        info.instructions = Some(format!(
            "Evaluation artifacts are cached at \
             $XDG_CACHE_HOME/sourcetrait/nushell_mcp/{id}/{namespace}/{{runs,interacts,calls}}/<nonce>/{{stdout,stderr}}; \
             closures cached at \
             $XDG_CACHE_HOME/sourcetrait/nushell_mcp/{id}/{namespace}/closures/<rerun_id>.json. \
             Registered libraries live in a signed git repo at \
             $XDG_DATA_HOME/sourcetrait/nushell_mcp/{id}/{namespace}/libraries/; signing keypair at \
             $XDG_DATA_HOME/sourcetrait/nushell_mcp/{id}/{namespace}/keypair/. \
             This server's state coordinate: id `{id}`, namespace `{namespace}`.",
        ));
        info
    }
}
