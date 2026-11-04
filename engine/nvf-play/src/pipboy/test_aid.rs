//! Test aid for the Pip-Boy: `NVF_PLAY_TEST_PIPBOY=<seconds>:<step>[,...]`, steps run in
//! time order (seconds after the start, as the other aids count them):
//!
//! - `open`: press Tab (the inventory opens and the Pip-Boy comes up);
//! - `close`: put it away;
//! - `page=stats|items|data`: show a page;
//! - `tab=<0-4>`: show a tab of the page shown;
//! - `key=<down|up|use|drop|pgdn|back>`: a key of the ITEMS list (`back`: Backspace,
//!   out of a Misc folder);
//! - `point=<x>:<y>`: put the pointer on that point of the canvas (1024 x 768 from its
//!   top left), through the model's screen where it is shown, as the mouse would be;
//!   `point=stats|items|data` puts it on that page's button on the device, with no click;
//!   `point=off` gives the pointer back to the mouse;
//! - `click=<x>:<y>`: the same, then a left click there;
//! - `click=stats|items|data`: a left click on the page's button on the device;
//! - `rclick=<x>:<y>`: as `click=`, with the right button (on an ITEMS row: drop);
//! - `drag=<dx>:<dy>`: drag the map shown by that much (canvas units), as the left button
//!   held on it and moved would;
//! - `repair=<open|down|up|use|cancel>`: R on the ITEMS page's selected item, and the
//!   repair list's keys (`super::repair`);
//! - `mod=<open|down|up|use|cancel>`: X on the ITEMS page's selected weapon, and the mod
//!   menu's keys (`super::item_mod`);
//! - `travel=<marker FormID>`: what a click on that map marker of the World Map does (the
//!   fast-travel question, docs/spec-notes/fast-travel.md); `key=use` takes the chosen
//!   answer (Yes at first), `key=down` moves to No.
//! - `note=<note FormID>`: on the DATA page's Notes tab, select that note and use it as a
//!   click on its row would (a holotape plays or stops, `crate::holotape`).
//! - `hotkey=<1-8>`: hold that hotkey's key on the ITEMS page (its panel shows and a
//!   click assigns, `super::hotkey_panel`) until `hotkey=off`.
//! - `frame=screen|device`: the Gameplay setting Use Pip-Boy off or on, as the settings
//!   page sets it (the Pip-Boy and its glove taken off or put on).
//!
//! Each step is logged. Screenshots are `NVF_PLAY_TEST_SHOTS`'.

use super::data::travel::FastTravelAsk;
use super::{AidTarget, Page, PipBoyAid, PipBoyInput};
use crate::menu::model::Command;
use crate::menu::{MenuInput, OpenInventory};
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    Open,
    Close,
    Page(Page),
    Tab(usize),
    Key(Command),
    Point(Option<AidTarget>),
    /// A click on the target; true with the right button.
    Click(AidTarget, bool),
    Drag(Vec2),
    Travel(u32),
    Note(u32),
    Repair(super::repair::RepairInput),
    Mod(super::item_mod::ModInput),
    /// Hold a hotkey's key (its slot), or let go.
    Hotkey(Option<usize>),
    /// Use Pip-Boy off (`true`, the screen menu) or on.
    Frame(bool),
}

fn page(p: &str) -> Option<Page> {
    match p.trim() {
        "stats" => Some(Page::Stats),
        "items" => Some(Page::Items),
        "data" => Some(Page::Data),
        _ => None,
    }
}

