//! JIP LN's fourth batch (2026-10-30, issue #955; docs/spec-notes/jip-traits-4.md): the
//! trait and flag getters and setters of classes, combat styles, lights, grass, texture
//! sets, climates, quests, factions, cells, waters and sounds, read from the decoded
//! records, and a few getters of the game's state (difficulty, session time, the
//! player's detection, reference types, doors, quest delays, references kept in
//! globals, editor positions, poison) [documented: their pages in the wiki export;
//! the field each trait reads: the schema, tagged per row in the spec note].
//!
//! A setter changes the record for the session: the getters, and any other reader of the
//! same field through `crate::nvse::record_value`, see the change; the engine's drawing,
//! combat and sky go on reading the records [hypothesis: not saved, as the other
//! record setters].

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use serde_json::Value as Json;
use std::collections::HashMap;

/// This file's session state (not saved).
#[derive(Debug, Default)]
pub struct Jip4State {
    /// SetQuestFlag's changes to the record flags of a quest (IDs 2–5 and 7), as the
    /// whole flag byte.
    pub quest_flags: HashMap<FormId, u8>,
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(n)) => *n,
        _ => dflt,
    }
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(if b { 1.0 } else { 0.0 })
}

/// A decoded field of `f`'s subrecord `sub`; a number in `path` indexes an array
/// (`tag_skills.2`). Flags read as their number.
fn json<'a>(g: &'a Game, f: FormId, sub: &str, path: &str) -> Option<&'a Json> {
    let mut v = g.data.record_field(g.unclone(f), sub, "")?;
    for k in path.split('.').filter(|k| !k.is_empty()) {
        v = match k.parse::<usize>() {
            Ok(i) if v.is_array() => v.get(i)?,
            _ => v.get(k)?,
        };
    }
    Some(v.get("value").filter(|_| v.get("set").is_some()).unwrap_or(v))
}

fn key(sub: &str, path: &str) -> String {
    format!("{sub}/{path}")
}

/// A record field as a number, with the session's changes.
fn get_num(g: &Game, f: FormId, sub: &str, path: &str) -> f64 {
    if let Some(XVal::Num(x)) = g.field_overrides.get(&(f, key(sub, path))) {
        return *x;
    }
    json(g, f, sub, path).and_then(Json::as_f64).map_or(0.0, crate::nvse::record_float)
}

/// A record field as a form (a hexadecimal "0x…" in the decoded record).
fn get_form(g: &Game, f: FormId, sub: &str, path: &str) -> u32 {
    if let Some(XVal::Ref(r)) = g.field_overrides.get(&(f, key(sub, path))) {
        return *r;
    }
    json(g, f, sub, path).and_then(Json::as_str).and_then(|s| s.strip_prefix("0x")).and_then(|h| u32::from_str_radix(h, 16).ok()).unwrap_or(0)
}

/// A record field as a string (a path in the record).
fn get_str(g: &Game, f: FormId, sub: &str, path: &str) -> String {
    if let Some(XVal::Str(s)) = g.field_overrides.get(&(f, key(sub, path))) {
        return s.clone();
    }
    match json(g, f, sub, path) {
        Some(Json::String(s)) if !s.starts_with("0x") => s.clone(),
        Some(v) => v.get("model").and_then(Json::as_str).unwrap_or_default().to_string(),
        None => String::new(),
    }
}

fn set(g: &mut Game, f: FormId, sub: &str, path: &str, v: XVal) {
    g.field_overrides.insert((f, key(sub, path)), v);
}

/// One bit of a flags field.
fn get_bit(g: &Game, f: FormId, sub: &str, path: &str, bit: i64) -> bool {
    (0..32).contains(&bit) && (get_num(g, f, sub, path) as u64) & (1 << bit) != 0
}

