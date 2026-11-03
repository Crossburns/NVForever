//! Synthetic schema and capture checks (docs/design/frame-telemetry.md §5).

use super::capture::test_support;
use super::capture::{Capture, Shutdown, WriterReport};
use super::records::{
    AttemptCounts, Diagnostic, DiagnosticKind, DiagnosticReason, DisplaySync, Frame, Header, HeaderMetadata, Marker, MarkerKind, Payload, Phase, Record,
    StopReason, Trailer,
};
use super::stats::PhaseStats;
use super::*;
use serde_json::{json, Value};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct Memory(Arc<Mutex<Vec<u8>>>);

impl Write for Memory {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Memory {
    fn lines(&self) -> Vec<Value> {
        let bytes = self.0.lock().unwrap();
        assert!(bytes.ends_with(b"\n"));
        assert!(!bytes.starts_with(&[0xef, 0xbb, 0xbf]));
        bytes
            .split_inclusive(|&b| b == b'\n')
            .map(|line| {
                assert!(line.len() <= MAX_LINE);
                serde_json::from_slice(line).unwrap()
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
enum HoldAt {
    Header,
    Trailer,
    FinalFlush,
}

// Hold a chosen writer operation so failure-path tests never depend on scheduling.
struct GateSink {
    memory: Memory,
    started: Option<Sender<()>>,
    release: Receiver<()>,
    done: Sender<()>,
    hold_at: HoldAt,
    saw_trailer: bool,
    fail_final_flush: bool,
}

impl GateSink {
    fn hold(&mut self) {
        if let Some(started) = self.started.take() {
            started.send(()).unwrap();
            let _ = self.release.recv();
        }
    }
}

impl Write for GateSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.saw_trailer |= bytes.windows(b"\"type\":\"trailer\"".len()).any(|part| part == b"\"type\":\"trailer\"");
        if matches!(self.hold_at, HoldAt::Header) || (matches!(self.hold_at, HoldAt::Trailer) && self.saw_trailer) {
            self.hold();
        }
        self.memory.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.saw_trailer {
            if matches!(self.hold_at, HoldAt::FinalFlush) {
                self.hold();
            }
            if self.fail_final_flush {
                return Err(io::Error::other("synthetic final flush failure"));
            }
        }
        Ok(())
    }
}

impl Drop for GateSink {
    fn drop(&mut self) {
        let _ = self.done.send(());
    }
}

struct Gate {
    memory: Memory,
    started: Receiver<()>,
    release: Option<Sender<()>>,
    done: Receiver<()>,
}

impl Gate {
    fn new() -> (Self, GateSink) {
        Self::at(HoldAt::Header)
    }

    fn at(hold_at: HoldAt) -> (Self, GateSink) {
        let memory = Memory::default();
        let (started_tx, started) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let sink = GateSink {
            memory: memory.clone(),
            started: Some(started_tx),
            release: release_rx,
            done: done_tx,
            hold_at,
            saw_trailer: false,
            fail_final_flush: false,
        };
        (
            Self {
                memory,
                started,
                release: Some(release),
                done,
            },
            sink,
        )
    }
    fn wait(&self) {
        self.started.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    fn open(&mut self) {
        self.release.take().unwrap().send(()).unwrap();
    }
}

fn config(gpu: bool) -> Config {
    parse_aids(Some("60"), Some(if gpu { "1" } else { "0" }), Some("run-1"), Some("scenario_1")).unwrap()
}

fn header(gpu: bool) -> Header {
    Header::new(
        "synthetic",
        config(gpu),
        HeaderMetadata {
            renderer_available: true,
            ..Default::default()
        },
    )
    .unwrap()
}

fn capture(gpu: bool) -> (Capture, Memory) {
    let memory = Memory::default();
    (
        test_support::start(memory.clone(), header(gpu), MAX_FRAMES, MAX_BYTES, FRAME_SLOTS, DIAGNOSTIC_SLOTS).unwrap(),
        memory,
    )
}

fn finish(capture: Capture) -> WriterReport {
    match capture.finish_at(50_000_000_000, StopReason::NormalExit, None) {
        Shutdown::Finished(report) => {
            assert!(report.error.is_none(), "{:?}", report.error);
            *report
        }
        Shutdown::TimedOut | Shutdown::TrailerInProgress => panic!("synthetic capture did not finish"),
    }
}

fn diagnostic(value: f64) -> Diagnostic {
    Diagnostic::measurement("render/elapsed_gpu", "ms", DiagnosticKind::GpuElapsed, value, 1, 0)
}

#[test]
fn cursor_drops_are_counted_as_record_attempts() {
    let (mut capture, memory) = capture(true);
    capture.drop_diagnostic_at(1);
    assert!(capture.diagnostic_at(2, diagnostic(1.0)));
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(trailer.dropped_diagnostics, 1);
    assert!(!trailer.complete());
    assert_eq!(memory.lines()[1]["seq"], 2);
}

#[test]
fn nonblocking_finish_can_poll_while_writer_is_held() {
    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, header(false), MAX_FRAMES, MAX_BYTES, 1, 1).unwrap();
    gate.wait();
    assert!(capture.frame_at(1, 0, 0, Phase::Startup));
    capture.begin_finish_at(2, StopReason::NormalExit, None);
    assert!(capture.poll_finish().is_none());
    assert!(!capture.frame_at(3, 1, 0, Phase::Gameplay));
    gate.open();
    let deadline = Instant::now() + Duration::from_secs(2);
    let result = loop {
        if let Some(result) = capture.poll_finish() {
            break result;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    let Shutdown::Finished(report) = result else { panic!() };
    assert!(report.trailer.unwrap().complete());
    assert!(capture.poll_finish().is_none());
    assert_eq!(gate.memory.lines().len(), 3);
}

#[test]
fn capture_can_be_stored_as_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Capture>();
}

fn stats(values_ms: &[u64]) -> stats::PhaseSummary {
    let mut stats = PhaseStats::default();
    for &value in values_ms {
        stats.record(Some(value * 1_000_000));
    }
    stats.summary()
}

#[test]
fn worked_distribution() {
    let s = stats(&[10, 20, 30, 40, 100]);
    assert_eq!((s.frames_written, s.samples), (5, 5));
    assert_eq!((s.mean_ms, s.worst_ms), (Some(40.0), Some(100.0)));
    assert_eq!(
        (s.p50_ms, s.p95_ms, s.p99_ms, s.slowest_1pct_mean_ms),
        (Some(30.0), Some(100.0), Some(100.0), Some(100.0))
    );
    assert_eq!((s.above_33_ms, s.above_50_ms, s.above_100_ms, s.above_250_ms), (2, 1, 0, 0));
}

#[test]
fn empty_single_and_constant_distributions() {
    let empty = stats(&[]);
    assert_eq!((empty.frames_written, empty.samples), (0, 0));
    assert_eq!(
        (
            empty.mean_ms,
            empty.worst_ms,
            empty.p50_ms,
            empty.p95_ms,
            empty.p99_ms,
            empty.slowest_1pct_mean_ms
        ),
        (None, None, None, None, None, None)
    );
    for values in [&[7][..], &[7, 7, 7][..]] {
        let s = stats(values);
        assert_eq!(
            (s.mean_ms, s.p50_ms, s.p95_ms, s.p99_ms, s.slowest_1pct_mean_ms),
            (Some(7.0), Some(7.0), Some(7.0), Some(7.0), Some(7.0))
        );
    }
}

#[test]
fn slowest_one_percent_uses_floor_at_99_100_199_200() {
    for (n, p99, tail) in [(99, 99.0, 99.0), (100, 99.0, 100.0), (199, 198.0, 199.0), (200, 198.0, 199.5)] {
        let values: Vec<u64> = (1..=n).rev().collect();
        let s = stats(&values);
        assert_eq!(s.p99_ms, Some(p99), "N={n}");
        assert_eq!(s.slowest_1pct_mean_ms, Some(tail), "N={n}");
    }
}

#[test]
fn thresholds_are_strict_integer_nanoseconds() {
    let mut s = PhaseStats::default();
    for t in [33_000_000, 50_000_000, 100_000_000, 250_000_000] {
        s.record(Some(t));
    }
    let summary = s.summary();
    assert_eq!(
        (summary.above_33_ms, summary.above_50_ms, summary.above_100_ms, summary.above_250_ms),
        (3, 2, 1, 0)
    );
    s.record(Some(250_000_001));
    assert_eq!(s.summary().above_250_ms, 1);
    let mut only_33 = PhaseStats::default();
    only_33.record(Some(33_000_000));
    assert_eq!(only_33.summary().above_33_ms, 0);
}

#[test]
fn first_null_later_zero_phases_and_frame_count_wrap() {
    let (mut capture, memory) = capture(false);
    assert!(capture.frame_at(1, u32::MAX, 999, Phase::Startup));
    assert!(capture.frame_at(2, 0, 0, Phase::Loading));
    assert!(capture.frame_at(3, 1, 10_000_000, Phase::Gameplay));
    let report = finish(capture);
    let trailer = report.trailer.unwrap();
    assert!(trailer.complete());
    assert_eq!((trailer.frames_attempted, trailer.frames_written), (3, 3));
    assert_eq!((trailer.phases[0].frames_written, trailer.phases[0].samples), (1, 0));
    assert_eq!(trailer.phases[1].mean_ms, Some(0.0));
    assert_eq!(trailer.phases[2].mean_ms, Some(10.0));
    let lines = memory.lines();
    assert!(lines[1]["duration_ns"].is_null());
    assert_eq!(lines[1]["duration_reason"], "no_previous_time_update");
    assert_eq!(lines[2]["duration_ns"], 0);
    assert!(lines[2]["duration_reason"].is_null());
    assert_eq!(
        (lines[1]["main_frame"].as_u64(), lines[2]["main_frame"].as_u64()),
        (Some(u64::from(u32::MAX)), Some(0))
    );
}

#[test]
fn aid_parsing_accepts_and_caps() {
    let off = parse_aids(None, None, None, None).unwrap();
    assert!(off.duration.is_zero());
    assert!(!off.gpu);
    assert_eq!(parse_aids(Some("1e-12"), None, None, None).unwrap().duration, Duration::from_nanos(1));
    assert!(parse_aids(Some("0"), Some("1"), None, None).unwrap().duration.is_zero());
    for (raw, ns) in [("0.25", 250_000_000), ("60", 60_000_000_000), ("61", 60_000_000_000), ("1e300", 60_000_000_000)] {
        assert_eq!(parse_aids(Some(raw), Some("0"), None, None).unwrap().duration.as_nanos(), ns);
    }
    let id = "a".repeat(64);
    assert_eq!(
        parse_aids(Some("1"), None, Some(&id), Some("A-z_09")).unwrap().run_id.as_deref(),
        Some(id.as_str())
    );
    assert_eq!(parse_aids(Some("1"), None, Some(""), None).unwrap().run_id.as_deref(), Some(""));
    // With capture off the other aids are not read: a bad one warns about nothing.
    assert_eq!(parse_aids(None, Some("2"), Some("a b"), Some("a/b")).unwrap(), off);
    assert_eq!(parse_aids(Some("0"), Some("true"), None, None).unwrap(), off);
}

#[test]
fn aid_parsing_rejects_bad_values_even_when_disabled() {
    for raw in ["-1", "NaN", "inf", "-inf", "1e999", "no", "", " 1"] {
        assert!(parse_aids(Some(raw), None, None, None).is_err(), "{raw}");
    }
    for raw in ["2", "true", "-1", "", "1.0", " 1"] {
        let error = parse_aids(Some("1"), Some(raw), None, None).unwrap_err();
        assert!(error.to_string().contains("GPU"));
    }
    for id in ["a".repeat(65), "é".to_owned(), "a/b".to_owned(), "a b".to_owned(), "a\n".to_owned()] {
        assert!(parse_aids(Some("1"), None, Some(&id), None).is_err());
        assert!(parse_aids(Some("1"), None, None, Some(&id)).is_err());
    }
}

#[test]
fn queue_overflow_preserves_frame_capacity_and_seq_gaps_equal_drops() {
    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, header(true), MAX_FRAMES, MAX_BYTES, FRAME_SLOTS, DIAGNOSTIC_SLOTS).unwrap();
    gate.wait();
    for i in 0..DIAGNOSTIC_SLOTS {
        assert!(capture.diagnostic_at(i as u64, diagnostic(1.0)));
    }
    assert!(!capture.diagnostic_at(4_000, diagnostic(1.0)));
    for i in 0..FRAME_SLOTS {
        assert!(capture.frame_at(4_001 + i as u64, i as u32, 1, Phase::Gameplay));
    }
    assert!(!capture.frame_at(6_000, 2_000, 1, Phase::Gameplay));
    gate.open();
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(
        (
            trailer.frames_attempted,
            trailer.frames_written,
            trailer.dropped_frames,
            trailer.dropped_diagnostics
        ),
        (1_025, 1_024, 1, 1)
    );
    assert!(!trailer.complete());
    let lines = gate.memory.lines();
    let gaps: u64 = lines
        .windows(2)
        .map(|pair| pair[1]["seq"].as_u64().unwrap() - pair[0]["seq"].as_u64().unwrap() - 1)
        .sum();
    assert_eq!(gaps, trailer.dropped_frames + trailer.dropped_diagnostics);
    assert!(lines.windows(2).all(|pair| pair[0]["t_ns"].as_u64() <= pair[1]["t_ns"].as_u64()));
}

#[test]
fn interleaved_queues_merge_by_seq() {
    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, header(true), MAX_FRAMES, MAX_BYTES, 4, 4).unwrap();
    gate.wait();
    for i in 1..=4 {
        assert!(capture.frame_at(i * 2, i as u32, 1, Phase::Gameplay));
        assert!(capture.diagnostic_at(i * 2 + 1, diagnostic(1.0)));
    }
    gate.open();
    assert!(finish(capture).trailer.unwrap().complete());
    for (seq, line) in gate.memory.lines().iter().enumerate() {
        assert_eq!(line["seq"], seq as u64);
    }
}

#[test]
fn marker_overflow_order_and_unwritten_frame_markers() {
    let (mut capture, memory) = capture(false);
    for i in 0..10 {
        assert_eq!(capture.marker(Marker::new(MarkerKind::Event, &i.to_string(), i).unwrap()), i < 8);
    }
    assert!(capture.frame_at(10, 0, 0, Phase::Gameplay));
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(trailer.dropped_markers, 2);
    assert!(!trailer.complete());
    let lines = memory.lines();
    let markers = lines[1]["markers"].as_array().unwrap();
    assert_eq!(markers.len(), 8);
    for (i, marker) in markers.iter().enumerate() {
        assert_eq!(marker["id"], i.to_string());
    }

    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, header(false), MAX_FRAMES, MAX_BYTES, 0, 0).unwrap();
    gate.wait();
    capture.marker(Marker::new(MarkerKind::Waypoint, "0", 0).unwrap());
    assert!(!capture.frame_at(1, 0, 0, Phase::Gameplay));
    gate.open();
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!((trailer.dropped_frames, trailer.dropped_markers), (1, 1));
}

#[test]
fn pending_markers_are_exporter_loss() {
    let (mut capture, _) = capture(false);
    assert!(capture.marker(Marker::new(MarkerKind::Event, "pending", 0).unwrap()));
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(trailer.dropped_markers, 1);
    assert!(!trailer.complete());
    assert!(Marker::new(MarkerKind::Event, "é", 0).is_err());
    assert!(Marker::new(MarkerKind::Event, &"x".repeat(65), 0).is_err());
}

#[test]
fn duration_limit_and_terminal_sentinel() {
    let (mut capture, memory) = capture(true);
    assert!(capture.frame_at(1, 0, 0, Phase::Startup));
    assert!(!capture.frame_at(60_000_000_000, 1, 0, Phase::Gameplay));
    assert_eq!(capture.stop_reason(), Some(StopReason::DurationLimit));
    assert!(!capture.marker(Marker::new(MarkerKind::Event, "late", 60_000_000_000).unwrap()));
    let sentinel = Diagnostic::gpu_sentinel(DiagnosticReason::NoSampleObserved, 60_000_000_000, 1).unwrap();
    let Shutdown::Finished(report) = capture.finish_at(60_000_000_000, StopReason::NormalExit, Some(sentinel)) else {
        panic!()
    };
    let trailer = report.trailer.unwrap();
    assert!(trailer.complete());
    assert_eq!((trailer.frames_attempted, trailer.gpu_observations_written), (1, 0));
    let lines = memory.lines();
    assert_eq!(lines[2]["reason"], "no_sample_observed");
    assert_eq!(lines[2]["t_ns"], lines[3]["t_ns"]);
}

#[test]
fn frame_limit_includes_dropped_attempts() {
    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, header(false), 2, MAX_BYTES, 1, 0).unwrap();
    gate.wait();
    assert!(capture.frame_at(1, 0, 0, Phase::Startup));
    assert!(!capture.frame_at(2, 1, 1, Phase::Gameplay));
    assert_eq!(capture.stop_reason(), Some(StopReason::FrameLimit));
    assert!(!capture.frame_at(3, 2, 1, Phase::Gameplay));
    gate.open();
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!((trailer.frames_attempted, trailer.frames_written, trailer.dropped_frames), (2, 1, 1));
}

