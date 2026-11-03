//! Statistics over wholly written frames (docs/design/frame-telemetry.md §3).

use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct PhaseSummary {
    pub frames_written: u64,
    pub samples: u64,
    pub above_33_ms: u64,
    pub above_50_ms: u64,
    pub above_100_ms: u64,
    pub above_250_ms: u64,
    pub mean_ms: Option<f64>,
    pub worst_ms: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub slowest_1pct_mean_ms: Option<f64>,
}

impl PhaseSummary {
    pub(super) fn json(&self) -> Value {
        json!({
            "frames_written": self.frames_written, "samples": self.samples,
            "above_33_ms": self.above_33_ms, "above_50_ms": self.above_50_ms,
            "above_100_ms": self.above_100_ms, "above_250_ms": self.above_250_ms,
            "mean_ms": self.mean_ms, "worst_ms": self.worst_ms,
            "p50_ms": self.p50_ms, "p95_ms": self.p95_ms, "p99_ms": self.p99_ms,
            "slowest_1pct_mean_ms": self.slowest_1pct_mean_ms,
        })
    }
}

#[derive(Default)]
pub(super) struct PhaseStats {
    frames_written: u64,
    durations: Vec<u64>,
}

impl PhaseStats {
    pub fn record(&mut self, duration: Option<u64>) {
        self.frames_written += 1;
        if let Some(duration) = duration {
            self.durations.push(duration);
        }
    }

    pub fn summary(&mut self) -> PhaseSummary {
        self.durations.sort_unstable();
        let n = self.durations.len();
        let mean =
            |values: &[u64]| (!values.is_empty()).then(|| values.iter().map(|&v| u128::from(v)).sum::<u128>() as f64 / values.len() as f64 / 1_000_000.0);
        let rank = |percent: usize| (n > 0).then(|| self.durations[(percent * n).div_ceil(100) - 1] as f64 / 1_000_000.0);
        PhaseSummary {
            frames_written: self.frames_written,
            samples: n as u64,
            above_33_ms: self.durations.iter().filter(|&&v| v > 33_000_000).count() as u64,
            above_50_ms: self.durations.iter().filter(|&&v| v > 50_000_000).count() as u64,
            above_100_ms: self.durations.iter().filter(|&&v| v > 100_000_000).count() as u64,
            above_250_ms: self.durations.iter().filter(|&&v| v > 250_000_000).count() as u64,
            mean_ms: mean(&self.durations),
            worst_ms: self.durations.last().map(|&v| v as f64 / 1_000_000.0),
            p50_ms: rank(50),
            p95_ms: rank(95),
            p99_ms: rank(99),
            slowest_1pct_mean_ms: if n == 0 { None } else { mean(&self.durations[n - (n / 100).max(1)..]) },
        }
    }
}
