//! Character creation (docs/spec-notes/chargen.md): the rules of the menus the opening's
//! scripts open — the name (menu 1051, `GetPlayerName`), the Vigor Tester's
//! S.P.E.C.I.A.L. (1074, `ShowLoveTesterMenuParams`), the tag skills (1048,
//! `SetTagSkills`) and the traits (1084, `ShowTraitMenu`) — as plain data the engine
//! draws, and what the player's Done writes into the game.
//!
//! Each sheet starts from the player as the game holds them (so a menu opened again shows
//! what was chosen before) and changes nothing until [`apply`](SpecialSheet::apply).

use crate::actor_values::{self as av, BARTER, BIG_GUNS, SKILLS, SPECIALS, STRENGTH};
use crate::cond::{Cmp, Condition, Operand};
use crate::functions::actor;
use crate::game::Game;
use fnv_script::vm::ArgValue;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The S.P.E.C.I.A.L. points a new character has in all: seven attributes at 5 and 5 to
/// spend [observed: the owner's screenshot of the Vigor Tester, all seven at 5 with "05
/// POINTS REMAINING"; the owner: "all start with default of 5 with an additional 5"].
pub const SPECIAL_POINTS: i32 = 40;
/// The range of an attribute in the menu [observed: the owner, "no lower than 1 or higher
/// than 10"; documented: GECK wiki GetThresholdedActorValue, 1–10].
pub const SPECIAL_MIN: i32 = 1;
pub const SPECIAL_MAX: i32 = 10;
/// The skills a character tags [observed: "SKILLS: 2/3 Selected" on the owner's
/// screenshot, Done dimmed until 3/3; the owner: "3 skills mandatory"].
pub const TAG_COUNT: usize = 3;
/// The traits a character may take when the game has no `iTraitMenuMaxNumTraits`
/// [observed: the masters set it to 2].
pub const DEFAULT_MAX_TRAITS: usize = 2;
/// The name the name menu offers when the game has no `sDefaultPlayerName` [observed: the
/// masters' value, and "Courier" in the owner's screenshot of the menu].
pub const DEFAULT_NAME: &str = "Courier";
/// The longest name the name menu takes [hypothesis: the original's limit is not
/// captured; long enough for any name the box shows on one line].
pub const NAME_MAX_CHARS: usize = 24;

/// A string game setting.
fn setting_str(g: &Game, name: &str) -> Option<String> {
    match g.world.setting_value(name)? {
        fnv_sim::settings::Setting::Str(s) => Some(s.clone()),
        _ => None,
    }
}

// ---- the name ----------------------------------------------------------------------------

/// The name the name menu starts with: the player's, else the game's default.
pub fn name_offered(g: &Game) -> String {
    g.player.name.clone().unwrap_or_else(|| setting_str(g, "sDefaultPlayerName").unwrap_or_else(|| DEFAULT_NAME.to_string()))
}

/// A typed name as the menu keeps it: printable characters only, at most
/// [`NAME_MAX_CHARS`].
pub fn clean_name(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(NAME_MAX_CHARS).collect()
}

/// Whether Ok takes the name: not empty once the spaces around it are taken off
/// [hypothesis: not captured].
pub fn name_acceptable(s: &str) -> bool {
    !s.trim().is_empty()
}

/// Done in the name menu: the player is called `name` (spaces around it taken off).
pub fn apply_name(g: &mut Game, name: &str) {
    g.player.name = Some(clean_name(name).trim().to_string());
}

// ---- the look (the race menu) ------------------------------------------------------------

/// What the character creator sets in place of the player record's appearance
/// (docs/spec-notes/character-creator.md): race, sex, hair (`None` is bald), hair colour,
/// eyes, head parts (beard, brows) and the FaceGen values (50 symmetric shape, 30
/// asymmetric shape, 50 texture; the player's own, the race's are added when drawn).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Look {
    pub race: FormId,
    pub female: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hair: Option<FormId>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hair_color: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub eyes: Option<FormId>,
    #[serde(default)]
    pub head_parts: Vec<FormId>,
    #[serde(default)]
    pub fggs: Vec<f32>,
    #[serde(default)]
    pub fgga: Vec<f32>,
    #[serde(default)]
    pub fgts: Vec<f32>,
}

