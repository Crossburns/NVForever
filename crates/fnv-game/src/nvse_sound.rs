//! The extenders' sound functions over the sounds scripts play (`PlaySound`,
//! `PlaySound3D`; `crate::sound`): IsSoundPlaying, StopSound, StopSoundAlt,
//! GetSoundPlayers, IsSoundPlayingFromPath [documented: their pages; observed: sound1, the
//! peer's captures of 2026-10-27].
//!
//! The rules keep each sound a script asked for ([`Played`]) until it ends; the engine plays
//! it and says how long it lasts or that it loops (`nvf_play::world_sounds`), and stops
//! what StopSound names. Headless, nothing says a sound ended: it counts as playing until
//! stopped [design].

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;

/// A sound a script asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Played {
    pub id: u32,
    pub sound: FormId,
    /// `PlaySound3D`'s reference; `None` for `PlaySound`.
    pub source: Option<FormId>,
    /// The frame it was asked for in.
    pub frame: u64,
    /// When it ends on [`SoundState::clock`]; `None` while the engine has not said, or for a
    /// looping sound.
    pub until: Option<f64>,
    pub looping: bool,
    /// The frame StopSound stopped it in.
    pub stopped: Option<u64>,
}

/// This file's session state (not saved).
#[derive(Debug, Default)]
pub struct SoundState {
    pub next_id: u32,
    pub played: Vec<Played>,
    /// Seconds of play since the session began, for the sounds' ends.
    pub clock: f64,
    /// Sounds StopSound stopped, for the engine to take: the SOUN and the id of the last
    /// sound asked for before the stop ([`SoundState::ask`]'s ids rise), so the engine
    /// stops the playings asked for before it, also those of the same frame, and still
    /// plays one asked for after it. So `PlaySound X; StopSound X` in one frame plays
    /// nothing [engineering choice; hypothesis: what the original does with that order in
    /// one frame is not known, docs/spec-notes/audio.md §11.5, observer check 82].
    pub stops: Vec<(FormId, u32)>,
}

impl SoundState {
    /// Note a sound asked for in `frame`; its id for the engine.
    pub fn ask(&mut self, sound: FormId, source: Option<FormId>, frame: u64) -> u32 {
        self.next_id = self.next_id.wrapping_add(1);
        self.played.push(Played { id: self.next_id, sound, source, frame, until: None, looping: false, stopped: None });
        self.next_id
    }

    /// The engine's word on sound `id`: it lasts `seconds`, or (`None`) it loops.
    pub fn heard(&mut self, id: u32, seconds: Option<f32>) {
        let clock = self.clock;
        if let Some(p) = self.played.iter_mut().find(|p| p.id == id) {
            match seconds {
                Some(s) => p.until = Some(clock + f64::from(s)),
                None => p.looping = true,
            }
        }
    }

    /// StopSound of `sound` in `frame`: every playing of it not stopped yet is stopped, and
    /// the engine is told (with the last id asked for so far) when there was one. A playing
    /// asked for earlier in the same frame is stopped too, before it is heard
    /// [engineering choice; the original's behaviour is not known, audio.md §11.5].
    pub fn stop(&mut self, sound: FormId, frame: u64) {
        let mut any = false;
        for p in self.played.iter_mut().filter(|p| p.sound == sound && p.stopped.is_none()) {
            p.stopped = Some(frame);
            any = true;
        }
        if any {
            self.stops.push((sound, self.next_id));
        }
    }

    /// One frame of `dt` seconds ends; sounds over or stopped before `frame` are dropped.
    pub fn tick(&mut self, dt: f64, frame: u64) {
        self.clock += dt;
        let clock = self.clock;
        self.played.retain(|p| p.stopped.is_none_or(|f| f >= frame) && p.until.is_none_or(|u| clock < u));
    }

