//! Config pins: a live layer in front of the startup config.

use crate::*;

/// A `[supervisor]` setting that may be pinned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PinnableKey {
    CpuWarnFraction,
    RamWarnFraction,
    VramWarnHeadroomMib,
    DiskWarnFraction,
}

/// The table every pinnable setting lives under.
const SUPERVISOR_TABLE: &str = "supervisor";

impl PinnableKey {
    /// The bare TOML field name, without its table prefix.
    pub(crate) fn field(self) -> &'static str {
        match self {
            Self::CpuWarnFraction => "cpu_warn_fraction",
            Self::RamWarnFraction => "ram_warn_fraction",
            Self::VramWarnHeadroomMib => "vram_warn_headroom_mib",
            Self::DiskWarnFraction => "disk_warn_fraction",
        }
    }

    /// The dotted config key, matching the TOML path exactly.
    pub(crate) fn name(self) -> String {
        format!("{SUPERVISOR_TABLE}.{}", self.field())
    }

    /// Parse a dotted key back to its setting.
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

/// A pinned value, in the shape the setting it overrides holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PinValue {
    Fraction(f64),
    Mib(u64),
}

/// One live pin: what it sets, and the process whose life it follows.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConfigPin {
    pub value: PinValue,
    pub pid: u32,
    pub start_time: u64,
}

/// The live pins, one per key.
static PINS: LazyLock<std::sync::Mutex<HashMap<PinnableKey, ConfigPin>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

fn pins() -> std::sync::MutexGuard<'static, HashMap<PinnableKey, ConfigPin>> {
    PINS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Validate a pinned value against the file layer's own check.
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
        _ => fraction_field(key.field(), Some(value)).map(PinValue::Fraction),
    }
}

/// Pin a setting to the lifetime of a process.
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
pub(crate) fn reap_pins() {
    pins().retain(|_, pin| process_start_time(pin.pid) == Some(pin.start_time));
}

/// The supervisor settings as they currently apply.
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
            _ => {}
        }
    }
    supervisor
}

/// Drop every pin, live or not.
pub(crate) fn clear_pins() {
    pins().clear();
}

/// Make a pin's recorded identity no longer match its process.
#[cfg(test)]
pub(crate) fn corrupt_start_time_for_test(key: PinnableKey) {
    if let Some(pin) = pins().get_mut(&key) {
        pin.start_time = pin.start_time.wrapping_add(1);
    }
}