fn set_bit(g: &mut Game, f: FormId, sub: &str, path: &str, bit: i64, on: bool) {
    if !(0..32).contains(&bit) {
        return;
    }
    let now = get_num(g, f, sub, path) as u64;
    let new = if on { now | (1 << bit) } else { now & !(1 << bit) };
    set(g, f, sub, path, XVal::Num(new as f64));
}

/// The class trait IDs 0–10 [documented: GetClassTraitNumeric]: the four tag skills, then
/// the seven attributes (ATTR).
fn class_trait(t: i64) -> Option<(&'static str, String)> {
    const ATTR: [&str; 7] = ["strength", "perception", "endurance", "charisma", "intelligence", "agility", "luck"];
    match t {
        0..=3 => Some(("DATA", format!("tag_skills.{t}"))),
        4..=10 => Some(("ATTR", ATTR[(t - 4) as usize].to_string())),
        _ => None,
    }
}

/// The class flag IDs [documented: GetClassFlag]: 0 Playable and 1 Guard are DATA flags
/// bits 0–1; 2–19 are the service flags from bit 0 (2 weapons … 12 miscellaneous = bit
/// 10, 18 recharge = bit 16, 19 repair = bit 17, as the schema's ServiceFlags names them).
fn class_flag(id: i64) -> Option<(&'static str, i64)> {
    match id {
        0 | 1 => Some(("flags", id)),
        2..=19 => Some(("services", id - 2)),
        _ => None,
    }
}

/// The combat style trait IDs 0–63 [documented: GetCombatStyleTraitNumeric, the names]:
/// CSSD, CSTD and CSAD in the record's field order. For 43–63 the order is the record's,
/// not the page's, which lists three base/mult pairs base first [observed: jip4 probe set,
/// FollowersCombatStyleRanged 43 reads CSAD's first field, -20].
const CSTY_TRAITS: [(&str, &str); 64] = [
    ("CSSD", "cover_search_radius"),
    ("CSSD", "take_cover_chance"),
    ("CSSD", "wait_timer_min"),
    ("CSSD", "wait_timer_max"),
    ("CSSD", "wait_to_fire_timer_min"),
    ("CSSD", "wait_to_fire_timer_max"),
    ("CSSD", "fire_timer_min"),
    ("CSSD", "fire_timer_max"),
    ("CSSD", "ranged_weapon_range_mult_min"),
    ("CSSD", "weapon_restrictions"),
    ("CSSD", "ranged_weapon_range_mult_max"),
    ("CSSD", "max_targeting_fov"),
    ("CSSD", "combat_radius"),
    ("CSSD", "semi_auto_firing_delay_mult_min"),
    ("CSSD", "semi_auto_firing_delay_mult_max"),
    ("CSTD", "dodge_chance"),
    ("CSTD", "left_right_chance"),
    ("CSTD", "dodge_lr_timer_min"),
    ("CSTD", "dodge_lr_timer_max"),
    ("CSTD", "dodge_forward_timer_min"),
    ("CSTD", "dodge_forward_timer_max"),
    ("CSTD", "dodge_back_timer_min"),
    ("CSTD", "dodge_back_timer_max"),
    ("CSTD", "idle_timer_min"),
    ("CSTD", "idle_timer_max"),
    ("CSTD", "block_chance"),
    ("CSTD", "attack_chance"),
    ("CSTD", "recoil_stagger_bonus_to_attack"),
    ("CSTD", "unconscious_bonus_to_attack"),
    ("CSTD", "hand_to_hand_bonus_to_attack"),
    ("CSTD", "power_attack_chance"),
    ("CSTD", "recoil_stagger_bonus_to_power"),
    ("CSTD", "unconscious_bonus_to_power_attack"),
    ("CSTD", "power_attack_normal"),
    ("CSTD", "power_attack_forward"),
    ("CSTD", "power_attack_back"),
    ("CSTD", "power_attack_left"),
    ("CSTD", "power_attack_right"),
    ("CSTD", "hold_timer_min"),
    ("CSTD", "hold_timer_max"),
    ("CSTD", "acrobatic_dodge_chance"),
    ("CSTD", "rushing_attack_chance"),
    ("CSTD", "rushing_attack_distance_mult"),
    ("CSAD", "dodge_fatigue_mod_mult"),
    ("CSAD", "dodge_fatigue_mod_base"),
    ("CSAD", "encumbered_speed_mod_base"),
    ("CSAD", "encumbered_speed_mod_mult"),
    ("CSAD", "dodge_while_under_attack_mult"),
    ("CSAD", "dodge_not_under_attack_mult"),
    ("CSAD", "dodge_back_while_under_attack_mult"),
    ("CSAD", "dodge_back_not_under_attack_mult"),
    ("CSAD", "dodge_forward_while_attacking_mult"),
    ("CSAD", "dodge_forward_not_attacking_mult"),
    ("CSAD", "block_skill_modifier_mult"),
    ("CSAD", "block_skill_modifier_base"),
    ("CSAD", "block_while_under_attack_mult"),
    ("CSAD", "block_not_under_attack_mult"),
    ("CSAD", "attack_skill_modifier_mult"),
    ("CSAD", "attack_skill_modifier_base"),
    ("CSAD", "attack_while_under_attack_mult"),
    ("CSAD", "attack_not_under_attack_mult"),
    ("CSAD", "attack_during_block_mult"),
    ("CSAD", "power_attack_fatigue_mod_base"),
    ("CSAD", "power_attack_fatigue_mod_mult"),
];

