//! Levelling up (docs/spec-notes/levelup.md): the rules of the level-up menu (1027,
//! `menus\levelup_menu.xml`) as plain data the engine draws, and what its Done writes.
//!
//! - [`advance`] is `AdvancePCLevel` (`AdvLevel`): the player's level goes up by one and
//!   the menu opens. [`open`] opens it for the level the player has now, for whatever
//!   raises the level by experience.
//! - The menu has two pages [documented: levelup_menu.xml `_CurrentPage`, `_EndPage`]:
//!   the skills ([`SkillSheet`]), then, at a level that brings a perk, the perks
//!   ([`PerkSheet`]).
//! - Each sheet starts from the player as the game holds them and changes nothing until
//!   its `apply`.

use crate::actor_values::{BIG_GUNS, INTELLIGENCE, SKILLS};
use crate::chargen;
use crate::combat::setting;
use crate::functions::actor;
use crate::game::{menus, Game};
use fnv_sim::FormId;
use std::collections::BTreeSet;

/// The highest a skill can be raised to [documented: levelup_menu.xml, the skill row's
/// `_MaxValue` 100].
pub const SKILL_MAX: i32 = 100;

/// `iLevelsPerPerk` when the masters do not set it [documented: GECK wiki, "iLevelsPerPerk
/// is 2 (default)"; observed: not among FalloutNV.esm's GMST].
pub const DEFAULT_LEVELS_PER_PERK: i32 = 2;

/// Perks chosen at a perk level [observed: "CHOOSE 1 PERK" at level 4].
pub const PERKS_PER_LEVEL: usize = 1;

/// The skill points a level brings at a permanent Intelligence:
/// ⌊min(Intelligence, 9) × 0.5 + 10⌋, and one more when the level and Intelligence are
/// both even [documented: GECK wiki "Skill Rate", New Vegas; observed: 12 points at
/// level 3 on the owner's screenshot, which Intelligence 4 or 5 gives]. The settings
/// `iLevelUpSkillPointsBase` / `iLevelUpSkillPointsInterval` are Fallout 3's rule and are
/// not read: "Unused in New Vegas" [documented: GECK wiki "Skill Rate", Individual
/// Settings].
pub fn skill_points(intelligence: i32, level: i32) -> i32 {
    let base = (intelligence.min(9) as f64 * 0.5 + 10.0).floor() as i32;
    base + i32::from(level % 2 == 0 && intelligence % 2 == 0)
}

/// The skill points the player gets for reaching `level`: [`skill_points`], then the
/// perks' Adjust Gained Skill Points entry point (Educated +2, Logan's Loophole 0 past
/// level 30; `crate::perks::ADJUST_GAINED_SKILL_POINTS`), never under 0 [hypothesis: whole
/// points, the fraction dropped]. The perk owner's conditions are read with the player at
/// the new level (the level is raised before the menu opens). Logged `levelup: <n> skill
/// points (<base> before <perk>, ...)` when a perk changed them.
pub fn points_for(g: &mut Game, level: i32) -> i32 {
    let int = actor::actor_value_base(g, FormId::PLAYER_REF, INTELLIGENCE).round() as i32;
    let base = skill_points(int, level);
    let (v, by) = g.entry_point(FormId::PLAYER_REF, crate::perks::ADJUST_GAINED_SKILL_POINTS, f64::from(base));
    let points = ((v * 1000.0).round() / 1000.0).trunc().max(0.0) as i32;
    if !by.is_empty() {
        let perks: Vec<String> = by.iter().map(|a| g.data.name(a.perk)).collect();
        tracing::info!("levelup: {points} skill points ({base} before {})", perks.join(", "));
    }
    points
}

/// Whether reaching `level` brings a perk: every `iLevelsPerPerk` levels [documented:
/// GECK wiki, iLevelsPerPerk; hypothesis: counted from level 0, so levels 2, 4, 6 …].
pub fn perk_due(g: &Game, level: i32) -> bool {
    let every = setting(g, "iLevelsPerPerk", DEFAULT_LEVELS_PER_PERK as f64) as i32;
    every > 0 && level % every == 0
}

/// Open the level-up menu for the player's level now.
pub fn open(g: &mut Game) {
    g.open_menu(menus::LEVEL_UP);
}

/// `AdvancePCLevel`: the player's level goes up by one and the level-up menu opens
/// [documented: GECK wiki AdvancePCLevel]. Experience is left as it is [hypothesis: the
/// page does not say]. The new level.
pub fn advance(g: &mut Game) -> i32 {
    let level = g.player_level() as i32 + 1;
    g.player.level = Some(level);
    g.sync_player_level();
    open(g);
    level
}

