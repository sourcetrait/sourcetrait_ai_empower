use crate::*;
use crate::server::watchdog::{
    DiskWatch, LevelGate, parse_cpu_ticks, parse_df, parse_mem_total_kb, parse_rss_kb, parse_vram,
    scan_hung,
};

/// Real `df -P` output from the box, including the two cases that matter: the 4 KiB
/// pseudo-mount pinned at 100% by design, and a mount point that is genuinely worth
/// watching at 70%.
const DF: &str = "Filesystem             1K-blocks       Used  Available Use% Mounted on
shm                        64000         40      63960   1% /dev/shm
tmpfs                    6559820        248    6559572   1% /etc/hosts
/dev/mapper/data-root 3835242120 2534268808 1106078876  70% /home/box
overlay               3835242120 2534268808 1106078876  70% /
tmpfs                          4          4          0 100% /run/nvidia-ctk-hook4682c966
";

#[test]
fn df_parses_to_mount_and_percentage() {
    let rows = parse_df(DF);
    assert_eq!(rows.len(), 5, "the header is skipped; got {rows:?}");
    assert_eq!(rows[0], ("/dev/shm".to_string(), 1));
    assert_eq!(rows[2], ("/home/box".to_string(), 70));
    assert_eq!(rows[4], ("/run/nvidia-ctk-hook4682c966".to_string(), 100));
}

#[test]
fn mem_total_is_read_in_kb() {
    let meminfo = "MemTotal:       65598248 kB\nMemFree:  123 kB\n";
    assert_eq!(parse_mem_total_kb(meminfo), Some(65598248));
    assert_eq!(parse_mem_total_kb("MemFree: 1 kB\n"), None);
}

#[test]
fn the_baseline_never_warns_and_excludes_what_is_already_over() {
    let mut watch = DiskWatch::default();
    let now = Instant::now();
    assert!(
        watch.sample(&parse_df(DF), 80, now).is_empty(),
        "the baseline pass establishes what to watch; it is not itself an observation",
    );
    // The pseudo-mount was at 100% from the start - a pre-existing condition by design,
    // never ours to report - so it stays silent no matter how long it sits there.
    let rows = vec![("/run/nvidia-ctk-hook4682c966".to_string(), 100)];
    for secs in [0, 120, 4000] {
        let at = now + tk::TkDuration::from_secs(secs);
        assert!(
            watch.sample(&rows, 80, at).is_empty(),
            "a mount excluded at baseline must never warn (t={secs})",
        );
    }
}

#[test]
fn a_watched_filesystem_warns_once_it_crosses() {
    let mut watch = DiskWatch::default();
    let base = Instant::now();
    watch.sample(&parse_df(DF), 80, base);

    let crossed = vec![("/home/box".to_string(), 81)];
    assert!(
        watch.sample(&crossed, 80, base).is_empty(),
        "one sample over the line is not yet sustained",
    );
    let emergencies = watch.sample(&crossed, 80, base + tk::TkDuration::from_secs(300));
    assert_eq!(emergencies.len(), 1, "sustained across two samples; got {emergencies:?}");
    assert!(matches!(
        &emergencies[0],
        Emergency::DiskWarning(d) if d.mount == "/home/box" && d.used_pct == 81,
    ));
}

const SUSTAIN: tk::TkDuration = tk::TkDuration::from_secs(60);
const REWARN: tk::TkDuration = tk::TkDuration::from_secs(30 * 60);

/// Sample a gate at `t` seconds from `base`, firing at 150.
fn at(
    gate: &mut LevelGate,
    base: Instant,
    secs: u64,
    value: f64,
) -> bool {
    gate.sample(
        value,
        150.0,
        base + tk::TkDuration::from_secs(secs),
        SUSTAIN,
        REWARN,
    )
}

#[test]
fn a_level_must_hold_before_it_warns() {
    let mut gate = LevelGate::default();
    let base = Instant::now();
    assert!(!at(&mut gate, base, 0, 200.0), "over, but not yet sustained");
    assert!(!at(&mut gate, base, 30, 200.0), "still inside the minute");
    assert!(at(&mut gate, base, 60, 200.0), "held for the full window");
    assert!(
        !at(&mut gate, base, 62, 200.0),
        "the agent needs ONE signal to decide on, not a stream of them",
    );
}

#[test]
fn a_momentary_spike_is_not_an_event() {
    let mut gate = LevelGate::default();
    let base = Instant::now();
    assert!(!at(&mut gate, base, 0, 200.0));
    assert!(!at(&mut gate, base, 2, 10.0), "dropped back");
    assert!(
        !at(&mut gate, base, 61, 200.0),
        "the clock restarts on the new excursion rather than counting the old one",
    );
}

#[test]
fn hovering_at_the_threshold_does_not_flap() {
    let mut gate = LevelGate::default();
    let base = Instant::now();
    at(&mut gate, base, 0, 200.0);
    assert!(at(&mut gate, base, 60, 200.0), "fires once");
    // Oscillating across the firing line: each dip restarts the sustain clock and the
    // re-warn floor outlasts the whole oscillation, so none of this is an event.
    for secs in [62, 64, 66, 68, 70, 200, 400] {
        let value = if secs % 4 == 0 { 149.0 } else { 200.0 };
        assert!(!at(&mut gate, base, secs, value), "no re-fire at t={secs}");
    }
}

#[test]
fn a_long_sustained_level_warns_only_twice_in_an_hour() {
    // The shape of a normal 30-60 minute inference run: resources pinned throughout,
    // which is NOT a fault. Warning about it repeatedly would talk over the work.
    let mut gate = LevelGate::default();
    let base = Instant::now();
    let mut warnings = 0;
    // Sample every 2s across a full hour, exactly as the watchdog ticks.
    for tick in 0..1800u64 {
        if at(&mut gate, base, tick * 2, 200.0) {
            warnings += 1;
        }
    }
    assert_eq!(
        warnings, 2,
        "one at the first sustained minute, one 30 minutes later - enough to notice, \
         not enough to nag",
    );
}

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
    let status = "Name:\tgrammar\nVmPeak:\t  100 kB\nVmRSS:\t  204800 kB\nThreads:\t9\n";
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
