use crate::*;

/// Parameters for `channel_verified()` (none).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ChannelVerifiedParams {}

#[mcp::tool_router(router = channel_verified_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Confirm the channel/Open packet was seen; ends the verify window."
    )]
    pub(crate) async fn channel_verified(
        &self,
        mcp::Parameters(_p): mcp::Parameters<ChannelVerifiedParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        match self.channel.mark_verified() {
            Ok(()) => Ok(mcp::CallToolResult::default()),
            Err(ChannelVerifyError::NotOpen) => {
                Ok(error_to_call_result(Error::ChannelNotOpen, None))
            }
            Err(ChannelVerifyError::NotClaimed) => {
                Ok(error_to_call_result(Error::ChannelNotClaimed, None))
            }
        }
    }
}