/// The light trait IDs 0–6 [documented: GetLightTraitNumeric]: radius, red, green, blue,
/// falloff exponent, FOV (DATA) and fade (FNAM).
fn light_trait(t: i64) -> Option<(&'static str, &'static str)> {
    Some(match t {
        0 => ("DATA", "radius"),
        1 => ("DATA", "color.red"),
        2 => ("DATA", "color.green"),
        3 => ("DATA", "color.blue"),
        4 => ("DATA", "falloff_exponent"),
        5 => ("DATA", "fov"),
        6 => ("FNAM", ""),
        _ => return None,
    })
}

/// The grass trait IDs 0–9 [documented: GetGrassTraitNumeric], DATA in order.
const GRASS_TRAITS: [&str; 10] = [
    "density",
    "min_slope",
    "max_slope",
    "unit_from_water_amount",
    "unit_from_water_type",
    "position_range",
    "height_range",
    "color_range",
    "wave_period",
    "flags",
];

/// The climate's timing byte `t` (TNAM), as the trait reads it: the four sun times are
/// kept in 10-minute units and read in minutes; moons are the top two bits of the last
/// byte, the phase length its low six [documented: GetClimateTraitNumeric, the units;
/// the schema's wbClmtTime note; hypothesis: the bit split of the last byte].
fn climate_trait(g: &Game, c: FormId, t: i64) -> f64 {
    const SUN: [&str; 4] = ["sunrise_begin", "sunrise_end", "sunset_begin", "sunset_end"];
    match t {
        0..=3 => get_num(g, c, "TNAM", SUN[t as usize]) * 10.0,
        4 => get_num(g, c, "TNAM", "volatility"),
        5 => ((get_num(g, c, "TNAM", "moons_phase_length") as u64 >> 6) & 3) as f64,
        6 => ((get_num(g, c, "TNAM", "moons_phase_length") as u64) & 0x3F) as f64,
        _ => 0.0,
    }
}

fn set_climate_trait(g: &mut Game, c: FormId, t: i64, v: f64) {
    const SUN: [&str; 4] = ["sunrise_begin", "sunrise_end", "sunset_begin", "sunset_end"];
    match t {
        0..=3 => set(g, c, "TNAM", SUN[t as usize], XVal::Num((v / 10.0).trunc().clamp(0.0, 255.0))),
        4 => set(g, c, "TNAM", "volatility", XVal::Num(v.trunc().clamp(0.0, 255.0))),
        5 | 6 => {
            let now = get_num(g, c, "TNAM", "moons_phase_length") as u64;
            let new = if t == 5 { (now & 0x3F) | (((v as u64) & 3) << 6) } else { (now & 0xC0) | ((v as u64) & 0x3F) };
            set(g, c, "TNAM", "moons_phase_length", XVal::Num(new as f64));
        }
        _ => {}
    }
}

