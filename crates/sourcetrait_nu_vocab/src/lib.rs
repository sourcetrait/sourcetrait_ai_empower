pub(crate) mod def {
    pub(crate) mod command;
}
pub(crate) mod model {
    pub(crate) mod model;
}

pub use crate::{
    def::command::{
        SignatureDef, ParameterDef, SignatureTrait, ExampleDef,
        SignatureCategoryTrait, SignatureParameterTrait,
    },
};

pub mod prelude {
    pub use crate::{
        SignatureTrait, SignatureCategoryTrait, SignatureParameterTrait,
    };
}

#[allow(unused)]
pub(crate) use std::{
    fs,
};

pub(crate) mod nu {
    pub(crate) use nu_protocol::{
        Category, Signature, SyntaxShape, Example, Value, Type, Span,
        engine::{
            EngineState, StateWorkingSet,
        },
    };
    pub(crate) use nu_parser::{
        parse_shape_name, parse_type,
    };
}

