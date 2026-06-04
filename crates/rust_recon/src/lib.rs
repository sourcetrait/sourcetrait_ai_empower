// 1. Inline module tree (no mod.rs anywhere):
pub(crate) mod cli;
pub(crate) mod error;
pub(crate) mod facts;
pub(crate) mod run;
pub(crate) mod scan;
pub(crate) mod walk;

// 2. Crate-internal re-exports - the workhorse of `use crate::*`:
pub(crate) use crate::{
    cli::{
        Cli,
        Command,
        ScanCommand,
    },
    error::{
        Error,
        Result,
    },
    facts::{
        FieldUsage,
        FieldPosition,
        Facts,
        FileFacts,
        FnPosition,
        FnSigUsage,
        MethodRefUsage,
        TypeAliasUsage,
    },
    scan::scan_file,
    walk::walk_workspace,
};

// 3. std re-exports (ALWAYS multiline, even with one item):
pub(crate) use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

// 4. External-crate types under namespaced shim mods:
pub(crate) mod ext_clap {
    pub(crate) use clap::{
        Parser,
        Subcommand,
    };
}


pub(crate) mod ext_serde {
    pub(crate) use serde::{
        Deserialize,
        Serialize,
    };
}


pub(crate) mod ext_syn {
    pub(crate) use syn::{
        AngleBracketedGenericArguments,
        Block,
        Expr,
        ExprCall,
        ExprMethodCall,
        Field,
        Fields,
        File as RsFile,
        FnArg,
        GenericArgument,
        GenericParam,
        ImplItem,
        Item,
        ItemEnum,
        ItemFn,
        ItemImpl,
        ItemMod,
        ItemStruct,
        ItemTrait,
        ItemType,
        ItemUnion,
        Local,
        PathArguments,
        PathSegment,
        ReturnType,
        Stmt,
        TraitBound,
        TraitItem,
        Type,
        TypeArray,
        TypeBareFn,
        TypeImplTrait,
        TypeParamBound,
        TypePath,
        TypeReference,
        TypeSlice,
        TypeTraitObject,
        TypeTuple,
        Variant,
        Visibility,
        WhereClause,
        WherePredicate,
        spanned::Spanned,
    };
}

pub(crate) mod ext_walkdir {
    pub(crate) use walkdir::WalkDir;
}

// 6. Public re-exports - ONLY the crate's intentional external face:
pub use crate::{
    run::run,
};
