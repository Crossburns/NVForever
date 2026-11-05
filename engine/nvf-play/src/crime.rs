//! Karma and crime in play (docs/spec-notes/crime-karma.md §6): the rules are the game's
//! (`fnv_game::crime`); this module tells the game what the player did and shows what
//! came of it.
//!
//! - **Theft**: an item reference the player picked up (it left the world: `RefChanged`
//!   with the reference deleted, as `crate::world_sounds` hears pick-ups) and items taken
//!   out of a container in the container menu ([`ItemsTaken`]); a dead actor's inventory
//!   is not owned through its cell [hypothesis].
//! - **Locks**: the lockpicking menu's crime hook (`crate::lockpick::LockOpened`), and a
//!   terminal hacked, the same crime as a lock picked (the owner, 2026-10-22).
//! - **Trespass**: [`fnv_game::Game::crime_tick`] four times a second of game time.
//! - **Karma**: each change shows a corner message with the game's own strings and icons
//!   (`sKarmaMinorGained` … and their `…Image` settings) and plays `UIKarmaUp` or
//!   `UIKarmaDown` [observed: the settings and sounds in FalloutNV.esm; hypothesis: which
//!   is used when, a change of `fKarmaModKillingEvilActor` or more being "major"].
//! - **Reputation**: a seen theft or kill adds infamy with the victim's reputations
//!   (`fnv_game::functions::crime_infamy`, noted as `Note::Infamy`).
//! - **Log**: `crime:` lines for each crime, warning and karma change, and a
//!   `crime summary` at exit.
//!
//! The crosshair's prompt for an owned item, container or lock (a terminal still to hack
//! among them) is drawn in the crime colour, and an item's action reads "Steal"
//! (`crate::activate`, `crate::reticle`).

use crate::corner_messages::CornerMessages;
use crate::lockpick::LockOpened;
use crate::script::{with_world, ScriptState};
use crate::script_events::Note as Corner;
use crate::sfx::PlaySound;
use crate::sim::SimState;
use crate::{PlaySet, RefChanged};
use bevy::prelude::*;
use fnv_game::crime::{Kind, Note};
use fnv_sim::forms::BaseData;
use fnv_sim::FormId;
use std::collections::HashSet;

/// Seconds of game time between trespass checks [design: as often as detection measures].
const TICK: f64 = 0.25;

pub struct CrimePlugin;

impl Plugin for CrimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrimeStats>()
            .add_message::<ItemsTaken>()
            // After the menus: items taken in a container menu are judged in the frame they
            // are taken, while the menu still holds the world (Take All closes it).
            .add_systems(Update, (report_deeds, tick, show_notes).chain().after(PlaySet::Scripts).after(crate::menu::MenuSystems).before(PlaySet::Sync))
            .add_systems(Last, summary);
    }
}

/// Items the player took out of a container in the container menu.
#[derive(Message, Clone, Debug)]
pub struct ItemsTaken {
    pub container: FormId,
    pub items: Vec<(FormId, i32)>,
}

#[derive(Resource, Default)]
struct CrimeStats {
    /// Picked-up references already judged.
    picked: HashSet<u32>,
    last_tick: f64,
    crimes: std::collections::BTreeMap<Kind, (u32, u32)>,
    turned: usize,
    warnings: usize,
    karma_changes: usize,
    karma_first: Option<f64>,
    karma_last: Option<f64>,
    faction_enemies: usize,
}

/// What the player did since the last frame, told to the game.
#[allow(clippy::too_many_arguments)]
fn report_deeds(
    mut changed: MessageReader<RefChanged>,
    mut taken: MessageReader<ItemsTaken>,
    mut locks: MessageReader<LockOpened>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    mut stats: ResMut<CrimeStats>,
) {
    let picked: Vec<u32> = changed.read().map(|c| c.form_id).collect();
    let taken: Vec<ItemsTaken> = taken.read().cloned().collect();
    let locks: Vec<LockOpened> = locks.read().copied().collect();
    if picked.is_empty() && taken.is_empty() && locks.is_empty() {
        return;
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let mut items = Vec::new();
    for id in picked {
        let Some(s) = sim.world.peek(FormId(id)) else { continue };
        let item = matches!(sim.forms.get(s.base).map(|b| &b.data), Some(BaseData::Item(_)));
        // Gone into the player's inventory, not removed by a script.
        let carried = sim.world.peek(FormId::PLAYER_REF).and_then(|p| p.inventory.as_ref()).is_some_and(|inv| inv.count(s.base) > 0);
        if item && s.deleted() && carried && stats.picked.insert(id) {
            items.push((FormId(id), s.base, s.count.max(1)));
        }
    }
    with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        for (r, base, count) in items {
            if g.player_stole(Some(r), None, base, count) {
                info!("crime: the player took {} ({r}) x{count}, owned by someone else", g.data.name(base));
            }
        }
        for t in taken {
            // An actor's inventory (a corpse searched) is not owned through its cell.
            let actor = g.world.placed.get(t.container).is_some_and(|p| matches!(&p.sig, b"ACHR" | b"ACRE"));
            if actor {
                continue;
            }
            for (item, count) in t.items {
                if g.player_stole(None, Some(t.container), item, count) {
                    info!("crime: the player took {} x{count} out of {}, owned by someone else", g.data.name(item), t.container);
                }
            }
        }
        for l in locks {
            if let Some(o) = g.player_opened_lock(FormId(l.reference)) {
                info!("crime: the player opened the lock of {} ({}), owned by {}", FormId(l.reference), l.how.word(), g.data.name(o.owner));
            }
        }
    });
}