/// The skills in the menu's order: every skill but Big Guns, by name [observed: the
/// owner's screenshots of the level-up menu list the thirteen of the tag-skill menu in
/// the same order].
pub fn menu_skills(g: &Game) -> Vec<u16> {
    let mut skills: Vec<u16> = SKILLS.filter(|&c| c != BIG_GUNS).collect();
    skills.sort_by_key(|&c| chargen::skill_name(g, c).to_lowercase());
    skills
}

// ---- the skills page ---------------------------------------------------------------------

/// The skills page: the points to spend and where they went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSheet {
    pub skills: Vec<u16>,
    /// Each skill's value when the menu opened: the current value, effects included
    /// [documented: the row's `_BaseValue` plus `_ExtraValue`, "values added … from
    /// perks, chems, and other enhancers"].
    pub start: Vec<i32>,
    /// The points spent on each skill.
    pub spent: Vec<i32>,
    /// What one point adds to each skill: `iSkillPointsTagSkillMult` for a tagged skill,
    /// else 1 [documented: GECK wiki Skill Settings, default 1; observed: not in the
    /// masters].
    pub per_point: Vec<i32>,
    /// The points the level brings.
    pub points: i32,
}

impl SkillSheet {
    pub fn new(skills: Vec<u16>, start: Vec<i32>, per_point: Vec<i32>, points: i32) -> SkillSheet {
        let spent = vec![0; skills.len()];
        SkillSheet { skills, start, spent, per_point, points }
    }

    /// The sheet for `level`, from the player as the game holds them.
    pub fn of_player(g: &mut Game, level: i32) -> SkillSheet {
        let skills = menu_skills(g);
        let tag_mult = setting(g, "iSkillPointsTagSkillMult", 1.0).max(1.0) as i32;
        let start = skills.iter().map(|&s| actor::actor_value(g, FormId::PLAYER_REF, s).unwrap_or(0.0).round() as i32).collect();
        let per_point = skills.iter().map(|s| if g.player.tag_skills.contains(s) { tag_mult } else { 1 }).collect();
        SkillSheet::new(skills, start, per_point, points_for(g, level))
    }

    pub fn remaining(&self) -> i32 {
        self.points - self.spent.iter().sum::<i32>()
    }

    /// Row `i`'s value as the menu shows it, never past [`SKILL_MAX`] (points that would
    /// push it over are kept but add nothing [documented: the row's `_OverflowValue`]).
    pub fn value(&self, i: usize) -> i32 {
        let raised = self.start[i] + self.spent[i] * self.per_point[i];
        raised.min(SKILL_MAX.max(self.start[i]))
    }

    /// Whether row `i`'s "+" is shown: points are left and the skill is under the most
    /// [documented: LUM_Template_RightArrow's `visible`].
    pub fn can_raise(&self, i: usize) -> bool {
        i < self.skills.len() && self.remaining() > 0 && self.value(i) < SKILL_MAX
    }

    /// Whether row `i`'s "−" is shown: points were spent on it in this menu (a skill
    /// never goes under its value at the start) [documented: LUM_Template_LeftArrow's
    /// `visible` and its comment].
    pub fn can_lower(&self, i: usize) -> bool {
        self.spent.get(i).is_some_and(|&n| n > 0)
    }

    pub fn raise(&mut self, i: usize) -> bool {
        let ok = self.can_raise(i);
        if ok {
            self.spent[i] += 1;
        }
        ok
    }

    pub fn lower(&mut self, i: usize) -> bool {
        let ok = self.can_lower(i);
        if ok {
            self.spent[i] -= 1;
        }
        ok
    }

    /// Reset: every point back [documented: LUM_ResetButton, "Resets all distributed
    /// points"].
    pub fn reset(&mut self) {
        self.spent.iter_mut().for_each(|n| *n = 0);
    }

    /// Whether Continue / Done may be pressed: every point spent [documented:
    /// LUM_ContinueButton's `target`, the points assigned equal to the most], or no skill
    /// left to raise [design: else a player with every skill at 100 could not leave].
    pub fn complete(&self) -> bool {
        self.remaining() <= 0 || !(0..self.skills.len()).any(|i| self.value(i) < SKILL_MAX)
    }

    /// What each skill gains, as (skill, amount), leaving out those that gain nothing.
    pub fn gains(&self) -> Vec<(u16, i32)> {
        (0..self.skills.len()).map(|i| (self.skills[i], self.value(i) - self.start[i])).filter(|&(_, d)| d != 0).collect()
    }

    /// The points go into the skills (`PlayerState::skill_points`).
    pub fn apply(&self, g: &mut Game) {
        self.shift(g, 1.0);
    }

    /// Take back what [`apply`](Self::apply) gave (Back from the perks page).
    pub fn undo(&self, g: &mut Game) {
        self.shift(g, -1.0);
    }

