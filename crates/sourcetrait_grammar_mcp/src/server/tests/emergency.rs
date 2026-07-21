use crate::*;

fn a_hang() -> Emergency {
    Emergency::HungEngineThread(HungEngineThreadEmergency {
        nonce: "abc123".to_string(),
        lane: "stateless".to_string(),
        tool: "run".to_string(),
        started_at: 1000,
        cancelled_at: 2000,
        hung_ms: 6000,
        pool_held: 4,
        pool_cap: 8,
    })
}

#[test]
fn to_nuon_line_is_single_line_and_parses() {
    let line = a_hang().to_nuon_line(9999, "nomAAA").unwrap();
    assert!(!line.contains('\n'), "an emergency line must not span lines: {line:?}");
    let v = nu::from_nuon(&line, None).expect("valid NUON record");
    let rec = v.as_record().expect("record");
    assert_eq!(rec.get("kind").and_then(|x| x.as_str().ok()), Some("hung_engine_thread"));
    assert_eq!(rec.get("mcp_nom").and_then(|x| x.as_str().ok()), Some("nomAAA"));
    assert_eq!(rec.get("nonce").and_then(|x| x.as_str().ok()), Some("abc123"));
    assert_eq!(rec.get("lane").and_then(|x| x.as_str().ok()), Some("stateless"));
    assert_eq!(rec.get("tool").and_then(|x| x.as_str().ok()), Some("run"));
}

#[test]
fn every_model_lives_under_the_mcp_reservation_and_is_distinct() {
    // The reservation is only worth anything if it holds for EVERY host-originated
    // model - one variant escaping it turns a mechanical provenance check back into a
    // judgement call. Distinctness matters because the model is what the agent routes on.
    let models: Vec<&str> = all_kinds().iter().map(|em| em.model()).collect();
    for model in &models {
        assert!(
            model.starts_with(MCP_RESERVED_PREFIX),
            "{model} must live under the {MCP_RESERVED_PREFIX} reservation",
        );
    }
    let mut unique = models.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), models.len(), "models must be distinct: {models:?}");
}

/// One of every variant, so neither test can silently miss a newly added Emergency.
fn all_kinds() -> Vec<Emergency> {
    vec![
        Emergency::HungEngineThread(HungEngineThreadEmergency {
            nonce: "n".into(),
            lane: "interact".into(),
            tool: "interact".into(),
            started_at: 1,
            cancelled_at: 2,
            hung_ms: 3,
            pool_held: 1,
            pool_cap: 2,
        }),
        Emergency::CpuWarning(CpuWarningEmergency {
            cpu_pct: 175.5,
            sample_ms: 2000,
        }),
        Emergency::RamWarning(RamWarningEmergency { rss_kb: 123456 }),
        Emergency::VramWarning(VramWarningEmergency {
            used_mib: 20000,
            total_mib: 24000,
        }),
        Emergency::BackgroundJobsWarning(BackgroundJobsWarningEmergency { job_count: 40 }),
        Emergency::ChannelSpamWarning(ChannelSpamWarningEmergency {
            from: "thread/abc".into(),
            hits: 10,
            window_secs: 10,
            rate: 10,
        }),
        Emergency::ChannelSpamError(ChannelSpamErrorEmergency {
            from: "thread/abc".into(),
            hits: 15,
            window_secs: 10,
            rate: 15,
            action: "signals triggered".into(),
        }),
        Emergency::Critical(CriticalEmergency {
            reason: "all stateless engine-thread permits held by hung threads".into(),
            hung: 8,
            cap: 8,
        }),
    ]
}

#[test]
fn every_kind_serializes_single_line() {
    for em in all_kinds() {
        let want = em.kind().name().to_string();
        let line = em.to_nuon_line(1, "nom").unwrap();
        assert!(!line.contains('\n'), "line spans multiple lines: {line:?}");
        let v = nu::from_nuon(&line, None).expect("valid NUON record");
        let rec = v.as_record().expect("record");
        assert_eq!(rec.get("kind").and_then(|x| x.as_str().ok()), Some(want.as_str()));
    }
}
