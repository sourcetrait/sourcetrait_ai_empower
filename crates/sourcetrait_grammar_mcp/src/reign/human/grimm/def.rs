use crate::*;

const SPAN: nu::Span = nu::Span::unknown();

impl GrimmChannelSend {
    pub(crate) const DEF: nuin::SignatureDef<GrimmCategory> = nuin::SignatureDef {
        name: "grimm channel send",
        description: "Emits a message to the Channel. Returns a message id on success.",
        category: GrimmCategory::Tool,
        examples: &[
            nuin::ExampleDef {
                description: "Send arbitrary event data (as seen from: nu(execute))",
                example: "grimm channel send 'my/adhoc/Thing' {foo:'bar', num:3}",
                result_fn: || nu::Value::record(
                    nu::record! {
                        "event_id" => nu::Value::string("987654321X", nu::Span::unknown()),
                    },
                    nu::Span::unknown()
                ),
            },
            nuin::ExampleDef {
                description: "Send arbitrary event data (as seen from: Channel)",
                example: "grimm channel send 'my/adhoc/Thing' {foo:'bar', num:3}",
                result_fn: || nu::Value::record(
                    nu::record! {
                        "id" => nu::Value::string("987654321X", SPAN),
                        "from" => nu::Value::string("mcp/nu/Execute", SPAN),
                        "msg" => nu::Value::record(
                            nu::record! {
                                "data" => nu::Value::record(
                                    nu::record! {
                                        "foo" => nu::Value::string("bar", SPAN),
                                        "num" => nu::Value::int(3, SPAN),
                                    },
                                    SPAN
                                ),
                                "attached" => nu::Value::nothing(SPAN),
                            },
                            SPAN
                        )
                    },
                    SPAN
                ),
            },
        ],
    };
    pub(crate) const DEF_MODEL: nuin::ParameterDef = nuin::ParameterDef {
        name: "model",
        description: "Namepath of the event data type",
    };
    pub(crate) const DEF_EVENT: nuin::ParameterDef = nuin::ParameterDef {
        name: "event",
        description: "Notification data",
    };
    pub(crate) const DEF_ATTACHED: nuin::ParameterDef = nuin::ParameterDef {
        name: "attached",
        description: "Detailed data. Stored in the Channel inbox for retrieval",
    };
}

impl GrimmRemoteChannelSend {
    pub(crate) const DEF: nuin::SignatureDef<GrimmCategory> = nuin::SignatureDef {
        name: "grimm remote channel send",
        description: "Emits a message to a remote Channel. Returns a message id on success.",
        category: GrimmCategory::Tool,
        examples: &[
            nuin::ExampleDef {
                description: "Send arbitrary event data to a remote",
                example: "grimm remote channel send '0123456789B' 'my/model/Event' {foo:'bar', num:3}",
                result_fn: || nu::Value::record(nu::record!{
                    "event_id" => nu::Value::string("0987654321Z", nu::Span::unknown()),
                }, nu::Span::unknown()),
            },
        ],
    };
}

