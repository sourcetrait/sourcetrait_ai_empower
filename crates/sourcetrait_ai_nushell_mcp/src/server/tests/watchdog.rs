use crate::*;
use crate::server::watchdog::{parse_cpu_ticks, parse_rss_kb, parse_vram, scan_hung};

fn watch_entry(nonce: &str, lane: Lane, cancelled_at: u64, finished: bool) -> HungWatch {
    HungWatch {
        nonce: nonce.to_string(),
        tool: "run",
        lane,
        started_at: 0,
        cancelled_at,
        finished: Arc::new(AtomicBool::new(finished)),
    }
}

fn registry(entries: Vec<HungWatch>) -> HungRegistry {
    let mut map = HashMap::new();
    for e in entries {
        map.insert(e.nonce.clone(), e);
    }
    Arc::new(std::sync::Mutex::new(map))
}

#[test]
fn scan_confirms_stateless_hang_past_grace() {
    let reg = registry(vec![watch_entry("w1", Lane::Stateless, 1000, false)]);
    // now 7000 - cancelled 1000 = hung 6000 > grace 5000 -> confirmed
    let (ems, confirmed) = scan_hung(&reg, 7000, 5000, 2, 8);
    assert_eq!(confirmed, 1);
    assert_eq!(ems.len(), 1);
    assert_eq!(ems[0].0, "hung:stateless:w1");
    assert!(matches!(ems[0].1, Emergency::HungEngineThread(_)));
}

#[test]
fn scan_ignores_within_grace() {
    let reg = registry(vec![watch_entry("w1", Lane::Stateless, 1000, false)]);
    // hung 2000 <= grace 5000 -> not yet confirmed
    let (ems, confirmed) = scan_hung(&reg, 3000, 5000, 0, 8);
    assert_eq!(confirmed, 0);
    assert!(ems.is_empty());
}

#[test]
fn scan_prunes_finished_entries() {
    let reg = registry(vec![watch_entry("done", Lane::Stateless, 1000, true)]);
    let (ems, confirmed) = scan_hung(&reg, 9999, 5000, 0, 8);
    assert_eq!(confirmed, 0);
    assert!(ems.is_empty());
    assert!(
        reg.lock().unwrap().is_empty(),
        "a finished entry must be pruned from the registry"
    );
}

#[test]
fn scan_interact_hang_is_not_a_stateless_permit() {
    let reg = registry(vec![watch_entry("i1", Lane::Interact, 0, false)]);
    let (ems, confirmed) = scan_hung(&reg, 10000, 5000, 0, 8);
    assert_eq!(confirmed, 0, "an interact-lane hang holds no pooled permit");
    assert_eq!(ems.len(), 1);
    assert_eq!(ems[0].0, "hung:interact:i1");
    assert!(matches!(ems[0].1, Emergency::HungEngineThread(_)));
}

#[test]
fn parse_rss_reads_vmrss_kb() {
    let status = "Name:\tnushell_mcp\nVmPeak:\t  100 kB\nVmRSS:\t  204800 kB\nThreads:\t9\n";
    assert_eq!(parse_rss_kb(status), Some(204800));
    assert_eq!(parse_rss_kb("Name:\tx\n"), None);
}

#[test]
fn parse_cpu_counts_fields_after_comm_parens() {
    // comm itself contains a ')' and a space; fields are counted after the LAST
    // ')': index 0 = state (field 3), so utime (field 14) = index 11 and stime
    // (field 15) = index 12. Here utime=200, stime=55.
    let stat = "1 (weird ) name) S 4 5 6 7 8 9 10 11 12 13 200 55 0 0";
    assert_eq!(parse_cpu_ticks(stat), Some(255));
}

#[test]
fn parse_vram_sums_across_gpus() {
    let csv = "1000, 24000\n2000, 24000\n";
    assert_eq!(parse_vram(csv), Some((3000, 48000)));
    assert_eq!(parse_vram(""), None);
    assert_eq!(parse_vram("garbage\n"), None);
}
