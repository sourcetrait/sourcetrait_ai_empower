// 1. Inline module tree (no mod.rs anywhere):
pub(crate) mod cli;
pub(crate) mod error;
pub(crate) mod facts;
pub(crate) mod items {
    pub(crate) mod filters;
    pub(crate) mod helpers;
    pub(crate) mod macros;
    pub(crate) mod types;
    pub(crate) mod walker;
    pub(crate) mod workspace;

    // Sibling re-exports for use via `use super::*` in each items sub-file.
    pub(crate) use filters::*;
    pub(crate) use helpers::*;
    pub(crate) use macros::*;
    pub(crate) use types::*;
    pub(crate) use walker::*;
    pub(crate) use workspace::*;
}
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
    collections::{
        BTreeMap,
        HashMap,
    },
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


pub(crate) mod ext_proc_macro2 {
    pub(crate) use proc_macro2::{
        Delimiter,
        TokenStream,
        TokenTree,
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
        Attribute,
        Block,
        Expr,
        ExprCall,
        ExprMethodCall,
        Field,
        Fields,
        File as RsFile,
        FnArg,
        ForeignItem,
        GenericArgument,
        GenericParam,
        ImplItem,
        Item,
        ItemEnum,
        ItemFn,
        ItemImpl,
        ItemMacro,
        ItemMod,
        ItemStruct,
        ItemTrait,
        ItemType,
        ItemUnion,
        ItemUse,
        Lit,
        Local,
        Meta,
        Path as SynPath,
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
        UseTree,
        Variant,
        Visibility,
        WhereClause,
        WherePredicate,
        spanned::Spanned,
    };
}


pub(crate) mod ext_syn_visit {
    pub(crate) use syn::visit::Visit;
}


pub(crate) mod ext_walkdir {
    pub(crate) use walkdir::WalkDir;
}

// 6. Public re-exports - ONLY the crate's intentional external face:
pub use crate::{
    run::run,
};