#[test]
fn lossless_frame_limit_is_complete() {
    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, header(false), 1, MAX_BYTES, 1, 0).unwrap();
    gate.wait();
    assert!(capture.frame_at(1, 0, 0, Phase::Startup));
    gate.open();
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(trailer.stop_reason, StopReason::FrameLimit);
    assert!(trailer.complete());
}

#[test]
fn byte_cut_is_lossless_and_ignores_later_attempts() {
    let h = header(true);
    let header_bytes = h.line().unwrap().len() as u64;
    let frame = Record {
        seq: 1,
        t_ns: 1,
        attempts: AttemptCounts::default(),
        payload: Payload::Frame(Frame {
            main_frame: 0,
            duration_ns: None,
            phase: Phase::Startup,
            markers: vec![],
        }),
    };
    let limit = header_bytes + frame.line().unwrap().len() as u64 + TRAILER_RESERVE;
    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, h, MAX_FRAMES, limit, 2, 1).unwrap();
    gate.wait();
    assert!(capture.frame_at(1, 0, 0, Phase::Startup));
    assert!(capture.frame_at(2, 1, 1, Phase::Gameplay));
    assert!(capture.diagnostic_at(3, diagnostic(1.0)));
    assert!(!capture.diagnostic_at(4, Diagnostic::measurement(&"x".repeat(1_025), "ms", DiagnosticKind::GpuElapsed, 1.0, 4, 0)));
    for i in 0..10 {
        capture.marker(Marker::new(MarkerKind::Event, &i.to_string(), 4).unwrap());
    }
    // The producer cannot observe byte capacity while the writer is held in the header.
    assert_eq!(capture.stop_reason(), None);
    gate.open();
    let report = finish(capture);
    let trailer = report.trailer.unwrap();
    assert!(trailer.complete());
    assert_eq!(trailer.stop_reason, StopReason::ByteLimit);
    assert_eq!(trailer.cut_seq, Some(1));
    assert_eq!(trailer.t_ns, 1);
    assert_eq!(trailer.frames_attempted, 1);
    assert_eq!((trailer.dropped_frames, trailer.dropped_diagnostics, trailer.dropped_markers), (0, 0, 0));
    assert_eq!(trailer.seq, 5);
    assert!(report.bytes_written <= limit);
    assert!(trailer.line().unwrap().len() as u64 <= TRAILER_RESERVE);
    let lines = gate.memory.lines();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[2]["cut_seq"], 1);
    assert_cut_invariants(&lines, &trailer);
}

