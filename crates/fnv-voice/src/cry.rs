//! One cry of a generic death file (issue #1539; docs/spec-notes/voice.md §9.1).
//!
//! Some voice types' generic death files (quest `Generic`, topic `Death`) hold several
//! cries back to back: MaleAdult07's `generic_death_00175182_1.ogg` lasts 15.23 s and
//! `…00175181_1.ogg` 9.80 s, where the other voice types' files of the same lines last
//! 0.68 to 3.43 s [observed: `fnvtool voice line … --info 00175181/00175182`; playtest log
//! 2026-11-05 in the issue]. Played whole, a corpse groans on for ten seconds or more.
//!
//! The rules [design, #1539: owner "pick a random start point in the file for generic
//! deaths, so a different cry is heard each time, and cut the sound after one cry"]:
//! - only a generic death file ([`is_generic_death`]) longer than [`LONG_SECONDS`] is
//!   cut; a shorter one is one cry (with its breath) and plays whole, as does every
//!   other line (a named NPC's death line lives in its own quest's files);
//! - the file is split at its silences ([`cries`]): runs of 10 ms windows whose level
//!   stays below [`SILENCE_RATIO`] of the loudest window for at least [`GAP_SECONDS`];
//! - one of the cries is picked at random and played alone ([`choose`]), from just
//!   before it starts to just after it ends, faded out over [`FADE_SECONDS`];
//! - a picked cry still longer than [`LONG_SECONDS`] (the split found no silence in it)
//!   plays [`FALLBACK_SECONDS`] of it from a random point instead, faded in and out.
//!
//! Headless: the samples are decoded by the engine (`nvf_play::voice`).

use crate::path::parse_voice_path;

/// A generic death file at most this long plays whole [design, #1539: the one-cry files
/// of FalloutNV.esm's three Generic Death lines last 0.68 to 4.08 s (FemaleGhoul02's
/// 00175180), the long ones 9.80 and 15.23 s (MaleAdult07's 00175181 and 00175182),
/// observed with `fnvtool voice line`].
pub const LONG_SECONDS: f64 = 5.0;
/// Length of a level window.
pub const WINDOW_SECONDS: f64 = 0.010;
/// A window quieter than this share of the loudest window's level (RMS) is silence
/// [design: about -26 dB].
pub const SILENCE_RATIO: f32 = 0.05;
/// Below this RMS a window is silence whatever the loudest is [design].
pub const SILENCE_FLOOR: f32 = 0.002;
/// Silence at least this long separates two cries; a shorter one is a breath inside a
/// cry [design].
pub const GAP_SECONDS: f64 = 0.25;
/// A loud run shorter than this is a click, not a cry [design].
pub const MIN_CRY_SECONDS: f64 = 0.08;
/// A cry is played from this long before its first loud window …
pub const LEAD_SECONDS: f64 = 0.03;
/// … to this long after its last (its decay), within the silences around it [design].
pub const TAIL_SECONDS: f64 = 0.15;
/// The cut sound's fade-out (and the fallback's fade-in) [design].
pub const FADE_SECONDS: f64 = 0.04;
/// A long file without silences plays this long from a random point [design].
pub const FALLBACK_SECONDS: f64 = 1.5;

/// Whether `path` (a normalised voice path, `sound\voice\<plugin>\<voice type>\…`) is a
/// line of the Generic quest's Death topic: its file name starts `generic_death_`
/// (`<quest>_<topic>_<FormID>_<n>`, voice.md §2.2). DLC quests named `…Generic` count
/// too [design].
pub fn is_generic_death(path: &str) -> bool {
    let Some(f) = parse_voice_path(path) else { return false };
    let Some((quest_topic, ..)) = f.parsed else { return false };
    let qt = quest_topic.to_ascii_lowercase();
    qt.strip_suffix("_death").is_some_and(|q| q.ends_with("generic"))
}

/// A stretch of the file, in seconds from its start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub start: f64,
    pub end: f64,
}

