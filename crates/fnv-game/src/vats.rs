//! VATS mode as the scripts see it (docs/design/vats-mode.md §12). NVForever's VATS mode
//! is its own Dead Eye-style mark and fire, not the original's VATS [design]; the engine
//! (`nvf-play` `vats_mode`) tells the runtime each frame which state it is in and which
//! marked shot is leaving ([`Game::set_vats`]), and the VATS functions answer from that:
//!
//! - **GetVATSMode** [documented: GECK wiki GetVATSMode, return values 0 none, 1 target
//!   select, 2 menu, 3 scan, 4 playback]: 0 off, 1 while marking, 4 while the marked
//!   shots are fired [design: the nearest of the original's views; 2 and 3 never].
//! - **GetVATSValue** [documented: GECK wiki GetVATSValue, its list of queries]: about the
//!   shot leaving now while firing, 0 otherwise. The query codes are the page's order
//!   from 0 with Weapon Type Is moved last [hypothesis; observed: the parameters the
//!   masters' conditions give each code, docs/spec-notes/perk-entry-points.md §4]; what
//!   each returns is in [`get_vats_value`].
//!
//! - **GetVATSTargetHeight** [documented: GECK wiki, "the delta z from the player to the
//!   targeted limb or bounding volume center point"]: the point of the shot leaving now
//!   less the player's feet [hypothesis: from the feet; the masters' conditions pick the
//!   VATS melee "Low" attacks below 30 to 50 units], 0 otherwise.
//! - **SetVatsTarget** [documented: GECK wiki, "turns VATS targeting on or off for a
//!   destructible object, overriding the object's base setting"]: kept on the reference
//!   (saved); VATS mode marks a placed object when [`Game::vats_targetable`] says so
//!   (vats-mode.md §20).
//!
//! Not part of the saved state: the mode is never saved (vats-mode.md §11.7).

use crate::functions::{base_of, list_members, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;

/// GetVATSMode's values for VATS mode's states.
pub const MODE_OFF: u8 = 0;
pub const MODE_TARGET_SELECT: u8 = 1;
pub const MODE_PLAYBACK: u8 = 4;

/// The marked shot leaving now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VatsShot {
    pub weapon: FormId,
    /// The marked actor's reference.
    pub target: FormId,
    /// From the player to the target, game units.
    pub distance: f64,
    /// The marked body part's condition actor value (25–31; its BPTD part's Actor
    /// Value), `None` for a whole actor or a part without one.
    pub condition: Option<u16>,
    /// The shot's hit roll (vats-mode.md §17.4): GetVATSValue Is Success. False while a
    /// mark's chance is worked out, before any roll.
    pub success: bool,
    /// The marked point's height over the player's feet, game units (GetVATSTargetHeight).
    pub height: f64,
}

/// How a shot came out, once it has landed (a kill camera's choice,
/// `crate::camera_paths`): GetVATSValue's Is Critical, Is Fatal, Explode Part,
/// Dismember Part and Cripple Part.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VatsOutcome {
    pub critical: bool,
    pub fatal: bool,
    pub explode: bool,
    pub dismember: bool,
    pub cripple: bool,
}

/// The room about the attacker and the target, game units, for the VATS area functions
/// (camera paths): `[right, left, back, front]` of the line from the attacker to the
/// target, measured by the engine (vats-mode.md §19.3) [hypothesis: what the functions
/// measure; xEdit names them only].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraRoom {
    /// How far the space is free beside the attacker (GetVATS…AreaFree).
    pub area_free: [f32; 4],
    /// How far from the target a camera on each side still sees it (GetVATS…TargetVisible).
    pub target_visible: [f32; 4],
}

/// What the engine says of VATS mode.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VatsState {
    pub mode: u8,
    pub shot: Option<VatsShot>,
    /// Known only while a kill camera is chosen (`crate::camera_paths`).
    pub outcome: Option<VatsOutcome>,
    pub room: Option<CameraRoom>,
}