impl Game<'_> {
    /// A quest's record flag byte, with SetQuestFlag's changes.
    fn jip4_quest_bits(&self, q: FormId) -> u8 {
        self.gaps.jip4.quest_flags.get(&q).copied().unwrap_or_else(|| self.data.quests.get(&q).map_or(0, |d| d.flags))
    }

    /// GetQuestFlag's IDs 0–7 [documented: GetQuestFlag]: 0 running, 1 completed and 6
    /// failed are the quest's state; 2 and 3 are the record's DATA bits of the same
    /// number; 4 (set running on reset) is DATA bit 0, "Start Game Enabled" [documented:
    /// the page's note, Start Game Enabled is flags 0 and 4]; 5 (shown in the Pip-Boy) a
    /// named quest that runs or ran [hypothesis]; 7 unknown, kept as set.
    fn jip4_quest_flag(&self, q: FormId, id: i64) -> bool {
        let s = self.quests.get(&q);
        let bits = self.jip4_quest_bits(q);
        match id {
            0 => s.is_some_and(|s| s.running),
            1 => s.is_some_and(|s| s.completed),
            6 => s.is_some_and(|s| s.failed),
            2 | 3 => bits & (1 << id) != 0,
            4 => bits & 1 != 0,
            5 if self.gaps.jip4.quest_flags.get(&q).is_some_and(|b| b & 0x20 != 0) => true,
            5 => self.data.quests.get(&q).is_some_and(|d| d.name.as_deref().is_some_and(|n| !n.is_empty())) && s.is_some_and(|s| s.running || s.completed),
            7 => bits & 0x80 != 0,
            _ => false,
        }
    }

    fn jip4_set_quest_flag(&mut self, q: FormId, id: i64, on: bool) {
        match id {
            0 => self.quest(q).running = on,
            1 => self.quest(q).completed = on,
            6 => self.quest(q).failed = on,
            2..=5 | 7 => {
                let bit = if id == 4 { 1u8 } else { 1u8 << id };
                let now = self.jip4_quest_bits(q);
                let new = if on { now | bit } else { now & !bit };
                self.gaps.jip4.quest_flags.insert(q, new);
            }
            _ => {}
        }
    }

    /// GetPCDetectionState [documented: its page, 0 hidden, 1 caution, 2 detected, 3
    /// danger]: the highest band any actor has the player in (`detection::Detections`);
    /// detected by an actor fighting the player is danger [hypothesis: the split of 2 and
    /// 3]; no measure reads 0.
    fn jip4_pc_detection(&self) -> f64 {
        use crate::detection::Band;
        match self.detection.highest(FormId::PLAYER_REF) {
            None => 0.0,
            Some((_, d)) if d.band == Band::Hidden => 0.0,
            Some((_, d)) if d.band == Band::Noticed => 1.0,
            Some((who, _)) if self.combat_target_of(who) == Some(FormId::PLAYER_REF) => 3.0,
            Some(_) => 2.0,
        }
    }

    /// An effect from a poison on the actor: an active effect whose source is an
    /// ingestible all of whose effects are hostile and detrimental (GetIsPoison's rule,
    /// `nvse_gaps`) [documented: GetIsPoisoned, "under the influence of a poison-type
    /// effect"; hypothesis: the rule].
    fn jip4_poisoned(&self, a: FormId) -> bool {
        self.effects.iter().filter(|e| e.target == a).any(|e| {
            self.data.sig(e.source) == Some(*b"ALCH") && {
                let list = self.data.effect_items.get(&e.source).map(Vec::as_slice).unwrap_or_default();
                !list.is_empty() && list.iter().all(|x| self.data.magic_effects.get(&x.effect).is_some_and(|m| m.flags & 0x5 == 0x5))
            }
        })
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let f0 = form(args, 0).unwrap_or(FormId(0));
    let k = num(args, 1, -1.0) as i64;
    let v = match name {
        // Classes [documented: GetClassFlag, SetClassFlag, GetClassTraitNumeric,
        // SetClassTraitNumeric].
        "getclassflag" => yes(class_flag(k).is_some_and(|(p, b)| get_bit(g, f0, "DATA", p, b))),
        "setclassflag" => {
            if let Some((p, b)) = class_flag(k) {
                set_bit(g, f0, "DATA", p, b, num(args, 2, 0.0) != 0.0);
            }
            zero
        }
        "getclasstraitnumeric" => XVal::Num(class_trait(k).map_or(0.0, |(s, p)| get_num(g, f0, s, &p))),
        "setclasstraitnumeric" => {
            let v = num(args, 2, 0.0).trunc();
            if let Some((s, p)) = class_trait(k) {
                set(g, f0, s, &p, XVal::Num(v));
            }
            XVal::Num(v)
        }
        // Combat styles [documented: Get/SetCombatStyleFlag, the flag IDs are CSTD flags'
        // bits as the schema names them; Get/SetCombatStyleTraitNumeric].
        "getcombatstyleflag" => yes(get_bit(g, f0, "CSTD", "flags", k)),
        "setcombatstyleflag" => {
            set_bit(g, f0, "CSTD", "flags", k, num(args, 2, 0.0) != 0.0);
            zero
        }
        "getcombatstyletraitnumeric" => XVal::Num(usize::try_from(k).ok().and_then(|i| CSTY_TRAITS.get(i)).map_or(0.0, |(s, p)| get_num(g, f0, s, p))),
        "setcombatstyletraitnumeric" => {
            if let Some((s, p)) = usize::try_from(k).ok().and_then(|i| CSTY_TRAITS.get(i)) {
                set(g, f0, s, p, XVal::Num(num(args, 2, 0.0)));
            }
            zero
        }
        // Lights [documented: Get/SetLightFlag, the IDs are DATA flags' bits;
        // Get/SetLightTraitNumeric].
        "getlightflag" => yes(get_bit(g, f0, "DATA", "flags", k)),
        "setlightflag" => {
            set_bit(g, f0, "DATA", "flags", k, num(args, 2, 0.0) != 0.0);
            zero
        }
        "getlighttraitnumeric" => XVal::Num(light_trait(k).map_or(0.0, |(s, p)| get_num(g, f0, s, p))),
        "setlighttraitnumeric" => {
            if let Some((s, p)) = light_trait(k) {
                set(g, f0, s, p, XVal::Num(num(args, 2, 0.0)));
            }
            zero
        }
        // Grass [documented: Get/SetGrassTraitNumeric; Get/SetGrassModel].
        "getgrasstraitnumeric" => XVal::Num(usize::try_from(k).ok().and_then(|i| GRASS_TRAITS.get(i)).map_or(0.0, |p| get_num(g, f0, "DATA", p))),
        "setgrasstraitnumeric" => {
            if let Some(p) = usize::try_from(k).ok().and_then(|i| GRASS_TRAITS.get(i)) {
                set(g, f0, "DATA", p, XVal::Num(num(args, 2, 0.0)));
            }
            zero
        }
        "getgrassmodel" => XVal::Str(get_str(g, f0, "MODL", "")),
        "setgrassmodel" => {
            set(g, f0, "MODL", "", XVal::Str(text(args, 1)));
            zero
        }
        // Texture sets: map IDs 0–5 are TX00–TX05 [documented: GetTextureSetTexture,
        // SetTextureSetTexture; the schema's TXST].
        "gettexturesettexture" => XVal::Str(if (0..=5).contains(&k) { get_str(g, f0, &format!("TX0{k}"), "") } else { String::new() }),
        "settexturesettexture" => {
            if (0..=5).contains(&k) {
                set(g, f0, &format!("TX0{k}"), "", XVal::Str(text(args, 2)));
            }
            zero
        }
        // Climates [documented: Get/SetClimateTraitNumeric].
        "getclimatetraitnumeric" => XVal::Num(climate_trait(g, f0, k)),
        "setclimatetraitnumeric" => {
            set_climate_trait(g, f0, k, num(args, 2, 0.0));
            zero
        }
        // Quests [documented: Get/SetQuestFlag].
        "getquestflag" => yes(g.jip4_quest_flag(f0, k)),
        "setquestflag" => {
            g.jip4_set_quest_flag(f0, k, num(args, 2, 0.0) != 0.0);
            zero
        }
        // Factions: the reputation (WMI1) [documented: Get/SetFactionReputationType].
        "getfactionreputationtype" => XVal::Ref(get_form(g, f0, "WMI1", "")),
        "setfactionreputationtype" => {
            set(g, f0, "WMI1", "", XVal::Ref(form(args, 1).map_or(0, |r| r.0)));
            zero
        }
        // Cells: climate (XCCM), lighting template (LTMP), water noise texture (XNAM)
        // [documented: GetCellClimate, GetCellLightingTemplate, GetCellNoiseTexture and
        // their setters; the fields, the schema's CELL].
        "getcellclimate" => XVal::Ref(get_form(g, f0, "XCCM", "")),
        "getcelllightingtemplate" => XVal::Ref(get_form(g, f0, "LTMP", "")),
        "getcellnoisetexture" => XVal::Str(get_str(g, f0, "XNAM", "")),
        "setcellclimate" | "setcelllightingtemplate" => {
            let sub = if name == "setcellclimate" { "XCCM" } else { "LTMP" };
            set(g, f0, sub, "", XVal::Ref(form(args, 1).map_or(0, |r| r.0)));
            zero
        }
        "setcellnoisetexture" => {
            set(g, f0, "XNAM", "", XVal::Str(text(args, 1)));
            zero
        }
        // Water types: the open sound (SNAM) and the noise map (NNAM) [documented:
        // GetWaterSound, GetWaterNoiseTexture and their setters].
        "getwatersound" => XVal::Ref(get_form(g, f0, "SNAM", "")),
        "getwaternoisetexture" => XVal::Str(get_str(g, f0, "NNAM", "")),
        "setwatersound" => {
            set(g, f0, "SNAM", "", XVal::Ref(form(args, 1).map_or(0, |r| r.0)));
            zero
        }
        "setwaternoisetexture" => {
            set(g, f0, "NNAM", "", XVal::Str(text(args, 1)));
            zero
        }
        // Sounds: the IDs are SNDD flags' bits (SNDX in older records) [documented:
        // GetSoundFlag; the schema's SOUN].
        "getsoundflag" => {
            let sub = if json(g, f0, "SNDD", "").is_some() { "SNDD" } else { "SNDX" };
            yes(get_bit(g, f0, sub, "flags", k))
        }
        // [documented: GetGameDifficulty, SetGameDifficulty: 0 very easy … 4 very hard].
        "getgamedifficulty" => XVal::Num(g.difficulty.code() as f64),
        "setgamedifficulty" => {
            if let Some(d) = crate::difficulty::Difficulty::from_code(num(args, 0, -1.0) as i64) {
                g.difficulty = d;
            }
            zero
        }
        // Seconds since this run of the game began, menus included [documented:
        // GetSessionTime; hypothesis: a loaded save does not start a new session]: each
        // frame's own length added up (`Game::time_real`), since the engine changes
        // `frame_seconds` every frame and the frame count times it jumps (#1297).
        "getsessiontime" => XVal::Num(g.time_real()),
        "getpcdetectionstate" => XVal::Num(g.jip4_pc_detection()),
        // The type code of the reference itself (REFR, ACHR, ACRE …), not its base
        // [documented: GetRefType; the codes: docs/spec-notes/ext-forms.md].
        "getreftype" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            // As the script host names a reference's type: the player is an ACHR, an
            // inventory reference or a reference made at run time a REFR (an actor's by
            // its base).
            let r = g.unclone(r);
            let sig = if r == FormId::PLAYER_REF {
                Some(*b"ACHR")
            } else if g.temp_item(r).is_some() {
                Some(*b"REFR")
            } else {
                g.data.sig(r).or_else(|| {
                    let base = g.world.peek(r)?.base;
                    Some(match g.data.sig(base) {
                        Some(s) if &s == b"NPC_" => *b"ACHR",
                        Some(s) if &s == b"CREA" => *b"ACRE",
                        _ => *b"REFR",
                    })
                })
            };
            XVal::Num(sig.and_then(|s| fnv_script::vm::form_type_code(&s)).map_or(0.0, f64::from))
        }
        // The door at the other end, SetDoorTeleport's change first [documented:
        // GetTeleportDoor; the same reading as GetLinkedDoor].
        "getteleportdoor" => XVal::Ref(subject.and_then(|d| g.door_teleport(d)).map_or(0, |t| t.0 .0)),
        // `quest`: seconds until the quest's script runs again, going down each frame and
        // below 0 for a moment before the script runs [documented: GetDelayElapsed;
        // observed: jip4 probe set, VDeadSea read 15.6 ms less each frame, VCG01 once < 0].
        "getdelayelapsed" => {
            let delay = g.quests.get(&f0).and_then(|s| s.delay).or_else(|| g.data.quests.get(&f0).map(|d| d.script_delay())).unwrap_or(0.0);
            let since = g.quests.get(&f0).map_or(0.0, |s| s.since_run);
            XVal::Num(delay - since)
        }
        // `global [form]`: a form kept in a global's own value, its float's bits being the
        // form ID [documented: GetGlobalRef, SetGlobalRef; observed: jip4 probe set,
        // GameYear reads 1.49665e-39 after SetGlobalRef of 00104C0F, and GetGlobalRef of
        // 2281.0 reads 0]. GetGlobalRef reads 0 when the bits name no form.
        "getglobalref" => {
            let id = g.world.global(f0).map_or(0, f32::to_bits);
            let known = id != 0 && (FormId(id) == FormId::PLAYER_REF || g.data.sig(FormId(id)).is_some() || g.world.peek(FormId(id)).is_some());
            XVal::Ref(if known { id } else { 0 })
        }
        "setglobalref" => {
            if f0.0 != 0 {
                let r = form(args, 1).or(subject).map_or(0, |r| r.0);
                let _ = g.world.set_global(f0, f32::from_bits(r));
            }
            zero
        }
        // The cell, place and angles (degrees) the reference has in the plugins; 0 for a
        // reference made in the game [documented: GetEditorPosition; hypothesis: the
        // angles in degrees, as GetAngle].
        "geteditorposition" => {
            let p = subject.and_then(|r| g.world.placed.get(r)).map(|p| (p.cell, p.pos, p.rot));
            let (cell, pos, rot) = p.unwrap_or((None, [0.0; 3], [0.0; 3]));
            let d = |a: f32| f64::from(a.to_degrees());
            let outs = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]), d(rot[0]), d(rot[1]), d(rot[2])];
            let ret = XVal::Ref(cell.map_or(0, |c| c.0));
            let entries = std::iter::once(ret).chain(outs.iter().map(|v| XVal::Num(*v))).enumerate().map(|(i, v)| (fnv_script::vm::XKey::Num(i as f64), v)).collect();
            XVal::Array(g.ext.new_array(fnv_script::vm::ArrayKind::List, entries))
        }
        "getispoisoned" => yes(subject.is_some_and(|a| g.jip4_poisoned(a))),
        _ => return None,
    };
    Some(Ok(v))
}