fn assert_cut_invariants(lines: &[Value], trailer: &Trailer) {
    let cut = trailer.cut_seq.unwrap();
    let interval: Vec<_> = lines.iter().filter(|line| line["seq"].as_u64().unwrap() <= cut).collect();
    assert_eq!(interval.last().unwrap()["seq"], cut);
    let gaps: u64 = interval
        .windows(2)
        .map(|pair| pair[1]["seq"].as_u64().unwrap() - pair[0]["seq"].as_u64().unwrap() - 1)
        .sum();
    assert_eq!(gaps, trailer.dropped_frames + trailer.dropped_diagnostics);
    assert_eq!(trailer.frames_attempted, trailer.frames_written + trailer.dropped_frames);
    assert_eq!(trailer.frames_written, trailer.phases.iter().map(|phase| phase.frames_written).sum::<u64>());
    assert!(trailer.seq > cut);
}

#[test]
fn byte_cut_retains_diagnostic_loss_and_marker_overflow_before_cut() {
    let h = header(true);
    let retained = Record {
        seq: 2,
        t_ns: 2,
        attempts: AttemptCounts::default(),
        payload: Payload::Frame(Frame {
            main_frame: 0,
            duration_ns: None,
            phase: Phase::Startup,
            markers: (0..8).map(|i| Marker::new(MarkerKind::Event, &i.to_string(), 0).unwrap()).collect(),
        }),
    };
    let limit = h.line().unwrap().len() as u64 + retained.line().unwrap().len() as u64 + TRAILER_RESERVE;
    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, h, MAX_FRAMES, limit, 2, 1).unwrap();
    gate.wait();
    assert!(!capture.diagnostic_at(1, Diagnostic::measurement(&"x".repeat(1_025), "ms", DiagnosticKind::GpuElapsed, 1.0, 1, 0)));
    for i in 0..10 {
        assert_eq!(capture.marker(Marker::new(MarkerKind::Event, &i.to_string(), 0).unwrap()), i < 8);
    }
    assert!(capture.frame_at(2, 0, 0, Phase::Startup));
    assert!(capture.frame_at(3, 1, 1, Phase::Gameplay));
    assert!(!capture.frame_at(4, 2, 1, Phase::Gameplay));
    assert!(capture.diagnostic_at(5, diagnostic(1.0)));
    gate.open();
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(trailer.cut_seq, Some(2));
    assert_eq!((trailer.frames_attempted, trailer.frames_written), (1, 1));
    assert_eq!((trailer.dropped_frames, trailer.dropped_diagnostics, trailer.dropped_markers), (0, 1, 2));
    assert!(!trailer.complete());
    assert_cut_invariants(&gate.memory.lines(), &trailer);
}

