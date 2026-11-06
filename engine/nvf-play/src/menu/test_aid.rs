//! Test aid for in-game checks of the menus without pressing keys.
//!
//! `NVF_PLAY_TEST_MENU=<container reference FormID, hex>` (or `nearest`: the container
//! nearest to the camera, as F9 picks it) runs a script through the same
//! paths as the keys (`OpenContainer`, `OpenInventory`, [`MenuInput`]): after
//! `NVF_PLAY_TEST_DELAY` seconds (default 6), one step per second, it opens the
//! container and logs both columns, takes all (which closes the menu, as in the game),
//! logs, opens the container again, stores one unit of the player's selected (first) row
//! back, logs, closes, opens the inventory, equips (or unequips)
//! the first weapon or armour, logs, drops one unit of it, logs, and closes. With
//! `NVF_PLAY_TEST_SHOT=<path.png>` it saves screenshots of the container menu (`<path>`)
//! and of the inventory (`<path stem>-inventory.png`).
//!
//! `NVF_PLAY_TEST_DROP=<seconds>:<item FormID hex>[:stack|:use|:ask|:<count>][,...]` drops (or
//! uses) a carried item through the inventory menu: at that time (once no menu is open) it
//! opens the inventory, half a second later selects the item's row and drops one unit
//! (`Drop`, a right click), the whole stack (`DropStack`, Shift + right click), `<count>` units (`Drop`,
//! then that amount and Ok on "How many?", which a stack of more than five asks; `ask`
//! leaves the prompt open until it is answered, for a screenshot or clicks) or uses
//! it (`One`, Enter: equips, or uses up an aid item) and logs the list, and half a second
//! later closes the menu. The other scripted checks answer "How many?" with 1.
//! Dropping a quest item is refused: the logged status says so and the item stays (give
//! one first with `NVF_PLAY_TEST_GIVE`, e.g. The Platinum Chip, 001164FB).
//!
//! `NVF_PLAY_TEST_BARTER=<merchant reference FormID, hex>[:discount]` opens the barter
//! menu with that merchant as `ShowBarterMenu` would (`OpenBarter`), after
//! `NVF_PLAY_TEST_DELAY` seconds (default 6), one step per second: logs both lists,
//! puts up the player's first row, logs the trade, puts up the merchant's first row,
//! logs, accepts, logs, and exits. With `NVF_PLAY_TEST_SHOT=<path.png>` it saves the
//! menu as opened (`<path stem>-barter.png`) and with the two rows put up
//! (`<path stem>-barter-trade.png`). With `*` for the merchant nothing is opened: the
//! steps wait for the next barter menu a script opens (a dialogue choice's
//! `ShowBarterMenu`, e.g. with `NVF_PLAY_TEST_TALK`). `NVF_PLAY_TEST_BARTER_DELAY`, when
//! set, is the barter check's delay instead, so it can run beside the container check.

use super::model::{Command, MenuModel, Mode, Side};
use super::{ActiveMenu, MenuInput, OpenBarter, OpenInventory, OpenNearestContainer};
use crate::OpenContainer;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use fnv_sim::forms::ItemKind;
use fnv_sim::FormId;
use std::path::PathBuf;

#[derive(Resource)]
pub(super) struct Script {
    /// `None`: the nearest container.
    container: Option<FormId>,
    shot: Option<PathBuf>,
    delay: f32,
    step: usize,
    next_at: Option<f32>,
    /// Times the inventory was asked for again because it did not open.
    retries: u32,
    /// The container the menu showed, to open again after Take All closed it.
    opened: Option<FormId>,
}

/// The barter check (`NVF_PLAY_TEST_BARTER`).
#[derive(Resource)]
pub(super) struct BarterScript {
    /// `None`: the next barter menu a script opens.
    merchant: Option<FormId>,
    discount: i32,
    shot: Option<PathBuf>,
    delay: f32,
    step: usize,
    next_at: Option<f32>,
}

