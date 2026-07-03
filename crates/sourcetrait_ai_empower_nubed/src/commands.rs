use crate::*;

/// Build the curated base engine state: the isolation surface of the crate.
///
/// ALLOWLIST ONLY - commands are explicitly registered, never inherited from
/// `create_default_context` / `add_shell_command_context`, so an upstream
/// addition can never widen the capability set silently. The registered set is
/// pure data processing: language core plus filters, strings, conversions,
/// formats, math, bytes, hash, date, url (no http), lexical path ops, random,
/// and generators.
///
/// Deliberately ABSENT, by class:
/// - process control: exit, exec, panic (unregistered here; also never
///   registered by nu-cmd-lang, so `ShellError::Exit` is unconstructible)
/// - externals: run-external / run-internal (a bare or `^` command fails at
///   eval; nothing can shell out)
/// - parse-time filesystem: source, source-env, use, export use/module,
///   overlay family (the parser reads files for these - a read capability)
/// - filesystem, system, network (http/port), platform (input/term/clip/
///   sleep/kill), env commands, shells (cd), jobs, stor, viewers, help,
///   scope, plugins, print
/// - fs-touching path subcommands: path exists / type / expand / self
///   (the lexical path ops stay)
pub(crate) fn base_engine_state() -> NuBedResult<nu::EngineState> {
    let mut engine_state = nu::EngineState::new();
    // Embedded mode: keeps host stdio clean (print -> stderr), nulls external
    // stdin (belt-and-braces; no externals are registered), $nu.is-mcp = true.
    engine_state.is_mcp = true;

    let delta = {
        let mut working_set = nu::StateWorkingSet::new(&engine_state);

        macro_rules! bind_command {
            ( $( $command:expr ),* $(,)? ) => {
                $( working_set.add_decl(Box::new($command)); )*
            };
        }

        // Language core (nu-cmd-lang picks).
        bind_command! {
            nu_cmd_lang::Alias,
            nu_cmd_lang::Break,
            nu_cmd_lang::Collect,
            nu_cmd_lang::Const,
            nu_cmd_lang::Continue,
            nu_cmd_lang::Def,
            nu_cmd_lang::Describe,
            nu_cmd_lang::Do,
            nu_cmd_lang::Echo,
            nu_cmd_lang::Error,
            nu_cmd_lang::ErrorMake,
            nu_cmd_lang::ExportCommand,
            nu_cmd_lang::ExportConst,
            nu_cmd_lang::ExportDef,
            nu_cmd_lang::For,
            nu_cmd_lang::If,
            nu_cmd_lang::Ignore,
            nu_cmd_lang::Let,
            nu_cmd_lang::Loop,
            nu_cmd_lang::Match,
            nu_cmd_lang::Mut,
            nu_cmd_lang::Return,
            nu_cmd_lang::Try,
            nu_cmd_lang::Version,
            nu_cmd_lang::While,
        };

        // Filters.
        bind_command! {
            nu_command::All,
            nu_command::Any,
            nu_command::Append,
            nu_command::ChunkBy,
            nu_command::Chunks,
            nu_command::Columns,
            nu_command::Compact,
            nu_command::Default,
            nu_command::Drop,
            nu_command::DropColumn,
            nu_command::DropNth,
            nu_command::Each,
            nu_command::Enumerate,
            nu_command::Every,
            nu_command::Filter,
            nu_command::Find,
            nu_command::First,
            nu_command::Flatten,
            nu_command::Get,
            nu_command::GroupBy,
            nu_command::Headers,
            nu_command::Insert,
            nu_command::Interleave,
            nu_command::IsEmpty,
            nu_command::IsNotEmpty,
            nu_command::Items,
            nu_command::Join,
            nu_command::Last,
            nu_command::Length,
            nu_command::Lines,
            nu_command::Merge,
            nu_command::MergeDeep,
            nu_command::Move,
            nu_command::ParEach,
            nu_command::Peek,
            nu_command::Prepend,
            nu_command::Reduce,
            nu_command::Reject,
            nu_command::Rename,
            nu_command::Reverse,
            nu_command::Select,
            nu_command::Shuffle,
            nu_command::Skip,
            nu_command::SkipUntil,
            nu_command::SkipWhile,
            nu_command::Slice,
            nu_command::Sort,
            nu_command::SortBy,
            nu_command::SplitList,
            nu_command::Take,
            nu_command::TakeUntil,
            nu_command::TakeWhile,
            nu_command::Tee,
            nu_command::Transpose,
            nu_command::Uniq,
            nu_command::UniqBy,
            nu_command::Update,
            nu_command::Upsert,
            nu_command::Values,
            nu_command::Where,
            nu_command::Window,
            nu_command::Wrap,
            nu_command::Zip,
        };

        // Charts (pure aggregation).
        bind_command! {
            nu_command::Histogram,
        };

        // Path: LEXICAL subcommands only (exists / type / expand / self stat
        // or read the filesystem and stay out).
        bind_command! {
            nu_command::Path,
            nu_command::PathBasename,
            nu_command::PathDirname,
            nu_command::PathJoin,
            nu_command::PathParse,
            nu_command::PathRelativeTo,
            nu_command::PathSplit,
        };

        // Debug picks (value-level only).
        bind_command! {
            nu_command::Debug,
            nu_command::Metadata,
        };

        // Strings.
        bind_command! {
            nu_command::Ansi,
            nu_command::AnsiLink,
            nu_command::AnsiStrip,
            nu_command::Char,
            nu_command::Decode,
            nu_command::DecodeBase32,
            nu_command::DecodeBase32Hex,
            nu_command::DecodeBase64,
            nu_command::DecodeHex,
            nu_command::Detect,
            nu_command::DetectColumns,
            nu_command::DetectType,
            nu_command::Encode,
            nu_command::EncodeBase32,
            nu_command::EncodeBase32Hex,
            nu_command::EncodeBase64,
            nu_command::EncodeHex,
            nu_command::Format,
            nu_command::FormatDate,
            nu_command::FormatDuration,
            nu_command::FormatFilesize,
            nu_command::Parse,
            nu_command::Split,
            nu_command::SplitChars,
            nu_command::SplitColumn,
            nu_command::SplitRow,
            nu_command::SplitWords,
            nu_command::Str,
            nu_command::StrCapitalize,
            nu_command::StrContains,
            nu_command::StrDistance,
            nu_command::StrDowncase,
            nu_command::StrEndswith,
            nu_command::StrEscapeRegex,
            nu_command::StrExpand,
            nu_command::StrIndexOf,
            nu_command::StrJoin,
            nu_command::StrLength,
            nu_command::StrReplace,
            nu_command::StrReverse,
            nu_command::StrStartsWith,
            nu_command::StrStats,
            nu_command::StrSubstring,
            nu_command::StrTrim,
            nu_command::StrUpcase,
        };

        // Date (clock reads are accepted nondeterminism).
        bind_command! {
            nu_command::Date,
            nu_command::DateFromHuman,
            nu_command::DateHumanize,
            nu_command::DateListTimezones,
            nu_command::DateNow,
            nu_command::DateToTimezone,
        };

        // Formats (from/to are pure value transforms).
        bind_command! {
            nu_command::From,
            nu_command::FromCsv,
            nu_command::FromJson,
            nu_command::FromMd,
            nu_command::FromMsgpack,
            nu_command::FromMsgpackz,
            nu_command::FromNuon,
            nu_command::FromOds,
            nu_command::FromSsv,
            nu_command::FromToml,
            nu_command::FromTsv,
            nu_command::FromXlsx,
            nu_command::FromXml,
            nu_command::FROM_YAML,
            nu_command::FROM_YML,
            nu_command::To,
            nu_command::ToCsv,
            nu_command::ToJson,
            nu_command::ToMd,
            nu_command::ToMsgpack,
            nu_command::ToMsgpackz,
            nu_command::ToNuon,
            nu_command::ToText,
            nu_command::ToToml,
            nu_command::ToTsv,
            nu_command::ToXml,
            nu_command::TO_YAML,
            nu_command::TO_YML,
        };

        // Conversions.
        bind_command! {
            nu_command::Fill,
            nu_command::Into,
            nu_command::IntoBinary,
            nu_command::IntoBool,
            nu_command::IntoCellPath,
            nu_command::IntoDatetime,
            nu_command::IntoDuration,
            nu_command::IntoFilesize,
            nu_command::IntoFloat,
            nu_command::IntoGlob,
            nu_command::IntoInt,
            nu_command::IntoRecord,
            nu_command::IntoString,
            nu_command::IntoValue,
            nu_command::SplitCellPath,
        };

        // Math.
        bind_command! {
            nu_command::Math,
            nu_command::MathAbs,
            nu_command::MathAvg,
            nu_command::MathCeil,
            nu_command::MathFloor,
            nu_command::MathLog,
            nu_command::MathMax,
            nu_command::MathMedian,
            nu_command::MathMin,
            nu_command::MathMode,
            nu_command::MathProduct,
            nu_command::MathRound,
            nu_command::MathSqrt,
            nu_command::MathStddev,
            nu_command::MathSum,
            nu_command::MathVariance,
        };

        // Bytes.
        bind_command! {
            nu_command::Bytes,
            nu_command::BytesAdd,
            nu_command::BytesAt,
            nu_command::BytesBuild,
            nu_command::BytesCollect,
            nu_command::BytesEndsWith,
            nu_command::BytesIndexOf,
            nu_command::BytesLen,
            nu_command::BytesRemove,
            nu_command::BytesReplace,
            nu_command::BytesReverse,
            nu_command::BytesSplit,
            nu_command::BytesStartsWith,
        };

        // Url (pure string ops; the network category - http/port - stays out).
        bind_command! {
            nu_command::Url,
            nu_command::UrlBuildQuery,
            nu_command::UrlDecode,
            nu_command::UrlEncode,
            nu_command::UrlJoin,
            nu_command::UrlParse,
            nu_command::UrlSplitQuery,
        };

        // Random (accepted nondeterminism; side-effect free).
        bind_command! {
            nu_command::Random,
            nu_command::RandomBinary,
            nu_command::RandomBool,
            nu_command::RandomChars,
            nu_command::RandomFloat,
            nu_command::RandomInt,
            nu_command::RandomUuid,
        };

        // Generators.
        bind_command! {
            nu_command::Cal,
            nu_command::Generate,
            nu_command::Seq,
            nu_command::SeqChar,
            nu_command::SeqDate,
        };

        // Hash.
        bind_command! {
            nu_command::Hash,
            nu_command::HashMd5::default(),
            nu_command::HashSha256::default(),
        };

        working_set.render()
    };

    engine_state
        .merge_delta(delta)
        .map_err(|error| SetupSnafu { message: error.to_string() }.build())?;
    Ok(engine_state)
}