/// Done in the race menu: the player looks as `look` says, and is of its sex
/// (GetIsSex, GetPCIsSex; GetIsRace reads the look's race).
pub fn apply_look(g: &mut Game, look: Look) {
    g.player.female = Some(look.female);
    // The look's race stands for an AgeRace made before it (`crate::appearance`).
    g.player.race = None;
    g.player.look = Some(look);
}

// ---- S.P.E.C.I.A.L. ----------------------------------------------------------------------

/// The Vigor Tester's seven attributes, Strength to Luck, with the points the menu has
/// to give out and how low each may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecialSheet {
    pub values: [i32; 7],
    /// The points the seven add up to when the menu may be left: [`SPECIAL_POINTS`],
    /// or what AddSPECIALPoints / SetSPECIALPoints asked for.
    pub total: i32,
    /// The lowest each attribute may be lowered to: [`SPECIAL_MIN`], or the value it
    /// had before AddSPECIALPoints opened the menu.
    pub floor: [i32; 7],
}

impl SpecialSheet {
    /// A sheet with these values, the character-creation total and floor.
    pub fn new(values: [i32; 7]) -> SpecialSheet {
        SpecialSheet { values, total: SPECIAL_POINTS, floor: [SPECIAL_MIN; 7] }
    }

    /// The player's attributes as they stand (their permanent values).
    pub fn of_player(g: &mut Game) -> SpecialSheet {
        let mut values = [5; 7];
        for (i, code) in SPECIALS.enumerate() {
            values[i] = actor::actor_value_base(g, FormId::PLAYER_REF, code).round() as i32;
        }
        SpecialSheet::new(values)
    }

    /// The sheet the S.P.E.C.I.A.L. menu opens with: the player's, under the rules a
    /// script asked for when it opened the menu (AddSPECIALPoints, SetSPECIALPoints;
    /// `functions::ui`), which are used up by this opening.
    pub fn for_menu(g: &mut Game) -> SpecialSheet {
        let mut s = SpecialSheet::of_player(g);
        if let Some(m) = g.ui.special.take() {
            let sum: i32 = s.values.iter().sum();
            match m {
                crate::functions::ui::SpecialMenu::Add(n) => {
                    s.total = sum + n;
                    s.floor = s.values;
                }
                crate::functions::ui::SpecialMenu::Set(n) => s.total = n,
            }
        } else {
            // The player's tweak sets the total when no script did [design: docs/design/tweaks.md].
            s.total = g.tweaks.special_points;
        }
        s
    }

    /// Points not given to any attribute yet (below 0 when a script had raised them past
    /// the total).
    pub fn remaining(&self) -> i32 {
        self.total - self.values.iter().sum::<i32>()
    }

    /// One more point for attribute `i` (0 Strength … 6 Luck), when one is left and the
    /// attribute is below 10.
    pub fn raise(&mut self, i: usize) -> bool {
        let ok = self.remaining() > 0 && self.values[i] < SPECIAL_MAX;
        if ok {
            self.values[i] += 1;
        }
        ok
    }

    /// Whether attribute `i` may give a point back: above 1, and above the value it had
    /// when AddSPECIALPoints opened the menu [documented: GECK wiki AddSPECIALPoints, "the
    /// player cannot decrease their character's SPECIAL points past their base values"].
    pub fn can_lower(&self, i: usize) -> bool {
        self.values[i] > self.floor[i].max(SPECIAL_MIN)
    }

    /// One point back from attribute `i` ([`SpecialSheet::can_lower`]).
    pub fn lower(&mut self, i: usize) -> bool {
        let ok = self.can_lower(i);
        if ok {
            self.values[i] -= 1;
        }
        ok
    }

    /// Whether the player may leave the menu: every point given out [observed: the
    /// owner, "all special available to assign must be assigned before continuing"; the
    /// screenshots' Done arrow dimmed at 05 and 02 points remaining].
    pub fn complete(&self) -> bool {
        self.remaining() == 0
    }

