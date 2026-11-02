//! Opt-in frame captures and their bounded core (docs/design/frame-telemetry.md).
//! The plugin supplies engine metadata, elapsed times and a guarded output path.

mod plugin;
pub use plugin::{finish_for_exit, FrameTelemetryPlugin};

mod git_paths;

pub mod capture;
pub mod records;
pub mod stats;

use std::fmt;
use std::time::Duration;

pub const MAX_FRAMES: u64 = 100_000;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const TRAILER_RESERVE: u64 = 16 * 1024;
const MAX_PAYLOAD: usize = 8 * 1024;
const MAX_LINE: usize = 16 * 1024;
const FRAME_SLOTS: usize = 1_024;
const DIAGNOSTIC_SLOTS: usize = 3_072;
const MARKERS_PER_FRAME: usize = 8;

/// Parsed aids. Zero duration disables capture, even when GPU observations are requested.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub duration: Duration,
    pub gpu: bool,
    pub run_id: Option<String>,
    pub scenario_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    pub field: &'static str,
    pub reason: &'static str,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

impl std::error::Error for ConfigError {}

/// Parse raw aids without consulting the process environment.
pub fn parse_aids(seconds: Option<&str>, gpu: Option<&str>, run_id: Option<&str>, scenario_id: Option<&str>) -> Result<Config, ConfigError> {
    let seconds = match seconds {
        None => 0.0,
        Some(raw) => raw.parse::<f64>().map_err(|_| ConfigError {
            field: "NVF_PLAY_TEST_FRAME_TELEMETRY",
            reason: "expected finite, nonnegative seconds",
        })?,
    };
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(ConfigError {
            field: "NVF_PLAY_TEST_FRAME_TELEMETRY",
            reason: "expected finite, nonnegative seconds",
        });
    }
    // Off: the other aids are not read, so a stray one warns about nothing (#1312).
    if seconds == 0.0 {
        return Ok(Config {
            duration: Duration::ZERO,
            gpu: false,
            run_id: None,
            scenario_id: None,
        });
    }
    let gpu = match gpu {
        None | Some("0") => false,
        Some("1") => true,
        _ => {
            return Err(ConfigError {
                field: "NVF_PLAY_TEST_FRAME_TELEMETRY_GPU",
                reason: "expected 0 or 1",
            })
        }
    };
    Ok(Config {
        // Keep a positive subnanosecond request enabled at the clock's resolution.
        duration: if seconds == 0.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(seconds.min(60.0)).max(Duration::from_nanos(1))
        },
        gpu,
        run_id: parse_id(run_id, "NVF_PLAY_TEST_FRAME_TELEMETRY_RUN_ID")?,
        scenario_id: parse_id(scenario_id, "NVF_PLAY_TEST_FRAME_TELEMETRY_SCENARIO_ID")?,
    })
}

fn parse_id(raw: Option<&str>, field: &'static str) -> Result<Option<String>, ConfigError> {
    // Design note: only capture_id has an explicit minimum length in the schema.
    raw.map(|id| {
        if id.len() <= 64 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
            Ok(id.to_owned())
        } else {
            Err(ConfigError {
                field,
                reason: "expected at most 64 ASCII letters, digits, underscores or hyphens",
            })
        }
    })
    .transpose()
}

#[cfg(test)]
mod tests;
