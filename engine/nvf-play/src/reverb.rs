//! Reverb: the acoustic space of the cell the listener is in names a reverb environment,
//! and the world's sound effects are played through it (docs/spec-notes/audio.md §14.6).
//!
//! - **The environment** [documented: xEdit ASPC `ANAM` "Environment Type", GECK wiki
//!   "Acoustic Space": a drop-down of DirectSound reverb presets]: its 30 names are the
//!   I3DL2 environment presets of DirectSound (`DSFX_I3DL2_ENVIRONMENT_PRESET_*`), whose
//!   parameters [`PRESETS`] copies [documented: Windows SDK 10.0.26100.0 `um/dsound.h`,
//!   `I3DL2_ENVIRONMENT_PRESET_*` and `DSFXI3DL2Reverb`]. `None` is no reverb.
//! - **Which space** [hypothesis]: the cell's `XCAS` (an interior's own; outside, the
//!   exterior cell under the listener), as the background loops (`crate::ambient`) take
//!   it. Outside, only the open-air presets ([`OUTDOOR`]: Mountains, City, Plain …) are
//!   played; a room's preset on an exterior space plays dry [hypothesis: 12 of the 14
//!   exterior spaces of FalloutNV.esm, the open desert, the lake and the airport among
//!   them, store Medium Room, which reads as a value left unset; the Strip and the slums
//!   store Mountains (observed, `fnvtool world audio`)]. Placed acoustic-space boxes are not read (the GECK wiki notes they do not work
//!   in New Vegas). [`RoomReverb`] follows it and logs each change (`reverb: …`).
//! - **Which sounds** [hypothesis]: the sound effects of [`crate::sfx`] (one-shots at a
//!   place or at the listener: shots, impacts, footsteps, doors, creatures, random
//!   markers) and the placed loops of [`crate::ambient`]; not the records flagged
//!   Environment Ignored or Menu Sound, not interface sounds (`UI…`), not the
//!   background loops (the space's own room tone), music or radios; not the
//!   loops [`crate::sfx::LoopSound`] starts (they repeat their file, and each repeat
//!   would cut the tail). A sound takes the environment of the moment it starts.
//! - **Voices** ([`for_voice`], played by [`crate::voice`]): a line said at a place (a
//!   SayTo or Say line, a hello, a bark, a line between two actors) goes into the
//!   listener's environment after the voice filter, at [`VOICE_SEND`], and its reverb
//!   stops with the line: "during dialogue the reverberation if any is abruptly cut off
//!   at the end of each spoken line" [documented: GECK wiki "Special Effect" notes,
//!   owner export], so no tail is played after the file ([`RoomSend::cut_at_end`]).
//!   The conversation with the player and 2D lines stay dry [design: they are heard
//!   whole and unplaced, as voice.md has them]. The Sound tab's Voice reverb switch
//!   (`voice-reverb`) keeps every voice dry; the Reverb switch keeps out voices too.
//! - **How much**: a sound sends into the reverb as much as its SOUN's Reverb
//!   Attenuation Control lets through, 100 nothing and 0 all of it [documented: GECK
//!   wiki Sound, Attenuation tab]; in between that share of the amplitude ([`send_of`],
//!   [hypothesis: linear]). 1,687 of FalloutNV.esm's 3,190 sounds store 80, 1,032 store
//!   0 and 107 store 100 [observed]. The dry sound is not changed.
//! - **Per sound** [hypothesis: DirectSound's I3DL2 reverb is an effect on each sound
//!   buffer, so the reverb of a sound falls off with its distance as the sound does]:
//!   each such sound runs through its own [`Reverb`] and is played in stereo, and goes
//!   on for its tail (the preset's delays and decay time, at most [`MAX_TAIL`]) after
//!   its file ends.
//! - **The processor** [hypothesis: how DirectSound renders the parameters is not
//!   documented; this is a Schroeder/Freeverb network driven by them]: the input is
//!   mixed to one channel. Room (mB) is the level of everything reflected, Room HF (mB)
//!   its loss at the HF reference (a one-pole low-pass); the early reflections are one
//!   tap at Reflections Delay at Room + Reflections (mB); the late reverb starts Reverb
//!   Delay later at Room + Reverb (mB) and is eight parallel feedback combs per ear
//!   ([`COMBS`], spread [`SPREAD`] for the right ear) into four series all-passes
//!   ([`ALLPASSES`]). Each comb's feedback makes it fall 60 dB in Decay Time; a one-pole
//!   low-pass in its loop makes the HF reference fall in Decay Time × Decay HF Ratio
//!   (a ratio above 1 is taken as 1: no HF gain). The combs' inputs are scaled so the
//!   late tail of an impulse carries its level's energy. Density scales the comb lengths
//!   (half to whole), Diffusion the all-pass gain (up to [`ALLPASS_GAIN`]). Room Rolloff
//!   Factor is 0 in every preset and not used.
//!
//! Switched off by the Sound tab's Reverb switch (`reverb`, kept in the INI) or
//! `nvforever --no-reverb`. A `reverb summary` line at the end.

