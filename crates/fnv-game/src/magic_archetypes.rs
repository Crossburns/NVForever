//! Base effect archetypes beyond value changes and scripts (docs/spec-notes/magic-archetypes.md):
//! Paralysis, Cure Paralysis, Cure Poison, Dispel, Concussion and Turbo, and the
//! Paralyzing Palm that casts a paralysis in VATS.
//!
//! - An effect's archetype acts when the effect starts ([`Game::archetype_start`], from
//!   `Game::effect_archetype_start`): a Paralysis effect knocks its target down paralysed
//!   (`crate::unconscious::Cause::Paralyzed`) for as long as a Paralysis effect lasts on
//!   it ([`Game::paralysis_holds`]); the cures and Dispel end the target's effects they
//!   name (§2).
//! - Concussion ([`Game::tick_concussions`], each frame): an actor with an active
//!   Concussion effect whose head becomes crippled is concussed for `fConcussionTimer`
//!   seconds ([`Game::is_concussed`], GetConcussed); on the player the
//!   `ImageSpaceConcussion` modifier plays [hypothesis, §2].
//! - Turbo ([`Game::turbo_time_scale`]): while the player's Turbo actor value is above 0,
//!   the engine runs the world at [`TURBO_TIME_SCALE`] and keeps the player at full
//!   speed [hypothesis: the number; design: the exemption].
//! - What happened is told to the engine as [`MagicNews`] (not trace events: the trace is
//!   compared with the original's).

use crate::data::{
    MagicEffectDef, ARCHETYPE_CONCUSSION, ARCHETYPE_CURE_PARALYSIS, ARCHETYPE_CURE_POISON, ARCHETYPE_DISPEL, ARCHETYPE_PARALYSIS,
};
use crate::functions::actor;
use crate::game::Game;
use crate::unconscious::Cause;
use fnv_sim::FormId;

/// SPEL SPIT type "Poison" [documented: schema `combat.json` SPIT; observed: the eight
/// spells Antivenom's script dispels are of this type, magic-archetypes.md §1].
pub const SPELL_POISON: i64 = 5;
/// The actor value Paralysis [documented: GECK wiki Actor Value Indices; `AV_NAMES`].
pub const PARALYSIS: u16 = 47;
/// The actor value Turbo [documented: as Paralysis].
pub const TURBO: u16 = 51;
/// The actor value Perception Condition (the head's) [documented: as Paralysis].
pub const PERCEPTION_CONDITION: u16 = 25;
/// The speed the world's clock runs at while the player's Turbo is above 0
/// [hypothesis: no setting or source gives the number; owner check].
pub const TURBO_TIME_SCALE: f64 = 0.5;
/// Seconds of game time a concussion lasts when the data has no `fConcussionTimer`
/// [observed: 40.0 in FalloutNV.esm].
pub const CONCUSSION_SECONDS: f64 = 40.0;
/// IMAD `ImageSpaceConcussion`, an engine form [observed: FalloutNV.esm 0x00000164,
/// animatable, 11 s].
pub const IMAGE_SPACE_CONCUSSION: FormId = FormId(0x0000_0164);
/// SPEL `PerkParalyzingPalm` "Paralyzing Palm" [documented: GECK wiki GetDefaultForms
/// lists it among the default forms; observed: 0x0006B20B, Paralysis 1 for 30 s, Touch].
/// The palm casts the DOBJ's Perk Paralysis (place 5, which FalloutNV.esm fills with this
/// spell [observed: `fnvtool esm decode --edid DefaultObjectManager`]); this is the
/// fallback when the list names none ([`Game::palm_spell`]).
pub const PARALYZING_PALM: FormId = FormId(0x0006_B20B);
/// The perk entry point "Has Paralyzing Palm" [documented: GECK wiki
/// GetNthPerkEntryFunction, entry point 29].
pub const HAS_PARALYZING_PALM: u8 = 29;
/// The chance per VATS hit when the data has no `fVATSParalyzePalmChance` [documented:
/// GECK wiki VATS Settings, default 0.3].
pub const PALM_CHANCE: f64 = 0.3;
/// The misc statistic "Paralyzing Punches" (`crate::misc_stats::NAMES`).
pub const PARALYZING_PUNCHES: u16 = 21;
/// The actor value Unarmed: a weapon of this skill hits as a palm can.
const UNARMED: i64 = 45;

