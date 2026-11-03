//! vheyes-nvse's functions (#1016): the moon's look (SetMoon…, GetMoonDoomState,
//! GetMoonAngle), the reference 3D refresh helpers (QueueRefClone, StampRef,
//! DeferRefUpdate3D, DetachRef3D, QueueRef3D …), the queued console command and Stewie's
//! Tweaks' stored ammunition [documented: each page in the wiki export;
//! docs/spec-notes/ext-vheyes.md]. vheyes-nvse is not installed in the probe game, so
//! nothing here is observed.
//!
//! NVForever loads a reference's 3D without the original's background clone queue, so
//! the queue is always settled (GetRefCloneQueueCount 0, no reference mid-clone)
//! [decision]. The moon's look and the 3D rebuilds are kept for the engine
//! ([`Game::moon_look`], [`Game::take_rebuild_3d`]); the engine's sky and model loader do
//! not read them yet (follow-up, ext-vheyes.md §3).

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use std::collections::HashMap;

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

/// The most references DeferRefUpdate3D keeps waiting [hypothesis: the page names "full
/// list" without its size].
pub const DEFER_LIMIT: usize = 4096;

/// The most requests a queue the engine takes ([`Game::take_rebuild_3d`],
/// `take_clamp_requests`, `take_allow_attack`) holds: a reference asked twice before
/// the engine takes the queue is in it once, and past the limit the oldest goes, so a
/// script asking every frame cannot grow it while nothing takes it [decision: the
/// engine does not take these queues yet, follow-up in ext-vheyes.md §3].
pub const QUEUE_LIMIT: usize = 1024;

/// Put `r` in an engine queue once, keeping it under [`QUEUE_LIMIT`].
pub(crate) fn queue_once(q: &mut Vec<FormId>, r: FormId) {
    if q.contains(&r) {
        return;
    }
    if q.len() >= QUEUE_LIMIT {
        q.remove(0);
    }
    q.push(r);
}

/// A moon cross-fade (SetMoonFade).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoonFade {
    pub start: f64,
    pub duration: f64,
    /// The looks applied at the invisible midpoint: doom set, doom texture, full.
    pub to: (bool, bool, bool),
    pub applied: bool,
}

/// The moon as scripts set it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MoonLook {
    /// SetMoonFull: full every night.
    pub full: bool,
    /// SetMoonDoom: the doom texture while full.
    pub doom: bool,
    /// SetMoonDoomSet: the per-phase doom textures.
    pub doom_set: bool,
    /// SetMoonScale's multiplier; `None`: the engine's size.
    pub scale: Option<f64>,
    /// SetMoonGrowth: base, per day, maximum, anchor day.
    pub growth: Option<(f64, f64, f64, f64)>,
    /// SetMoonShadowMult: the backing's multiplier and whether it applies with no scale.
    pub shadow: (f64, bool),
    pub fade: Option<MoonFade>,
}

/// vheyes-nvse's session state.
#[derive(Debug, Default)]
pub struct VheyesState {
    pub moon: MoonLook,
    /// The moon's angle as the engine's sky last told ([`Game::set_moon_angle`]).
    pub moon_angle: f64,
    /// QueueRefClone's stamps: reference -> its base's model path (lower case) when last
    /// acknowledged.
    pub stamps: HashMap<FormId, String>,
    /// DeferRefUpdate3D's waiting references, with their stamp flags.
    pub deferred: Vec<(FormId, bool)>,
    /// References whose 3D the engine should build again from their base's model.
    pub rebuild: Vec<FormId>,
    /// ArmQueuedConsoleCommand: the frame the queued lines run at.
    pub armed: Option<u64>,
}