impl Span {
    pub fn seconds(&self) -> f64 {
        self.end - self.start
    }
}

/// What of a death file to play.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cut {
    /// The part played.
    pub span: Span,
    /// Which cry (from 0) of how many; `None` for the fallback (no silences found).
    pub cry: Option<(usize, usize)>,
    /// Fade-in at the start (the fallback starts inside a sound).
    pub fade_in: f64,
    pub fade_out: f64,
}

/// The cries of interleaved `samples` (`channels` per frame, `rate` frames per second),
/// in order, each padded by [`LEAD_SECONDS`] and [`TAIL_SECONDS`] without passing the
/// middle of the silence next to it. Empty for silence.
pub fn cries(samples: &[f32], channels: u16, rate: u32) -> Vec<Span> {
    let c = channels.max(1) as usize;
    let rate = rate.max(1) as f64;
    let frames = samples.len() / c;
    let win = ((WINDOW_SECONDS * rate).round() as usize).max(1);
    let levels: Vec<f32> = (0..frames.div_ceil(win))
        .map(|w| {
            let a = w * win * c;
            let b = ((w + 1) * win * c).min(frames * c);
            let s = &samples[a..b];
            let sum: f64 = s.iter().map(|&x| (x as f64) * (x as f64)).sum();
            (sum / s.len().max(1) as f64).sqrt() as f32
        })
        .collect();
    let peak = levels.iter().copied().fold(0.0f32, f32::max);
    let threshold = (peak * SILENCE_RATIO).max(SILENCE_FLOOR);
    let gap = (GAP_SECONDS / WINDOW_SECONDS).ceil() as usize;
    // Loud runs, merged across silences shorter than a gap: (first, last) window.
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, &l) in levels.iter().enumerate() {
        if l < threshold {
            continue;
        }
        match runs.last_mut() {
            Some((_, last)) if i - *last <= gap => *last = i,
            _ => runs.push((i, i)),
        }
    }
    let min = (MIN_CRY_SECONDS / WINDOW_SECONDS).ceil() as usize;
    runs.retain(|&(a, b)| b + 1 - a >= min);
    let total = frames as f64 / rate;
    let w = win as f64 / rate;
    let spans: Vec<(f64, f64)> = runs.iter().map(|&(a, b)| (a as f64 * w, ((b + 1) as f64 * w).min(total))).collect();
    (0..spans.len())
        .map(|k| {
            let (s, e) = spans[k];
            let lo = if k == 0 { 0.0 } else { (spans[k - 1].1 + s) / 2.0 };
            let hi = spans.get(k + 1).map_or(total, |n| (e + n.0) / 2.0);
            Span { start: (s - LEAD_SECONDS).max(lo), end: (e + TAIL_SECONDS).min(hi) }
        })
        .collect()
}

/// What to play of a generic death file `total` seconds long whose cries are `found`
/// ([`cries`]), `roll` a random number in [0, 1). `None`: play it whole (short, or
/// silent).
pub fn choose(found: &[Span], total: f64, roll: f64) -> Option<Cut> {
    if total <= LONG_SECONDS {
        return None;
    }
    let roll = roll.clamp(0.0, 0.999_999);
    let n = found.len();
    if n == 0 {
        return None;
    }
    let k = ((roll * n as f64) as usize).min(n - 1);
    let cry = found[k];
    if cry.seconds() <= LONG_SECONDS {
        return Some(Cut { span: cry, cry: Some((k, n)), fade_in: 0.0, fade_out: FADE_SECONDS });
    }
    // No silence inside: a stretch of it from a random point (the roll's share within
    // the cry it picked).
    let within = roll * n as f64 - k as f64;
    let start = cry.start + within * (cry.seconds() - FALLBACK_SECONDS).max(0.0);
    Some(Cut { span: Span { start, end: (start + FALLBACK_SECONDS).min(cry.end) }, cry: None, fade_in: FADE_SECONDS, fade_out: FADE_SECONDS })
}

