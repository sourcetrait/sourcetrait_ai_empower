use crate::*;
use crate::server::pin::{PinnableKey, PinValue, clear_pins};
use crate::server::teardown::parse_start_time;
use sourcetrait_common::testing::prelude::*;

/// Static module setup: seed `CONFIG` once with the embedded defaults.
///
/// The pin layer lays over the startup config, so a test of the effective value
/// needs one to lay over, and the unit-test binary runs no server, so nothing
/// else sets it. `Config::default()` is the right base precisely because it is
/// the embedded defaults: the values a pin overrides in production. Tolerant
/// (`let _ =`) because `CONFIG` is a process-global OnceLock and first-set wins.
static TESTING: testing::Module = testing::module!(Unit, {
    .setup(|_module| {
        let _ = CONFIG.set(Config::default());
    })
});

/// This process, which is trivially alive - the only pid a test can pin to and
/// still be sure of the answer.
fn me() -> u32 {
    process::id()
}

#[tested]
fn start_time_is_field_22_counted_after_the_last_paren() {
    // comm is unquoted and may contain both spaces and parens, so fields are
    // counted after the last ')': index 0 is state (field 3), so starttime
    // (field 22) is index 19. Here the run of integers makes the position
    // readable - the 19th value after the state is 8675309.
    let stat = "42 (weird ) name) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 8675309 rest";
    assert_eq!(parse_start_time(stat), Some(8675309));
}

#[tested]
fn a_truncated_stat_yields_no_start_time() {
    assert!(parse_start_time("").is_none());
    assert!(parse_start_time("no parens here").is_none());
    assert!(
        parse_start_time("5 (x) Z 1 2 3").is_none(),
        "a stat cut short before field 22 must not read a nearby field as the start time",
    );
}

#[tested]
fn our_own_process_has_a_start_time() {
    // The pid-reuse guard rests on this being readable for a live process; if it
    // were not, every pin would be refused as dead.
    assert!(
        process_start_time(me()).is_some(),
        "the running test process must report a start time",
    );
}

#[tested]
fn every_pinnable_key_round_trips_its_name() {
    for key in PinnableKey::ALL {
        assert_eq!(
            PinnableKey::from_name(&key.name()),
            Some(key),
            "{} must parse back to itself",
            key.name(),
        );
        assert!(
            key.name().starts_with("supervisor."),
            "the pinnable set is [supervisor] only; got {}",
            key.name(),
        );
    }
}

#[tested]
fn a_channel_key_is_not_pinnable() {
    // config_channel is the sole mutator of the channel settings. A value with two
    // mutators is a value whose setting depends on which surface you ask. A refused
    // key never reaches the registry, so this stays a parallel test.
    for key in [
        "channel.port",
        "channel.cert_dir",
        "channel.spam_warn_rate",
        "channel.spam_error_window_secs",
    ] {
        let err = pin(key, me(), 1.0).expect_err("{key} must be refused");
        assert!(
            err.contains("not pinnable"),
            "the refusal should say why; got {err}",
        );
        assert!(
            err.contains("supervisor.cpu_warn_fraction"),
            "and should name the set that IS pinnable; got {err}",
        );
    }
}

#[tested]
fn an_unknown_key_is_refused() {
    assert!(pin("supervisor.nonesuch", me(), 0.5).is_err());
    assert!(pin("nonesuch", me(), 0.5).is_err());
    assert!(pin("", me(), 0.5).is_err());
}

#[tested]
fn a_pin_value_carries_the_shape_its_setting_holds() {
    // The enum pairs shape to key at construction, so a fraction can never be
    // written into the MiB slot by a later edit that only touches one side.
    assert_eq!(PinValue::Fraction(0.5), PinValue::Fraction(0.5));
    assert_ne!(PinValue::Fraction(1.0), PinValue::Mib(1));
}

/// The whole registry lifecycle as ONE sequenced test.
///
/// Every step mutates or reads the one process-global pin registry, so the
/// coverage runs as named Stepper steps inside a single test fn: cargo's
/// default parallelism cannot interleave clear/pin/read across test fns, by
/// construction rather than by a `--test-threads=1` flag. Each step opens on a
/// cleared registry.
#[tested]
fn registry_lifecycle_is_sequenced() {
    testing::Stepper::builder("pin_registry_lifecycle")
        .init(|_t, _o: ()| {
            clear_pins();
            testing::StepState((), ())
        })
        .step("a_pin_holds_the_value_the_watchdog_then_reads", step_pin_holds)
        .step("last_write_wins_on_the_same_key", step_last_write_wins)
        .step("pins_on_different_keys_coexist", step_coexist)
        .step("the_motivating_case_pins_vram_headroom_to_zero", step_vram_headroom_zero)
        .step("a_pin_is_held_to_the_file_layers_own_validators", step_validator_bounds)
        .step("a_dead_pid_is_refused_rather_than_created_and_reaped", step_dead_pid_refused)
        .step("a_pin_whose_process_is_gone_is_reaped", step_identity_mismatch_reaped)
        .finalize()
        .run(TESTING.as_testable(), ());
}

fn step_pin_holds(
    _t: &testing::Testable<'_, '_, '_>,
    _s: (),
    _p: (),
) -> testing::StepState<(), ()> {
    clear_pins();
    let before = effective_supervisor().cpu_warn_fraction;
    pin("supervisor.cpu_warn_fraction", me(), 0.25).expect("a live pid is pinnable");
    assert_eq!(
        effective_supervisor().cpu_warn_fraction,
        0.25,
        "the pin layer is what every reader goes through, so a pin is the setting",
    );
    assert_ne!(before, 0.25, "the fixture would prove nothing if it matched the default");
    clear_pins();
    assert_eq!(
        effective_supervisor().cpu_warn_fraction, before,
        "and clearing restores the startup config",
    );
    testing::StepState((), ())
}