    fn shift(&self, g: &mut Game, sign: f64) {
        for (skill, d) in self.gains() {
            let e = g.player.skill_points.entry(skill).or_default();
            *e += sign * d as f64;
            if *e == 0.0 {
                g.player.skill_points.remove(&skill);
            }
        }
    }
}

/// The skills page's counter [observed: "ASSIGN 12 SKILL POINTS", "ASSIGN 7 SKILL POINTS",
/// nothing with every point spent; hypothesis: "ASSIGN 1 SKILL POINT"].
pub fn skill_counter(remaining: i32) -> String {
    match remaining {
        n if n <= 0 => String::new(),
        1 => "ASSIGN 1 SKILL POINT".into(),
        n => format!("ASSIGN {n} SKILL POINTS"),
    }
}

/// The menu's title [observed: "WELCOME TO LEVEL 3"].
pub fn title(level: i32) -> String {
    format!("WELCOME TO LEVEL {level}")
}

// ---- the perks page ----------------------------------------------------------------------

/// The perks page: every perk of which the player has a rank to come, those that can be
/// taken now first, and the ones taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerkSheet {
    pub perks: Vec<FormId>,
    /// Whether each can be taken now (the others are drawn faint and refused).
    pub available: Vec<bool>,
    pub chosen: BTreeSet<FormId>,
    pub count: usize,
}