impl Game<'_> {
    /// The moon's look for the engine's sky: the flags, the scale (SetMoonScale, else
    /// SetMoonGrowth's for `days` game days passed), and the cross-fade's alpha now (1
    /// when none).
    pub fn moon_look(&self, days: f64) -> (MoonLook, Option<f64>, f64) {
        let m = self.vheyes.moon;
        // [documented: SetMoonGrowth, "base * perDay ^ (GetGameDaysPassed - anchorDay),
        // clamped to max and never below the anchor"].
        let grown = m.growth.map(|(base, per, max, anchor)| (base * per.powf(days - anchor)).min(max).max(base));
        let alpha = m.fade.map_or(1.0, |f| {
            let half = f.duration / 2.0;
            let el = self.time_real() - f.start;
            if half <= 0.0 { 1.0 } else { ((el - half).abs() / half).clamp(0.0, 1.0) }
        });
        (m, m.scale.or(grown), alpha)
    }

    /// The engine's sky tells the moon's angle (90 overhead).
    pub fn set_moon_angle(&mut self, angle: f64) {
        self.vheyes.moon_angle = angle;
    }

    /// The moon's fade for the time now: the looks change at the midpoint, the fade ends
    /// after its duration.
    fn moon_fade_step(&mut self) {
        let now = self.time_real();
        if let Some(mut f) = self.vheyes.moon.fade {
            if !f.applied && now - f.start >= f.duration / 2.0 {
                (self.vheyes.moon.doom_set, self.vheyes.moon.doom, self.vheyes.moon.full) = f.to;
                f.applied = true;
            }
            self.vheyes.moon.fade = (now - f.start < f.duration).then_some(f);
        }
    }

    /// The engine takes the references whose 3D is to be built again.
    pub fn take_rebuild_3d(&mut self) -> Vec<FormId> {
        std::mem::take(&mut self.vheyes.rebuild)
    }

    /// The per-frame part: the moon's fade, DeferRefUpdate3D's flush and the queued
    /// console command. Called from [`Game::tick`].
    pub(crate) fn vheyes_tick(&mut self) {
        self.moon_fade_step();
        // The queue is always settled here, so the waiting references go every frame:
        // still valid ones are rebuilt and, with the flag, stamped [documented:
        // DeferRefUpdate3D; "dropped/invalid entries are never stamped"].
        for (r, stamp) in std::mem::take(&mut self.vheyes.deferred) {
            if crate::functions::base_of(self, r).is_none() {
                continue;
            }
            queue_once(&mut self.vheyes.rebuild, r);
            if stamp {
                if let Some(m) = crate::nif_blocks::model_of(self, r) {
                    self.vheyes.stamps.insert(r, m.to_ascii_lowercase());
                }
            }
        }
        if self.vheyes.armed.is_some_and(|f| self.frame >= f) {
            self.vheyes.armed = None;
            let text = self.mod_files.read_text("Config\\vheyes-nvse\\QueuedCommand.txt").unwrap_or_default();
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(8) {
                crate::nvse_console::call(self, "console", None, &[XVal::Str(line.to_string())]);
            }
        }
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    // A reference the 3D functions act on: not the player, and placed [documented: their
    // "0 = player/invalid"].
    let valid = |g: &Game, r: Option<FormId>| r.filter(|&r| r != FormId::PLAYER_REF && crate::functions::base_of(g, r).is_some());
    let v = match name {
        // ---- the moon (void) [documented: each page] ----
        "setmoondoom" => {
            g.vheyes.moon.doom = num(args, 0, 0.0) != 0.0;
            zero
        }
        "setmoondoomset" => {
            g.vheyes.moon.doom_set = num(args, 0, 0.0) != 0.0;
            zero
        }
        "setmoonfull" => {
            g.vheyes.moon.full = num(args, 0, 0.0) != 0.0;
            zero
        }
        // Above 0 a multiplier; 0 or below hands the size back [documented].
        "setmoonscale" => {
            let s = num(args, 0, 0.0);
            g.vheyes.moon.scale = (s > 0.0).then_some(s);
            zero
        }
        "setmoongrowth" => {
            g.vheyes.moon.growth = Some((num(args, 0, 1.0), num(args, 1, 1.0), num(args, 2, 1.0), num(args, 3, 0.0)));
            zero
        }
        "setmoonshadowmult" => {
            g.vheyes.moon.shadow = (num(args, 0, 1.0), num(args, 1, 0.0) != 0.0);
            zero
        }
        // `duration doomSet doomTex full`: the looks change at the invisible midpoint; a
        // call while one runs is ignored [documented: SetMoonFade].
        "setmoonfade" => {
            if g.vheyes.moon.fade.is_none() {
                let now = g.time_real();
                let to = (num(args, 1, 0.0) != 0.0, num(args, 2, 0.0) != 0.0, num(args, 3, 0.0) != 0.0);
                let duration = num(args, 0, 0.0).max(0.0);
                g.vheyes.moon.fade = Some(MoonFade { start: now, duration, to, applied: false });
                // A zero fade changes the looks at once; only the fade's step runs, not
                // the frame's flush or queued console lines.
                if duration == 0.0 {
                    g.moon_fade_step();
                }
            }
            zero
        }
        // 2 forced-full doom moon, 1 the doom set, 0 vanilla [documented].
        "getmoondoomstate" => XVal::Num(if g.vheyes.moon.doom {
            2.0
        } else if g.vheyes.moon.doom_set {
            1.0
        } else {
            0.0
        }),
        // [documented: GetMoonAngle, "Returns 0 if there is no sky/moon"]: what the
        // engine's sky told, 0 before it tells (follow-up: it does not tell yet).
        "getmoonangle" => XVal::Num(g.vheyes.moon_angle),
        // ---- the 3D refresh helpers ----
        "getrefclonequeuecount" | "isrefmidclone" => zero,
        // The 3D torn down [documented: DetachRef3D]: the scene mirror forgets it.
        "detachref3d" => match valid(g, subject) {
            Some(r) => {
                g.scene.forget(r);
                XVal::Num(1.0)
            }
            None => zero,
        },
        // [documented: QueueRef3D, "Queues the calling reference for background 3D
        // rebuild"].
        "queueref3d" => match valid(g, subject) {
            Some(r) => {
                queue_once(&mut g.vheyes.rebuild, r);
                XVal::Num(1.0)
            }
            None => zero,
        },
        // `[stampFlag]`: waits for the settled queue (the next frame here) [documented].
        "deferrefupdate3d" => match valid(g, subject) {
            Some(r) if g.vheyes.deferred.len() < DEFER_LIMIT || g.vheyes.deferred.iter().any(|(d, _)| *d == r) => {
                let stamp = num(args, 0, 0.0) != 0.0;
                g.vheyes.deferred.retain(|(d, _)| *d != r);
                g.vheyes.deferred.push((r, stamp));
                XVal::Num(1.0)
            }
            _ => zero,
        },
        // 1 when stamped, 0 when the base has no model path [documented: StampRef].
        "stampref" => {
            let m = subject.and_then(|r| crate::nif_blocks::model_of(g, r).map(|m| (r, m)));
            match m {
                Some((r, m)) => {
                    g.vheyes.stamps.insert(r, m.to_ascii_lowercase());
                    XVal::Num(1.0)
                }
                None => zero,
            }
        }
        // A stamp that matches: 0; one that does not: 1; none yet: stamped and 0
        // (QueueRefCloneHard: 1, not stamped) [documented: QueueRefClone;
        // QueueRefCloneHard]. A reference without a model path: 0 [hypothesis].
        "queuerefclone" | "queuerefclonehard" => {
            let Some((r, m)) = subject.and_then(|r| crate::nif_blocks::model_of(g, r).map(|m| (r, m.to_ascii_lowercase()))) else { return Some(Ok(zero)) };
            match g.vheyes.stamps.get(&r) {
                Some(s) => yes(*s != m),
                None if name == "queuerefclonehard" => XVal::Num(1.0),
                None => {
                    g.vheyes.stamps.insert(r, m);
                    zero
                }
            }
        }
        // Every stamp and the deferred list wiped; always 1 [documented].
        "clearrefstamps" => {
            g.vheyes.stamps.clear();
            g.vheyes.deferred.clear();
            XVal::Num(1.0)
        }
        // `[delayTicks]` (2): Data\Config\vheyes-nvse\QueuedCommand.txt's lines (up to 8)
        // run as console lines that many frames later, once; 1 armed [documented:
        // ArmQueuedConsoleCommand; the file is read when the lines run, from NVForever's
        // layer first (`crate::mod_files`)].
        "armqueuedconsolecommand" => {
            let delay = num(args, 0, 2.0).max(0.0) as u64;
            g.vheyes.armed = Some(g.frame + delay);
            XVal::Num(1.0)
        }
        // [decision: NVForever keeps no Stewie's Tweaks "Remember Weapon Ammos" data]:
        // the pages' "no stored data" values, -1 and 0.
        "getstoredammorounds" | "getstoredclip" => XVal::Num(-1.0),
        "getstoredammotype" | "getstoredammoform" => XVal::Ref(0),
        _ => return None,
    };
    Some(Ok(v))
}