#[test]
fn byte_cut_retains_frame_and_marker_loss_before_cut() {
    let h = header(true);
    let retained = Record {
        seq: 2,
        t_ns: 2,
        attempts: AttemptCounts::default(),
        payload: Payload::Diagnostic(diagnostic(1.0)),
    };
    let limit = h.line().unwrap().len() as u64 + retained.line().unwrap().len() as u64 + TRAILER_RESERVE;
    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, h, MAX_FRAMES, limit, 0, 2).unwrap();
    gate.wait();
    for i in 0..3 {
        capture.marker(Marker::new(MarkerKind::Event, &i.to_string(), 0).unwrap());
    }
    assert!(!capture.frame_at(1, 0, 0, Phase::Startup));
    // These markers precede the retained diagnostic, but belong to the later frame.
    for i in 0..10 {
        capture.marker(Marker::new(MarkerKind::Event, &i.to_string(), 1).unwrap());
    }
    assert!(capture.diagnostic_at(2, diagnostic(1.0)));
    assert!(capture.diagnostic_at(3, diagnostic(1.0)));
    assert!(!capture.frame_at(4, 1, 0, Phase::Gameplay));
    gate.open();
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(trailer.cut_seq, Some(2));
    assert_eq!((trailer.frames_attempted, trailer.frames_written, trailer.gpu_observations_written), (1, 0, 1));
    assert_eq!((trailer.dropped_frames, trailer.dropped_diagnostics, trailer.dropped_markers), (1, 0, 3));
    assert!(!trailer.complete());
    assert_cut_invariants(&gate.memory.lines(), &trailer);
}

#[test]
fn first_stop_wins_with_duration_then_frame_precedence() {
    let memory = Memory::default();
    let mut capture = test_support::start(memory, header(false), 0, MAX_BYTES, 1, 1).unwrap();
    // Both duration and frame checks would stop here; duration wins.
    assert!(!capture.frame_at(60_000_000_000, 0, 0, Phase::Startup));
    assert_eq!(capture.stop_reason(), Some(StopReason::DurationLimit));
    capture.stop_at(StopReason::SettingsChanged, 61_000_000_000);
    assert_eq!(finish(capture).trailer.unwrap().stop_reason, StopReason::DurationLimit);

    let memory = Memory::default();
    let mut capture = test_support::start(memory, header(false), 1, MAX_BYTES, 1, 1).unwrap();
    assert!(capture.frame_at(1, 0, 0, Phase::Startup));
    capture.stop_at(StopReason::DurationLimit, 60_000_000_000);
    assert_eq!(finish(capture).trailer.unwrap().stop_reason, StopReason::FrameLimit);

    let (capture, _) = self::capture(false);
    capture.stop_at(StopReason::SettingsChanged, 1);
    capture.stop_at(StopReason::NormalExit, 2);
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(trailer.stop_reason, StopReason::SettingsChanged);
    assert!(!trailer.complete());
}

