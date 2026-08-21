use sourcetrait_common::testing::prelude::*;
use sourcetrait_grammar_engine::{self as engine, nuin};

struct Harness {
    engine: engine::Engine,
}

static TESTING: testing::ModuleWith<Harness> = testing::module_with!(Integration, {
    .setup(|_| {
        let engine = tokio::runtime::Runtime::new()
            .expect("tokio runtime")
            .block_on(async {
                engine::EngineBuilder::new()
                    .equip_id("test")
                    .equip_namespace("testing")
                    .start()
                    .await
            })
            .expect("engine");
        
        Harness { engine }
    })
});

#[tested(tokio)]
async fn test_nu_repl() {
    let engine = &TESTING.harness().engine;

    // math int
    let response = engine.request(engine::NuReplRequest {
        nu: indoc::formatdoc! {r#"
            1 + 2
        "#},
    }).await.expect("response");
    assert_eq!(response, Ok(nuin::Val::Int(3)));

    // error command
    let response = engine.request(engine::NuReplRequest {
        nu: indoc::formatdoc! {r#"
            yoink
        "#},
    }).await.expect("response");
    assert_eq!(response, Err(nuin::ValError::Unknown)); // nu::shell::external_command
}

#[tested(tokio)]
async fn test_nu_def_execute_local() {
    let engine = &TESTING.harness().engine;

    // math int
    let response = engine.request(engine::NuDefRequest {
        kind: engine::NuDefKind::Execute,
        host: engine::Host::Local,
        args: nuin::Val::Record([
            ("left".into(), nuin::Val::Int(1)),
            ("right".into(), nuin::Val::Int(2)),
        ].into_iter().collect()),
        def: DEF_EXECUTE_SUM.into(),
    }).await.expect("response");
    let engine::NuDefResponse { result,.. } = response;
    assert_eq!(result, Ok(nuin::Val::Int(3)));

    // error name_not_found
    let response = engine.request(engine::NuDefRequest {
        kind: engine::NuDefKind::Execute,
        host: engine::Host::Local,
        args: nuin::Val::Record([
            ("left".into(), nuin::Val::Int(1)),
            ("right".into(), nuin::Val::Int(2)),
        ].into_iter().collect()),
        def: DEF_EXECUTE_SUM_ERR.into(),
    }).await.expect("response");
    let engine::NuDefResponse { result,.. } = response;
    assert_eq!(result, Err(nuin::ValError::Unknown)); // nu::shell::name_not_found
}

#[tested(tokio)]
async fn test_renu_local() {
    let engine = &TESTING.harness().engine;

    // first call
    let response = engine.request(engine::NuDefRequest {
        kind: engine::NuDefKind::Execute,
        host: engine::Host::Local,
        args: nuin::Val::Record([
            ("left".into(), nuin::Val::Int(1)),
            ("right".into(), nuin::Val::Int(2)),
        ].into_iter().collect()),
        def: DEF_EXECUTE_SUM.into(),
    }).await.expect("response");
    let engine::NuDefResponse { result, nonce } = response;
    assert_eq!(result, Ok(nuin::Val::Int(3)));

    // re-nu with different values
    let response = engine.request(engine::ReNuRequest {
        host: engine::Host::Local,
        nonce,
        args: nuin::Val::Record([
            ("left".into(), nuin::Val::Int(1113)),
            ("right".into(), nuin::Val::Int(2)),
        ].into_iter().collect()),
    }).await.expect("response");
    let engine::NuDefResponse { result, nonce: ntwice } = response;
    assert_ne!(nonce, ntwice);
    assert_eq!(result, Ok(nuin::Val::Int(1115)));
}

/// sums two integers (`$args.left` + `$args.right`)
/// - returns result`.sum` (int)
const DEF_EXECUTE_SUM: &'static str = indoc::indoc! {r#"
    def execute [args: record<left: int, right: int>]: nothing -> record<sum: int> {
        let sum: int = $args.left + $args.right;
        
        {
            sum: $sum,
        }
    }
"#};

/// intentional typo for `$args.right`. one-off of [DEF_EXECUTE_SUM]
/// - errors with `nu::shell::name_not_found`
const DEF_EXECUTE_SUM_ERR: &'static str = indoc::indoc! {r#"
    def execute [args: record<left: int, right: int>]: nothing -> record<sum: int> {
        let sum: int = $args.left + $args.bright;
        
        {
            sum: $sum,
        }
    }
"#};
