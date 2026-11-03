//! Terminals as the game sees them (docs/spec-notes/hacking.md §1, §6): whether a
//! terminal opens straight away, must be hacked, refuses, or is locked for good; which
//! of its menu items are listed; and what choosing one does (its result script, its
//! note, its sub menu). The screens are the engine's; the hacking game itself is
//! [`crate::hacking`].

use crate::combat::setting;
use crate::data::{terminal_flag, TerminalDef};
use crate::functions::actor::actor_value_capped;
use crate::game::{Game, RunCtx};
use crate::hacking::{self, Difficulty, Refusal, Settings};
use crate::trace::EventKind;
use fnv_sim::FormId;
use std::collections::BTreeMap;

/// What became of a terminal reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Access {
    /// Hacked: it opens straight away from now on [documented: GECK wiki Lock, "if
    /// player successfully hacked terminal … will still have terminal access"].
    Hacked,
    /// Four wrong words: it can never be hacked again [documented: GECK wiki Lock,
    /// "if player failed to hack terminal … will still be locked out"; the owner,
    /// 2026-10-11].
    LockedOut,
}

/// A terminal a script locked or unlocked (Lock, Unlock).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScriptLock {
    pub locked: bool,
    /// The lock level GetLockLevel reads: what Lock set (0 to 5), -1 after Unlock.
    pub level: i32,
}

/// Every terminal reference's [`Access`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Terminals {
    pub access: BTreeMap<FormId, Access>,
    /// Lock and Unlock on terminals ([`lock`], [`unlock`]).
    pub script_locks: BTreeMap<FormId, ScriptLock>,
    /// Terminals locked out again on Computer Whiz's try (perk-entry-points.md §7).
    pub retried: std::collections::BTreeSet<FormId>,
    /// ForceTerminalBack was called by the result script running now
    /// ([`choose`] hands it on as [`Chosen::back`]).
    pub back: bool,
}

/// What activating a terminal does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Opening {
    /// Its menu: unlocked, hacked before, or the player has its password note.
    Menu,
    /// The hacking game with this many words.
    Hack { difficulty: Difficulty, words: u32 },
    Refused(Refusal),
    LockedOut,
}

/// The player's Science skill (0 to 100).
pub fn science(g: &mut Game) -> f32 {
    actor_value_capped(g, FormId::PLAYER_REF, crate::actor_values::SCIENCE) as f32
}

/// Whether the player holds the terminal's password note: among their notes, or carried
/// as an item.
pub fn has_password(g: &mut Game, t: &TerminalDef) -> bool {
    let Some(note) = t.password_note else { return false };
    g.player.notes.contains(&note) || g.world.item_count(FormId::PLAYER_REF, note).is_ok_and(|n| n > 0)
}

/// The lock of terminal reference `reference` of base `base` when it does not open at
/// its menu (it must be hacked, is refused, or is locked out): its difficulty; None for
/// a terminal that opens at its menu, or a base that is not a terminal. The activate
/// prompt shows it as a lock's (the owner, 2026-10-22).
pub fn still_locked(g: &mut Game, reference: FormId, base: FormId) -> Option<Difficulty> {
    let t = g.data.terminals.get(&base)?;
    let script = g.terminals.script_locks.get(&reference).copied();
    // Unlock gives access "regardless of lock level or previously being locked out"
    // [documented: GECK wiki Unlock]; Lock overrides the Unlocked flag and an earlier
    // Unlock [documented: GECK wiki Lock].
    if script.is_some_and(|l| !l.locked) {
        return None;
    }
    let unlocked = script.is_none() && t.has(terminal_flag::UNLOCKED);
    // [hypothesis, hacking.md §6] The password note opens a terminal without hacking,
    // even one locked out (GECK wiki Lock: the note keeps access); it is checked first.
    if unlocked || g.terminals.access.get(&reference) == Some(&Access::Hacked) || has_password(g, t) {
        return None;
    }
    // The Leveled flag is not applied: how the level would change with the player's
    // level is not documented [hacking.md §6].
    let code = match script {
        Some(l) if l.level >= 0 => l.level.min(5) as u8,
        _ => t.difficulty,
    };
    Some(Difficulty::from_code(code))
}