/// Trespass and aggro radius warnings and attacks, four times a second of game time.
fn tick(mut sim: ResMut<SimState>, mut script: Option<ResMut<ScriptState>>, mut stats: ResMut<CrimeStats>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let now = rt.game.time;
    if now - stats.last_tick < TICK && now >= stats.last_tick {
        return;
    }
    stats.last_tick = now;
    with_world(rt, &mut sim, |rt| {
        rt.game.crime_tick();
        // Aggro radius warnings and attacks, on the same clock (`fnv_game::aggro`).
        rt.game.aggro_tick();
    });
}

fn name(g: &fnv_game::Game, id: FormId) -> String {
    if id == FormId::PLAYER_REF {
        "the player".into()
    } else {
        format!("{} {id}", g.data.name(id))
    }
}

/// Log what the game noted; karma changes on screen and heard.
fn show_notes(
    mut script: Option<ResMut<ScriptState>>,
    sim: Res<SimState>,
    mut stats: ResMut<CrimeStats>,
    corner: Option<ResMut<CornerMessages>>,
    mut sounds: MessageWriter<PlaySound>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let notes = rt.game.crimes.take_notes();
    if notes.is_empty() {
        return;
    }
    let mut corner = corner;
    let g = &mut rt.game;
    let major = g.crime_settings().killing_evil.abs();
    for n in notes {
        match n {
            Note::Karma { change, now, why } => {
                stats.karma_changes += 1;
                stats.karma_first.get_or_insert(now - change);
                stats.karma_last = Some(now);
                info!("crime: karma {change:+} for {why}, now {now}");
                if change == 0.0 {
                    continue;
                }
                let (text, image, sound) = match (change > 0.0, change.abs() >= major) {
                    (true, true) => ("sKarmaMajorGained", "sKarmaMajorGainedImage", "UIKarmaUp"),
                    (true, false) => ("sKarmaMinorGained", "sKarmaMinorGainedImage", "UIKarmaUp"),
                    (false, true) => ("sKarmaMajorLost", "sKarmaMajorLostImage", "UIKarmaDown"),
                    (false, false) => ("sKarmaMinorLost", "sKarmaMinorLostImage", "UIKarmaDown"),
                };
                let setting = |n: &str| match sim.forms.settings.get(&n.to_ascii_lowercase()) {
                    Some(fnv_sim::settings::Setting::Str(s)) => Some(s.clone()),
                    _ => None,
                };
                let line = setting(text).unwrap_or_else(|| if change > 0.0 { "You gained Karma".into() } else { "You lost Karma".into() });
                if let Some(c) = corner.as_deref_mut() {
                    c.push(Corner { text: line, icon: setting(image).map(|i| i.to_ascii_lowercase()), seconds: None });
                }
                if let Some(s) = sim.forms.by_edid(sound) {
                    sounds.write(PlaySound { sound: s, at: None });
                }
            }
            Note::Crime { kind, victim, seen_by, hostile } => {
                let e = stats.crimes.entry(kind).or_default();
                e.0 += 1;
                e.1 += u32::from(!seen_by.is_empty());
                stats.turned += hostile.len();
                let seen: Vec<String> = seen_by.iter().map(|&w| name(g, w)).collect();
                let turned: Vec<String> = hostile.iter().map(|&w| name(g, w)).collect();
                info!(
                    "crime: {} against {}; seen by [{}]; turned on the player: [{}]",
                    kind.name(),
                    victim.map_or("nobody".into(), |v| name(g, v)),
                    seen.join(", "),
                    turned.join(", ")
                );
            }
            Note::Warned { actor, level } => {
                stats.warnings += 1;
                info!("crime: {} warns the trespassing player (warning {level})", name(g, actor));
            }
            // A new title shows its box (`crate::reputation`).
            Note::Infamy { reputation, before, after } => info!("crime: infamy with {} {before} -> {after}", crate::reputation::name(g, reputation)),
            Note::FactionEnemy { faction } => {
                stats.faction_enemies += 1;
                info!("crime: the player is now an enemy of {}", g.data.name(faction));
            }
            Note::Trespassing { cell, owner, now } => {
                let what = if now { "trespasses in" } else { "left" };
                info!("crime: the player {what} {} (owned by {})", g.data.name(cell), g.data.name(owner));
            }
            Note::NoAlarm { door, cell } => info!("crime: the player came through {door}, a No Alarm door, into {}: no trespass alarm there", g.data.name(cell)),
        }
    }
}

fn summary(mut exit: MessageReader<AppExit>, stats: Res<CrimeStats>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = &*stats;
    let count = |k: Kind| s.crimes.get(&k).copied().unwrap_or_default();
    let (theft, lock, trespass, assault, murder) = (count(Kind::Theft), count(Kind::Lock), count(Kind::Trespass), count(Kind::Assault), count(Kind::Murder));
    let karma = match (s.karma_first, s.karma_last) {
        (Some(a), Some(b)) => format!("from {a} to {b}"),
        _ => "unchanged".into(),
    };
    info!(
        "crime summary: {} thefts ({} seen), {} owned locks opened ({} seen), {} trespasses, {} assaults, {} murders ({} seen); {} actors turned on the player, {} trespass warnings, {} factions made enemies; karma changed {} times, {karma}",
        theft.0, theft.1, lock.0, lock.1, trespass.0, assault.0, murder.0, murder.1, s.turned, s.warnings, s.faction_enemies, s.karma_changes
    );
}