use crate::music::{cell_at, AudioPlaces};
use crate::player::Player;
use crate::sfx::{Sfx, SfxSettings};
use crate::{Location, PlayWorld};
use bevy::prelude::*;
use fnv_combat::fx::Sound;
use fnv_world::audio;
use std::collections::HashMap;

/// An I3DL2 environment preset (`DSFXI3DL2Reverb`'s fields, in its order).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    /// The `ANAM` name xEdit gives it.
    pub name: &'static str,
    /// Millibels: the reflected sound's level, and its loss at `hf_reference`.
    pub room: i32,
    pub room_hf: i32,
    pub room_rolloff: f32,
    /// Seconds to fall 60 dB at low frequencies; the high ones' decay over it.
    pub decay_time: f32,
    pub decay_hf_ratio: f32,
    /// Millibels over `room`, and seconds after the direct sound.
    pub reflections: i32,
    pub reflections_delay: f32,
    /// Millibels over `room`, and seconds after the early reflections.
    pub reverb: i32,
    pub reverb_delay: f32,
    /// Percent.
    pub diffusion: f32,
    pub density: f32,
    /// Hertz.
    pub hf_reference: f32,
}

#[allow(clippy::too_many_arguments)]
const fn p(name: &'static str, room: i32, room_hf: i32, room_rolloff: f32, decay_time: f32, decay_hf_ratio: f32, reflections: i32, reflections_delay: f32, reverb: i32, reverb_delay: f32, diffusion: f32, density: f32, hf_reference: f32) -> Preset {
    Preset { name, room, room_hf, room_rolloff, decay_time, decay_hf_ratio, reflections, reflections_delay, reverb, reverb_delay, diffusion, density, hf_reference }
}