/// What activating the terminal reference `reference` of base `base` does, for a player
/// of Science skill `sci` ([`science`]).
pub fn opening(g: &mut Game, reference: FormId, base: FormId, sci: f32) -> Option<Opening> {
    g.data.terminals.get(&base)?;
    let Some(difficulty) = still_locked(g, reference, base) else { return Some(Opening::Menu) };
    if g.terminals.access.get(&reference) == Some(&Access::LockedOut) {
        // Computer Whiz's Ignore Locked Terminal: one more try at a locked-out terminal
        // [documented: GECK wiki Entry Point, "run when the player attempts to activate a
        // locked terminal; if non-zero another chance will be granted"; observed: the perk,
        // "one extra attempt to hack a locked-down terminal"].
        if g.terminals.retried.contains(&reference) || !g.player_has_entry(crate::perks::IGNORE_LOCKED_TERMINAL) {
            return Some(Opening::LockedOut);
        }
        tracing::info!("terminal: {reference} is locked out; Ignore Locked Terminal (Computer Whiz) gives one more try");
    }
    let s = Settings::default();
    Some(match hacking::may_hack(sci, difficulty, &s) {
        Err(r) => Opening::Refused(r),
        Ok(()) => Opening::Hack { difficulty, words: hacking::word_count(sci, difficulty, &s) },
    })
}

/// `Lock level` on a terminal: locked, at `level` (above 5: 5, Requires Key); level 0
/// keeps the level it had [documented: GECK wiki Lock, "Lock 0 ... will lock it but NOT
/// change the lock level"; observed, function probes `fp3_terminal` 2026-10-17: `Lock 50`
/// on an Easy terminal gave GetLocked 2 and GetLockLevel 5]. A terminal hacked before
/// keeps its access, one locked out stays so [documented: GECK wiki Lock]: [`opening`]
/// checks those before the level.
pub fn lock(g: &mut Game, reference: FormId, level: i32) {
    let had = lock_level(g, reference);
    let level = if level > 0 { level.min(5) } else { had.max(0) };
    g.terminals.script_locks.insert(reference, ScriptLock { locked: true, level });
}

/// `Unlock` on a terminal: it opens at its menu from now on [documented: GECK wiki
/// Unlock], GetLocked 0 and GetLockLevel -1 [observed: `fp3_terminal`].
pub fn unlock(g: &mut Game, reference: FormId) {
    g.terminals.script_locks.insert(reference, ScriptLock { locked: false, level: -1 });
}

/// GetLocked of a terminal reference in a loaded cell: 0 for an Unlocked terminal (no lock
/// data) or one a script unlocked, 2 for one a script locked or the player locked out,
/// else 1, also once hacked [documented: GECK wiki GetLocked, "Does not change state when
/// hacked"; observed, `fp3_terminal`: an Easy terminal read 1, 2 after Lock 50, 0 after
/// Unlock]. The caller answers 0 for a terminal whose cell is not loaded [observed: the
/// same terminal read 0 from another cell].
pub fn locked(g: &Game, reference: FormId) -> u8 {
    match g.terminals.script_locks.get(&reference) {
        Some(l) => 2 * l.locked as u8,
        None if g.terminals.access.get(&reference) == Some(&Access::LockedOut) => 2,
        None => {
            let base = crate::functions::base_of(g, reference);
            let unlocked = base.and_then(|b| g.data.terminals.get(&b)).is_none_or(|t| t.has(terminal_flag::UNLOCKED));
            (!unlocked) as u8
        }
    }
}

/// GetLockLevel of a terminal reference in a loaded cell: its Base Hacking Difficulty
/// (0 Very Easy ... 4 Very Hard, 5 Requires Key), or what Lock / Unlock left [observed,
/// `fp3_terminal`: 1 for an Easy terminal, 5 after `Lock 50`, -1 after Unlock; documented:
/// GECK wiki GetLockLevel's 0 to 5]. The Leveled flag is not applied (hacking.md §6).
pub fn lock_level(g: &Game, reference: FormId) -> i32 {
    match g.terminals.script_locks.get(&reference) {
        Some(l) => l.level,
        None => crate::functions::base_of(g, reference).and_then(|b| g.data.terminals.get(&b)).map_or(0, |t| t.difficulty as i32),
    }
}

/// The hacking game ended; a hacked terminal gives the player its XP ([`xp`]). The
/// experience earned.
pub fn hacked(g: &mut Game, reference: FormId, success: bool) -> f64 {
    let a = if success { Access::Hacked } else { Access::LockedOut };
    // Locked out again on Computer Whiz's try: that try is spent.
    if g.terminals.access.insert(reference, a) == Some(Access::LockedOut) && !success {
        g.terminals.retried.insert(reference);
    }
    if !success {
        return 0.0;
    }
    g.count_stat(crate::misc_stats::COMPUTERS_HACKED, 1.0);
    let difficulty = crate::functions::base_of(g, reference).and_then(|b| g.data.terminals.get(&b)).map(|t| Difficulty::from_code(t.difficulty));
    let x = difficulty.map_or(0.0, |d| xp(g, d));
    crate::experience::award(g, crate::experience::Source::HackTerminal, x)
}