impl PerkSheet {
    /// Every playable, not hidden perk that is not a trait and of which the player has a
    /// rank to come [documented: xEdit PERK DATA Trait, Min Level, Ranks, Playable,
    /// Hidden]: those whose least level the player has reached and whose requirements
    /// the player meets first, then the others, each part by least level, then by name
    /// [observed: the owner's level-4 screenshots: Confirmed Bachelor … Swift Learner
    /// (level 2), Cannibal, Comprehension, Educated (4) bright; then faint In Shining
    /// Armor, Junk Rounds, Light Touch, Old World Gourmet (1) … Toughness, Vigilant
    /// Recycler (6), Commando, Cowboy, Grunt … Stonewall (8)]. A perk of which the player
    /// has every rank is left out [hypothesis: none was seen], and so are the perks of
    /// least level 0, the challenges' rewards scripts give [observed: Bug Stomper and Set
    /// Lasers for Fun, level 0 without conditions, are not on the owner's list, which
    /// starts with Confirmed Bachelor].
    pub fn of_player(g: &mut Game, level: i32) -> PerkSheet {
        let offered: Vec<FormId> = g
            .data
            .perks
            .iter()
            .filter(|(id, p)| !p.is_trait && p.playable && !p.hidden && p.min_level > 0 && g.player.perks.get(id).copied().unwrap_or(0) < p.ranks.max(1))
            .map(|(&id, _)| id)
            .collect();
        let mut rows: Vec<(bool, u8, String, FormId)> = offered
            .into_iter()
            .map(|id| {
                let p = &g.data.perks[&id];
                let (min, name) = (p.min_level, p.name.to_lowercase());
                (min as i32 <= level && chargen::requirements_met(g, id), min, name, id)
            })
            .collect();
        rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then_with(|| a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
        // The player's tweak can let more than one be taken [design: docs/design/tweaks.md].
        PerkSheet { perks: rows.iter().map(|r| r.3).collect(), available: rows.iter().map(|r| r.0).collect(), chosen: BTreeSet::new(), count: g.tweaks.perks_per_level.max(1) }
    }

    /// A sheet of perks that can all be taken (tests).
    pub fn new(perks: Vec<FormId>) -> PerkSheet {
        let available = vec![true; perks.len()];
        PerkSheet { perks, available, chosen: BTreeSet::new(), count: PERKS_PER_LEVEL }
    }

    pub fn can_take(&self, p: FormId) -> bool {
        self.perks.iter().position(|&q| q == p).is_some_and(|i| self.available[i])
    }

    /// Take a perk that can be taken while fewer than the count are taken, or give a
    /// taken one back. Whether anything changed.
    pub fn toggle(&mut self, p: FormId) -> bool {
        if self.chosen.remove(&p) {
            return true;
        }
        if self.chosen.len() < self.count && self.can_take(p) {
            self.chosen.insert(p);
            return true;
        }
        false
    }

    pub fn left(&self) -> usize {
        self.count.saturating_sub(self.chosen.len())
    }

    /// Whether Done may be pressed: the perks taken [observed: "Done A)" dim with none
    /// taken, bright with Intense Training taken], or none can be taken [design].
    pub fn complete(&self) -> bool {
        self.left() == 0 || !self.available.contains(&true)
    }

    /// Done: one more rank of each perk taken, with what gaining it does
    /// (`crate::perks::gain_rank`: Intense Training opens the attributes page).
    pub fn apply(&self, g: &mut Game) {
        for p in &self.chosen {
            crate::perks::gain_rank(g, *p);
        }
    }
}

/// The perks page's counter [observed: "CHOOSE 1 PERK", nothing once it is taken;
/// hypothesis: "CHOOSE 2 PERKS"].
pub fn perk_counter(left: usize) -> String {
    match left {
        0 => String::new(),
        1 => "CHOOSE 1 PERK".into(),
        n => format!("CHOOSE {n} PERKS"),
    }
}

/// A perk's row: its name, and the rank it would reach in brackets when the player has a
/// rank of it already [observed: "Swift Learner (2)"].
pub fn perk_row(name: &str, has: u8) -> String {
    if has == 0 {
        name.to_string()
    } else {
        format!("{name} ({})", u16::from(has) + 1)
    }
}

/// The text beside a perk [observed: "Req: Level 2, INT 4", "Ranks: 3" (Swift Learner's
/// ranks of all), an empty line, the description; as the trait menu's].
pub fn perk_info(requirements: &str, ranks: u8, description: &str) -> String {
    format!("Req: {requirements}
Ranks: {ranks}

{}", description.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_per_level_follow_the_skill_rate() {
        // ⌊min(INT, 9) / 2 + 10⌋ ...
        assert_eq!(skill_points(1, 3), 10);
        assert_eq!(skill_points(5, 3), 12);
        assert_eq!(skill_points(9, 3), 14);
        assert_eq!(skill_points(10, 3), 14);
        // ... and one more when the level and Intelligence are both even.
        assert_eq!(skill_points(4, 4), 13);
        assert_eq!(skill_points(4, 3), 12);
        assert_eq!(skill_points(5, 4), 12);
        assert_eq!(skill_points(10, 2), 15);
    }

    /// The owner's screenshots: twelve points, all spent on Science (17 → 22 with seven
    /// left, then 29 with none).
    #[test]
    fn the_screenshot_sheet() {
        let mut s = SkillSheet::new(vec![32, 40], vec![17, 17], vec![1, 1], 12);
        assert_eq!(skill_counter(s.remaining()), "ASSIGN 12 SKILL POINTS");
        assert!(!s.can_lower(1) && s.can_raise(1) && !s.complete());
        for _ in 0..5 {
            assert!(s.raise(1));
        }
        assert_eq!((s.value(1), s.remaining()), (22, 7));
        assert_eq!(skill_counter(s.remaining()), "ASSIGN 7 SKILL POINTS");
        assert!(s.can_lower(1) && !s.can_lower(0));
        for _ in 0..7 {
            assert!(s.raise(1));
        }
        assert_eq!((s.value(1), s.remaining()), (29, 0));
        assert_eq!(skill_counter(0), "");
        // No "+" anywhere with nothing left; Done lights.
        assert!(!s.can_raise(0) && !s.raise(0) && s.complete());
        assert_eq!(s.gains(), vec![(40, 12)]);
        s.reset();
        assert_eq!((s.value(1), s.remaining()), (17, 12));
        assert_eq!(title(3), "WELCOME TO LEVEL 3");
    }

    #[test]
    fn a_skill_stops_at_100() {
        let mut s = SkillSheet::new(vec![32, 40], vec![98, 100], vec![2, 1], 10);
        assert!(!s.can_raise(1));
        assert!(s.raise(0));
        assert_eq!(s.value(0), 100);
        assert!(!s.raise(0), "100 is the top");
        // A point that pushed past the top gains nothing.
        let s = SkillSheet { spent: vec![2, 0], ..s };
        assert_eq!((s.value(0), s.gains()), (100, vec![(32, 2)]));
        // Every skill at the top: Done may be pressed with points left.
        let full = SkillSheet::new(vec![32], vec![100], vec![1], 10);
        assert!(full.complete());
    }

    #[test]
    fn one_perk_is_taken() {
        let mut p = PerkSheet::new(vec![FormId(1), FormId(2), FormId(4)]);
        p.available[2] = false;
        assert!(!p.complete());
        assert_eq!(perk_counter(p.left()), "CHOOSE 1 PERK");
        assert!(!p.toggle(FormId(4)), "a faint perk is refused");
        assert!(p.toggle(FormId(1)));
        assert!(!p.toggle(FormId(2)), "a second perk is refused");
        assert!(p.complete());
        assert_eq!(perk_counter(p.left()), "");
        assert!(p.toggle(FormId(1)) && p.chosen.is_empty());
        assert!(!p.toggle(FormId(3)), "a perk not on offer");
        let mut none = PerkSheet::new(vec![FormId(1)]);
        none.available[0] = false;
        assert!(none.complete(), "nothing can be taken");
        assert_eq!(perk_info("Level 2, INT 4", 3, " More XP. "), "Req: Level 2, INT 4
Ranks: 3

More XP.");
        assert_eq!((perk_row("Swift Learner", 1), perk_row("Hunter", 0)), ("Swift Learner (2)".to_string(), "Hunter".to_string()));
    }
}
