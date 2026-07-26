//! Config pins: a live layer in front of the startup config.
//!
//! `CONFIG` is a `OnceLock` set at startup and never written again, so anything
//! adjustable at runtime needs its own live copy. The channel's spam thresholds
//! are the existing precedent for that shape; this is the same idea applied to
//! the `[supervisor]` warning lines, with one addition - a pin is bound to the
//! lifetime of a process rather than left to be unset by hand.
//!
//! The motivating case is a training run: pin `vram_warn_headroom_mib` to 0 for
//! the run's own pid. The gate fires at `total - headroom`, so zero headroom puts
//! the line at the card's full capacity and usage never reaches it. When the
//! training process exits the pin lapses on its own, which is the whole point -
//! nothing has to remember to put the threshold back.

use crate::*;

/// A `[supervisor]` setting that may be pinned.
///
/// An enum rather than a string key, because the pinnable set is closed by design
/// and this makes it closed in the type system too: a new pinnable setting cannot
/// be added without the compiler asking what it means everywhere.
///
/// `[channel]` is deliberately absent. `config_channel` already mutates those, and
/// a value with two mutators is a value whose effective setting depends on which
/// surface you happen to ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PinnableKey {
    CpuWarnFraction,
    RamWarnFraction,
    VramWarnHeadroomMib,
    DiskWarnFraction,
}

/// The table every pinnable setting lives under, and the prefix its dotted key
/// carries.
const SUPERVISOR_TABLE: &str = "supervisor";

impl PinnableKey {
    /// The bare TOML field name, without its table prefix.
    ///
    /// This is what the file layer's validators take, because they compose the
    /// prefix themselves. Passing the already-dotted `name()` to one produced
    /// `supervisor.supervisor.cpu_warn_fraction` in a live error message - caught
    /// by the production smoke test rather than by the unit test, which asserted
    /// only that the value was refused and never read the text back.
    pub(crate) fn field(self) -> &'static str {
        match self {
            Self::CpuWarnFraction => "cpu_warn_fraction",
            Self::RamWarnFraction => "ram_warn_fraction",
            Self::VramWarnHeadroomMib => "vram_warn_headroom_mib",
            Self::DiskWarnFraction => "disk_warn_fraction",
        }
    }

    /// The dotted config key, matching the TOML path exactly.
    ///
    /// Composed from `field()` rather than written out a second time, so the two
    /// spellings of one setting cannot drift apart.
    pub(crate) fn name(self) -> String {
        format!("{SUPERVISOR_TABLE}.{}", self.field())
    }

    /// Parse a dotted key back to its setting.
    ///
    /// Matched against `name()` rather than against a third list of literals, for
    /// the same reason `name()` composes from `field()`: one spelling, one place.
    pub(crate) fn from_name(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == key)
    }

    pub(crate) const ALL: [Self; 4] = [
        Self::CpuWarnFraction,
        Self::RamWarnFraction,
        Self::VramWarnHeadroomMib,
        Self::DiskWarnFraction,
    ];
}

/// A pinned value, in the shape the setting it overrides actually holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PinValue {
    Fraction(f64),
    Mib(u64),
}

/// One live pin: what it sets, and the process whose life it is bound to.
///
/// `start_time` is the pid-reuse guard. A pid alone is not an identity on a
/// long-lived host - the kernel recycles them - so the pair (pid, start_time) is
/// what actually names the process. On every check the stored start time is
/// compared against the live one, and a mismatch means the pin's process is gone
/// and something else now holds its number.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConfigPin {
    pub value: PinValue,
    pub pid: u32,
    pub start_time: u64,
}

/// The live pins, one per key.
///
/// A map keyed by the setting is what makes last-write-wins fall out rather than
/// be enforced: a second pin on the same key replaces the first, so there is never
/// a set of competing pins to arbitrate between.
static PINS: LazyLock<std::sync::Mutex<HashMap<PinnableKey, ConfigPin>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

fn pins() -> std::sync::MutexGuard<'static, HashMap<PinnableKey, ConfigPin>> {
    PINS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Validate a pinned value against the same check the file layer applies.