/// The XP for hacking a terminal of `difficulty` [documented: GECK wiki Experience
/// Settings]: the difficulty's code picks the first class whose
/// `iXPLevelHackComputer<class>` (defaults 0 to 4; not in the masters) it does not pass,
/// and the class gives `iXPRewardHackComputer<class>` (defaults 20 to 60; FalloutNV.esm
/// sets the same five [observed]). None for Requires Key, which cannot be hacked. The
/// master also has an `iXPRewardHackComputer` of 20 without a class [observed]; what
/// reads it is unknown and it is not used.
pub fn xp(g: &Game, difficulty: Difficulty) -> f64 {
    const CLASSES: [(&str, f64, f64); 5] = [("VeryEasy", 0.0, 20.0), ("Easy", 1.0, 30.0), ("Average", 2.0, 40.0), ("Hard", 3.0, 50.0), ("VeryHard", 4.0, 60.0)];
    if difficulty == Difficulty::RequiresKey {
        return 0.0;
    }
    let code = f64::from(difficulty.code());
    let class = CLASSES.iter().find(|(c, level, _)| code <= setting(g, &format!("iXPLevelHackComputer{c}"), *level)).unwrap_or(&CLASSES[4]);
    setting(g, &format!("iXPRewardHackComputer{}", class.0), class.2)
}

/// The items of terminal `base` listed now, by index: those whose conditions hold, run
/// on the terminal reference with the player as target [hypothesis, hacking.md §6].
pub fn items(g: &mut Game, reference: FormId, base: FormId) -> Vec<usize> {
    let Some(t) = g.data.terminals.get(&base) else { return Vec::new() };
    let mut out = Vec::new();
    for (i, item) in t.items.iter().enumerate() {
        let label = format!("terminal 0x{:08X} item {i}", base.0);
        if g.conditions_pass(&item.conditions, Some(reference), Some(FormId::PLAYER_REF), &label) {
            out.push(i);
        }
    }
    out
}

/// A script's changes to a menu item for the session (JohnnyGuitar's
/// Set…TerminalMenuItem…, `crate::nvse_jg3`): each field set replaces the record's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemEdit {
    pub text: Option<String>,
    pub result: Option<String>,
    pub flags: Option<u8>,
    pub note: Option<Option<FormId>>,
    pub submenu: Option<Option<FormId>>,
}

/// Item `item` of terminal `base` as it stands now: the record's, with a script's changes
/// ([`ItemEdit`]).
pub fn item(g: &Game, base: FormId, item: usize) -> Option<crate::data::TerminalItem> {
    let mut it = g.data.terminals.get(&base)?.items.get(item)?.clone();
    if let Some(e) = g.other.terminal_items.get(&(base, item)) {
        if let Some(t) = &e.text {
            it.text = t.clone();
        }
        if let Some(r) = &e.result {
            it.result = r.clone();
        }
        it.flags = e.flags.unwrap_or(it.flags);
        it.note = e.note.unwrap_or(it.note);
        it.submenu = e.submenu.unwrap_or(it.submenu);
    }
    Some(it)
}

/// What choosing an item showed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chosen {
    /// RNAM, shown under the menu.
    pub result: String,
    /// The terminal to show next as a sub menu.
    pub submenu: Option<FormId>,
    /// INAM, the note shown in the screen.
    pub note: Option<FormId>,
    /// The item's result script called ForceTerminalBack: the terminal goes back one
    /// level, or closes from its top level [documented: GECK wiki ForceTerminalBack].
    pub back: bool,
}

/// The player chooses item `item` of terminal `base` at reference `reference`: its result
/// script runs on the reference [hypothesis, hacking.md §6], an Add Note item gives its
/// note to the player [documented: xEdit flag name].
pub fn choose(g: &mut Game, reference: FormId, base: FormId, item: usize) -> Option<Chosen> {
    let data = g.data;
    let it = self::item(g, base, item)?;
    g.terminals.back = false;
    if let Some(u) = it.unit {
        g.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(reference) });
        g.run_result(u, RunCtx { self_ref: Some(reference), action_ref: Some(FormId::PLAYER_REF), quest: None });
    }
    if it.flags & 1 != 0 {
        if let Some(n) = it.note {
            g.file_note(n);
        }
    }
    let back = std::mem::take(&mut g.terminals.back);
    Some(Chosen { result: it.result, submenu: it.submenu, note: it.note, back })
}

/// The server line of the header: "-Server 1-" to "-Server 10-" [documented: xEdit
/// TERM ServerType names]; the game's own text for each is a setting
/// (`sTerminalServerText<N>`), of which only the tenth is in the plugins.
pub fn server_text(server: u8) -> String {
    format!("-Server {}-", server as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_count_from_one() {
        assert_eq!(server_text(0), "-Server 1-");
        assert_eq!(server_text(5), "-Server 6-");
        assert_eq!(server_text(9), "-Server 10-");
    }
}