/// The environments by their ASPC `ANAM` names (xEdit's, values 1–30), with the
/// parameters of DirectSound's preset of that name [documented: dsound.h, module notes].
pub static PRESETS: [Preset; 30] = [
    p("Default", -1000, -100, 0.0, 1.49, 0.83, -2602, 0.007, 200, 0.011, 100.0, 100.0, 5000.0),
    p("Generic", -1000, -100, 0.0, 1.49, 0.83, -2602, 0.007, 200, 0.011, 100.0, 100.0, 5000.0),
    p("Padded Cell", -1000, -6000, 0.0, 0.17, 0.10, -1204, 0.001, 207, 0.002, 100.0, 100.0, 5000.0),
    p("Room", -1000, -454, 0.0, 0.40, 0.83, -1646, 0.002, 53, 0.003, 100.0, 100.0, 5000.0),
    p("Bathroom", -1000, -1200, 0.0, 1.49, 0.54, -370, 0.007, 1030, 0.011, 100.0, 60.0, 5000.0),
    p("Livingroom", -1000, -6000, 0.0, 0.50, 0.10, -1376, 0.003, -1104, 0.004, 100.0, 100.0, 5000.0),
    p("Stone Room", -1000, -300, 0.0, 2.31, 0.64, -711, 0.012, 83, 0.017, 100.0, 100.0, 5000.0),
    p("Auditorium", -1000, -476, 0.0, 4.32, 0.59, -789, 0.020, -289, 0.030, 100.0, 100.0, 5000.0),
    p("Concerthall", -1000, -500, 0.0, 3.92, 0.70, -1230, 0.020, -2, 0.029, 100.0, 100.0, 5000.0),
    p("Cave", -1000, 0, 0.0, 2.91, 1.30, -602, 0.015, -302, 0.022, 100.0, 100.0, 5000.0),
    p("Arena", -1000, -698, 0.0, 7.24, 0.33, -1166, 0.020, 16, 0.030, 100.0, 100.0, 5000.0),
    p("Hangar", -1000, -1000, 0.0, 10.05, 0.23, -602, 0.020, 198, 0.030, 100.0, 100.0, 5000.0),
    p("Carpeted Hallway", -1000, -4000, 0.0, 0.30, 0.10, -1831, 0.002, -1630, 0.030, 100.0, 100.0, 5000.0),
    p("Hallway", -1000, -300, 0.0, 1.49, 0.59, -1219, 0.007, 441, 0.011, 100.0, 100.0, 5000.0),
    p("Stone Corridor", -1000, -237, 0.0, 2.70, 0.79, -1214, 0.013, 395, 0.020, 100.0, 100.0, 5000.0),
    p("Alley", -1000, -270, 0.0, 1.49, 0.86, -1204, 0.007, -4, 0.011, 100.0, 100.0, 5000.0),
    p("Forest", -1000, -3300, 0.0, 1.49, 0.54, -2560, 0.162, -613, 0.088, 79.0, 100.0, 5000.0),
    p("City", -1000, -800, 0.0, 1.49, 0.67, -2273, 0.007, -2217, 0.011, 50.0, 100.0, 5000.0),
    p("Mountains", -1000, -2500, 0.0, 1.49, 0.21, -2780, 0.300, -2014, 0.100, 27.0, 100.0, 5000.0),
    p("Quarry", -1000, -1000, 0.0, 1.49, 0.83, -10000, 0.061, 500, 0.025, 100.0, 100.0, 5000.0),
    p("Plain", -1000, -2000, 0.0, 1.49, 0.50, -2466, 0.179, -2514, 0.100, 21.0, 100.0, 5000.0),
    p("Parkinglot", -1000, 0, 0.0, 1.65, 1.50, -1363, 0.008, -1153, 0.012, 100.0, 100.0, 5000.0),
    p("Sewerpipe", -1000, -1000, 0.0, 2.81, 0.14, 429, 0.014, 648, 0.021, 80.0, 60.0, 5000.0),
    p("Underwater", -1000, -4000, 0.0, 1.49, 0.10, -449, 0.007, 1700, 0.011, 100.0, 100.0, 5000.0),
    p("Small Room", -1000, -600, 0.0, 1.10, 0.83, -400, 0.005, 500, 0.010, 100.0, 100.0, 5000.0),
    p("Medium Room", -1000, -600, 0.0, 1.30, 0.83, -1000, 0.010, -200, 0.020, 100.0, 100.0, 5000.0),
    p("Large Room", -1000, -600, 0.0, 1.50, 0.83, -1600, 0.020, -1000, 0.040, 100.0, 100.0, 5000.0),
    p("Medium Hall", -1000, -600, 0.0, 1.80, 0.70, -1300, 0.015, -800, 0.030, 100.0, 100.0, 5000.0),
    p("Large Hall", -1000, -600, 0.0, 1.80, 0.70, -2000, 0.030, -1400, 0.060, 100.0, 100.0, 5000.0),
    p("Plate", -1000, -200, 0.0, 1.30, 0.90, 0, 0.002, 0, 0.010, 100.0, 75.0, 5000.0),
];

/// The presets of open air (DirectSound's names for outdoor places); the others are
/// rooms, halls and passages.
pub const OUTDOOR: [&str; 8] = ["Alley", "Forest", "City", "Mountains", "Quarry", "Plain", "Parkinglot", "Underwater"];

impl Preset {
    /// One of the [`OUTDOOR`] presets.
    pub fn outdoor(&self) -> bool {
        OUTDOOR.contains(&self.name)
    }
}

/// The preset of an `ANAM` name; `None` for "None", no name or one not listed.
pub fn preset(name: Option<&str>) -> Option<&'static Preset> {
    let name = name?.trim();
    PRESETS.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