impl Game<'_> {
    /// The engine's VATS mode state and the shot leaving now (only while firing).
    pub fn set_vats(&mut self, mode: u8, shot: Option<VatsShot>) {
        self.vats = VatsState { mode, shot: shot.filter(|_| mode == MODE_PLAYBACK), outcome: None, room: None };
    }
}

/// The VATS area condition functions, by condition index [documented: xEdit
/// wbDefinitionsFNV.pas wbCTDAFunctions 518–521 GetVATSRightAreaFree, …LeftAreaFree,
/// …BackAreaFree, …FrontAreaFree and 525–528 GetVATSRightTargetVisible, …Left…, …Back…,
/// …Front…, each with a reference parameter]. The catalogue has no page for them; only
/// the camera paths use them [observed: FalloutNV.esm's CPTH conditions, compared with
/// 40 to 320, so they give distances (hypothesis)].
pub const RIGHT_AREA_FREE: u16 = 518;
pub const FRONT_AREA_FREE: u16 = 521;
pub const RIGHT_TARGET_VISIBLE: u16 = 525;
pub const FRONT_TARGET_VISIBLE: u16 = 528;

/// Whether condition index `f` is one of the VATS area functions.
pub fn is_area_function(f: u16) -> bool {
    (RIGHT_AREA_FREE..=FRONT_AREA_FREE).contains(&f) || (RIGHT_TARGET_VISIBLE..=FRONT_TARGET_VISIBLE).contains(&f)
}

/// A VATS area function's value: the room measured for the camera choice, 0 otherwise.
pub fn area_value(g: &Game, f: u16) -> f64 {
    let Some(r) = g.vats.room else { return 0.0 };
    let v = if (RIGHT_AREA_FREE..=FRONT_AREA_FREE).contains(&f) {
        r.area_free[(f - RIGHT_AREA_FREE) as usize]
    } else if (RIGHT_TARGET_VISIBLE..=FRONT_TARGET_VISIBLE).contains(&f) {
        r.target_visible[(f - RIGHT_TARGET_VISIBLE) as usize]
    } else {
        0.0
    };
    v as f64
}

/// "Has Concentrated Fire" [documented: GECK wiki GetNthPerkEntryFunction, code 33; Entry
/// Point, "run when the player enters VATS; if non-zero the player will be able to use
/// concentrated fire"; observed: Concentrated Fire's one entry, Set 1].
pub const HAS_CONCENTRATED_FIRE: u8 = 33;

/// VATS mode's to-hit perks for one mark (vats-mode.md §17.3).
#[derive(Debug, Clone, Default)]
pub struct HitPerks {
    /// The Calculate To Hit Chance entries that apply to this mark, in order.
    pub ops: Vec<(FormId, crate::perks::Op)>,
    /// The player has Concentrated Fire.
    pub concentrated_fire: bool,
}

impl HitPerks {
    /// The chance after the entries.
    pub fn apply(&self, v: f64) -> f64 {
        self.ops.iter().fold(v, |v, (_, op)| op.apply(v))
    }
}

impl Game<'_> {
    /// The player's to-hit perks for a mark: the Calculate To Hit Chance entries with the
    /// tabs Weapon and Target [documented: GECK wiki GetPerkModifier, the entry functions'
    /// arguments; observed: Trigger Discipline tests IsWeaponSkillType on tab 1, Sneering
    /// Imperialist GetInFaction on tab 2], their GetVATSValue conditions (Sniper's head,
    /// Hobbler's legs) seeing `shot` as the shot about to leave and GetVATSMode the mode
    /// as it is; and Has Concentrated Fire.
    pub fn vats_hit_perks(&mut self, shot: VatsShot) -> HitPerks {
        let was = self.vats;
        self.vats = VatsState { mode: was.mode, shot: Some(shot), outcome: None, room: None };
        let ops = self.entry_ops(FormId::PLAYER_REF, crate::perks::CALCULATE_TO_HIT_CHANCE, &[Some(shot.weapon), Some(shot.target)], Some(shot.target));
        let concentrated_fire = self.entry_point(FormId::PLAYER_REF, HAS_CONCENTRATED_FIRE, 0.0).0 != 0.0;
        self.vats = was;
        HitPerks { ops, concentrated_fire }
    }
}