#[test]
fn diagnostics_are_finite_explicit_and_frames_only_is_quiet() {
    let (mut capture, memory) = capture(true);
    for (i, value) in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 2.5].into_iter().enumerate() {
        assert!(capture.diagnostic_at(i as u64, diagnostic(value)));
    }
    assert!(capture.diagnostic_at(
        4,
        Diagnostic::measurement("render/elapsed_cpu", "ns", DiagnosticKind::RenderCpuElapsed, 1.0, 0, 0)
    ));
    assert!(capture.diagnostic_at(5, Diagnostic::measurement("render/wrong_suffix", "ms", DiagnosticKind::GpuElapsed, 1.0, 0, 0)));
    assert!(capture.diagnostic_at(6, Diagnostic::gpu_sentinel(DiagnosticReason::TimestampQueriesUnsupported, 0, 0).unwrap()));
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(trailer.gpu_observations_written, 1);
    assert!(trailer.complete());
    let lines = memory.lines();
    for line in &lines[1..4] {
        assert!(line["value"].is_null());
        assert_eq!(line["status"], "invalid");
        assert_eq!(line["reason"], "non_finite_value");
    }
    assert_eq!(lines[5]["reason"], "unit_mismatch");
    assert_eq!(lines[6]["reason"], "unit_mismatch");
    assert_eq!(lines[7]["reason"], "timestamp_queries_unsupported");
    let (mut capture, memory) = self::capture(false);
    assert!(!capture.diagnostic_at(1, diagnostic(1.0)));
    assert!(finish(capture).trailer.unwrap().complete());
    assert_eq!(memory.lines().len(), 2);
}

#[test]
fn overlong_diagnostic_path_is_counted_as_a_drop() {
    let (mut capture, memory) = capture(true);
    assert!(!capture.diagnostic_at(1, Diagnostic::measurement(&"x".repeat(1_025), "ms", DiagnosticKind::GpuElapsed, 1.0, 1, 0)));
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!(trailer.dropped_diagnostics, 1);
    assert!(!trailer.complete());
    assert_eq!(memory.lines()[1]["seq"], 2);
}

#[test]
fn header_normalizes_metadata_and_unavailable_origins() {
    let metadata = HeaderMetadata {
        build_features: vec!["z".to_owned(), "a".to_owned(), "a".to_owned()],
        adapter: Some("é".repeat(513)),
        driver: Some(String::new()),
        backend: Some("vulkan".to_owned()),
        renderer_available: false,
        ..Default::default()
    };
    let h = Header::new("capture-0", config(true), metadata).unwrap();
    let v: Value = serde_json::from_slice(&h.line().unwrap()).unwrap();
    assert_eq!(v["build_features"], json!(["a", "z"]));
    assert!(v["adapter"].is_null());
    assert_eq!(v["unavailable"]["adapter"], "over_1024_bytes");
    assert_eq!(v["unavailable"]["driver"], "empty");
    assert!(v["unavailable"].get("backend").is_none());
    assert_eq!(
        (v["diagnostics"].as_str(), v["diagnostics_reason"].as_str()),
        (Some("unavailable"), Some("no_renderer"))
    );
    assert_eq!(v["gpu_origin"], "unavailable_from_bevy_0_20_1");
    assert_eq!(v["render_cpu_origin"], "source_frame_unavailable_from_bevy_0_20_1");
    assert!(Header::new("", config(false), HeaderMetadata::default()).is_err());
    assert!(Header::new("bad/id", config(false), HeaderMetadata::default()).is_err());
    assert!(Header::new("valid", parse_aids(None, None, None, None).unwrap(), HeaderMetadata::default()).is_err());
}

#[test]
fn payload_and_line_byte_limits_include_lf_correctly() {
    // The object punctuation and field name occupy eight bytes before its string data.
    let payload = json!({ "s": "x".repeat(MAX_PAYLOAD - 8) });
    assert_eq!(records::encode(&payload, true).unwrap().len(), MAX_PAYLOAD + 1);
    assert!(records::encode(&json!({ "s": "x".repeat(MAX_PAYLOAD - 7) }), true).is_err());
    let line = json!({ "s": "x".repeat(MAX_LINE - 9) });
    assert_eq!(records::encode(&line, false).unwrap().len(), MAX_LINE);
    assert!(records::encode(&json!({ "s": "x".repeat(MAX_LINE - 8) }), false).is_err());
    let metadata = HeaderMetadata {
        build_features: vec!["x".repeat(MAX_LINE)],
        ..Default::default()
    };
    assert!(Header::new("large", config(false), metadata).unwrap().line().is_err());
    let h = header(false);
    let too_small = h.line().unwrap().len() as u64 + TRAILER_RESERVE - 1;
    let capture = test_support::start(Memory::default(), h, MAX_FRAMES, too_small, 1, 1).unwrap();
    let Shutdown::Finished(report) = capture.finish_at(1, StopReason::NormalExit, None) else {
        panic!()
    };
    assert!(report.error.is_some());
    assert!(report.trailer.is_none());
}

#[test]
fn largest_possible_frame_and_diagnostic_fit_payload_limit() {
    // NUL expands from one ASCII byte to six JSON bytes, the maximum escape expansion.
    for duration_ns in [None, Some(u64::MAX)] {
        let record = Record {
            seq: u64::MAX,
            t_ns: u64::MAX,
            attempts: AttemptCounts::default(),
            payload: Payload::Frame(Frame {
                main_frame: u32::MAX,
                duration_ns,
                phase: Phase::Gameplay,
                markers: (0..MARKERS_PER_FRAME)
                    .map(|_| Marker::new(MarkerKind::ScenarioBegin, &"\0".repeat(64), u64::MAX).unwrap())
                    .collect(),
            }),
        };
        assert!(record.payload.within_bounds());
        assert!(record.line().unwrap().len() - 1 < MAX_PAYLOAD);
    }
    for (kind, suffix) in [(DiagnosticKind::RenderCpuElapsed, "elapsed_cpu"), (DiagnosticKind::GpuElapsed, "elapsed_gpu")] {
        let path = format!("{}{suffix}", "\0".repeat(1_024 - suffix.len()));
        let diagnostics = [
            Diagnostic::measurement(&path, "ms", kind, f64::MAX, u64::MAX, u32::MAX),
            Diagnostic::measurement(&path, "ms", kind, -f64::MAX, u64::MAX, u32::MAX),
            Diagnostic::measurement(&path, "ms", kind, f64::NAN, u64::MAX, u32::MAX),
            Diagnostic::measurement(&path, "wrong", kind, 1.0, u64::MAX, u32::MAX),
            Diagnostic::unavailable(&path, kind, DiagnosticReason::TimestampQueriesUnsupported, u64::MAX, u32::MAX).unwrap(),
            Diagnostic::unavailable(&path, kind, DiagnosticReason::NoSampleObserved, u64::MAX, u32::MAX).unwrap(),
        ];
        for diagnostic in diagnostics {
            let record = Record {
                seq: u64::MAX,
                t_ns: u64::MAX,
                payload: Payload::Diagnostic(diagnostic),
                attempts: AttemptCounts::default(),
            };
            assert!(record.payload.within_bounds());
            assert!(record.line().unwrap().len() - 1 < MAX_PAYLOAD);
        }
    }
}