/// The samples of `cut` out of interleaved `samples` (`channels` per frame, `rate` frames
/// per second), whole frames, with its fades (linear gain ramps).
pub fn cut_samples(samples: &[f32], channels: u16, rate: u32, cut: &Cut) -> Vec<f32> {
    let c = channels.max(1) as usize;
    let rate = rate.max(1) as f64;
    let frames = samples.len() / c;
    let frame = |t: f64| ((t.max(0.0) * rate).round() as usize).min(frames);
    let (a, b) = (frame(cut.span.start), frame(cut.span.end));
    let n = b.saturating_sub(a);
    let fade_in = (cut.fade_in * rate) as usize;
    let fade_out = (cut.fade_out * rate) as usize;
    let mut out = samples[a * c..a.max(b) * c].to_vec();
    for (i, f) in out.chunks_mut(c).enumerate() {
        let mut g = 1.0f32;
        if i < fade_in {
            g = g.min(i as f32 / fade_in as f32);
        }
        if fade_out > 0 && n - i <= fade_out {
            g = g.min((n - i - 1) as f32 / fade_out as f32);
        }
        if g < 1.0 {
            f.iter_mut().for_each(|x| *x *= g);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_is_whole_frames_with_its_fades() {
        // Stereo at 100 frames per second: 3 s of ones.
        let s = vec![1.0f32; 600];
        let cut = Cut { span: Span { start: 1.0, end: 2.0 }, cry: None, fade_in: 0.1, fade_out: 0.2 };
        let out = cut_samples(&s, 2, 100, &cut);
        assert_eq!(out.len(), 200, "100 frames of 2 samples");
        assert_eq!((out[0], out[1]), (0.0, 0.0), "faded in from nothing");
        assert_eq!(out[2 * 50], 1.0, "whole in the middle");
        assert_eq!(out[199], 0.0, "faded out to nothing");
        assert!(out[2 * 85] > 0.0 && out[2 * 85] < 1.0);
        // A cut without fades is a plain copy; one past the end stops at the end.
        let plain = Cut { span: Span { start: 2.5, end: 9.0 }, cry: None, fade_in: 0.0, fade_out: 0.0 };
        assert_eq!(cut_samples(&s, 2, 100, &plain), vec![1.0; 100]);
    }

    const RATE: u32 = 24_000;

    /// Mono samples: `parts` of (seconds, loud) one after another; a loud part is a
    /// 200 Hz tone at 0.5, a quiet one faint noise at 0.001.
    fn pcm(parts: &[(f64, bool)]) -> Vec<f32> {
        let mut out = Vec::new();
        for &(s, loud) in parts {
            let n = (s * RATE as f64) as usize;
            for i in 0..n {
                let t = i as f32 / RATE as f32;
                out.push(if loud { 0.5 * (t * 200.0 * std::f32::consts::TAU).sin() } else { 0.001 * ((i % 7) as f32 / 3.5 - 1.0) });
            }
        }
        out
    }

    #[test]
    fn generic_death_files_are_told_by_name() {
        assert!(is_generic_death(r"sound\voice\falloutnv.esm\maleadult07\generic_death_00175182_1.ogg"));
        assert!(is_generic_death(r"sound\voice\falloutnv.esm\maleadult07\GENERIC_DEATH_00175181_1.ogg"));
        assert!(is_generic_death(r"sound\voice\deadmoney.esm\maleadult07\nvdlc01generic_death_01000aa0_1.ogg"));
        // A quest's own Death line (a named NPC), and other generic topics: not cut.
        assert!(!is_generic_death(r"sound\voice\falloutnv.esm\bennyvoice\vmsbennydialogue_death_00112233_1.ogg"));
        assert!(!is_generic_death(r"sound\voice\falloutnv.esm\maleadult07\generic_deathresponse_00175190_1.ogg"));
        assert!(!is_generic_death(r"sound\voice\falloutnv.esm\maleadult07\generic_hit_00175170_1.ogg"));
        assert!(!is_generic_death("not a voice path"));
    }

    #[test]
    fn cries_are_split_at_silences() {
        // 0.5 s quiet, 1 s cry, 1 s quiet, 0.8 s cry with a 0.1 s breath, 2 s quiet,
        // 1.2 s cry, 0.4 s quiet.
        let s = pcm(&[(0.5, false), (1.0, true), (1.0, false), (0.4, true), (0.1, false), (0.4, true), (2.0, false), (1.2, true), (0.4, false)]);
        let c = cries(&s, 1, RATE);
        assert_eq!(c.len(), 3, "{c:?}");
        let near = |a: f64, b: f64| (a - b).abs() < 0.02;
        assert!(near(c[0].start, 0.5 - LEAD_SECONDS) && near(c[0].end, 1.5 + TAIL_SECONDS), "{:?}", c[0]);
        assert!(near(c[1].start, 2.5 - LEAD_SECONDS) && near(c[1].end, 3.4 + TAIL_SECONDS), "the breath stays inside: {:?}", c[1]);
        assert!(near(c[2].start, 5.4 - LEAD_SECONDS) && near(c[2].end, 6.6 + TAIL_SECONDS), "{:?}", c[2]);
        // Each roll picks one cry, and every cry can be picked.
        let total = s.len() as f64 / RATE as f64;
        let picked: Vec<usize> = [0.0, 0.4, 0.9].iter().map(|&r| choose(&c, total, r).and_then(|x| x.cry).unwrap().0).collect();
        assert_eq!(picked, vec![0, 1, 2]);
        let cut = choose(&c, total, 0.5).unwrap();
        assert_eq!(cut.span, c[1]);
        assert_eq!((cut.fade_in, cut.fade_out), (0.0, FADE_SECONDS));
    }

    #[test]
    fn stereo_and_clicks() {
        let mono = pcm(&[(0.3, false), (0.02, true), (1.0, false), (0.6, true), (0.5, false), (0.6, true), (2.5, false)]);
        let stereo: Vec<f32> = mono.iter().flat_map(|&x| [x, x]).collect();
        let c = cries(&stereo, 2, RATE);
        assert_eq!(c.len(), 2, "a 20 ms click is no cry: {c:?}");
    }

    #[test]
    fn short_files_play_whole() {
        // One cry with its breath, 1.5 s: whole.
        let s = pcm(&[(0.1, false), (0.6, true), (0.3, false), (0.4, true), (0.1, false)]);
        let c = cries(&s, 1, RATE);
        assert_eq!(choose(&c, s.len() as f64 / RATE as f64, 0.7), None);
        // Two cries, but within LONG_SECONDS: whole as well.
        let s = pcm(&[(0.8, true), (1.0, false), (0.8, true)]);
        let c = cries(&s, 1, RATE);
        assert_eq!(c.len(), 2);
        assert_eq!(choose(&c, s.len() as f64 / RATE as f64, 0.7), None);
        // Silence: whole (nothing to cut).
        assert_eq!(choose(&[], 10.0, 0.3), None);
    }

    #[test]
    fn a_long_file_without_silences_plays_a_stretch() {
        let s = pcm(&[(6.0, true)]);
        let c = cries(&s, 1, RATE);
        assert_eq!(c.len(), 1);
        let cut = choose(&c, 6.0, 0.5).unwrap();
        assert_eq!(cut.cry, None);
        assert!((cut.span.start - 2.25).abs() < 1e-9 && (cut.span.seconds() - FALLBACK_SECONDS).abs() < 1e-9, "{cut:?}");
        assert_eq!(cut.fade_in, FADE_SECONDS);
        // A long file whose one cry is short (a cry then a long silence): that cry.
        let s = pcm(&[(1.0, true), (5.0, false)]);
        let c = cries(&s, 1, RATE);
        let cut = choose(&c, 6.0, 0.5).unwrap();
        assert_eq!((cut.span, cut.cry), (c[0], Some((0, 1))));
    }
}