pub fn get_vats_mode(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(g.vats.mode as f64))
}

/// `GetVATSTargetHeight` (module notes): the height of the shot leaving now over the
/// player's feet, 0 when none leaves.
pub fn get_vats_target_height(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(g.vats.shot.map_or(0.0, |s| s.height)))
}

/// `ref.SetVatsTarget flag` (module notes).
pub fn set_vats_target(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let on = a.num_or(0, 0.0) != 0.0;
    g.extra.entry(r).or_default().vats_target = Some(on);
    tracing::info!("vats: {:08X} VATS targeting {}", r.0, if on { "on" } else { "off" });
    Ok(Value::ZERO)
}

impl Game<'_> {
    /// Whether VATS mode may mark the placed object `r`: SetVatsTarget's setting, else its
    /// base's DEST flag "VATS Targetable"; never once it is destroyed. Actors are marked
    /// by VATS mode's own rules, not this.
    pub fn vats_targetable(&self, r: FormId) -> bool {
        let extra = self.extra.get(&r);
        if extra.is_some_and(|e| e.destroyed) {
            return false;
        }
        extra.and_then(|e| e.vats_target).unwrap_or_else(|| base_of(self, r).and_then(|b| self.destruction(b)).is_some_and(|d| d.vats_targetable))
    }
}

/// The query's parameter as a form: a reference value from a script, or the raw FormID a
/// condition's `ptVATSValueParam` carries as an integer (`crate::cond`), taken as a
/// load-order FormID [hypothesis: right for FalloutNV.esm's records, whose local and
/// load-order FormIDs agree].
fn param_form(a: &Args) -> Option<FormId> {
    a.peek_form(1).or_else(|| match a.get(1)? {
        ArgValue::Value(v) => Some(FormId(v.num() as i64 as u32)).filter(|f| f.0 != 0),
        ArgValue::Enum(c) => Some(FormId(*c as u32)),
        _ => None,
    })
}

/// The query's parameter as a number (an animation type, an actor value, an action).
fn param_num(a: &Args) -> Option<i64> {
    match a.get(1)? {
        ArgValue::Enum(c) => Some(*c as i64),
        ArgValue::Value(v) => Some(v.num() as i64),
        _ => None,
    }
}

