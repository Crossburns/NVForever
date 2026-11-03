//! Schema 1 records and compact JSONL encoding (docs/design/frame-telemetry.md §3).

use super::stats::PhaseSummary;
use super::{parse_id, Config, MAX_BYTES, MAX_FRAMES, MAX_LINE, MAX_PAYLOAD};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Startup,
    Loading,
    Gameplay,
}

impl Phase {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Loading => "loading",
            Self::Gameplay => "gameplay",
        }
    }

    pub(super) fn index(self) -> usize {
        match self {
            Self::Startup => 0,
            Self::Loading => 1,
            Self::Gameplay => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplaySync {
    Off,
    Vsync,
    Variable,
}

impl DisplaySync {
    fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Vsync => "vsync",
            Self::Variable => "variable",
        }
    }
}

/// Values collected by the engine, without depending on its types or querying it.
#[derive(Default)]
pub struct HeaderMetadata {
    pub started_at_utc: Option<String>,
    pub engine_revision: Option<String>,
    pub build_features: Vec<String>,
    pub adapter: Option<String>,
    pub backend: Option<String>,
    pub driver: Option<String>,
    pub driver_info: Option<String>,
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
    pub display_sync: Option<DisplaySync>,
    pub cap_fps: Option<u32>,
    pub refresh_millihz: Option<u32>,
    pub window_present_mode: Option<String>,
    pub renderer_available: bool,
    pub unavailable: BTreeMap<String, String>,
}

pub struct Header {
    pub(super) config: Config,
    capture_id: String,
    metadata: HeaderMetadata,
}