#[test]
fn writer_oversize_backstop_drops_without_stopping() {
    // Bypass producer construction to exercise a future regression in its field bounds.
    let oversized = Record {
        seq: 1,
        t_ns: 1,
        attempts: AttemptCounts {
            frames: 1,
            diagnostics: 0,
            markers: 24,
        },
        payload: Payload::Frame(Frame {
            main_frame: 0,
            duration_ns: None,
            phase: Phase::Startup,
            markers: (0..24).map(|_| Marker::new(MarkerKind::Event, &"\0".repeat(64), 0).unwrap()).collect(),
        }),
    };
    assert!(oversized.line().is_err());
    let next = Record {
        seq: 2,
        t_ns: 2,
        attempts: AttemptCounts {
            frames: 2,
            diagnostics: 0,
            markers: 24,
        },
        payload: Payload::Frame(Frame {
            main_frame: 1,
            duration_ns: Some(0),
            phase: Phase::Gameplay,
            markers: vec![],
        }),
    };
    let memory = Memory::default();
    let report = test_support::write_records(memory.clone(), header(false), vec![oversized, next]);
    assert!(report.error.is_none());
    let trailer = report.trailer.unwrap();
    assert_eq!(trailer.stop_reason, StopReason::NormalExit);
    assert_eq!(trailer.cut_seq, None);
    assert_eq!(
        (
            trailer.frames_attempted,
            trailer.frames_written,
            trailer.dropped_frames,
            trailer.dropped_markers
        ),
        (2, 1, 1, 24)
    );
    assert_eq!(trailer.phases[2].samples, 1);
    assert!(!trailer.complete());
    assert_eq!(memory.lines()[1]["seq"], 2);
}

#[test]
fn header_snapshot_values_and_all_display_sync_modes() {
    for (sync, text) in [(DisplaySync::Off, "off"), (DisplaySync::Vsync, "vsync"), (DisplaySync::Variable, "variable")] {
        let metadata = HeaderMetadata {
            started_at_utc: Some("2026-11-01T12:00:00Z".to_owned()),
            engine_revision: Some("abcdef".to_owned()),
            adapter: Some("x".repeat(1_024)),
            backend: Some("Vulkan".to_owned()),
            driver: Some("driver".to_owned()),
            driver_info: Some("info".to_owned()),
            width_px: Some(1280),
            height_px: Some(720),
            display_sync: Some(sync),
            cap_fps: Some(0),
            refresh_millihz: Some(60_000),
            window_present_mode: Some("AutoVsync".to_owned()),
            renderer_available: true,
            ..Default::default()
        };
        let h = Header::new("snapshot", config(false), metadata).unwrap();
        let v: Value = serde_json::from_slice(&h.line().unwrap()).unwrap();
        assert_eq!(v["display_sync"], text);
        assert_eq!(v["adapter"].as_str().unwrap().len(), 1_024);
        assert_eq!(v["unavailable"], json!({}));
        assert_eq!(v["cap_fps"], 0);
        assert_eq!(v["width_px"], 1280);
        assert_eq!(v["height_px"], 720);
        assert_eq!(v["diagnostics"], "disabled");
        assert_eq!(v["diagnostics_reason"], "not_requested");
    }
}

#[test]
fn settings_changed_stops_before_next_payload() {
    let (mut capture, memory) = capture(false);
    assert!(capture.frame_at(1, 0, 0, Phase::Gameplay));
    capture.stop_at(StopReason::SettingsChanged, 2);
    assert!(!capture.frame_at(3, 1, 1, Phase::Gameplay));
    let trailer = finish(capture).trailer.unwrap();
    assert_eq!((trailer.frames_attempted, trailer.frames_written), (1, 1));
    assert_eq!(trailer.stop_reason, StopReason::SettingsChanged);
    assert!(!trailer.complete());
    assert_eq!(memory.lines().len(), 3);
}

#[test]
fn sentinel_obeys_byte_budget_without_consuming_frame_capacity() {
    let h = header(true);
    let limit = h.line().unwrap().len() as u64 + TRAILER_RESERVE;
    let memory = Memory::default();
    let capture = test_support::start(memory.clone(), h, MAX_FRAMES, limit, 1, 1).unwrap();
    let sentinel = Diagnostic::gpu_sentinel(DiagnosticReason::NoSampleObserved, 1, 0).unwrap();
    let Shutdown::Finished(report) = capture.finish_at(1, StopReason::NormalExit, Some(sentinel)) else {
        panic!()
    };
    let trailer = report.trailer.unwrap();
    assert!(trailer.complete());
    assert_eq!(trailer.stop_reason, StopReason::ByteLimit);
    assert_eq!(trailer.cut_seq, Some(0));
    assert_eq!(trailer.seq, 2);
    assert_eq!(trailer.t_ns, 0);
    assert!(report.bytes_written <= limit);
    assert_eq!(memory.lines().len(), 2);
    assert_cut_invariants(&memory.lines(), &trailer);
}