/// GetVATSValue `query` [`parameter`], while a marked shot leaves (0 otherwise):
///
/// | Code | Query | Returns |
/// |---|---|---|
/// | 0 | WeaponIs | 1 if the shot's weapon is the parameter |
/// | 1 | Weapon In List | 1 if the weapon is in the list |
/// | 2 | Target Is | 1 if the target, or its base, is the parameter |
/// | 3 | Target In List | 1 if the target or its base is in the list |
/// | 4 | Target Distance | units from the player to the target |
/// | 5 | Targeted Part | 1 if the marked part's condition actor value is the parameter |
/// | 6 | VATS Action | 1 for action 0 [hypothesis: Attack is the first action] |
/// | 7 | Is Success | 1 if the shot's hit roll succeeded (vats-mode.md §17.4) |
/// | 8, 11–14 | Is Critical, Is Fatal, Explode, Dismember, Cripple Part | 1 if the landed shot did so, when a kill camera is chosen; 0 before the shot lands [design] |
/// | 9, 10 | Crit Effect Is, Crit Effect In List | 0: critical effects are not told [design] |
/// | 15 | Weapon Type Is | 1 if the weapon's DNAM animation type is the parameter |
///
/// The codes [hypothesis] from the parameters FalloutNV.esm's 248 GetVATSValue conditions
/// give them [observed: 0, 2 and 3 a form; 4, 7, 12–14, 16 and 17 none, 11 none on the
/// camera paths (three idles keep a form there); 6 the numbers 15 or 18; 5 a condition actor
/// value, 25 and 31 on Sniper's head entry and 26 on Center of Mass's torso one; 15 a
/// small number 1–13, an animation type], so Weapon Type Is, third on the wiki page, is
/// not code 2.
pub fn get_vats_value(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let Some(s) = g.vats.shot else { return Ok(Value::ZERO) };
    let query = a.code(0)?;
    let in_list = |g: &Game, list: Option<FormId>, f: &[FormId]| list.is_some_and(|l| list_members(g, l).iter().any(|m| f.contains(m)));
    let target = [Some(s.target), base_of(g, s.target)].into_iter().flatten().collect::<Vec<_>>();
    let b = match query {
        0 => param_form(a) == Some(s.weapon),
        1 => in_list(g, param_form(a), &[s.weapon]),
        2 => param_form(a).is_some_and(|f| target.contains(&f)),
        3 => in_list(g, param_form(a), &target),
        4 => return Ok(Value::Num(s.distance)),
        5 => param_num(a).is_some_and(|c| s.condition.map(i64::from) == Some(c)),
        6 => param_num(a).unwrap_or(0) == 0,
        7 => s.success,
        // Known once the shot has landed, for a kill camera's choice (vats-mode.md §19).
        8 => g.vats.outcome.is_some_and(|o| o.critical),
        11 => g.vats.outcome.is_some_and(|o| o.fatal),
        12 => g.vats.outcome.is_some_and(|o| o.explode),
        13 => g.vats.outcome.is_some_and(|o| o.dismember),
        14 => g.vats.outcome.is_some_and(|o| o.cripple),
        15 => param_num(a).is_some_and(|t| g.data.weapon_anim_types.get(&s.weapon) == Some(&t)),
        _ => false,
    };
    Ok(Value::bool(b))
}

/// "Mysterious Stranger", "Miss Fortune", "VATS HelperChance", "Player Kill AP Reward" and
/// "Action Point Regen" [documented: GECK wiki GetNthPerkEntryFunction, codes 28, 41, 67,
/// 35 and 39; Entry Point, what each changes; observed: Mysterious Stranger's and Miss
/// Fortune's one entry Set 1, Friendly Help's Multiply 2, Grim Reaper's Sprint's Add 20,
/// Nerves of Steel's Multiply 1.2] (docs/design/vats-mode.md §18).
pub const MYSTERIOUS_STRANGER: u8 = 28;
pub const MISS_FORTUNE: u8 = 41;
pub const VATS_HELPER_CHANCE: u8 = 67;
pub const PLAYER_KILL_AP_REWARD: u8 = 35;
pub const ACTION_POINT_REGEN: u8 = 39;

/// The misc statistic "Mysterious Stranger Visits" [observed: its place in the Pip-Boy's
/// list (`crate::misc_stats::NAMES`) and the challenge "He Moves in Mysterious Ways", a
/// Miscellaneous Stat challenge on stat 25 to 15, whose script adds Friendly Help].
pub const STRANGER_VISITS: u16 = 25;

/// A VATS helper (vats-mode.md §18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Helper {
    /// The Mysterious Stranger: `Stranger` (NPC_), his .44 does 9,000 damage [observed].
    Stranger,
    /// Miss Fortune: `MissFortune` (NPC_), her gun's object effect knocks down, paralyses,
    /// cripples or blows up [observed: NVMissFortuneGunSpellScript].
    Fortune,
}

impl Helper {
    /// The NPC_'s Editor ID [observed: FalloutNV.esm].
    pub fn edid(self) -> &'static str {
        match self {
            Helper::Stranger => "Stranger",
            Helper::Fortune => "MissFortune",
        }
    }

    /// The weapon they shoot with [observed: their NPC_ CNTO; MysteriousStranger44Magnum,
    /// 9,000 damage; WeapNVMissFortuneGun, 10 damage and the object effect
    /// VMissFortuneGunObjectEffect].
    pub fn weapon_edid(self) -> &'static str {
        match self {
            Helper::Stranger => "MysteriousStranger44Magnum",
            Helper::Fortune => "WeapNVMissFortuneGun",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Helper::Stranger => "the Mysterious Stranger",
            Helper::Fortune => "Miss Fortune",
        }
    }
}

