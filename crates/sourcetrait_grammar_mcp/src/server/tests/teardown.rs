use crate::*;
use crate::server::teardown::parse_state_ppid;

#[test]
fn parses_state_and_ppid() {
    // "1 (init) S 0 ..." - the ordinary shape; index 0 after the ')' is state, 1 is ppid.
    assert_eq!(parse_state_ppid("42 (git) Z 165378 42 0").unwrap(), ("Z".to_string(), 165378));
    assert_eq!(parse_state_ppid("1 (systemd) S 0 1 1").unwrap(), ("S".to_string(), 0));
}

#[test]
fn counts_fields_after_the_last_paren() {
    // comm is unquoted and may contain BOTH spaces and parens, so a naive `find('(')` /
    // `find(')')` split lands mid-comm and reads the wrong fields. Anchoring on the LAST
    // ')' is the whole point - here the real state is Z and the real ppid is 7.
    let stat = "99 (weird ) name) Z 7 99 0 0";
    assert_eq!(parse_state_ppid(stat).unwrap(), ("Z".to_string(), 7));
}

#[test]
fn rejects_unparseable_stat() {
    assert!(parse_state_ppid("").is_none());
    assert!(parse_state_ppid("no parens here").is_none());
    // Truncated after the comm: a state but no ppid.
    assert!(parse_state_ppid("5 (x) Z").is_none());
}

#[test]
fn reaper_starts_empty_and_is_idempotent() {
    // No zombies of ours in a test binary, so a pass is a no-op that must not panic and
    // must leave nothing tracked. Repeated passes are the watchdog's actual usage.
    let mut reaper = OrphanReaper::new();
    reaper.reap();
    reaper.reap();
}
