use crate::*;
use crate::server::emergency::append_line;

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
fn every_kind_serializes_single_line() {
    let ems = vec![
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
        Emergency::HostCpu(HostCpuEmergency {
            cpu_pct: 175.5,
            sample_ms: 2000,
        }),
        Emergency::HostMemory(HostMemoryEmergency { rss_kb: 123456 }),
        Emergency::Vram(VramEmergency {
            used_mib: 20000,
            total_mib: 24000,
        }),
        Emergency::BackgroundJobs(BackgroundJobsEmergency { job_count: 40 }),
        Emergency::Critical(CriticalEmergency {
            reason: "all stateless engine-thread permits held by hung threads".into(),
            hung: 8,
            cap: 8,
        }),
    ];
    for em in ems {
        let want = em.kind().name().to_string();
        let line = em.to_nuon_line(1, "nom").unwrap();
        assert!(!line.contains('\n'), "line spans multiple lines: {line:?}");
        let v = nu::from_nuon(&line, None).expect("valid NUON record");
        let rec = v.as_record().expect("record");
        assert_eq!(rec.get("kind").and_then(|x| x.as_str().ok()), Some(want.as_str()));
    }
}

#[test]
fn append_line_writes_readable_nuonl() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log").join("nomZ").join("emergency.nuonl");
    let l1 = a_hang().to_nuon_line(1, "nomZ").unwrap();
    let l2 = Emergency::HostMemory(HostMemoryEmergency { rss_kb: 9 })
        .to_nuon_line(2, "nomZ")
        .unwrap();
    append_line(&path, &l1).unwrap();
    append_line(&path, &l2).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines.len(), 2, "two records appended, one per line");
    for line in lines {
        nu::from_nuon(line, None).expect("each line is a parseable NUON record");
    }
}
