pub(crate) mod val {
    pub(crate) mod data;
}
pub(crate) mod sig {
    pub(crate) mod command;
}

pub use crate::{
    val::data::ValueData,
    sig::command::{
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
    num::NonZero,
    ops::Bound,
};

pub(crate) mod nu {
    pub(crate) use nu_protocol::{
        Category, Signature, SyntaxShape, Example, Value,
        Span, Range,
        ast::PathMember,
        casing::Casing,
    };
}

pub(crate) use sourcetrait_cereal::{self as cereal, prelude::*};