fn setup_barter(app: &mut App) {
    let Ok(v) = std::env::var("NVF_PLAY_TEST_BARTER") else { return };
    let (id, discount) = v.trim().split_once(':').map_or((v.trim(), "0"), |(a, b)| (a, b));
    let merchant = if id.trim() == "*" { Ok(None) } else { id.parse::<FormId>().map(Some) };
    let (merchant, discount) = match (merchant, discount.trim().parse::<i32>()) {
        (Ok(m), Ok(d)) => (m, d),
        _ => {
            warn!("NVF_PLAY_TEST_BARTER: expected <merchant FormID>[:discount], got {v:?}");
            return;
        }
    };
    // Its own delay, so that it can run beside the container check.
    let delay = ["NVF_PLAY_TEST_BARTER_DELAY", "NVF_PLAY_TEST_DELAY"].iter().find_map(|k| std::env::var(k).ok().and_then(|d| d.parse().ok())).unwrap_or(6.0);
    let shot = std::env::var_os("NVF_PLAY_TEST_SHOT").map(PathBuf::from);
    app.insert_resource(BarterScript { merchant, discount, shot, delay, step: 0, next_at: None });
}

fn shot_named(p: &std::path::Path, suffix: &str) -> PathBuf {
    let stem = p.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    p.with_file_name(format!("{stem}-{suffix}.png"))
}

fn log_trade(label: &str, m: Option<&MenuModel>) {
    log_menu(label, m);
    if let Some(t) = m.and_then(|m| m.trade.as_ref()) {
        info!("test aid [{label}]   caps: merchant {}, player {}; balance {} (positive: the player is paid)", t.purse[0], t.purse[1], t.balance);
    }
}

pub(super) fn run_barter(
    time: crate::pause::AidTime,
    script: Option<ResMut<BarterScript>>,
    active: Res<ActiveMenu>,
    mut commands: Commands,
    mut open: MessageWriter<OpenBarter>,
    mut input: MessageWriter<MenuInput>,
) {
    let Some(mut s) = script else { return };
    let Some(now) = time.world_secs() else { return };
    if now < s.next_at.unwrap_or(s.delay) {
        return;
    }
    s.next_at = Some(now + 1.0);
    let m = active.0.as_ref();
    let mut send = |c| {
        input.write(MenuInput(c));
    };
    match s.step {
        0 => match s.merchant {
            Some(merchant) => {
                info!("test aid: barter with {merchant} (discount {})", s.discount);
                open.write(OpenBarter { merchant: merchant.0, discount: s.discount });
            }
            None if m.is_some_and(|m| matches!(m.mode, Mode::Barter(_))) => {
                info!("test aid: a script opened the barter menu");
                s.next_at = Some(now + 0.5);
            }
            None => {
                s.next_at = Some(now + 0.25);
                return;
            }
        },
        1 => {
            log_trade("barter opened", m);
            if let Some(p) = s.shot.clone() {
                screenshot(&mut commands, shot_named(&p, "barter"));
            }
        }
        2 => {
            send(Command::Hover(Side::Player, 0));
            send(Command::One);
            // One unit, also of a stack that asks "How many?".
            send(Command::Amount(1));
            send(Command::AmountOk);
        }
        3 => {
            log_trade("player's first row put up", m);
            // The merchant's own first row (the row put up is listed above it).
            let own = m.and_then(|m| m.list(Side::Container).rows.iter().position(|r| !r.bartered)).unwrap_or(0);
            send(Command::Hover(Side::Container, own));
            send(Command::One);
            send(Command::Amount(1));
            send(Command::AmountOk);
        }
        4 => {
            log_trade("merchant's first row put up", m);
            if let Some(p) = s.shot.clone() {
                screenshot(&mut commands, shot_named(&p, "barter-trade"));
            }
        }
        5 => send(Command::Accept),
        6 => log_trade("after accept", m),
        7 => send(Command::Close),
        8 => {
            log_menu("barter closed", m);
            info!("test aid: barter done");
        }
        _ => {
            commands.remove_resource::<BarterScript>();
            return;
        }
    }
    s.step += 1;
}

pub(super) fn setup(app: &mut App) {
    setup_drops(app);
    setup_barter(app);
    let Ok(v) = std::env::var("NVF_PLAY_TEST_MENU") else { return };
    let container = match v.trim() {
        "nearest" => None,
        v => match v.parse::<FormId>() {
            Ok(id) => Some(id),
            Err(e) => {
                warn!("NVF_PLAY_TEST_MENU: {e}");
                return;
            }
        },
    };
    let delay = std::env::var("NVF_PLAY_TEST_DELAY").ok().and_then(|d| d.parse().ok()).unwrap_or(6.0);
    let shot = std::env::var_os("NVF_PLAY_TEST_SHOT").map(PathBuf::from);
    app.insert_resource(Script { container, shot, delay, step: 0, next_at: None, retries: 0, opened: None });
}