    /// Done: each attribute's permanent value becomes the sheet's (the base moves; a
    /// script's modifier stays).
    pub fn apply(&self, g: &mut Game) {
        for (i, code) in SPECIALS.enumerate() {
            let now = actor::actor_value_base(g, FormId::PLAYER_REF, code);
            let d = self.values[i] as f64 - now;
            if d != 0.0 {
                let _ = actor::shift_base(g, FormId::PLAYER_REF, code, d);
            }
        }
    }
}

/// The attribute code of a sheet index.
pub fn special_code(i: usize) -> u16 {
    STRENGTH + i as u16
}

// ---- tag skills --------------------------------------------------------------------------

/// The tag-skill menu: its list of skills and those tagged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSheet {
    /// The skills in the menu's order: every skill but Big Guns, by name [observed: the
    /// owner's screenshot lists the thirteen in alphabetical order; Big Guns' AVIF is
    /// named "Big Guns - OBSOLETE" in the masters].
    pub skills: Vec<u16>,
    pub tagged: BTreeSet<u16>,
    /// How many skills are tagged when the menu may be left: [`TAG_COUNT`], or more
    /// after AddTagSkills.
    pub count: usize,
    /// Skills that may not be untagged (those tagged before AddTagSkills opened the
    /// menu).
    pub locked: BTreeSet<u16>,
}

impl TagSheet {
    /// The list, with the skills the player has tagged now (the psych test's three in the
    /// opening) tagged [observed: layer5-captures.md §10.3, VCG01Test tags three skills
    /// before stage 90 opens the menu].
    pub fn of_player(g: &Game) -> TagSheet {
        let mut skills: Vec<u16> = SKILLS.filter(|&c| c != BIG_GUNS).collect();
        skills.sort_by_key(|&c| skill_name(g, c).to_lowercase());
        let tagged = g.player.tag_skills.iter().copied().filter(|c| skills.contains(c)).collect();
        TagSheet { skills, tagged, count: TAG_COUNT, locked: BTreeSet::new() }
    }

    /// The sheet the tag-skill menu opens with: the player's, with the extra skills to
    /// tag that AddTagSkills asked for (used up by this opening): the skills tagged
    /// before stay tagged and the player picks that many more [documented: GECK wiki
    /// AddTagSkills, "the player may not un-Tag their initial Tagged Skills"].
    pub fn for_menu(g: &mut Game) -> TagSheet {
        let mut s = TagSheet::of_player(g);
        if let Some(n) = g.ui.extra_tags.take() {
            s.count = s.tagged.len() + n;
            s.locked = s.tagged.clone();
        }
        // SetTagSkills n [show]: n skills tagged in all, those tagged before counting
        // ("Assuming the player initially has 3 Tagged Skills, calling SetTagSkills 4 will
        // let the player choose 1 more"); with show 0 the skills tagged before are not
        // shown and may be chosen over [documented: GECK wiki SetTagSkills; hypothesis:
        // they may be untagged either way, as nothing says they are locked].
        if let Some((n, show)) = g.ui.set_tags.take() {
            s.count = n;
            s.locked.clear();
            if !show {
                s.tagged.clear();
            }
        }
        s
    }

    /// Tag an untagged skill while fewer than the count are, or untag a tagged one that
    /// is not locked. Whether anything changed.
    pub fn toggle(&mut self, skill: u16) -> bool {
        if self.locked.contains(&skill) {
            return false;
        }
        if self.tagged.remove(&skill) {
            return true;
        }
        if self.tagged.len() < self.count && self.skills.contains(&skill) {
            self.tagged.insert(skill);
            return true;
        }
        false
    }

    /// Skills still to tag.
    pub fn remaining(&self) -> usize {
        self.count.saturating_sub(self.tagged.len())
    }

    /// Whether Done may be pressed: as many tagged as the count.
    pub fn complete(&self) -> bool {
        self.tagged.len() == self.count
    }