fn exact_fields(value: &Value, expected: &[&str]) {
    let mut actual: Vec<_> = value.as_object().unwrap().keys().map(String::as_str).collect();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn json_lines_have_exact_schema_fields_and_types() {
    let (mut capture, memory) = capture(true);
    capture.marker(Marker::new(MarkerKind::PhaseEnter, "startup", 0).unwrap());
    capture.frame_at(1, 0, 0, Phase::Startup);
    capture.diagnostic_at(2, diagnostic(1.25));
    finish(capture);
    let lines = memory.lines();
    assert_eq!(lines.len(), 4);
    exact_fields(
        &lines[0],
        &[
            "type",
            "seq",
            "t_ns",
            "schema_version",
            "capture_id",
            "run_id",
            "scenario_id",
            "started_at_utc",
            "engine_revision",
            "build_features",
            "adapter",
            "backend",
            "driver",
            "driver_info",
            "width_px",
            "height_px",
            "display_sync",
            "cap_fps",
            "refresh_millihz",
            "window_present_mode",
            "unavailable",
            "max_duration_ns",
            "max_frames",
            "max_bytes",
            "diagnostics",
            "diagnostics_reason",
            "gpu_origin",
            "render_cpu_origin",
        ],
    );
    exact_fields(
        &lines[1],
        &["type", "seq", "t_ns", "main_frame", "duration_ns", "duration_reason", "phase", "markers"],
    );
    exact_fields(&lines[1]["markers"][0], &["kind", "id", "t_ns"]);
    exact_fields(
        &lines[2],
        &[
            "type",
            "seq",
            "t_ns",
            "path",
            "unit",
            "kind",
            "value",
            "status",
            "reason",
            "observation_t_ns",
            "observation_main_frame",
        ],
    );
    exact_fields(
        &lines[3],
        &[
            "type",
            "seq",
            "t_ns",
            "status",
            "stop_reason",
            "cut_seq",
            "frames_attempted",
            "frames_written",
            "dropped_frames",
            "dropped_diagnostics",
            "dropped_markers",
            "gpu_observations_written",
            "phases",
        ],
    );
    exact_fields(&lines[3]["phases"], &["startup", "loading", "gameplay"]);
    for phase in ["startup", "loading", "gameplay"] {
        let s = &lines[3]["phases"][phase];
        exact_fields(
            s,
            &[
                "frames_written",
                "samples",
                "above_33_ms",
                "above_50_ms",
                "above_100_ms",
                "above_250_ms",
                "mean_ms",
                "worst_ms",
                "p50_ms",
                "p95_ms",
                "p99_ms",
                "slowest_1pct_mean_ms",
            ],
        );
        for field in ["frames_written", "samples", "above_33_ms", "above_50_ms", "above_100_ms", "above_250_ms"] {
            assert!(s[field].is_u64());
        }
        for field in ["mean_ms", "worst_ms", "p50_ms", "p95_ms", "p99_ms", "slowest_1pct_mean_ms"] {
            assert!(s[field].is_null() || s[field].as_f64().unwrap().is_finite());
        }
    }
    assert_eq!(lines[0]["max_frames"], MAX_FRAMES);
    assert_eq!(lines[0]["max_bytes"], MAX_BYTES);
    assert_eq!(lines[2]["value"], 1.25);
    assert!(lines[2]["reason"].is_null());
    assert!(lines[3]["cut_seq"].is_null());
    for line in &lines {
        assert!(line["seq"].is_u64());
        assert!(line["t_ns"].is_u64());
    }
}

#[test]
fn u64_values_stay_exact_and_huge_durations_have_finite_statistics() {
    let record = Record {
        seq: u64::MAX,
        t_ns: u64::MAX,
        attempts: AttemptCounts::default(),
        payload: Payload::Frame(Frame {
            main_frame: u32::MAX,
            duration_ns: Some(u64::MAX),
            phase: Phase::Gameplay,
            markers: vec![],
        }),
    };
    let v: Value = serde_json::from_slice(&record.line().unwrap()).unwrap();
    for key in ["seq", "t_ns", "duration_ns"] {
        assert_eq!(v[key].as_u64(), Some(u64::MAX));
    }
    let mut s = PhaseStats::default();
    s.record(Some(u64::MAX));
    s.record(Some(u64::MAX));
    assert!(s.summary().mean_ms.unwrap().is_finite());
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static UNIQUE: AtomicU64 = AtomicU64::new(0);
        let epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nvf-frame-telemetry-{}-{epoch}-{}",
            std::process::id(),
            UNIQUE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn create_new_never_overwrites_and_missing_directory_fails() {
    let temp = TempDir::new();
    let path = temp.0.join("capture.jsonl");
    fs::write(&path, b"keep").unwrap();
    let capture = Capture::create(&path, header(false), Instant::now()).unwrap();
    let Shutdown::Finished(report) = capture.finish_at(1, StopReason::NormalExit, None) else {
        panic!()
    };
    assert_eq!(report.stop_reason, StopReason::WriterError);
    assert!(report.error.is_some());
    assert!(report.trailer.is_none());
    assert_eq!(fs::read(&path).unwrap(), b"keep");
    let capture = Capture::create(&temp.0.join("missing/capture.jsonl"), header(false), Instant::now()).unwrap();
    let Shutdown::Finished(report) = capture.finish_at(1, StopReason::NormalExit, None) else {
        panic!()
    };
    assert_eq!(report.stop_reason, StopReason::WriterError);
    assert!(report.error.is_some());
    assert!(report.trailer.is_none());
}

#[test]
fn file_capture_flushes_and_drop_finishes_normally() {
    let temp = TempDir::new();
    let path = temp.0.join("capture.jsonl");
    let mut capture = Capture::create(&path, header(false), Instant::now()).unwrap();
    capture.frame_at(0, 0, 0, Phase::Startup);
    drop(capture);
    let bytes = fs::read(path).unwrap();
    let lines: Vec<Value> = bytes
        .split_inclusive(|&b| b == b'\n')
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(lines.last().unwrap()["status"], "complete");
    assert_eq!(lines.last().unwrap()["stop_reason"], "normal_exit");
}

struct FailingSink {
    memory: Memory,
    bytes_before_failure: usize,
    failed: bool,
    permanent: bool,
    flush_failure: bool,
}

impl Write for FailingSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes_before_failure == 0 && (!self.failed || self.permanent) {
            self.failed = true;
            return Err(io::Error::other("synthetic write failure"));
        }
        let n = if self.failed {
            bytes.len()
        } else {
            bytes.len().min(self.bytes_before_failure)
        };
        self.bytes_before_failure = self.bytes_before_failure.saturating_sub(n);
        self.memory.write(&bytes[..n])
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.flush_failure || (self.permanent && self.failed) {
            Err(io::Error::other("synthetic flush failure"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn writer_error_overrides_stop_and_partial_frames_are_not_samples() {
    let h = header(false);
    let memory = Memory::default();
    let sink = FailingSink {
        memory: memory.clone(),
        bytes_before_failure: h.line().unwrap().len() + 5,
        failed: false,
        permanent: false,
        flush_failure: false,
    };
    let mut capture = test_support::start(sink, h, 1, MAX_BYTES, 1, 0).unwrap();
    assert!(capture.frame_at(1, 0, 0, Phase::Gameplay));
    let Shutdown::Finished(report) = capture.finish_at(2, StopReason::NormalExit, None) else {
        panic!()
    };
    assert_eq!(report.error.as_deref(), Some("synthetic write failure"));
    let trailer = report.trailer.unwrap();
    assert_eq!(trailer.stop_reason, StopReason::WriterError);
    assert_eq!((trailer.frames_written, trailer.dropped_frames, trailer.phases[2].samples), (0, 1, 0));
    assert!(!trailer.complete());
    let bytes = memory.0.lock().unwrap();
    let lines: Vec<_> = bytes.split_inclusive(|&b| b == b'\n').collect();
    assert!(serde_json::from_slice::<Value>(lines[1]).is_err());
    assert_eq!(serde_json::from_slice::<Value>(lines[2]).unwrap()["stop_reason"], "writer_error");
}

#[test]
fn permanent_write_and_flush_failures_do_not_claim_a_trailer() {
    for (permanent, flush_failure, bytes_before_failure) in [(true, false, 0), (false, true, usize::MAX)] {
        let sink = FailingSink {
            memory: Memory::default(),
            bytes_before_failure,
            failed: false,
            permanent,
            flush_failure,
        };
        let capture = test_support::start(sink, header(false), MAX_FRAMES, MAX_BYTES, 1, 1).unwrap();
        let Shutdown::Finished(report) = capture.finish_at(1, StopReason::NormalExit, None) else {
            panic!()
        };
        assert!(report.error.is_some());
        assert!(report.trailer.is_none());
    }
}

struct FlushOnce {
    memory: Memory,
    failed: bool,
}

impl Write for FlushOnce {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.memory.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            return Ok(());
        }
        self.failed = true;
        Err(io::Error::other("synthetic first flush failure"))
    }
}

#[test]
fn recoverable_payload_flush_failure_writes_truncated_trailer() {
    let memory = Memory::default();
    let sink = FlushOnce {
        memory: memory.clone(),
        failed: false,
    };
    let capture = test_support::start(sink, header(false), MAX_FRAMES, MAX_BYTES, 1, 1).unwrap();
    let Shutdown::Finished(report) = capture.finish_at(1, StopReason::NormalExit, None) else {
        panic!()
    };
    assert_eq!(report.error.as_deref(), Some("synthetic first flush failure"));
    let trailer = report.trailer.unwrap();
    assert_eq!(trailer.stop_reason, StopReason::WriterError);
    assert!(!trailer.complete());
    assert_eq!(memory.lines().last().unwrap()["status"], "truncated");
}

#[test]
fn shutdown_timeout_freezes_and_late_writer_leaves_no_trailer() {
    let (mut gate, sink) = Gate::new();
    let mut capture = test_support::start(sink, header(false), MAX_FRAMES, MAX_BYTES, 1, 1).unwrap();
    gate.wait();
    assert!(capture.frame_at(1, 0, 0, Phase::Startup));
    assert!(matches!(test_support::shutdown(&mut capture, Duration::from_millis(1)), Shutdown::TimedOut));
    assert!(!capture.frame_at(2, 1, 0, Phase::Gameplay));
    gate.open();
    gate.done.recv_timeout(Duration::from_secs(2)).unwrap();
    let lines = gate.memory.lines();
    assert!(lines.iter().all(|line| line["type"] != "trailer"));
    let report = test_support::late_report(&mut capture);
    assert_eq!(report.stop_reason, StopReason::ShutdownTimeout);
    assert!(report.trailer.is_none());
}

fn held_trailer_agrees_with_report(hold_at: HoldAt, fail_final_flush: bool) {
    let (mut gate, mut sink) = Gate::at(hold_at);
    sink.fail_final_flush = fail_final_flush;
    let mut capture = test_support::start(sink, header(false), MAX_FRAMES, MAX_BYTES, 1, 1).unwrap();
    assert!(capture.frame_at(1, 0, 0, Phase::Startup));
    capture.begin_finish_at(2, StopReason::NormalExit, None);
    gate.wait();
    // Advance the poll clock beyond both graces while the claimed trailer/flush is
    // held. This reproduces the late-write interleaving without a wall-clock sleep.
    assert!(matches!(test_support::poll_past_graces(&mut capture), Some(Shutdown::TrailerInProgress)));
    gate.open();
    let report = test_support::late_report(&mut capture);
    let trailer = report.trailer.unwrap();
    assert_eq!(report.stop_reason, trailer.stop_reason);
    assert_eq!(report.flush_confirmed, !fail_final_flush);
    assert_eq!(report.error.is_some(), fail_final_flush);
    let expected: Value = serde_json::from_slice(&trailer.line().unwrap()).unwrap();
    assert_eq!(gate.memory.lines().last().unwrap(), &expected);
    assert!(trailer.complete());
}

#[test]
fn trailer_write_past_grace_keeps_file_and_report_consistent() {
    held_trailer_agrees_with_report(HoldAt::Trailer, false);
}

#[test]
fn final_flush_past_grace_keeps_file_and_report_consistent() {
    held_trailer_agrees_with_report(HoldAt::FinalFlush, false);
}

#[test]
fn final_flush_failure_preserves_the_serialized_trailer() {
    held_trailer_agrees_with_report(HoldAt::FinalFlush, true);
}

#[test]
fn header_encoding_failure_is_reported_by_writer() {
    let temp = TempDir::new();
    let metadata = HeaderMetadata {
        build_features: vec!["x".repeat(MAX_LINE)],
        ..Default::default()
    };
    let header = Header::new("oversized", config(false), metadata).unwrap();
    let capture = Capture::create(&temp.0.join("oversized.jsonl"), header, Instant::now()).unwrap();
    let Shutdown::Finished(report) = capture.finish_at(1, StopReason::NormalExit, None) else {
        panic!()
    };
    assert!(report.error.is_some());
    assert_eq!(report.stop_reason, StopReason::WriterError);
    assert_eq!(report.bytes_written, 0);
    assert!(report.trailer.is_none());
}

#[test]
fn file_opener_runs_on_writer_without_waiting_on_producer() {
    let memory = Memory::default();
    let output = memory.clone();
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let producer = std::thread::current().id();
    let mut capture = test_support::start_with_open(
        move || {
            entered_tx.send(std::thread::current().id()).unwrap();
            release_rx.recv().unwrap();
            Ok(output)
        },
        header(false),
    )
    .unwrap();
    assert_ne!(entered.recv_timeout(Duration::from_secs(2)).unwrap(), producer);
    assert!(capture.frame_at(1, 0, 0, Phase::Startup));
    capture.begin_finish_at(2, StopReason::NormalExit, None);
    assert!(capture.poll_finish().is_none());
    release.send(()).unwrap();
    assert!(finish(capture).trailer.unwrap().complete());
    assert_eq!(memory.lines().len(), 3);
}
