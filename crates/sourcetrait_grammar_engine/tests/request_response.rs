use std::collections::HashMap;

use sourcetrait_common::testing::prelude::*;
use sourcetrait_grammar_engine::{self as engine, nuin};

struct Harness {
    _runtime: tokio::runtime::Runtime,
    engine: engine::Engine,
}

static TESTING: testing::ModuleWith<Harness> = testing::module_with!(Integration, {
    .setup(|_| {
        let orig = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            orig(info);
            std::process::exit(1);
        }));
        
        std::thread::spawn(|| {
            let _runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            let engine = _runtime.block_on(async {
                    engine::EngineBuilder::new()
                        .equip_id("test")
                        .equip_namespace("testing")
                        .start()
                        .await
                })
                .expect("engine");
            
            Harness { _runtime, engine }
        })
        .join()
        .expect("joined")
    })
});

/// this will be removed and the other tests unignored once actual
/// engine calls are implemented
#[tested(tokio)]
async fn test_loopback() {
    let engine = &TESTING.harness().engine;
    let pairs: HashMap<String, nuin::ValResult> = [
        (  "1 + 2".into(),
           Ok(nuin::Val::Int(3)) ),
        (  "'hello there' | str snake-case".into(),
           Ok(nuin::Val::String("hello_there".into())) ),
        (  "'{ foo: \"bar\" }' | from nuon".into(),
           Ok(nuin::Val::Record(Vec::from([
               ("foo".into(), nuin::Val::String("bar".into()))
            ])))
        ),
    ].into_iter().collect();

    for pair in pairs {
        let response = engine.request(engine::NuReplRequest {
            host: engine::Host::Local,
            nu: pair.0,
        }).await.expect("response").expect("ok");
        let engine::NuReplResponse { result } = response;
        assert_eq!(pair.1, result);
    }
}

#[tested(tokio)]
#[ignore]
async fn test_nu_repl() {
    let engine = &TESTING.harness().engine;

    // math int
    let response = engine.request(engine::NuReplRequest {
        host: engine::Host::Local,
        nu: indoc::formatdoc! {r#"
            1 + 2
        "#},
    }).await.expect("response").expect("ok");
    let engine::NuReplResponse { result } = response;
    assert_eq!(Ok(nuin::Val::Int(3)), result);

    // error command
    let response = engine.request(engine::NuReplRequest {
        host: engine::Host::Local,
        nu: indoc::formatdoc! {r#"
            yoink
        "#},
    }).await.expect("response").expect("ok");
    let engine::NuReplResponse { result } = response;
    assert_eq!(result, Err(nuin::ValError::Unknown)); // nu::shell::external_command
}

#[tested(tokio)]
#[ignore]
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
    let engine::NuDefResponse { result,.. } = response.expect("ok");
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
    }).await.expect("response").expect("ok");
    let engine::NuDefResponse { result,.. } = response;
    assert_eq!(result, Err(nuin::ValError::Unknown)); // nu::shell::name_not_found
}

#[tested(tokio)]
#[ignore]
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
    }).await.expect("response").expect("ok");
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
    }).await.expect("response").expect("ok");
    let engine::ReNuResponse { result, nonce: ntwice } = response;
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