    /// The skill's value as the menu shows it: the player's, with the tag bonus as the
    /// sheet has it [observed: Speech and Survival 35 tagged at Charisma and Endurance 7,
    /// Guns 18 untagged and 33 tagged at Agility 6, Luck 7: 2 + 2 × attribute +
    /// ⌈Luck / 2⌉, plus 15].
    pub fn value(&self, g: &mut Game, skill: u16) -> i32 {
        let bonus = crate::combat::setting(g, "fAVDTagSkillBonus", 15.0);
        let now = actor::actor_value_base(g, FormId::PLAYER_REF, skill);
        let was = if g.player.tag_skills.contains(&skill) { bonus } else { 0.0 };
        let will = if self.tagged.contains(&skill) { bonus } else { 0.0 };
        av::capped(skill, now - was + will).round() as i32
    }

    /// Done: the player's tags become the sheet's.
    pub fn apply(&self, g: &mut Game) {
        g.player.tag_skills.retain(|c| !SKILLS.contains(c));
        g.player.tag_skills.extend(self.tagged.iter().copied());
        g.player.tag_slots.clear();
    }
}

/// A skill's or attribute's name (AVIF FULL), else the code's xEdit name.
pub fn skill_name(g: &Game, code: u16) -> String {
    g.data
        .actor_value_info(code)
        .map(|i| i.name.clone())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| actor::AV_NAMES.get(code as usize).copied().unwrap_or("?").to_string())
}

/// The index of a skill code in code order (Barter 0 … Unarmed 13).
pub fn skill_index(code: u16) -> Option<usize> {
    SKILLS.contains(&code).then(|| (code - BARTER) as usize)
}

// ---- traits ------------------------------------------------------------------------------

/// The trait menu: the traits on offer and those taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitSheet {
    /// Every playable, not hidden PERK with the Trait flag in the load order, by name
    /// [observed: the owner's screenshots list the ten of FalloutNV.esm and the six of
    /// OldWorldBlues.esm in alphabetical order].
    pub traits: Vec<FormId>,
    pub chosen: BTreeSet<FormId>,
    /// `iTraitMenuMaxNumTraits` [observed: 2 in the masters; "CHOOSE UP TO 2 TRAITS (OR
    /// NONE)"].
    pub max: usize,
}

impl TraitSheet {
    pub fn of_player(g: &Game) -> TraitSheet {
        let mut traits: Vec<FormId> = g.data.perks.iter().filter(|(_, p)| p.is_trait && p.playable && !p.hidden).map(|(&id, _)| id).collect();
        traits.sort_by_key(|id| (g.data.perks[id].name.to_lowercase(), *id));
        let chosen = traits.iter().copied().filter(|t| g.player.perks.contains_key(t)).collect();
        let max = g.world.setting_f32("iTraitMenuMaxNumTraits").map_or(DEFAULT_MAX_TRAITS, |v| v.max(0.0) as usize);
        TraitSheet { traits, chosen, max }
    }

    /// Take a trait the player meets the requirements of while fewer than the most are
    /// taken, or give a taken one back. Whether anything changed. [hypothesis: a trait
    /// whose requirements fail cannot be taken; none of the screenshots shows one]
    pub fn toggle(&mut self, t: FormId, allowed: bool) -> bool {
        if self.chosen.remove(&t) {
            return true;
        }
        if allowed && self.chosen.len() < self.max && self.traits.contains(&t) {
            self.chosen.insert(t);
            return true;
        }
        false
    }

    /// Traits that may still be taken ("2 TRAITS LEFT").
    pub fn left(&self) -> usize {
        self.max.saturating_sub(self.chosen.len())
    }

    /// Done: the player has exactly the sheet's traits among those on offer (one rank
    /// each). What a trait does is its perk's effects, which are not modelled yet.
    pub fn apply(&self, g: &mut Game) {
        for t in &self.traits {
            if self.chosen.contains(t) {
                g.player.perks.entry(*t).or_insert(1);
            } else {
                g.player.perks.remove(t);
            }
        }
    }
}