/// The comb lengths in samples at 44.1 kHz [design: Freeverb's tuning].
pub const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
/// The all-pass lengths in samples at 44.1 kHz [design: Freeverb's tuning].
pub const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
/// The right ear's extra length in samples at 44.1 kHz [design: Freeverb's].
pub const SPREAD: usize = 23;
/// The all-pass gain at 100% diffusion [design: Freeverb's 0.5].
pub const ALLPASS_GAIN: f32 = 0.5;
/// The longest tail played after a sound ends, seconds [design].
pub const MAX_TAIL: f32 = 6.0;

/// Millibels as a linear gain.
pub fn gain_of_mb(mb: i32) -> f32 {
    10f32.powf(mb as f32 / 2000.0)
}

/// The coefficient `a` of the one-pole low-pass `y = (1 − a)·x + a·y₋₁` whose gain at
/// `hz` is `g` (0 < g ≤ 1) at sample rate `rate`; 0 (no filtering) for a gain of 1.
pub fn lowpass_coefficient(g: f32, hz: f32, rate: f32) -> f32 {
    let g = g.clamp(1e-4, 1.0);
    if g >= 0.9999 {
        return 0.0;
    }
    let c = (std::f32::consts::TAU * hz.min(rate * 0.49) / rate).cos();
    let g2 = g * g;
    let b = 1.0 - g2 * c;
    let disc = (b * b - (1.0 - g2) * (1.0 - g2)).max(0.0);
    ((b - disc.sqrt()) / (1.0 - g2)).clamp(0.0, 0.9999)
}

struct Comb {
    buf: Vec<f32>,
    at: usize,
    feedback: f32,
    damp: f32,
    store: f32,
    input_gain: f32,
}

impl Comb {
    fn new(len: usize, preset: &Preset, rate: f32, count: usize) -> Comb {
        let len = len.max(1);
        let t60 = preset.decay_time.max(0.05);
        let feedback = 10f32.powf(-3.0 * len as f32 / (t60 * rate));
        let hf_feedback = 10f32.powf(-3.0 * len as f32 / (t60 * preset.decay_hf_ratio.clamp(0.05, 1.0) * rate));
        let damp = lowpass_coefficient(hf_feedback / feedback, preset.hf_reference, rate);
        let input_gain = ((1.0 - feedback * feedback) / count as f32).sqrt();
        Comb { buf: vec![0.0; len], at: 0, feedback, damp, store: 0.0, input_gain }
    }

    fn run(&mut self, x: f32) -> f32 {
        let out = self.buf[self.at];
        self.store = out * (1.0 - self.damp) + self.store * self.damp;
        self.buf[self.at] = x * self.input_gain + self.store * self.feedback;
        self.at = (self.at + 1) % self.buf.len();
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    at: usize,
    gain: f32,
}

impl Allpass {
    fn run(&mut self, x: f32) -> f32 {
        let delayed = self.buf[self.at];
        let v = x + self.gain * delayed;
        self.buf[self.at] = v;
        self.at = (self.at + 1) % self.buf.len();
        delayed - self.gain * v
    }
}

/// One sound's reverb (module notes): stereo frames in, the dry sound with its
/// reflections and reverb out.
pub struct Reverb {
    /// The mono input, delayed: the early tap and the late input read from it.
    line: Vec<f32>,
    at: usize,
    early_delay: usize,
    late_delay: usize,
    early_gain: f32,
    late_gain: f32,
    hf_a: f32,
    hf_store: f32,
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
    /// Frames of tail still to play once the input has ended.
    tail: usize,
    rate: f32,
    /// The share of the sound sent into the reverb, 0 to 1 ([`RoomSend::send`]).
    send: f32,
}

/// The reverb a sound plays through: the environment, and how much of the sound goes
/// into it (module notes, "How much").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoomSend {
    pub preset: &'static Preset,
    /// 0 to 1, from the SOUN's Reverb Attenuation Control ([`send_of`]).
    pub send: f32,
    /// The reverb ends with the sound: no tail after it (a spoken line, [`for_voice`]).
    pub cut_at_end: bool,
}

impl RoomSend {
    /// The whole sound into `preset`.
    pub fn full(preset: &'static Preset) -> RoomSend {
        RoomSend { preset, send: 1.0, cut_at_end: false }
    }

