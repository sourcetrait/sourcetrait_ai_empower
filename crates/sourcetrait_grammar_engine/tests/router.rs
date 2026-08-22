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
async fn test_routing() {
    use sourcetrait_common::sysgreen as green;
    let engine = &TESTING.harness().engine;

    fn matches_msg<T>(msg: &green::MsgFromSys<T>) -> bool {
        match msg {
            green::MsgFromSys::Green(
                green::FromGreenSys::StatusChange(green::Packet {
                })
            ),
            _ => false,
        }
    }

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

}