/// Whether the player meets a perk's requirements (a perk without readable conditions
/// is taken to have none).
pub fn requirements_met(g: &mut Game, perk: FormId) -> bool {
    let Some(conds) = g.data.perks.get(&perk).and_then(|p| p.conditions.clone()) else { return true };
    g.conditions_pass(&conds, Some(FormId::PLAYER_REF), None, "trait requirement")
}

/// The function indices (opcode − 0x1000) of the conditions the requirement line names:
/// GetActorValue and GetPermanentActorValue.
const AV_FUNCTIONS: [u16; 2] = [0x0E, 0x1EF];

/// One requirement as the menu writes it: the actor value's short name, the comparison
/// and the number, "PER < 10" [observed: Four Eyes' screenshot], with "at least" left
/// unwritten, "INT 4" for Intelligence ≥ 4 [observed: the level-up menu's "Req: Level 2,
/// INT 4" (Swift Learner), "LCK 6, Repair 45" (Junk Rounds), "Level 8, END 6, STR 6"
/// (Stonewall), each a ≥ condition of the record]. `None` for a condition the line
/// leaves out: one on another function, and a S.P.E.C.I.A.L. "greater than 1"
/// [hypothesis: Four Eyes holds Perception > 1 and < 10 and the screenshot shows only
/// "PER < 10"; why the first is left out is not known].
fn requirement(g: &Game, c: &Condition) -> Option<String> {
    if !AV_FUNCTIONS.contains(&c.function) {
        return None;
    }
    let code = match c.params.first()? {
        ArgValue::Enum(code) => *code,
        other => other.value()?.num() as u16,
    };
    let Operand::Value(n) = c.rhs else { return None };
    if SPECIALS.contains(&code) && c.cmp == Cmp::Gt && n <= SPECIAL_MIN as f64 {
        return None;
    }
    let name = g.data.actor_value_info(code).map(|i| i.short_name.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| skill_name(g, code));
    let op = match c.cmp {
        Cmp::Ge => return Some(format!("{name} {n}")),
        Cmp::Eq => "=",
        Cmp::Ne => "!=",
        Cmp::Gt => ">",
        Cmp::Lt => "<",
        Cmp::Le => "<=",
    };
    Some(format!("{name} {op} {n}"))
}