/// One scheduled drop (`NVF_PLAY_TEST_DROP`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DropJob {
    at: f32,
    item: FormId,
    /// `Drop`, `DropStack` or `One` (use / equip).
    cmd: Command,
    /// What a `Drop` answers "How many?" with (1 unless the job names a number); 0
    /// leaves the prompt open (`ask`, for a screenshot) until it is answered; the menu
    /// then closes as after the other drops.
    amount: i32,
}

pub(super) fn parse_drops(v: &str) -> Result<Vec<DropJob>, String> {
    crate::test_aid::timed(v)?
        .into_iter()
        .map(|(at, rest)| {
            let (item, cmd, amount) = match rest.split_once(':') {
                Some((i, "stack")) => (i, Command::DropStack, 1),
                Some((i, "use")) => (i, Command::One, 1),
                Some((i, "ask")) => (i, Command::Drop, 0),
                Some((i, n)) if n.parse::<i32>().is_ok_and(|n| n > 0) => (i, Command::Drop, n.parse().expect("checked")),
                Some((_, x)) => return Err(format!("{rest:?}: expected <item>[:stack|:use|:ask|:<count>], not {x:?}")),
                None => (rest, Command::Drop, 1),
            };
            let item = item.parse::<FormId>().map_err(|e| format!("{rest:?}: {e}"))?;
            Ok(DropJob { at, item, cmd, amount })
        })
        .collect()
}

/// Scheduled drops, soonest last, and the one in progress (its stage and when the next
/// stage is due).
#[derive(Resource)]
pub(super) struct Drops {
    jobs: Vec<DropJob>,
    current: Option<(DropJob, u8, f32)>,
    /// Times the inventory was asked for again because it did not open.
    retries: u32,
}