impl Header {
    pub fn new(capture_id: &str, config: Config, mut metadata: HeaderMetadata) -> io::Result<Self> {
        if capture_id.is_empty() || parse_id(Some(capture_id), "capture_id").is_err() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid capture_id"));
        }
        if config.duration.is_zero() || config.duration.as_secs_f64() > 60.0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "capture duration must be in (0, 60] seconds"));
        }
        parse_id(config.run_id.as_deref(), "run_id").map_err(io::Error::other)?;
        parse_id(config.scenario_id.as_deref(), "scenario_id").map_err(io::Error::other)?;
        metadata.build_features.sort();
        metadata.build_features.dedup();
        for (name, value) in [
            ("started_at_utc", &mut metadata.started_at_utc),
            ("engine_revision", &mut metadata.engine_revision),
            ("adapter", &mut metadata.adapter),
            ("backend", &mut metadata.backend),
            ("driver", &mut metadata.driver),
            ("driver_info", &mut metadata.driver_info),
            ("window_present_mode", &mut metadata.window_present_mode),
        ] {
            let reason = match value.as_deref() {
                Some(s) if s.len() > 1_024 => Some("over_1024_bytes"),
                Some("") => Some("empty"),
                None => Some("not_available"),
                Some(_) => None,
            };
            if let Some(reason) = reason {
                *value = None;
                metadata.unavailable.entry(name.to_owned()).or_insert_with(|| reason.to_owned());
            } else {
                metadata.unavailable.remove(name);
            }
        }
        for (name, missing) in [
            ("width_px", metadata.width_px.is_none()),
            ("height_px", metadata.height_px.is_none()),
            ("display_sync", metadata.display_sync.is_none()),
            ("cap_fps", metadata.cap_fps.is_none()),
            ("refresh_millihz", metadata.refresh_millihz.is_none()),
            ("window_present_mode", metadata.window_present_mode.is_none()),
        ] {
            if missing {
                metadata.unavailable.entry(name.to_owned()).or_insert_with(|| "not_available".to_owned());
            } else {
                metadata.unavailable.remove(name);
            }
        }
        metadata.unavailable.remove("diagnostics_reason");
        let header = Self {
            config,
            capture_id: capture_id.to_owned(),
            metadata,
        };
        Ok(header)
    }

    pub(super) fn line(&self) -> io::Result<Vec<u8>> {
        let m = &self.metadata;
        let (diagnostics, reason) = if !self.config.gpu {
            ("disabled", Some("not_requested"))
        } else if m.renderer_available {
            ("enabled", None)
        } else {
            ("unavailable", Some("no_renderer"))
        };
        encode(
            &json!({
                "type": "header", "seq": 0_u64, "t_ns": 0_u64, "schema_version": 1_u32,
                "capture_id": self.capture_id, "run_id": self.config.run_id, "scenario_id": self.config.scenario_id,
                "started_at_utc": m.started_at_utc, "engine_revision": m.engine_revision, "build_features": m.build_features,
                "adapter": m.adapter, "backend": m.backend, "driver": m.driver, "driver_info": m.driver_info,
                "width_px": m.width_px, "height_px": m.height_px, "display_sync": m.display_sync.map(DisplaySync::name),
                "cap_fps": m.cap_fps, "refresh_millihz": m.refresh_millihz, "window_present_mode": m.window_present_mode,
                "unavailable": m.unavailable, "max_duration_ns": self.config.duration.as_nanos() as u64,
                "max_frames": MAX_FRAMES, "max_bytes": MAX_BYTES, "diagnostics": diagnostics, "diagnostics_reason": reason,
                "gpu_origin": "unavailable_from_bevy_0_20_1",
                "render_cpu_origin": "source_frame_unavailable_from_bevy_0_20_1",
            }),
            false,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // The marker vocabulary also supports optional harness event producers.
pub enum MarkerKind {
    PhaseEnter,
    ScenarioBegin,
    ScenarioEnd,
    Waypoint,
    Event,
}

impl MarkerKind {
    fn name(self) -> &'static str {
        match self {
            Self::PhaseEnter => "phase_enter",
            Self::ScenarioBegin => "scenario_begin",
            Self::ScenarioEnd => "scenario_end",
            Self::Waypoint => "waypoint",
            Self::Event => "event",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Marker {
    kind: MarkerKind,
    id: String,
    pub(super) t_ns: u64,
}

impl Marker {
    pub fn new(kind: MarkerKind, id: &str, t_ns: u64) -> io::Result<Self> {
        if !id.is_ascii() || id.len() > 64 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "marker ID must be at most 64 ASCII bytes"));
        }
        Ok(Self { kind, id: id.to_owned(), t_ns })
    }

    fn json(&self) -> Value {
        json!({ "kind": self.kind.name(), "id": self.id, "t_ns": self.t_ns })
    }
}

#[derive(Debug)]
pub(super) struct Frame {
    pub main_frame: u32,
    pub duration_ns: Option<u64>,
    pub phase: Phase,
    pub markers: Vec<Marker>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    RenderCpuElapsed,
    GpuElapsed,
}

impl DiagnosticKind {
    fn name(self) -> &'static str {
        match self {
            Self::RenderCpuElapsed => "render_cpu_elapsed",
            Self::GpuElapsed => "gpu_elapsed",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::RenderCpuElapsed => "elapsed_cpu",
            Self::GpuElapsed => "elapsed_gpu",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticReason {
    TimestampQueriesUnsupported,
    NoSampleObserved,
    NonFiniteValue,
    UnitMismatch,
}

impl DiagnosticReason {
    fn name(self) -> &'static str {
        match self {
            Self::TimestampQueriesUnsupported => "timestamp_queries_unsupported",
            Self::NoSampleObserved => "no_sample_observed",
            Self::NonFiniteValue => "non_finite_value",
            Self::UnitMismatch => "unit_mismatch",
        }
    }

    fn status(self) -> &'static str {
        match self {
            Self::TimestampQueriesUnsupported | Self::NoSampleObserved => "unavailable",
            Self::NonFiniteValue | Self::UnitMismatch => "invalid",
        }
    }
}

#[derive(Debug)]
pub struct Diagnostic {
    path: String,
    kind: DiagnosticKind,
    value: Option<f64>,
    reason: Option<DiagnosticReason>,
    observation_t_ns: u64,
    observation_main_frame: u32,
}

impl Diagnostic {
    /// Invalid values remain explicit null observations; overlong paths are exporter drops.
    pub fn measurement(path: &str, unit: &str, kind: DiagnosticKind, value: f64, observation_t_ns: u64, observation_main_frame: u32) -> Self {
        let reason = if unit != "ms" || !path.ends_with(kind.suffix()) {
            Some(DiagnosticReason::UnitMismatch)
        } else if !value.is_finite() {
            Some(DiagnosticReason::NonFiniteValue)
        } else {
            None
        };
        Self {
            path: path.to_owned(),
            kind,
            value: reason.is_none().then_some(value),
            reason,
            observation_t_ns,
            observation_main_frame,
        }
    }

    pub fn gpu_sentinel(reason: DiagnosticReason, observation_t_ns: u64, observation_main_frame: u32) -> io::Result<Self> {
        Self::unavailable(
            "render/elapsed_gpu",
            DiagnosticKind::GpuElapsed,
            reason,
            observation_t_ns,
            observation_main_frame,
        )
    }

    pub fn unavailable(path: &str, kind: DiagnosticKind, reason: DiagnosticReason, observation_t_ns: u64, observation_main_frame: u32) -> io::Result<Self> {
        if !matches!(reason, DiagnosticReason::TimestampQueriesUnsupported | DiagnosticReason::NoSampleObserved) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "absence requires an unavailable reason"));
        }
        Ok(Self {
            path: path.to_owned(),
            kind,
            value: None,
            reason: Some(if path.ends_with(kind.suffix()) {
                reason
            } else {
                DiagnosticReason::UnitMismatch
            }),
            observation_t_ns,
            observation_main_frame,
        })
    }

    pub(super) fn available_gpu(&self) -> bool {
        self.kind == DiagnosticKind::GpuElapsed && self.reason.is_none()
    }

    fn json(&self) -> Value {
        json!({
            "path": self.path, "unit": "ms", "kind": self.kind.name(), "value": self.value,
            "status": self.reason.map_or("available", DiagnosticReason::status), "reason": self.reason.map(DiagnosticReason::name),
            "observation_t_ns": self.observation_t_ns, "observation_main_frame": self.observation_main_frame,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    DurationLimit,
    FrameLimit,
    ByteLimit,
    NormalExit,
    SettingsChanged,
    WriterError,
    ShutdownTimeout,
}

impl StopReason {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::DurationLimit => "duration_limit",
            Self::FrameLimit => "frame_limit",
            Self::ByteLimit => "byte_limit",
            Self::NormalExit => "normal_exit",
            Self::SettingsChanged => "settings_changed",
            Self::WriterError => "writer_error",
            Self::ShutdownTimeout => "shutdown_timeout",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Trailer {
    pub seq: u64,
    pub t_ns: u64,
    pub stop_reason: StopReason,
    pub cut_seq: Option<u64>,
    pub frames_attempted: u64,
    pub frames_written: u64,
    pub dropped_frames: u64,
    pub dropped_diagnostics: u64,
    pub dropped_markers: u64,
    pub gpu_observations_written: u64,
    pub phases: [PhaseSummary; 3],
}

impl Trailer {
    pub fn complete(&self) -> bool {
        matches!(
            self.stop_reason,
            StopReason::DurationLimit | StopReason::FrameLimit | StopReason::ByteLimit | StopReason::NormalExit
        ) && self.dropped_frames == 0
            && self.dropped_diagnostics == 0
            && self.dropped_markers == 0
    }

    pub(super) fn line(&self) -> io::Result<Vec<u8>> {
        encode(
            &json!({
                "type": "trailer", "seq": self.seq, "t_ns": self.t_ns, "status": if self.complete() { "complete" } else { "truncated" },
                "stop_reason": self.stop_reason.name(), "cut_seq": self.cut_seq,
                "frames_attempted": self.frames_attempted, "frames_written": self.frames_written,
                "dropped_frames": self.dropped_frames, "dropped_diagnostics": self.dropped_diagnostics, "dropped_markers": self.dropped_markers,
                "gpu_observations_written": self.gpu_observations_written,
                "phases": { "startup": self.phases[0].json(), "loading": self.phases[1].json(), "gameplay": self.phases[2].json() },
            }),
            false,
        )
    }
}

#[derive(Debug)]
pub(super) enum Payload {
    Frame(Frame),
    Diagnostic(Diagnostic),
}

impl Payload {
    /// Constructors bound marker IDs; the remaining checks need no JSON encoding.
    pub fn within_bounds(&self) -> bool {
        match self {
            Self::Frame(frame) => frame.markers.len() <= super::MARKERS_PER_FRAME,
            Self::Diagnostic(diagnostic) => diagnostic.path.len() <= 1_024,
        }
    }
}

/// Cumulative attempts through this record's seq. Markers belong to their frame attempt.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AttemptCounts {
    pub frames: u64,
    pub diagnostics: u64,
    pub markers: u64,
}

#[derive(Debug)]
pub(super) struct Record {
    pub seq: u64,
    pub t_ns: u64,
    pub payload: Payload,
    pub attempts: AttemptCounts,
}

impl Record {
    pub fn line(&self) -> io::Result<Vec<u8>> {
        let mut value = match &self.payload {
            Payload::Frame(f) => json!({
                "type": "frame", "main_frame": f.main_frame, "duration_ns": f.duration_ns,
                "duration_reason": if f.duration_ns.is_none() { Some("no_previous_time_update") } else { None },
                "phase": f.phase.name(), "markers": f.markers.iter().map(Marker::json).collect::<Vec<_>>(),
            }),
            Payload::Diagnostic(d) => {
                if d.path.len() > 1_024 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "diagnostic path exceeds 1024 bytes"));
                }
                let mut value = d.json();
                value["type"] = json!("diagnostic");
                value
            }
        };
        value["seq"] = json!(self.seq);
        value["t_ns"] = json!(self.t_ns);
        encode(&value, true)
    }
}

pub(super) fn encode(value: &Value, payload: bool) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    if (payload && bytes.len() > MAX_PAYLOAD) || bytes.len() + 1 > MAX_LINE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "telemetry record exceeds its byte limit"));
    }
    bytes.push(b'\n');
    Ok(bytes)
}