///
/// Shared rather than restated, so a pin cannot reach a state a config load would
/// have refused - a fraction stays inside (0, 1], and a headroom stays a
/// non-negative whole number of MiB. Zero headroom is legal, and is the motivating
/// case: it puts the warning line at the card's full capacity.
fn validate(
    key: PinnableKey,
    value: f64,
) -> Result<PinValue, String> {
    match key {
        PinnableKey::VramWarnHeadroomMib => {
            if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
                return Err(format!(
                    "{} must be a whole number of MiB, zero or greater",
                    key.name(),
                ));
            }
            Ok(PinValue::Mib(value as u64))
        }
        // The bare field name: `fraction_field` composes the `supervisor.` prefix
        // itself, so handing it the dotted key doubles it.
        _ => fraction_field(key.field(), Some(value)).map(PinValue::Fraction),
    }
}

/// Pin a setting to the lifetime of a process.
///
/// The pid is proven live first. A pin against an already-dead process is refused
/// rather than created and reaped a tick later, because those two outcomes look
/// identical to the caller a second afterwards and only one of them is honest
/// about what happened.
pub(crate) fn pin(
    key: &str,
    pid: u32,
    value: f64,
) -> Result<(), String> {
    let Some(key) = PinnableKey::from_name(key) else {
        return Err(format!(
            "`{key}` is not pinnable; the pinnable settings are {}",
            PinnableKey::ALL
                .iter()
                .map(|k| k.name())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    };
    let value = validate(key, value)?;
    let Some(start_time) = process_start_time(pid) else {
        return Err(format!("process {pid} is not running; nothing to pin to"));
    };
    pins().insert(
        key,
        ConfigPin {
            value,
            pid,
            start_time,
        },
    );
    Ok(())
}

/// Drop every pin whose process has gone.
///
/// Called on the watchdog tick, which already scans /proc, and again before any
/// read of the effective config, so a caller can never observe a pin held by a
/// process that has already exited, whatever the tick happens to be doing.
pub(crate) fn reap_pins() {
    pins().retain(|_, pin| process_start_time(pin.pid) == Some(pin.start_time));
}

/// The supervisor settings as they currently apply: the startup config with any
/// live pin laid over it.
///
/// Every reader goes through this rather than `config().supervisor`, which is what
/// keeps a pin from being something the watchdog honors while `get_config` reports
/// the old value.
pub(crate) fn effective_supervisor() -> SupervisorConfig {
    reap_pins();
    let mut supervisor = config().supervisor;
    for (key, pin) in pins().iter() {
        match (key, pin.value) {
            (PinnableKey::CpuWarnFraction, PinValue::Fraction(v)) => {
                supervisor.cpu_warn_fraction = v;
            }
            (PinnableKey::RamWarnFraction, PinValue::Fraction(v)) => {
                supervisor.ram_warn_fraction = v;
            }
            (PinnableKey::DiskWarnFraction, PinValue::Fraction(v)) => {
                supervisor.disk_warn_fraction = v;
            }
            (PinnableKey::VramWarnHeadroomMib, PinValue::Mib(v)) => {
                supervisor.vram_warn_headroom_mib = v;
            }
            // Unreachable: `validate` is the only constructor and it pairs the
            // shape to the key. Ignoring rather than panicking keeps a future
            // mispairing a wrong reading instead of a dead host.
            _ => {}
        }
    }
    supervisor
}

/// Drop every pin, live or not.
///
/// Test-only in purpose - the registry is process-global, so a test that pins has
/// to be able to put it back - but not `cfg(test)`, because the integration tests
/// are separate crates that see only `pub` items and reach it through `guts`.
pub(crate) fn clear_pins() {
    pins().clear();
}

/// Make a pin's recorded identity no longer match its process.
///
/// This is what a pid reuse looks like from the reaper's side - the pid still
/// resolves, but the process wearing it is not the one that took the pin - and it
/// is not otherwise reachable in a test: forcing a real reuse means exhausting the
/// pid space.
#[cfg(test)]
pub(crate) fn corrupt_start_time_for_test(key: PinnableKey) {
    if let Some(pin) = pins().get_mut(&key) {
        pin.start_time = pin.start_time.wrapping_add(1);
    }
}