/// The "Req:" line of a perk or trait [observed: the trait and level-up menus' screenshots,
/// docs/spec-notes/levelup.md §8]:
/// - "Level n" when its least level is over 1 (Junk Rounds, level 1, shows none; the
///   traits are level 1);
/// - then each condition before its first effect, as [`requirement`] writes it, after a
///   comma when anything came before it, even one the line leaves out ("Level 2," for
///   Confirmed Bachelor, whose one condition is on the player's sex; "PER < 10" for Four
///   Eyes, whose first condition is left out);
/// - "--" right after the rest when it has no condition ("Level 2--", Intense Training;
///   "--", Early Bird).
///
/// Conditions joined with OR are written with "or" between them [hypothesis: no
/// screenshot shows one].
pub fn requirement_line(g: &Game, perk: FormId) -> String {
    let p = g.data.perks.get(&perk);
    let mut line = match p {
        Some(p) if p.min_level > 1 => format!("Level {}", p.min_level),
        _ => String::new(),
    };
    let conds: &[Condition] = p.and_then(|p| p.conditions.as_deref()).unwrap_or_default();
    if conds.is_empty() {
        line.push_str("--");
        return line;
    }
    for (k, c) in conds.iter().enumerate() {
        if !line.is_empty() {
            line.push_str(if k > 0 && conds[k - 1].or { " or " } else { ", " });
        }
        if let Some(r) = requirement(g, c) {
            line.push_str(&r);
        }
    }
    line.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_is_saved_with_the_player() {
        let mut p = crate::game::PlayerState::default();
        // No look: nothing written, so older saves read the same.
        assert!(!serde_json::to_string(&p).unwrap().contains("look"));
        let look = Look { race: FormId(0x38E5), female: true, hair: None, hair_color: Some([10, 20, 30]), eyes: Some(FormId(4)), head_parts: vec![], fggs: vec![0.5; 50], fgga: vec![0.0; 30], fgts: vec![-1.0; 50] };
        p.look = Some(look.clone());
        let back: crate::game::PlayerState = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.look, Some(look));
    }

    #[test]
    fn special_points_stay_within_the_rules() {
        let mut s = SpecialSheet::new([5; 7]);
        assert_eq!(s.remaining(), 5);
        assert!(!s.complete());
        for _ in 0..5 {
            assert!(s.raise(0));
        }
        assert_eq!(s.values[0], 10);
        // Nothing left, and 10 is the top.
        assert!(!s.raise(1) && !s.raise(0));
        assert!(s.complete());
        assert!(s.lower(0));
        assert!(s.raise(0) && !s.raise(0));
        for _ in 0..4 {
            assert!(s.lower(6));
        }
        assert_eq!(s.values[6], 1);
        assert!(!s.lower(6));
        assert_eq!(s.remaining(), 4);
    }

    #[test]
    fn the_screenshot_sheet_is_complete() {
        // S5 P3 E7 C7 I5 A6 L7: "00 POINTS REMAINING".
        let s = SpecialSheet::new([5, 3, 7, 7, 5, 6, 7]);
        assert_eq!(s.remaining(), 0);
        assert!(s.complete());
        // S5 P3 E5 C7 I5 A6 L7: "02 POINTS REMAINING".
        assert_eq!(SpecialSheet::new([5, 3, 5, 7, 5, 6, 7]).remaining(), 2);
    }

    #[test]
    fn three_tags_and_no_more() {
        let mut t = TagSheet { skills: vec![32, 34, 35, 41, 43, 44], tagged: BTreeSet::new(), count: TAG_COUNT, locked: BTreeSet::new() };
        assert_eq!(t.remaining(), 3);
        assert!(t.toggle(43) && t.toggle(44));
        assert_eq!(t.remaining(), 1);
        assert!(!t.complete());
        assert!(t.toggle(41));
        assert!(t.complete());
        assert!(!t.toggle(32), "a fourth tag is refused");
        assert!(t.toggle(41) && !t.complete());
        assert!(!t.toggle(33), "Big Guns is not in the list");
    }

    #[test]
    fn added_points_keep_the_old_values_as_the_floor() {
        // AddSPECIALPoints 1 on a finished character: one point to give, none to take.
        let mut s = SpecialSheet::new([5, 3, 7, 7, 5, 6, 7]);
        s.total = 41;
        s.floor = s.values;
        assert_eq!(s.remaining(), 1);
        assert!(!s.lower(1), "below the value before the menu");
        assert!(s.raise(1) && s.complete());
        assert!(s.lower(1) && !s.lower(1));
    }

    #[test]
    fn added_tags_lock_the_old_ones() {
        let old: BTreeSet<u16> = [41, 43, 44].into();
        let mut t = TagSheet { skills: vec![32, 34, 35, 41, 43, 44], tagged: old.clone(), count: 4, locked: old };
        assert_eq!(t.remaining(), 1);
        assert!(!t.toggle(41), "a tag from before stays");
        assert!(t.toggle(32) && t.complete());
        assert!(!t.toggle(34), "a fifth is refused");
    }

    #[test]
    fn up_to_two_traits_or_none() {
        let (a, b, c) = (FormId(1), FormId(2), FormId(3));
        let mut t = TraitSheet { traits: vec![a, b, c], chosen: BTreeSet::new(), max: 2 };
        assert_eq!(t.left(), 2);
        assert!(!t.toggle(a, false), "requirements not met");
        assert!(t.toggle(a, true) && t.toggle(b, true));
        assert_eq!(t.left(), 0);
        assert!(!t.toggle(c, true));
        assert!(t.toggle(a, true), "taking one back needs no requirement");
        assert_eq!(t.left(), 1);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("Cou\nrier"), "Courier");
        assert_eq!(clean_name(&"x".repeat(40)).len(), NAME_MAX_CHARS);
        assert!(!name_acceptable("   "));
        assert!(name_acceptable(" Six "));
    }
}
