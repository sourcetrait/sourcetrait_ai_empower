use crate::*;

impl NuSh {
    pub(crate) fn tool_router() -> mcp::ToolRouter<Self> {
        let deny = &config().deny;
        let mut router = Self::processes_router()
            + Self::kill_router()
            + Self::info_router()
            + Self::inspect_router();
        if !deny.denies(DeniableTool::Run) {
            router += Self::run_router();
        }
        if !deny.denies(DeniableTool::Rerun) {
            router += Self::rerun_router();
        }
        if !deny.denies(DeniableTool::Interact) {
            router += Self::interact_router();
        }
        if !deny.denies(DeniableTool::Call) {
            router += Self::call_router();
        }
        if !deny.denies(DeniableTool::Learn) {
            router += Self::learn_router();
        }
        if !deny.denies(DeniableTool::New) {
            router += Self::new_router();
        }
        if !deny.denies(DeniableTool::Commit) {
            router += Self::commit_router();
        }
        if !deny.denies(DeniableTool::Rig) {
            router += Self::rig_router();
        }
        if !deny.denies(DeniableTool::ChannelOpen) {
            router += Self::channel_open_router();
        }
        if !deny.denies(DeniableTool::ChannelVerified) {
            router += Self::channel_verified_router();
        }
        if !deny.denies(DeniableTool::ChannelClose) {
            router += Self::channel_close_router();
        }
        if !deny.denies(DeniableTool::ConfigChannel) {
            router += Self::config_channel_router();
        }
        if !deny.denies(DeniableTool::Purviews) {
            router += Self::purviews_router();
        }
        if !deny.denies(DeniableTool::PurviewConfigure) {
            router += Self::purview_configure_router();
        }
        if !deny.denies(DeniableTool::PurviewExtend) {
            router += Self::purview_extend_router();
        }
        if !deny.denies(DeniableTool::Purview) {
            router += Self::purview_router();
        }
        if !deny.denies(DeniableTool::RemoteChannelOpen) {
            router += Self::remote_channel_open_router();
        }
        if !deny.denies(DeniableTool::RemoteChannelClose) {
            router += Self::remote_channel_close_router();
        }
        if !deny.denies(DeniableTool::RemoteChannels) {
            router += Self::remote_channels_router();
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
            "grammar".to_string()
        } else {
            format!("grammar ({namespace})")
        };
        let mut info = mcp::ServerInfo::default();
        info.capabilities = mcp::ServerCapabilities::builder().enable_tools().build();
        info.server_info = mcp::Implementation::new(
            lib_grammar::consts::GRAMMAR,
            env!("CARGO_PKG_VERSION"),
        )
        .with_title(title);
        info.instructions = Some(format!(
            "Evaluation artifacts are cached at \
             $XDG_CACHE_HOME/sourcetrait/grammar/{id}/{namespace}/{{runs,interacts,calls}}/<nonce>/{{stdout,stderr}}; \
             a run's cached body co-locates at \
             $XDG_CACHE_HOME/sourcetrait/grammar/{id}/{namespace}/runs/<nonce>/body.nuon \
             (the rerun(nonce) handle). \
             Registered rigs live in a signed git repo at \
             $XDG_DATA_HOME/sourcetrait/grammar/{id}/{namespace}/rigs/; signing keypair at \
             $XDG_DATA_HOME/sourcetrait/grammar/{id}/{namespace}/keypair/. \
             This server's namespace: id `{id}`, namespace `{namespace}`.",
        ));
        info
    }
}