    /// This sound's reverb at `rate`.
    pub fn reverb(&self, rate: u32) -> Reverb {
        let r = Reverb::new(self.preset, rate);
        let tail = if self.cut_at_end { 0 } else { r.tail };
        Reverb { send: self.send.clamp(0.0, 1.0), tail, ..r }
    }
}

/// `fDialogReverbAttenuation` of the [Audio] section of the install's
/// `Fallout_default.ini` [observed: `fDialogReverbAttenuation=0.15`]; nothing documents
/// its unit or what it does.
pub const DIALOG_REVERB_ATTENUATION: f32 = 0.15;
/// The share of a spoken line sent into the reverb: [`DIALOG_REVERB_ATTENUATION`] read
/// as a SOUN's Reverb Attenuation Control is (0 all of it, 1 nothing), so 0.85
/// [hypothesis: the name says attenuation, as the SOUN field's does; a fraction, not
/// decibels].
pub const VOICE_SEND: f32 = 1.0 - DIALOG_REVERB_ATTENUATION;

/// The share of a sound sent into the reverb by its Reverb Attenuation Control: "When
/// set to 100, the sound's reverb send is 100% attenuated -- no sound is sent into the
/// current reverb preset. When set to 0, ... all of the sound is allowed into the current
/// reverb preset" [documented: GECK wiki Sound, Attenuation tab; owner export]; in
/// between, that share of the amplitude [hypothesis: linear, not in decibels].
pub fn send_of(control_percent: f32) -> f32 {
    (1.0 - control_percent / 100.0).clamp(0.0, 1.0)
}

impl Reverb {
    pub fn new(preset: &Preset, rate: u32) -> Reverb {
        // Buffers are sized by the rate: a broken file's rate of billions would ask for
        // gigabytes (an allocation failure ends the process), so it is held to the rates
        // sound files have.
        let r = rate.clamp(8_000, 192_000) as f32;
        let scale = r / 44100.0;
        let early_delay = (preset.reflections_delay * r).round() as usize;
        let late_delay = ((preset.reflections_delay + preset.reverb_delay) * r).round() as usize;
        let density = 0.5 + 0.5 * (preset.density / 100.0).clamp(0.0, 1.0);
        let combs = [0, SPREAD].map(|spread| {
            COMBS.iter().map(|&n| Comb::new((((n + spread) as f32) * scale * density).round() as usize, preset, r, COMBS.len())).collect()
        });
        let ap_gain = ALLPASS_GAIN * (preset.diffusion / 100.0).clamp(0.0, 1.0);
        let allpasses = [0, SPREAD].map(|spread| {
            ALLPASSES.iter().map(|&n| Allpass { buf: vec![0.0; ((((n + spread) as f32) * scale).round() as usize).max(1)], at: 0, gain: ap_gain }).collect()
        });
        let room = gain_of_mb(preset.room);
        let tail = ((preset.reflections_delay + preset.reverb_delay + preset.decay_time).min(MAX_TAIL) * r) as usize;
        Reverb {
            line: vec![0.0; late_delay.max(early_delay) + 1],
            at: 0,
            early_delay,
            late_delay,
            early_gain: room * gain_of_mb(preset.reflections),
            late_gain: room * gain_of_mb(preset.reverb),
            hf_a: lowpass_coefficient(gain_of_mb(preset.room_hf), preset.hf_reference, r),
            hf_store: 0.0,
            combs,
            allpasses,
            tail,
            rate: r,
            send: 1.0,
        }
    }

    /// Seconds of tail still to play.
    pub fn seconds_left(&self) -> f32 {
        self.tail as f32 / self.rate
    }

    /// Seconds of tail at `rate` after the input ends.
    pub fn tail_seconds(preset: &Preset) -> f32 {
        (preset.reflections_delay + preset.reverb_delay + preset.decay_time).min(MAX_TAIL)
    }