fn step_last_write_wins(
    _t: &testing::Testable<'_, '_, '_>,
    _s: (),
    _p: (),
) -> testing::StepState<(), ()> {
    clear_pins();
    pin("supervisor.disk_warn_fraction", me(), 0.5).expect("first pin");
    pin("supervisor.disk_warn_fraction", me(), 0.9).expect("second pin");
    assert_eq!(
        effective_supervisor().disk_warn_fraction,
        0.9,
        "keying the map by setting is what makes last-write-wins fall out - there \
         is never a set of competing pins to arbitrate",
    );
    testing::StepState((), ())
}

fn step_coexist(
    _t: &testing::Testable<'_, '_, '_>,
    _s: (),
    _p: (),
) -> testing::StepState<(), ()> {
    clear_pins();
    pin("supervisor.cpu_warn_fraction", me(), 0.11).expect("cpu");
    pin("supervisor.vram_warn_headroom_mib", me(), 0.0).expect("vram");
    let supervisor = effective_supervisor();
    assert_eq!(supervisor.cpu_warn_fraction, 0.11);
    assert_eq!(supervisor.vram_warn_headroom_mib, 0);
    assert_eq!(
        supervisor.ram_warn_fraction, config().supervisor.ram_warn_fraction,
        "an unpinned setting keeps its configured value",
    );
    testing::StepState((), ())
}

fn step_vram_headroom_zero(
    _t: &testing::Testable<'_, '_, '_>,
    _s: (),
    _p: (),
) -> testing::StepState<(), ()> {
    clear_pins();
    // The gate fires at `total - headroom`, so zero headroom puts the line at the
    // card's full capacity and usage never reaches it. This is the training-run
    // case the whole feature exists for, so it must not be refused as a boundary.
    pin("supervisor.vram_warn_headroom_mib", me(), 0.0).expect("zero headroom is legal");
    assert_eq!(effective_supervisor().vram_warn_headroom_mib, 0);
    testing::StepState((), ())
}

fn step_validator_bounds(
    _t: &testing::Testable<'_, '_, '_>,
    _s: (),
    _p: (),
) -> testing::StepState<(), ()> {
    clear_pins();
    // A fraction outside (0, 1] would either warn always or never - the same bound
    // the file layer enforces, shared rather than restated so a pin cannot reach a
    // state a config load would have refused.
    for bad in [0.0, -0.1, 1.5] {
        let err = pin("supervisor.ram_warn_fraction", me(), bad)
            .expect_err("fraction {bad} must be refused");
        // Read the message, not merely the Err. Asserting only is_err() let a
        // doubled `supervisor.supervisor.ram_warn_fraction` reach production,
        // because the validators compose the table prefix themselves and the pin
        // path was handing them an already-dotted key.
        assert!(
            err.contains("supervisor.ram_warn_fraction"),
            "the refusal should name the setting once; got {err}",
        );
        assert!(
            !err.contains("supervisor.supervisor"),
            "the table prefix must not be doubled; got {err}",
        );
    }
    assert!(pin("supervisor.ram_warn_fraction", me(), 1.0).is_ok(), "1.0 is the top of the range");
    clear_pins();
    // A headroom is a whole number of MiB, so a fractional one is not a quantity
    // the setting can hold.
    assert!(pin("supervisor.vram_warn_headroom_mib", me(), 4096.5).is_err());
    assert!(pin("supervisor.vram_warn_headroom_mib", me(), -1.0).is_err());
    testing::StepState((), ())
}

fn step_dead_pid_refused(
    _t: &testing::Testable<'_, '_, '_>,
    _s: (),
    _p: (),
) -> testing::StepState<(), ()> {
    clear_pins();
    // pid 0 is never a real process. Refusing at pin time rather than accepting
    // and reaping a tick later matters because the two outcomes are
    // indistinguishable to the caller a second afterwards, and only one of them is
    // honest about what happened.
    let err = pin("supervisor.cpu_warn_fraction", 0, 0.5)
        .expect_err("a dead pid must be refused");
    assert!(err.contains("not running"), "got {err}");
    assert_eq!(
        effective_supervisor().cpu_warn_fraction,
        config().supervisor.cpu_warn_fraction,
        "a refused pin must leave nothing behind",
    );
    testing::StepState((), ())
}

fn step_identity_mismatch_reaped(
    _t: &testing::Testable<'_, '_, '_>,
    _s: (),
    _p: (),
) -> testing::StepState<(), ()> {
    clear_pins();
    pin("supervisor.cpu_warn_fraction", me(), 0.25).expect("pin to a live pid");
    // Rewrite the pin's identity to a start time this process cannot have. That is
    // exactly what a pid reuse looks like from the reaper's side: the pid resolves,
    // but the process wearing it is not the one that took the pin.
    crate::server::pin::corrupt_start_time_for_test(PinnableKey::CpuWarnFraction);
    reap_pins();
    assert_eq!(
        effective_supervisor().cpu_warn_fraction,
        config().supervisor.cpu_warn_fraction,
        "a pin whose process identity no longer matches must lapse",
    );
    clear_pins();
    testing::StepState((), ())
}