    /// Whether `p` counts as playing in `frame`: from the second frame after it was asked
    /// for, until its end, and in the frame StopSound stopped it [observed: sound1 on both
    /// machines, sound2a–d (twice) and sky-sound, IsSoundPlaying 0 in PlaySound's frame and
    /// the next and 1 from the one after; 1 in StopSound's frame, 0 from the next].
    fn playing(&self, p: &Played, frame: u64) -> bool {
        p.frame + 1 < frame && p.stopped.is_none_or(|f| f == frame) && p.until.is_none_or(|u| self.clock < u)
    }
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(r) if *r != 0 => Some(FormId(*r)),
        _ => None,
    }
}

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k)? {
        XVal::Str(s) => Some(s.clone()),
        _ => None,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(if b { 1.0 } else { 0.0 })
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let frame = g.frame;
    let v = match name {
        // `sound [source]`: a sound a script played is playing (`SoundState::playing`); with a source, only one
        // played at that reference [observed: sound1, PlaySound3D on the player read 1
        // with the player, 0 with Doc Mitchell's base, 1 with no source; a looping SOUN
        // (EMTMedicalBeep01) still 1 after 120 frames, a one-shot (UIVATSLoop) 1 for one
        // frame; hypothesis: sounds the game plays itself are not counted].
        "issoundplaying" => {
            let Some(s) = form(args, 0) else { return Some(Ok(zero)) };
            let src = form(args, 1);
            let st = &g.gaps.sounds;
            yes(st.played.iter().any(|p| p.sound == s && src.is_none_or(|r| p.source == Some(r)) && st.playing(p, frame)))
        }
        // `sound`: every playing of the sound stops; IsSoundPlaying still reads 1 in this
        // frame [observed: sound1, 1 in StopSound's frame, 0 from the next].
        "stopsound" => {
            if let Some(s) = form(args, 0) {
                g.gaps.sounds.stop(s, frame);
            }
            zero
        }
        // JohnnyGuitar's `sound ref`: returns 0 and leaves the sound playing [observed:
        // sound1, a PlaySound3D on the player still playing 6 frames on; hypothesis: it
        // stops only sounds the game attached to the reference itself].
        "stopsoundalt" => zero,
        // `sound`: the references a placed (3D) playing of the sound is at; none is no
        // array [observed: sound1, ar_Size -1 for a PlaySound sound, 1 for PlaySound3D on
        // the player].
        "getsoundplayers" => {
            let Some(s) = form(args, 0) else { return Some(Ok(zero)) };
            let st = &g.gaps.sounds;
            let mut at: Vec<u32> = st.played.iter().filter(|p| p.sound == s && st.playing(p, frame)).filter_map(|p| p.source.map(|r| r.0)).collect();
            at.dedup();
            if at.is_empty() {
                zero
            } else {
                let entries = at.into_iter().enumerate().map(|(i, r)| (XKey::Num(i as f64), XVal::Ref(r))).collect();
                XVal::Array(g.ext.new_array(ArrayKind::List, entries))
            }
        }
        // `path [source]`: 0 for every path form tried while the sound played [observed:
        // sound1, "fx\..", "Data\Sound\fx\..", with and without the player; hypothesis:
        // a path form not yet found reads 1].
        "issoundplayingfrompath" => {
            let _ = (text(args, 0), subject);
            zero
        }
        _ => return None,
    };
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_plays_from_the_second_frame_until_it_ends_or_stops() {
        let mut st = SoundState::default();
        let id = st.ask(FormId(7), None, 10);
        let p = st.played[0].clone();
        assert!(!st.playing(&p, 10));
        assert!(!st.playing(&p, 11));
        assert!(st.playing(&p, 12));
        st.heard(id, Some(0.05));
        st.tick(0.02, 11);
        assert!(st.playing(&st.played[0].clone(), 12));
        st.tick(0.04, 12);
        assert!(st.played.is_empty());
        // Stopped: still playing in the stop's frame, gone after.
        st.ask(FormId(7), None, 20);
        st.played[0].stopped = Some(23);
        assert!(st.playing(&st.played[0].clone(), 23));
        st.tick(0.02, 24);
        assert!(st.played.is_empty());
    }

    #[test]
    fn a_stop_names_the_last_sound_asked_for_before_it() {
        let mut st = SoundState::default();
        // PlaySound, then StopSound in the same frame: the stop covers that play.
        let first = st.ask(FormId(7), None, 10);
        st.stop(FormId(7), 10);
        assert_eq!(st.stops, vec![(FormId(7), first)]);
        // StopSound, then PlaySound: the new one is asked for after it, a higher id.
        let second = st.ask(FormId(7), None, 10);
        assert!(second > first);
        // A stop with nothing of the sound playing tells the engine nothing.
        st.stops.clear();
        st.stop(FormId(8), 10);
        assert!(st.stops.is_empty());
    }
}