    /// The reflected sound only, for one frame of mono input.
    fn wet(&mut self, x: f32) -> (f32, f32) {
        self.hf_store = x * (1.0 - self.hf_a) + self.hf_store * self.hf_a;
        let n = self.line.len();
        self.line[self.at] = self.hf_store;
        let tap = |d: usize| self.line[(self.at + n - d.min(n - 1)) % n];
        let early = tap(self.early_delay) * self.early_gain;
        let late_in = tap(self.late_delay) * self.late_gain;
        self.at = (self.at + 1) % n;
        let mut out = [0.0f32; 2];
        for (ear, o) in out.iter_mut().enumerate() {
            let mut s: f32 = self.combs[ear].iter_mut().map(|c| c.run(late_in)).sum();
            for a in self.allpasses[ear].iter_mut() {
                s = a.run(s);
            }
            *o = early + s;
        }
        (out[0], out[1])
    }

    /// One frame: the dry frame with its reflections and reverb.
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (wl, wr) = self.wet(0.5 * (l + r) * self.send);
        (l + wl, r + wr)
    }

    /// The next frame of the tail after the input has ended; `None` once it has died.
    pub fn tail(&mut self) -> Option<(f32, f32)> {
        if self.tail == 0 {
            return None;
        }
        self.tail -= 1;
        Some(self.wet(0.0))
    }
}

/// The reverb a sound effect is played through now: the listener's environment, with the
/// share of the sound its Reverb Attenuation Control sends; none when reverb is off, the
/// record keeps out of it or sends nothing (module notes).
pub fn for_sound(room: Option<&'static Preset>, settings: &SfxSettings, s: &Sound) -> Option<RoomSend> {
    if !settings.reverb || s.has("Environment Ignored") || s.has("Menu Sound") || crate::sfx::is_interface(s.edid.as_deref()) {
        return None;
    }
    let send = send_of(s.reverb_attenuation_percent);
    room.filter(|_| send > 0.0).map(|preset| RoomSend { preset, send, cut_at_end: false })
}

/// The reverb a spoken line is played through (module notes, "Voices"): the listener's
/// environment at [`VOICE_SEND`], cut at the line's end; none for a line not said at a
/// place (`placed` false: the conversation with the player, a 2D line) or when either
/// switch is off.
pub fn for_voice(room: Option<&'static Preset>, reverb_on: bool, voice_reverb_on: bool, placed: bool) -> Option<RoomSend> {
    room.filter(|_| reverb_on && voice_reverb_on && placed).map(|preset| RoomSend { preset, send: VOICE_SEND, cut_at_end: true })
}

/// The environment of the cell the listener is in (module notes).
#[derive(Resource, Default)]
pub struct RoomReverb {
    cell: Option<u32>,
    /// The acoustic space (ASPC) and its preset.
    pub space: Option<u32>,
    pub preset: Option<&'static Preset>,
    outside: bool,
    /// CELL `XCAS` by cell.
    spaces: HashMap<u32, Option<u32>>,
    pub changes: u32,
    /// The environments the listener has been in, by name.
    pub heard: std::collections::BTreeSet<&'static str>,
}

fn follow(mut room: ResMut<RoomReverb>, d: Res<PlayWorld>, loc: Res<Location>, places: Option<Res<AudioPlaces>>, player: Option<Res<Player>>, sfx: Option<ResMut<Sfx>>) {
    let (Some(places), Some(player), Some(mut sfx)) = (places, player, sfx) else { return };
    let cell = cell_at(&d, &loc, player.eye());
    if cell == room.cell {
        return;
    }
    room.cell = cell;
    let space = cell.and_then(|c| *room.spaces.entry(c).or_insert_with(|| audio::cell_audio(d.lo, d.schema, c).acoustic_space));
    if space == room.space && room.changes > 0 && room.outside == loc.interior.is_none() {
        return;
    }
    let aspc = space.and_then(|s| places.forms.spaces.get(&s));
    let named = aspc.and_then(|a| preset(a.environment.as_deref()));
    let outside = loc.interior.is_none();
    let preset = named.filter(|p| !outside || p.outdoor());
    room.space = space;
    room.outside = outside;
    room.preset = preset;
    sfx.room_reverb = preset;
    room.changes += 1;
    if let Some(p) = preset {
        room.heard.insert(p.name);
    }
    let place = match (loc.interior, cell) {
        (Some(c), _) => format!("interior {c:08X}"),
        (None, Some(c)) => format!("exterior cell {c:08X}"),
        (None, None) => "outside any cell".to_string(),
    };
    info!(
        "reverb: {place}: acoustic space {} environment {} -> {}",
        aspc.map_or_else(|| "none".to_string(), |a| format!("{} ({:08X})", a.edid.as_deref().unwrap_or("?"), a.form_id)),
        aspc.and_then(|a| a.environment.as_deref()).unwrap_or("-"),
        match (preset, named) {
            (Some(p), _) => p.name,
            (None, Some(_)) => "no reverb (a room's environment outside)",
            (None, None) => "no reverb",
        }
    );
}