/// Each helper's odds for one mark (0 when the player lacks the perk or the mark does not
/// qualify).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HelperOdds {
    pub stranger: f64,
    pub fortune: f64,
    /// How far from the player a helper appears, units (`fVATSStrangerDistance`, 164)
    /// [hypothesis: the setting's role, from its name].
    pub distance: f64,
}

/// Which helper a roll `u` in [0, 1) brings: the Stranger on the first share of the line,
/// Miss Fortune on the next, none after [design: one helper a mark].
pub fn pick_helper(odds: HelperOdds, u: f64) -> Option<Helper> {
    if u < odds.stranger {
        Some(Helper::Stranger)
    } else if u < odds.stranger + odds.fortune {
        Some(Helper::Fortune)
    } else {
        None
    }
}

impl Game<'_> {
    /// The helpers' odds for a mark on `target` (vats-mode.md §18.1): with Mysterious
    /// Stranger `fVATSStrangerOdds` (0.1) when the target lives and has at most
    /// `iVATSStrangerMaxHP` (150) health [documented: GECK wiki VATS Settings, "enemy not
    /// dead, but less than 150 hitpoints"]; with Miss Fortune `fVATSFortuneOdds` (0.1) when
    /// it lives [hypothesis: no health limit is documented for her]; both times the VATS
    /// HelperChance entries (Friendly Help × 2).
    pub fn vats_helper_odds(&mut self, target: FormId) -> HelperOdds {
        if self.is_dead(target) {
            return HelperOdds::default();
        }
        let has = |g: &mut Game, point: u8| g.entry_point(FormId::PLAYER_REF, point, 0.0).0 != 0.0;
        let (stranger, fortune) = (has(self, MYSTERIOUS_STRANGER), has(self, MISS_FORTUNE));
        if !stranger && !fortune {
            return HelperOdds::default();
        }
        let mult = self.entry_point(FormId::PLAYER_REF, VATS_HELPER_CHANCE, 1.0).0.max(0.0);
        let health = crate::functions::actor_value_capped(self, target, crate::actor_values::HEALTH);
        let max_hp = crate::combat::setting(self, "iVATSStrangerMaxHP", 150.0);
        let odds = |g: &Game, name: &str| crate::combat::setting(g, name, 0.1) * mult;
        HelperOdds {
            distance: crate::combat::setting(self, "fVATSStrangerDistance", 164.0),
            stranger: if stranger && health <= max_hp { odds(self, "fVATSStrangerOdds") } else { 0.0 },
            fortune: if fortune { odds(self, "fVATSFortuneOdds") } else { 0.0 },
        }
    }

    /// Bring `helper` into the player's cell at `pos`, turned to `heading` (radians, the
    /// game's Z angle): a new reference of its NPC_, made as PlaceAtMe makes them. A
    /// Stranger's visit counts in Mysterious Stranger Visits (and so towards Friendly
    /// Help). `None` when the load order has no such NPC.
    pub fn summon_vats_helper(&mut self, helper: Helper, pos: [f32; 3], heading: f32) -> Option<FormId> {
        let base = self.data.by_edid(helper.edid())?;
        let cell = self.world.get(FormId::PLAYER_REF).ok()?.cell;
        let r = self.world.place(base, cell, pos, [0.0, 0.0, heading], 1).ok()?;
        crate::functions::objects::note_created(self, r);
        if helper == Helper::Stranger {
            self.count_stat(STRANGER_VISITS, 1.0);
        }
        tracing::info!("vats mode: {} appears ({r})", helper.label());
        Some(r)
    }

    /// The helper leaves: faded out (Disable 1) and deleted (MarkForDelete).
    pub fn dismiss_vats_helper(&mut self, r: FormId) {
        use crate::functions::{Args, Ctx};
        let ctx = Ctx { unit: "vats mode".into(), opcode: 0, on: Some(r), self_ref: Some(r), action_ref: None, quest: None, inst: None, in_condition: false };
        let fade = [ArgValue::Value(Value::Num(1.0))];
        if let Err(e) = crate::functions::refs::disable(self, &ctx, &Args(&fade)) {
            tracing::warn!("vats mode: the helper {r} could not leave: {e}");
        }
        let _ = crate::functions::refs::mark_for_delete(self, &ctx, &Args(&[]));
    }

    /// AP the player gets back when VATS mode's marks are done and it killed someone
    /// (Player Kill AP Reward: from 0; Grim Reaper's Sprint + 20).
    pub fn vats_kill_ap_reward(&mut self) -> f64 {
        self.entry_point(FormId::PLAYER_REF, PLAYER_KILL_AP_REWARD, 0.0).0.max(0.0)
    }

    /// The factor on the player's AP regeneration (Action Point Regen: Nerves of Steel
    /// × 1.2, Atomic! by its rads, Ain't Like That Now × 1.25).
    pub fn ap_regen_mult(&mut self) -> f64 {
        self.entry_point(FormId::PLAYER_REF, ACTION_POINT_REGEN, 1.0).0.max(0.0)
    }

    /// The player's kills so far (people and creatures, the misc statistics).
    pub fn player_kill_count(&self) -> f64 {
        use crate::misc_stats::{CREATURES_KILLED, PEOPLE_KILLED};
        [PEOPLE_KILLED, CREATURES_KILLED].iter().map(|c| self.player.misc_stats.get(c).copied().unwrap_or(0.0)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conditions_raw_formid_or_a_scripts_reference_is_a_form() {
        // A condition's ptVATSValueParam arrives as an integer (crate::cond).
        let raw = [ArgValue::Enum(0), ArgValue::Value(Value::Num(0x0008F216 as f64))];
        assert_eq!(param_form(&Args(&raw)), Some(FormId(0x0008F216)));
        let script = [ArgValue::Enum(0), ArgValue::Value(Value::Ref(0x0008F216))];
        assert_eq!(param_form(&Args(&script)), Some(FormId(0x0008F216)));
        let none = [ArgValue::Enum(0), ArgValue::Value(Value::Num(0.0))];
        assert_eq!(param_form(&Args(&none)), None);
        assert_eq!(param_form(&Args(&[ArgValue::Enum(5)])), None);
        // A code is a parameter of its own kind here, not a misread form (#1122).
        let outer = crate::functions::begin_unread();
        let _ = param_form(&Args(&[ArgValue::Enum(6), ArgValue::Enum(0x14)]));
        assert_eq!(crate::functions::end_unread(outer), []);
    }

    #[test]
    fn one_helper_a_roll_the_stranger_first() {
        let odds = HelperOdds { stranger: 0.1, fortune: 0.2, distance: 164.0 };
        assert_eq!(pick_helper(odds, 0.05), Some(Helper::Stranger));
        assert_eq!(pick_helper(odds, 0.15), Some(Helper::Fortune));
        assert_eq!(pick_helper(odds, 0.35), None);
        assert_eq!(pick_helper(HelperOdds::default(), 0.0), None);
        assert_eq!(pick_helper(HelperOdds { stranger: 0.0, fortune: 0.1, distance: 164.0 }, 0.05), Some(Helper::Fortune));
    }

    #[test]
    fn numbers_come_as_codes_or_values() {
        assert_eq!(param_num(&Args(&[ArgValue::Enum(6), ArgValue::Enum(25)])), Some(25));
        assert_eq!(param_num(&Args(&[ArgValue::Enum(2), ArgValue::Value(Value::Num(3.0))])), Some(3));
        assert_eq!(param_num(&Args(&[ArgValue::Enum(7)])), None);
    }
}
