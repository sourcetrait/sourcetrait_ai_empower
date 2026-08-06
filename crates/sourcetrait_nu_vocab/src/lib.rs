pub(crate) mod reign {
    // REIGN HUMAN
    pub(crate) mod human {
        pub(crate) mod def {
            pub(crate) mod command;
        }
    }
}

pub use crate::{
    reign::human::{  // REIGN HUMAN
        def::command::{
            SignatureDef, ParameterDef, SignatureTrait,
            SignatureCategoryTrait, SignatureParameterTrait,
        },
    },
};

pub mod prelude {
    pub use crate::{
        SignatureTrait, SignatureCategoryTrait, SignatureParameterTrait,
    };
}

pub(crate) mod nu {
    pub(crate) use nu_protocol::{
        Category, Signature, SyntaxShape,
    };
}