fn canvas(p: &str) -> Option<Vec2> {
    let (x, y) = p.split_once(':')?;
    Some(Vec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

#[derive(Resource)]
struct Script(Vec<(f32, Step)>);

fn step(s: &str) -> Option<Step> {
    let s = s.trim();
    Some(match s {
        "open" => Step::Open,
        "close" => Step::Close,
        _ => match s.split_once('=')? {
            ("page", p) => Step::Page(page(p)?),
            ("point", "off") => Step::Point(None),
            ("point", p) => Step::Point(Some(page(p).map(AidTarget::Button).or_else(|| canvas(p).map(AidTarget::Canvas))?)),
            ("drag", p) => Step::Drag(canvas(p)?),
            ("travel", m) => Step::Travel(u32::from_str_radix(m.trim().trim_start_matches("0x"), 16).ok()?),
            ("note", m) => Step::Note(u32::from_str_radix(m.trim().trim_start_matches("0x"), 16).ok()?),
            ("click", p) => Step::Click(page(p).map(AidTarget::Button).or_else(|| canvas(p).map(AidTarget::Canvas))?, false),
            ("rclick", p) => Step::Click(AidTarget::Canvas(canvas(p)?), true),
            ("repair", r) => Step::Repair(match r.trim() {
                "open" => super::repair::RepairInput::Open,
                "down" => super::repair::RepairInput::Down,
                "up" => super::repair::RepairInput::Up,
                "use" => super::repair::RepairInput::Use,
                "cancel" => super::repair::RepairInput::Cancel,
                _ => return None,
            }),
            ("mod", r) => Step::Mod(match r.trim() {
                "open" => super::item_mod::ModInput::Open,
                "down" => super::item_mod::ModInput::Down,
                "up" => super::item_mod::ModInput::Up,
                "use" => super::item_mod::ModInput::Use,
                "cancel" => super::item_mod::ModInput::Cancel,
                _ => return None,
            }),
            ("frame", f) => Step::Frame(match f.trim() {
                "screen" => true,
                "device" => false,
                _ => return None,
            }),
            ("hotkey", "off") => Step::Hotkey(None),
            ("hotkey", n) => Step::Hotkey(Some(n.trim().parse::<usize>().ok().filter(|n| (1..=8).contains(n))? - 1)),
            ("tab", t) => Step::Tab(t.trim().parse().ok().filter(|t| *t < 5)?),
            ("key", k) => Step::Key(match k.trim() {
                "down" => Command::Down,
                "up" => Command::Up,
                "use" => Command::One,
                "drop" => Command::Drop,
                "pgdn" => Command::PageDown,
                "back" => Command::Back,
                _ => return None,
            }),
            _ => return None,
        },
    })
}

fn parse(v: &str) -> Vec<(f32, Step)> {
    let mut out: Vec<(f32, Step)> = v
        .split(',')
        .filter_map(|item| {
            let (t, s) = item.split_once(':')?;
            let parsed = t.trim().parse().ok().zip(step(s));
            if parsed.is_none() {
                warn!("NVF_PLAY_TEST_PIPBOY: cannot read {item:?}");
            }
            parsed
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

pub(super) fn setup(app: &mut App) {
    let Ok(v) = std::env::var("NVF_PLAY_TEST_PIPBOY") else { return };
    app.insert_resource(Script(parse(&v))).add_systems(Update, run.in_set(crate::PlaySet::Input));
}

#[allow(clippy::too_many_arguments)]
fn run(
    time: crate::pause::AidTime,
    mut script: ResMut<Script>,
    mut open: MessageWriter<OpenInventory>,
    mut menu: MessageWriter<MenuInput>,
    mut pip: MessageWriter<PipBoyInput>,
    mut aid: ResMut<PipBoyAid>,
    (mut ask, mut repair, mut modding): (MessageWriter<FastTravelAsk>, MessageWriter<super::repair::RepairInput>, MessageWriter<super::item_mod::ModInput>),
    mut hold: ResMut<super::hotkey_panel::HotkeyHold>,
    mut use_pipboy: ResMut<super::UsePipBoy>,
) {
    let Some(now) = time.world_secs() else { return };
    while let Some(&(t, s)) = script.0.first() {
        if t > now {
            break;
        }
        script.0.remove(0);
        info!("test aid: pipboy {s:?} at {now:.1} s");
        match s {
            Step::Open => {
                open.write(OpenInventory);
            }
            Step::Close => {
                menu.write(MenuInput(Command::Close));
            }
            Step::Page(p) => {
                pip.write(PipBoyInput::Page(p));
            }
            Step::Tab(t) => {
                pip.write(PipBoyInput::Tab(t));
            }
            Step::Key(c) => {
                menu.write(MenuInput(c));
                // One unit, also of a stack that asks "How many?".
                if c == Command::Drop {
                    menu.write(MenuInput(Command::Amount(1)));
                    menu.write(MenuInput(Command::AmountOk));
                }
            }
            Step::Point(p) => {
                aid.target = p;
            }
            Step::Click(t, right) => {
                aid.target = Some(t);
                aid.click = 3;
                aid.right = right;
            }
            Step::Drag(d) => {
                aid.drag += d;
            }
            Step::Travel(m) => {
                ask.write(FastTravelAsk(m));
            }
            Step::Note(n) => {
                aid.note = Some(n);
            }
            Step::Repair(r) => {
                repair.write(r);
            }
            Step::Mod(m) => {
                modding.write(m);
            }
            Step::Hotkey(n) => {
                hold.aid = n;
            }
            Step::Frame(screen) => {
                use_pipboy.set(!screen);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_read_in_time_order() {
        let s = parse("9:close, 3:open,4:page=stats, 5:tab=2, 6:key=down, 7:tab=9, x:open, 8:click=data, 8.5:click=100:650, 8.6:rclick=260:150, 8.7:point=off, 8.8:point=1:2, 8.85:point=items, 8.9:drag=-40:25, 8.95:travel=0x000CDF51, 8.96:note=001618BF, 8.97:repair=use, 8.98:mod=open, 8.99:hotkey=3, 8.995:hotkey=off, 8.996:hotkey=9, 8.997:frame=screen, 8.998:frame=device, 8.999:frame=up");
        assert_eq!(
            s,
            vec![
                (3.0, Step::Open),
                (4.0, Step::Page(Page::Stats)),
                (5.0, Step::Tab(2)),
                (6.0, Step::Key(Command::Down)),
                (8.0, Step::Click(AidTarget::Button(Page::Data), false)),
                (8.5, Step::Click(AidTarget::Canvas(Vec2::new(100.0, 650.0)), false)),
                (8.6, Step::Click(AidTarget::Canvas(Vec2::new(260.0, 150.0)), true)),
                (8.7, Step::Point(None)),
                (8.8, Step::Point(Some(AidTarget::Canvas(Vec2::new(1.0, 2.0))))),
                (8.85, Step::Point(Some(AidTarget::Button(Page::Items)))),
                (8.9, Step::Drag(Vec2::new(-40.0, 25.0))),
                (8.95, Step::Travel(0x000C_DF51)),
                (8.96, Step::Note(0x0016_18BF)),
                (8.97, Step::Repair(super::super::repair::RepairInput::Use)),
                (8.98, Step::Mod(super::super::item_mod::ModInput::Open)),
                (8.99, Step::Hotkey(Some(2))),
                (8.995, Step::Hotkey(None)),
                (8.997, Step::Frame(true)),
                (8.998, Step::Frame(false)),
                (9.0, Step::Close)
            ]
        );
    }
}
