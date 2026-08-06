use crate::*;

impl GrimmChannelSend {
    pub(crate) const DEF: nuvocab::SignatureDef<GrimmCategory> = nuvocab::SignatureDef {
        name: "grimm channel send",
        description: "Emits a message to the Channel. Returns a message id on success.",
        category: GrimmCategory::Tool,
    };
    pub(crate) const DEF_MODEL: nuvocab::ParameterDef = nuvocab::ParameterDef {
        name: "model",
        description: "Namepath of the event data type",
    };
    pub(crate) const DEF_EVENT: nuvocab::ParameterDef = nuvocab::ParameterDef {
        name: "event",
        description: "Notification data",
    };
    pub(crate) const DEF_ATTACHED: nuvocab::ParameterDef = nuvocab::ParameterDef {
        name: "attached",
        description: "Detailed data. Stored in the Channel inbox for retrieval",
    };
}

impl GrimmRemoteChannelSend {
    pub(crate) const DEF: nuvocab::SignatureDef<GrimmCategory> = nuvocab::SignatureDef {
        name: "grimm remote channel send",
        description: "Emits a message to a remote Channel. Returns a message id on success.",
        category: GrimmCategory::Tool,
    };
}