fn setup_drops(app: &mut App) {
    let Ok(v) = std::env::var("NVF_PLAY_TEST_DROP") else { return };
    match parse_drops(&v) {
        Ok(mut jobs) => {
            jobs.sort_by(|a, b| b.at.total_cmp(&a.at));
            app.insert_resource(Drops { jobs, current: None, retries: 0 });
        }
        Err(e) => warn!("NVF_PLAY_TEST_DROP: {e}"),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_drops(
    time: crate::pause::AidTime,
    drops: Option<ResMut<Drops>>,
    active: Res<ActiveMenu>,
    mut commands: Commands,
    mut open_inventory: MessageWriter<OpenInventory>,
    mut input: MessageWriter<MenuInput>,
    pip: Option<ResMut<crate::pipboy::PipBoy>>,
    sim: Res<crate::sim::SimState>,
    script: Option<Res<crate::script::ScriptState>>,
) {
    let Some(mut d) = drops else { return };
    let Some(now) = time.world_secs() else { return };
    let m = active.0.as_ref();
    match d.current {
        None => {
            if !d.jobs.last().is_some_and(|j| j.at <= now) {
                if d.jobs.is_empty() {
                    commands.remove_resource::<Drops>();
                }
                return;
            }
            // Wait until no menu is open, a script's message box included (the inventory
            // would not open over it), and until the scripts run (an aid item's effects
            // are theirs).
            let script_menu = script.as_ref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.menu.is_some());
            if m.is_some() || script_menu || script.as_ref().is_some_and(|s| s.loading()) {
                return;
            }
            let job = d.jobs.pop().expect("checked");
            info!("test aid: {:?} {}: open the inventory", job.cmd, job.item);
            open_inventory.write(OpenInventory);
            d.current = Some((job, 1, now + 0.5));
        }
        Some((job, stage, due)) => {
            if now < due {
                return;
            }
            match (stage, m) {
                (1 | 3, Some(m)) if m.mode == Mode::Inventory => {
                    match m.list(Side::Player).rows.iter().position(|r| r.item == job.item) {
                        Some(i) => {
                            // Selecting the selected row would use (equip) it: only when needed.
                            if m.selected[Side::Player as usize] != i {
                                input.write(MenuInput(Command::Select(Side::Player, i)));
                            }
                            input.write(MenuInput(job.cmd));
                            // A stack of more than five asks "How many?" first.
                            if job.cmd == Command::Drop && job.amount > 0 {
                                input.write(MenuInput(Command::Amount(job.amount)));
                                input.write(MenuInput(Command::AmountOk));
                            }
                        }
                        // The Pip-Boy's ITEMS page lists one tab's kind of item: turn to
                        // the item's tab and look again.
                        None if stage == 1 && pip.is_some() => {
                            let kind = sim.forms.sig(job.item).and_then(|s| ItemKind::from_sig(&s));
                            if let (Some(mut p), Some(tab)) = (pip, crate::pipboy::items::TABS.iter().position(|c| c.holds_item(kind, sim.forms.item(job.item)))) {
                                p.tabs[crate::pipboy::Page::Items.index()] = tab;
                            }
                            d.current = Some((job, 3, now + 0.3));
                            return;
                        }
                        None => info!("test aid: {} is not carried", job.item),
                    }
                    d.current = Some((job, 2, now + 0.5));
                }
                // `ask`: the prompt stays open until answered (a screenshot, a pointer's
                // clicks; T15a's `tiles-quantity`), then the menu closes as for the others.
                (2, Some(m)) if job.cmd == Command::Drop && job.amount == 0 && m.quantity.is_some() => {
                    d.current = Some((job, 2, now + 0.5));
                }
                (2, m) => {
                    log_menu("after test drop", m);
                    // An open "How many?" takes the first Close (as its Cancel).
                    if m.is_some_and(|m| m.quantity.is_some()) {
                        input.write(MenuInput(Command::Close));
                    }
                    input.write(MenuInput(Command::Close));
                    d.current = None;
                }
                // A script's box closed in the same frame and the inventory request
                // was ignored (the game had not left the menu yet): ask again (a few
                // times), as the container aid does.
                (1, None) if d.retries < 5 => {
                    d.retries += 1;
                    info!("test aid: drop {}: the inventory did not open: asking again", job.item);
                    d.jobs.push(job);
                    d.current = None;
                }
                _ => {
                    info!("test aid: drop {}: the inventory did not open", job.item);
                    d.current = None;
                }
            }
        }
    }
}

fn log_menu(label: &str, m: Option<&MenuModel>) {
    let Some(m) = m else {
        info!("test aid [{label}]: no menu open");
        return;
    };
    info!("test aid [{label}]: {:?} \"{}\", status {:?}", m.mode, m.title, m.status);
    let sides: &[Side] = match m.mode {
        Mode::Container(_) | Mode::Barter(_) => &[Side::Container, Side::Player],
        Mode::Inventory => &[Side::Player],
    };
    for &side in sides {
        let l = m.list(side);
        let rows: Vec<String> = l
            .rows
            .iter()
            .map(|r| {
                let price = r.price.map(|p| format!(" at {}", super::barter::price_text(p))).unwrap_or_default();
                format!("{}{}{} x{}{price}", if r.equipped { "*" } else { "" }, if r.bartered { "(up) " } else { "" }, r.name, r.count)
            })
            .collect();
        info!("test aid [{label}]   {side:?}: {} stacks, weight {:.1}, value {}: {}", l.rows.len(), l.weight, l.value, rows.join(", "));
    }
    match &m.encumbrance {
        Ok(e) => info!("test aid [{label}]   encumbrance {:.1} / {:.0} over {}", e.weight, e.capacity, e.over),
        Err(e) => info!("test aid [{label}]   encumbrance unknown: {e}"),
    }
}

fn screenshot(commands: &mut Commands, path: PathBuf) {
    info!("test aid: screenshot {}", path.display());
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    time: crate::pause::AidTime,
    script: Option<ResMut<Script>>,
    active: Res<ActiveMenu>,
    mut commands: Commands,
    mut open_container: MessageWriter<OpenContainer>,
    mut open_nearest: MessageWriter<OpenNearestContainer>,
    mut open_inventory: MessageWriter<OpenInventory>,
    mut input: MessageWriter<MenuInput>,
    scripts: Option<Res<crate::script::ScriptState>>,
) {
    let Some(mut s) = script else { return };
    let Some(now) = time.world_secs() else { return };
    let due = s.next_at.unwrap_or(s.delay);
    if now < due {
        return;
    }
    // The steps that open a menu wait while a script's message box is up (the menu
    // would not open over it) or the scripts are still loading (their first frames
    // show the add-on packs' boxes and give their items), as the drop aid does. On a
    // busy machine the container check ran into those boxes: the inventory never
    // opened and the ui-sounds check lost its four sounds.
    let script_menu = scripts.as_ref().and_then(|x| x.rt.as_ref()).is_some_and(|rt| rt.game.menu.is_some());
    let loading = scripts.as_ref().is_some_and(|x| x.loading());
    if matches!(s.step, 0 | 7) && (script_menu || loading || (s.step == 7 && active.0.is_some())) {
        return;
    }
    s.next_at = Some(now + 1.0);
    let m = active.0.as_ref();
    let mut send = |c| {
        input.write(MenuInput(c));
    };
    match s.step {
        0 => {
            match s.container {
                Some(c) => {
                    info!("test aid: open container {c}");
                    open_container.write(OpenContainer { container: c.0 });
                }
                None => {
                    info!("test aid: open the nearest container");
                    open_nearest.write(OpenNearestContainer);
                }
            }
        }
        1 => {
            log_menu("opened", m);
            s.opened = m.and_then(|m| match m.mode {
                Mode::Container(c) => Some(c),
                _ => None,
            });
            if let Some(p) = s.shot.clone() {
                screenshot(&mut commands, p);
            }
        }
        2 => send(Command::TakeAll),
        3 => {
            log_menu("after take all", m);
            // Take All closes the menu (list 5 item 60): open the container again to
            // store one back.
            if let (None, Some(c)) = (m, s.opened) {
                info!("test aid: Take All closed the menu; open container {c} again");
                open_container.write(OpenContainer { container: c.0 });
            }
        }
        4 => {
            send(Command::Focus(Side::Player));
            send(Command::One);
            send(Command::Amount(1));
            send(Command::AmountOk);
        }
        5 => log_menu("after storing one back", m),
        6 => send(Command::Close),
        7 => {
            log_menu("closed", m);
            open_inventory.write(OpenInventory);
        }
        // A script's box came up in the same frame and the inventory did not open: ask
        // again (a few times).
        8 if m.is_none() && s.retries < 5 => {
            s.retries += 1;
            info!("test aid: the inventory did not open: asking again");
            s.step = 7;
            return;
        }
        8 => log_menu("inventory", m),
        9 => {
            let wearable = m.and_then(|m| {
                m.list(Side::Player).rows.iter().position(|r| matches!(r.kind, Some(ItemKind::Weapon | ItemKind::Armor)))
            });
            match (wearable, m) {
                (Some(i), Some(m)) if m.selected[Side::Player as usize] != i => {
                    send(Command::Select(Side::Player, i));
                    send(Command::One);
                }
                (Some(_), Some(_)) => send(Command::One),
                _ => info!("test aid: no weapon or armour to equip"),
            }
        }
        10 => {
            log_menu("after equip", m);
            send(Command::Drop);
            send(Command::Amount(1));
            send(Command::AmountOk);
        }
        11 => {
            log_menu("after drop", m);
            if let Some(p) = s.shot.clone() {
                let stem = p.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
                screenshot(&mut commands, p.with_file_name(format!("{stem}-inventory.png")));
            }
        }
        12 => send(Command::Close),
        13 => {
            log_menu("closed", m);
            info!("test aid: done");
        }
        _ => {
            commands.remove_resource::<Script>();
            return;
        }
    }
    s.step += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_jobs() {
        let j = parse_drops("12:00015169, 20.5:0x00015169:stack").unwrap();
        assert_eq!(j, vec![
            DropJob { at: 12.0, item: FormId(0x15169), cmd: Command::Drop, amount: 1 },
            DropJob { at: 20.5, item: FormId(0x15169), cmd: Command::DropStack, amount: 1 },
        ]);
        assert_eq!(parse_drops("3:00015169:use").unwrap()[0].cmd, Command::One);
        assert_eq!(parse_drops("3:0008ED03:7").unwrap()[0], DropJob { at: 3.0, item: FormId(0x8ED03), cmd: Command::Drop, amount: 7 });
        assert!(parse_drops("3:0008ED03:0").is_err());
        assert_eq!(parse_drops("3:0008ED03:ask").unwrap()[0].amount, 0);
        assert!(parse_drops("12:00015169:all").is_err());
        assert!(parse_drops("12:zz").is_err());
    }
}