/// What an archetype did, for the engine to log ([`Game::take_magic_news`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MagicNews {
    /// A Paralysis effect took hold of `actor` for `seconds` (its effect's duration).
    Paralyzed { actor: FormId, effect: FormId, seconds: f64 },
    /// A cure or Dispel ended `count` effects of `actor`; `what` names the archetype.
    Cured { actor: FormId, what: &'static str, count: usize },
    /// `actor` is concussed for `seconds`.
    Concussed { actor: FormId, seconds: f64 },
    /// The player's palm strike paralysed `target`.
    Palm { target: FormId },
}

impl Game<'_> {
    /// What an effect's archetype does when it starts (see the module notes); `id` is the
    /// active effect, `archetype` its base effect's.
    pub(crate) fn archetype_start(&mut self, id: u32, archetype: i64) {
        let Some(e) = self.effects.iter().find(|e| e.id == id) else { return };
        let (target, source, effect, seconds) = (e.target, e.source, e.effect, e.duration - e.elapsed);
        let caster = e.caster;
        let data = self.data;
        let arch = |e: &crate::game::ActiveEffect| data.magic_effects.get(&e.effect).map(|m| m.archetype);
        match archetype {
            ARCHETYPE_PARALYSIS => {
                // Down for as long as a Paralysis effect lasts (`tick_knockouts`).
                self.knock_out(target, Cause::Paralyzed, caster);
                self.magic_news.push(MagicNews::Paralyzed { actor: target, effect, seconds });
            }
            ARCHETYPE_CURE_PARALYSIS => {
                let n = self.dispel(target, None, |e| e.id != id && arch(e) == Some(ARCHETYPE_PARALYSIS));
                self.magic_news.push(MagicNews::Cured { actor: target, what: "Cure Paralysis", count: n });
            }
            ARCHETYPE_CURE_POISON => {
                let n = self.dispel(target, None, |e| e.id != id && data.spell_types.get(&e.source) == Some(&SPELL_POISON));
                self.magic_news.push(MagicNews::Cured { actor: target, what: "Cure Poison", count: n });
            }
            ARCHETYPE_DISPEL => {
                let n = self.dispel(target, None, |e| e.id != id && !e.permanent && e.source != source);
                self.magic_news.push(MagicNews::Cured { actor: target, what: "Dispel", count: n });
            }
            _ => {}
        }
    }

    /// Whether an active (started) Paralysis effect lasts on `r`.
    pub fn paralysis_holds(&self, r: FormId) -> bool {
        self.effects.iter().any(|e| e.target == r && !e.inactive && self.data.magic_effects.get(&e.effect).is_some_and(|m| m.archetype == ARCHETYPE_PARALYSIS))
    }

    /// The speed the world's clock runs at for Turbo: [`TURBO_TIME_SCALE`] while the
    /// player's Turbo actor value is above 0, else 1. The engine keeps the player at full
    /// speed (docs/spec-notes/magic-archetypes.md §2).
    pub fn turbo_time_scale(&mut self) -> f64 {
        if actor::actor_value(self, FormId::PLAYER_REF, TURBO).is_ok_and(|v| v > 0.0) {
            TURBO_TIME_SCALE
        } else {
            1.0
        }
    }

    /// GetConcussed: `r` is concussed now.
    pub fn is_concussed(&self, r: FormId) -> bool {
        self.extra.get(&r).and_then(|e| e.concussed_until).is_some_and(|t| self.time < t)
    }

    /// One frame: an actor with an active Concussion effect whose head has just become
    /// crippled is concussed (see the module notes).
    pub fn tick_concussions(&mut self) {
        let data = self.data;
        let mut able: Vec<FormId> =
            self.effects.iter().filter(|e| !e.inactive && data.magic_effects.get(&e.effect).is_some_and(|m| m.archetype == ARCHETYPE_CONCUSSION)).map(|e| e.target).collect();
        able.sort();
        able.dedup();
        // Those no longer able forget their head (a crippling later counts as new).
        let watched: Vec<FormId> = self.extra.iter().filter(|(_, x)| x.head_crippled).map(|(r, _)| *r).collect();
        for r in watched {
            if !able.contains(&r) {
                if let Some(x) = self.extra.get_mut(&r) {
                    x.head_crippled = false;
                }
            }
        }
        for r in able {
            let crippled = actor::actor_value(self, r, PERCEPTION_CONDITION).is_ok_and(|v| v <= 0.0);
            let was = self.extra.get(&r).is_some_and(|x| x.head_crippled);
            if crippled == was {
                continue;
            }
            self.extra.entry(r).or_default().head_crippled = crippled;
            if !crippled || self.is_dead(r) {
                continue;
            }
            let seconds = crate::combat::setting(self, "fConcussionTimer", CONCUSSION_SECONDS);
            self.extra.entry(r).or_default().concussed_until = Some(self.time + seconds);
            if r == FormId::PLAYER_REF && self.data.image_modifiers.contains_key(&IMAGE_SPACE_CONCUSSION) {
                self.apply_image_modifier(IMAGE_SPACE_CONCUSSION);
            }
            self.magic_news.push(MagicNews::Concussed { actor: r, seconds });
        }
    }

    /// The player hit `target` with `weapon` (`None`: fists) while VATS plays back: with
    /// the perk entry point "Has Paralyzing Palm" above 0, an unarmed hit paralyses with
    /// the chance `fVATSParalyzePalmChance` (`PerkParalyzingPalm` as a hit effect) and
    /// counts a Paralyzing Punch [documented: settings and entry point; hypothesis: the
    /// weapon test]. `roll` is a number in [0, 1). Returns whether it paralysed.
    pub fn paralyzing_palm(&mut self, target: FormId, weapon: Option<FormId>, roll: f64) -> bool {
        let palm = self.palm_spell();
        if self.vats.mode != crate::vats::MODE_PLAYBACK || self.is_dead(target) || !self.data.effect_items.contains_key(&palm) {
            return false;
        }
        if weapon.is_some_and(|w| self.data.weapon_skills.get(&w) != Some(&UNARMED)) {
            return false;
        }
        if self.entry_point(FormId::PLAYER_REF, HAS_PARALYZING_PALM, 0.0).0 <= 0.0 {
            return false;
        }
        if roll >= crate::combat::setting(self, "fVATSParalyzePalmChance", PALM_CHANCE) {
            return false;
        }
        let applied = self.start_hit_effect(target, palm, Some(FormId::PLAYER_REF));
        let took = applied.iter().any(|a| self.data.magic_effects.get(&a.effect).is_some_and(|m| m.archetype == ARCHETYPE_PARALYSIS));
        if took {
            self.count_stat(PARALYZING_PUNCHES, 1.0);
            self.magic_news.push(MagicNews::Palm { target });
        }
        took
    }

    /// The spell a Paralyzing Palm casts: the DOBJ's Perk Paralysis (place 5) [documented:
    /// xEdit DOBJ names], else [`PARALYZING_PALM`].
    pub fn palm_spell(&self) -> FormId {
        palm_spell_in(self.data.default_object(crate::data::DefaultObject::PerkParalysis))
    }

    /// What the archetypes did since the last call.
    pub fn take_magic_news(&mut self) -> Vec<MagicNews> {
        std::mem::take(&mut self.magic_news)
    }
}

/// A number in [0, 1) from the game time and a reference (splitmix64), for rolls that
/// must not move the game's random sequence.
pub fn hash_roll(time: f64, r: FormId) -> f64 {
    let mut z = time.to_bits() ^ (u64::from(r.0) << 17) ^ 0x9E37_79B9_7F4A_7C15;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// Whether a base effect is of an archetype this module acts on when it starts.
pub fn acts_on_start(m: &MagicEffectDef) -> bool {
    matches!(m.archetype, ARCHETYPE_PARALYSIS | ARCHETYPE_CURE_PARALYSIS | ARCHETYPE_CURE_POISON | ARCHETYPE_DISPEL)
}

/// The palm's spell from the DOBJ's Perk Paralysis entry, or the fixed fallback.
fn palm_spell_in(dobj: Option<FormId>) -> FormId {
    dobj.unwrap_or(PARALYZING_PALM)
}

#[cfg(test)]
mod palm_tests {
    use super::*;

    #[test]
    fn the_palm_casts_the_default_object() {
        assert_eq!(palm_spell_in(Some(FormId(0x0100_0ABC))), FormId(0x0100_0ABC));
        assert_eq!(palm_spell_in(None), PARALYZING_PALM);
    }
}