fn report(mut exit: MessageReader<AppExit>, room: Res<RoomReverb>, sfx: Option<Res<Sfx>>, voice: Option<Res<crate::voice::VoiceState>>) {
    if exit.read().last().is_some() && room.changes > 0 {
        let heard: Vec<&str> = room.heard.iter().copied().collect();
        let (sounds, kept_out, sends) = sfx.map_or((0, 0, 0.0), |s| (s.reverbed, s.reverb_kept_out, s.reverb_send_sum));
        let mean = if sounds > 0 { sends / sounds as f32 } else { 0.0 };
        let voices = voice.map_or(0, |v| v.reverbed);
        info!(
            "reverb summary: {} changes of space, environments {}, {} sounds through a reverb (mean send {mean:.2}), {kept_out} kept out by their reverb attenuation control, {voices} voice lines through it (cut at their end)",
            room.changes,
            if heard.is_empty() { "none".to_string() } else { heard.join(", ") },
            sounds
        );
    }
}

/// Added by [`crate::sfx::SfxPlugin`].
pub(crate) fn build(app: &mut App) {
    app.init_resource::<RoomReverb>().add_systems(Update, follow.before(crate::PlaySet::Activation)).add_systems(Last, report);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_by_name() {
        assert_eq!(preset(Some("Stone Corridor")).map(|p| p.decay_time), Some(2.70));
        assert_eq!(preset(Some("small room")).map(|p| p.reverb), Some(500));
        assert!(preset(Some("None")).is_none() && preset(None).is_none() && preset(Some("Bogus")).is_none());
        // Every preset's parameters inside DSFXI3DL2Reverb's ranges.
        for p in &PRESETS {
            assert!((-10000..=0).contains(&p.room) && (-10000..=0).contains(&p.room_hf), "{}", p.name);
            assert!((0.1..=20.0).contains(&p.decay_time) && (0.1..=2.0).contains(&p.decay_hf_ratio), "{}", p.name);
            assert!((-10000..=1000).contains(&p.reflections) && (0.0..=0.3).contains(&p.reflections_delay), "{}", p.name);
            assert!((-10000..=2000).contains(&p.reverb) && (0.0..=0.1).contains(&p.reverb_delay), "{}", p.name);
        }
    }

    #[test]
    fn lowpass_gain_at_reference() {
        let rate = 44100.0;
        for g in [0.2f32, 0.5, 0.9] {
            let a = lowpass_coefficient(g, 5000.0, rate);
            let w = std::f32::consts::TAU * 5000.0 / rate;
            let h = (1.0 - a) / (1.0 - 2.0 * a * w.cos() + a * a).sqrt();
            assert!((h - g).abs() < 1e-3, "{g}: {h}");
        }
        assert_eq!(lowpass_coefficient(1.0, 5000.0, rate), 0.0);
    }

    /// An impulse through a preset: the dry sample first, the early reflection at its
    /// delay, a tail whose energy follows the late level and that dies within the tail.
    #[test]
    fn impulse_response() {
        let p = preset(Some("Stone Corridor")).unwrap();
        let rate = 22050;
        let mut r = Reverb::new(p, rate);
        let mut out = vec![r.process(1.0, 1.0)];
        while let Some(f) = r.tail() {
            out.push(f);
        }
        assert_eq!(out[0], (1.0, 1.0));
        assert_eq!(out.len(), 1 + (Reverb::tail_seconds(p) * rate as f32) as usize);
        // Nothing before the early reflection; the reflection at its delay and level.
        let early = (p.reflections_delay * rate as f32).round() as usize;
        assert!(out[1..early].iter().all(|f| f.0 == 0.0 && f.1 == 0.0));
        let room_hf = lowpass_coefficient(gain_of_mb(p.room_hf), p.hf_reference, rate as f32);
        let expect = gain_of_mb(p.room) * gain_of_mb(p.reflections) * (1.0 - room_hf);
        assert!((out[early].0 - expect).abs() < 1e-4, "{:?} vs {expect}", out[early]);
        // The late energy is near its level's (the combs' damping and the input
        // low-pass take some off), and the two ears differ.
        let late: f32 = out[early + 1..].iter().map(|f| f.0 * f.0).sum();
        let level = (gain_of_mb(p.room) * gain_of_mb(p.reverb)).powi(2);
        assert!(late > 0.1 * level && late < 1.5 * level, "late energy {late} vs {level}");
        assert!(out.iter().any(|f| (f.0 - f.1).abs() > 1e-6));
        // Faded by the end: the last 50 ms are 50 dB under the first after the onset.
        let peak = out[early..].iter().map(|f| f.0.abs()).fold(0.0, f32::max);
        let end = out[out.len() - rate as usize / 20..].iter().map(|f| f.0.abs()).fold(0.0, f32::max);
        assert!(end < peak * 0.003, "end {end} vs peak {peak}");
    }

    #[test]
    fn reverb_attenuation_control_sets_the_send() {
        assert_eq!(send_of(0.0), 1.0);
        assert_eq!(send_of(100.0), 0.0);
        assert!((send_of(80.0) - 0.2).abs() < 1e-6);
        let p = preset(Some("Stone Corridor")).unwrap();
        let early = (p.reflections_delay * 22050.0).round() as usize;
        let reflected = |send: f32| {
            let mut r = RoomSend { preset: p, send, cut_at_end: false }.reverb(22050);
            let first = r.process(1.0, 1.0);
            let mut out = vec![first];
            while let Some(f) = r.tail() {
                out.push(f);
            }
            out[early].0
        };
        // The dry sound is whole; what is reflected scales with the send.
        let mut r = RoomSend { preset: p, send: 0.2, cut_at_end: false }.reverb(22050);
        assert_eq!(r.process(1.0, 1.0), (1.0, 1.0));
        assert!((reflected(0.2) - 0.2 * reflected(1.0)).abs() < 1e-5);
        assert_eq!(reflected(0.0), 0.0);
    }

    #[test]
    fn voices_reverb_placed_and_cut_at_their_end() {
        let p = preset(Some("Stone Corridor"));
        // Only a line said at a place, with both switches on, in an environment.
        let send = for_voice(p, true, true, true).unwrap();
        assert!(send.cut_at_end && (send.send - 0.85).abs() < 1e-6);
        assert!(for_voice(p, true, true, false).is_none());
        assert!(for_voice(p, false, true, true).is_none());
        assert!(for_voice(p, true, false, true).is_none());
        assert!(for_voice(None, true, true, true).is_none());
        // The line is reverbed while it lasts and nothing plays after it.
        let rate = 22050;
        let mut r = send.reverb(rate);
        assert_eq!(r.seconds_left(), 0.0);
        let early = (p.unwrap().reflections_delay * rate as f32).round() as usize;
        let mut out = vec![r.process(1.0, 1.0)];
        for _ in 0..early {
            out.push(r.process(0.0, 0.0));
        }
        assert!(out[early].0 > 0.0, "reflected while the line plays");
        assert_eq!(r.tail(), None);
        // A world sound's send keeps its tail.
        assert!(RoomSend::full(p.unwrap()).reverb(rate).seconds_left() > 1.0);
    }

    #[test]
    fn longer_decay_rings_longer() {
        // The part of a tail's energy left 0.4 s after the impulse.
        let left_after = |name: &str, secs: f32| {
            let p = preset(Some(name)).unwrap();
            let mut r = Reverb::new(p, 22050);
            r.process(1.0, 1.0);
            let (mut all, mut late, mut n) = (0.0, 0.0, 0);
            while let Some(f) = r.tail() {
                n += 1;
                all += f.0 * f.0;
                if n as f32 > secs * 22050.0 {
                    late += f.0 * f.0;
                }
            }
            late / all
        };
        let (long, short) = (left_after("Auditorium", 0.4), left_after("Small Room", 0.4));
        assert!(long > 10.0 * short, "{long} vs {short}");
    }
}
