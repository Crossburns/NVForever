//! The quest lines at the left of the view, drawn as the original draws them
//! (docs/spec-notes/quest-messages.md): "Quest added", "Quest completed" or "Quest failed"
//! over the quest's name in large capitals, and the objectives a script shows or
//! completes, each behind a box (filled when completed).
//!
//! - What comes here: [`QuestNotice`]s of `script_events::effects` (`ScriptEffect::Quest`).
//!   They no longer go to the corner messages at the top left (§1).
//! - One at a time, in the order they came; objectives that come before their list is
//!   on screen join that list [design, §5]. [`QuestMessages`] is the queue (pure, tested).
//! - [`layout`] places the text and the boxes (pure, tested); the numbers are measured on
//!   the owner's screenshots (§4) and the HUD menu file's (§2).
//! - While a message box, a script's menu or any other menu is open (any state but
//!   `PlayState::Playing`) the area is hidden and its clock stands still, as the corner
//!   messages' (`crate::corner_messages::held`).

use crate::interface::{amber, at, bc_supported, face, plain_font, set_text, Face, Pictures, Set};
use crate::message_menu::MessagePanel;
use crate::script_events::{Objective, QuestNotice};
use crate::sfx::{PlaySound, Sfx};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_font::{Font, Line, Placed};
use fnv_sim::FormId;
use nvf_assets::GameData;
use std::collections::{HashMap, VecDeque};

// ---- measured on the owner's screenshots [observed, §4] --------------------------------

/// Where the text starts, from the view's left edge, in the interface's units.
pub(crate) const TEXT_X: f32 = 45.0;
/// The name's letters carry more glow before them: it starts this much further left.
pub(crate) const TITLE_X: f32 = -1.5;
/// The line "Quest added" stands on, and the quest's name.
pub(crate) const LABEL_BASE: f32 = 247.0;
pub(crate) const TITLE_BASE: f32 = 311.0;
/// The line the first objective stands on; the next lines are the font's size and
/// [`LEADING`] lower.
pub(crate) const LIST_BASE: f32 = 263.0;
pub(crate) const LEADING: f32 = 2.0;
/// An objective's text is broken at this width [hypothesis: the screenshots' lines break
/// between 556 and 590 units, §4].
pub(crate) const WRAP: f32 = 570.0;
/// The box before an objective, from where its first line starts and the line it stands
/// on: left, top, width, height. Both boxes are drawn 26 wide, the filled one's size in
/// the HUD file; the open one's 16 there would draw its square half as large as the
/// screenshots show it (§4).
pub(crate) const BOX: [f32; 4] = [-16.0, -21.0, 26.0, 26.0];

// ---- the HUD menu file's [documented: menus\main\hud_main_menu.xml, §2] -----------------

const OPEN_BOX_FILE: &str = "interface\\shared\\marker\\glow_square_small.dds";
const DONE_BOX_FILE: &str = "interface\\shared\\marker\\glow_square_filled_small.dds";

/// Font 7, as the HUD's text templates [documented: hudtemplates.xml; Fallout_default.ini
/// `sFontFile_7`], and font 8 for the quest's name [observed: only its capitals fit the
/// screenshots' name].
const LABEL_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";
const TITLE_FONT: &str = "textures\\fonts\\glow_futura_caps_large.fnt";

// ---- NVForever's own [design, §5] -------------------------------------------------------

/// The seconds "Quest added" and the name stay.
const BANNER_SECONDS: f32 = 4.0;
/// The seconds a list of objectives stays: this for the first, and more for each other.
const LIST_SECONDS: f32 = 4.0;
const LIST_SECONDS_MORE: f32 = 1.5;
/// The seconds a line takes to come and, after its time, to go.
const FADE: f32 = 0.25;
/// The name comes in from left to right over this many seconds [hypothesis: the
/// screenshot of "Quest added" shows the name's right end fainter, §4].
const SWEEP: f32 = 0.6;
const MAX_WAITING: usize = 32;

/// The words over the name [observed: "Quest added", "Quest completed"; "Quest failed"
/// is NVForever's by analogy].
pub(crate) fn label(n: &QuestNotice) -> Option<&'static str> {
    match n {
        QuestNotice::Added(_) => Some("Quest added"),
        QuestNotice::Completed(_) => Some("Quest completed"),
        QuestNotice::Failed(_) => Some("Quest failed"),
        // [observed: the owner's screenshot "Discovered", the same place and fonts as
        // "Quest added"].
        QuestNotice::Discovered(_) => Some("You have discovered"),
        QuestNotice::Objectives(_) => None,
    }
}

/// The sound a line comes with [hypothesis: by the SOUN records' names, §3].
fn sound(n: &QuestNotice) -> Option<&'static str> {
    match n {
        QuestNotice::Added(_) => Some("UIPopUpQuestNew"),
        QuestNotice::Completed(_) => Some("UIPopUpQuestComplete"),
        QuestNotice::Failed(_) => None,
        QuestNotice::Discovered(_) => Some("UIPopUpMapMarkerAdded"),
        QuestNotice::Objectives(_) => Some("UIQuestUpdate"),
    }
}

pub(crate) fn objective_text(o: &Objective) -> String {
    if o.completed {
        format!("COMPLETED: {}", o.text.trim())
    } else {
        o.text.trim().to_string()
    }
}

/// A line for the log.
pub fn describe(n: &QuestNotice) -> String {
    match n {
        QuestNotice::Added(name) | QuestNotice::Completed(name) | QuestNotice::Failed(name) | QuestNotice::Discovered(name) => format!("{}: {}", label(n).unwrap_or_default(), name.to_uppercase()),
        QuestNotice::Objectives(list) => list.iter().map(objective_text).collect::<Vec<_>>().join(" | "),
    }
}

/// How long a line stays at full strength.
fn seconds(n: &QuestNotice) -> f32 {
    match n {
        QuestNotice::Objectives(list) => LIST_SECONDS + LIST_SECONDS_MORE * list.len().saturating_sub(1) as f32,
        _ => BANNER_SECONDS,
    }
}

/// The lines waiting and the one on screen.
#[derive(Resource, Default)]
pub struct QuestMessages {
    waiting: VecDeque<QuestNotice>,
    /// The line on screen and the seconds it has been there.
    showing: Option<(QuestNotice, f32)>,
    /// AutoDisplayObjectives is on (`fnv_game::game::PlayerState::auto_objectives`): a
    /// list of objectives on screen stays at full strength until another line waits
    /// [hypothesis: the switch keeps the objectives in view while the Pip-Boy cannot be
    /// used, the GECK wiki's reason for it].
    pub hold_objectives: bool,
    /// The game's KillQuestUpdates count last acted on (`fnv_game::functions::fo3`).
    updates_killed: u64,
    /// The kinds of line the player's tweaks leave out (`crate::tweaks`).
    pub hidden: Hidden,
}

/// The kinds of line the player's tweaks leave out [design: docs/design/tweaks.md].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hidden {
    pub failed: bool,
    pub discovered: bool,
    pub objectives: bool,
}

impl Hidden {
    /// Whether `n` is left out.
    pub fn hides(&self, n: &QuestNotice) -> bool {
        match n {
            QuestNotice::Failed(_) => self.failed,
            QuestNotice::Discovered(_) => self.discovered,
            QuestNotice::Objectives(_) => self.objectives,
            QuestNotice::Added(_) | QuestNotice::Completed(_) => false,
        }
    }
}

impl QuestMessages {
    /// Add a line behind those waiting. Objectives join a list of their own quest's that
    /// waits last; a line
    /// that says what the last one says is left out, and so is any when too many wait.
    pub fn push(&mut self, n: QuestNotice) {
        if self.hidden.hides(&n) {
            return;
        }
        let quest = |l: &[Objective]| l.first().map(|o| o.quest);
        if let (QuestNotice::Objectives(more), Some(QuestNotice::Objectives(list))) = (&n, self.waiting.back_mut()) {
            if quest(more) != quest(list) {
                return self.queue(n);
            }
            // One completed before its list was on screen is shown completed, once.
            for o in more {
                match list.iter_mut().find(|e| e.text == o.text) {
                    Some(e) => e.completed = o.completed,
                    None => list.push(o.clone()),
                }
            }
            return;
        }
        self.queue(n);
    }

    fn queue(&mut self, n: QuestNotice) {
        let last = self.waiting.back().or(self.showing.as_ref().map(|s| &s.0));
        if last != Some(&n) && self.waiting.len() < MAX_WAITING {
            self.waiting.push_back(n);
        }
    }

    /// Let `dt` seconds pass; `true` when another line (or none) is now on screen.
    pub fn step(&mut self, dt: f32) -> bool {
        let mut changed = false;
        let hold = self.hold_objectives && self.waiting.is_empty();
        if let Some((n, age)) = &mut self.showing {
            *age += dt;
            if hold && matches!(n, QuestNotice::Objectives(_)) {
                *age = age.min(seconds(n));
            }
            if *age >= seconds(n) + FADE {
                self.showing = None;
                changed = true;
            }
        }
        if self.showing.is_none() {
            if let Some(next) = self.waiting.pop_front() {
                self.showing = Some((next, 0.0));
                changed = true;
            }
        }
        changed
    }

    pub fn showing(&self) -> Option<&QuestNotice> {
        self.showing.as_ref().map(|s| &s.0)
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// Drop the lines waiting (KillQuestUpdates); the one on screen goes after its time.
    /// Returns how many were dropped.
    pub fn kill_waiting(&mut self) -> usize {
        let n = self.waiting.len();
        self.waiting.clear();
        n
    }

    /// How strongly a piece that starts to come `from` seconds after the line is drawn:
    /// it comes over [`FADE`] and goes, with the rest, after the line's time.
    pub fn strength(&self, from: f32) -> f32 {
        let Some((n, age)) = &self.showing else { return 0.0 };
        let left = seconds(n) + FADE - age;
        ((age - from) / FADE).min(left / FADE).clamp(0.0, 1.0)
    }
}

/// A piece of the area: a run of text, or the box before an objective. `from` is when it
/// starts to come (the name's letters, one after another).
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text { set: Set, title: bool, from: f32 },
    /// The box's picture and where it lies: left, top, width, height.
    Box { completed: bool, rect: [f32; 4] },
}

/// The pieces of a line, from the view's top left corner.
pub fn layout(n: &QuestNotice, label_font: &Font, title_font: &Font) -> Vec<Piece> {
    let mut out = Vec::new();
    match n {
        QuestNotice::Added(name) | QuestNotice::Completed(name) | QuestNotice::Failed(name) | QuestNotice::Discovered(name) => {
            if let Some(line) = label_font.lines(label(n).unwrap_or_default(), None).into_iter().next() {
                out.push(Piece::Text { set: Set { line, x: TEXT_X, base: LABEL_BASE }, title: false, from: 0.0 });
            }
            // The name in capitals (the font has no small letters), set letter by letter
            // so that each can come at its own time.
            let line = title_font.lines(&name.trim().to_uppercase(), None).into_iter().next().unwrap_or_default();
            let width = line.width.max(1.0);
            for p in &line.glyphs {
                let one = Line { glyphs: vec![Placed { code: p.code, x: 0.0 }], width: title_font.glyph(p.code).advance() };
                out.push(Piece::Text { set: Set { line: one, x: TEXT_X + TITLE_X + p.x, base: TITLE_BASE }, title: true, from: SWEEP * p.x / width });
            }
        }
        QuestNotice::Objectives(list) => {
            let pitch = label_font.size + LEADING;
            let mut base = LIST_BASE;
            for o in list {
                let [x, y, w, h] = BOX;
                out.push(Piece::Box { completed: o.completed, rect: [TEXT_X + x, base + y, w, h] });
                for line in label_font.lines(&objective_text(o), Some(WRAP)) {
                    out.push(Piece::Text { set: Set { line, x: TEXT_X, base }, title: false, from: 0.0 });
                    base += pitch;
                }
            }
        }
    }
    out
}

// ---- the game's files -------------------------------------------------------------------

/// What the area is drawn with, read when the first line comes.
struct Kit {
    label: Face,
    title: Face,
    pictures: Pictures,
}

impl Kit {
    fn load(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
        Kit {
            label: face(LABEL_FONT, plain_font(36.0, 14.0), data, images, bc),
            title: face(TITLE_FONT, plain_font(52.0, 22.0), data, images, bc),
            pictures: Pictures::load(data),
        }
    }
}

// ---- drawing ----------------------------------------------------------------------------

pub struct QuestMessagesPlugin;

impl Plugin for QuestMessagesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<QuestMessages>().add_systems(Update, draw.in_set(crate::PlaySet::Ui));
    }
}

// The area's root; the marker lives with the tile HUD's, which hides it while the HUD is
// drawn from tiles (`crate::tiles::vanilla::hide_hand_built`).
pub(crate) use crate::tiles::vanilla::QuestArea as AreaRoot;

/// A piece that comes and goes with the line, from `0` seconds after it appears.
#[derive(Component, Clone)]
struct Part(f32);

/// A plain box drawn when the game's picture cannot be read, and how strong it is.
#[derive(Component)]
struct Fill(f32);

fn spawn_area(commands: &mut Commands, n: &QuestNotice, kit: &mut Kit, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) {
    let clear = amber(255.0).with_alpha(0.0);
    let pieces = layout(n, &kit.label.font, &kit.title.font);
    let root = Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), ..default() };
    commands.spawn((AreaRoot, root, GlobalZIndex(50))).with_children(|a| {
        for piece in &pieces {
            match piece {
                Piece::Text { set, title, from } => {
                    let face = if *title { &kit.title } else { &kit.label };
                    set_text(a, face, set, Vec2::ZERO, clear, Part(*from));
                }
                Piece::Box { completed, rect: [x, y, w, h] } => {
                    let file = if *completed { DONE_BOX_FILE } else { OPEN_BOX_FILE };
                    match kit.pictures.picture(file, data, images, bc) {
                        Some((image, rect)) => {
                            a.spawn((at(*x, *y, *w, *h), ImageNode { rect, color: clear, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }, Part(0.0)));
                        }
                        // Without the game's picture: a small square where its square is,
                        // faint when open.
                        None => {
                            let strength = if *completed { 1.0 } else { 0.4 };
                            a.spawn((at(x + w * 0.125, y + h * 0.125, w * 0.47, h * 0.47), BackgroundColor(clear), Fill(strength)));
                        }
                    }
                }
            }
        }
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    time: Res<Time>,
    mut quests: ResMut<QuestMessages>,
    panel: Res<MessagePanel>,
    data: Option<Res<GameData>>,
    sfx: Option<Res<Sfx>>,
    mut sounds: MessageWriter<PlaySound>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut roots: Query<(Entity, &mut Visibility), With<AreaRoot>>,
    mut pictures: Query<(&mut ImageNode, &Part)>,
    mut texts: Query<(&mut TextColor, &Part)>,
    mut fills: Query<(&mut BackgroundColor, &Fill)>,
    mut kit: Local<Option<Kit>>,
    (state, script, driven, kill_cam): (Option<Res<State<crate::PlayState>>>, Option<Res<crate::script::ScriptState>>, Option<Res<crate::player::ViewDriven>>, Option<Res<crate::kill_cam::KillCam>>),
    mut sound_forms: Local<HashMap<&'static str, Option<FormId>>>,
) {
    // The HUD's lengths at the UI scale (`crate::interface::hud_units`, #1631).
    let _hud = crate::interface::hud_units();
    let hold = script.as_ref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.player.auto_objectives);
    if quests.hold_objectives != hold {
        info!("quest messages: objectives {} (AutoDisplayObjectives)", if hold { "held on screen" } else { "go after their time" });
        quests.hold_objectives = hold;
    }
    // Fallout 3's KillQuestUpdates: the lines waiting are dropped.
    let killed = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| rt.game.ui.quest_updates_killed);
    if let Some(k) = killed.filter(|&k| k != quests.updates_killed) {
        quests.updates_killed = k;
        let n = quests.kill_waiting();
        info!("quest messages: {n} waiting line(s) dropped (KillQuestUpdates)");
    }
    // Hidden with the clock stopped as the corner messages are (`crate::corner_messages::held`).
    let held = crate::corner_messages::held(panel.view.is_some(), state.map(|s| *s.get()), driven.is_some_and(|d| d.0), kill_cam.is_some_and(|k| k.active()));
    for (_, mut v) in &mut roots {
        v.set_if_neq(if held { Visibility::Hidden } else { Visibility::Inherited });
    }
    if held {
        return;
    }
    if quests.bypass_change_detection().step(time.delta_secs()) {
        for (e, _) in &roots {
            commands.entity(e).despawn();
        }
        if let Some(n) = quests.showing() {
            let bc = bc_supported(formats.as_deref());
            let kit = kit.get_or_insert_with(|| Kit::load(data.as_deref(), &mut images, bc));
            info!("quest message on screen: {} ({} waiting)", describe(n), quests.waiting());
            spawn_area(&mut commands, n, kit, data.as_deref(), &mut images, bc);
            let found = sound(n).and_then(|name| {
                let sfx = sfx.as_deref()?;
                *sound_forms.entry(name).or_insert_with(|| sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(name))).map(|(f, _)| *f))
            });
            if let Some(sound) = found {
                sounds.write(PlaySound { sound, at: None });
            }
        }
        // Drawn from the next frame, when the pieces are there.
        return;
    }
    let color = |from: f32| amber(255.0).with_alpha(quests.strength(from));
    for (mut p, part) in &mut pictures {
        if p.color != color(part.0) {
            p.color = color(part.0);
        }
    }
    for (mut t, part) in &mut texts {
        t.set_if_neq(TextColor(color(part.0)));
    }
    for (mut f, fill) in &mut fills {
        f.set_if_neq(BackgroundColor(amber(255.0).with_alpha(quests.strength(0.0) * fill.0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tweaks_leave_out_their_lines() {
        let mut q = QuestMessages { hidden: Hidden { failed: true, discovered: true, objectives: true }, ..Default::default() };
        q.push(QuestNotice::Failed("A".into()));
        q.push(QuestNotice::Discovered("B".into()));
        q.push(objectives(&[("C", false)]));
        assert_eq!(q.waiting(), 0);
        q.push(QuestNotice::Added("D".into()));
        assert_eq!(q.waiting(), 1, "added quests still show");
    }

    fn objectives(list: &[(&str, bool)]) -> QuestNotice {
        QuestNotice::Objectives(list.iter().map(|(t, c)| Objective { quest: 1, text: t.to_string(), completed: *c }).collect())
    }

    /// Letters 14 wide and lines of 36, about font 7; capitals 22 wide, about font 8.
    fn fonts() -> (Font, Font) {
        (plain_font(36.0, 14.0), plain_font(52.0, 22.0))
    }

    #[test]
    fn auto_display_holds_the_objectives_until_another_line_waits() {
        let mut q = QuestMessages { hold_objectives: true, ..default() };
        q.push(objectives(&[("Follow Doc Mitchell", false)]));
        assert!(q.step(0.0));
        q.step(60.0);
        assert!(matches!(q.showing(), Some(QuestNotice::Objectives(_))));
        assert_eq!(q.strength(0.0), 1.0);
        // A banner waits: the list goes after its time.
        q.push(QuestNotice::Added("Back in the Saddle".into()));
        q.step(LIST_SECONDS + FADE);
        assert!(matches!(q.showing(), Some(QuestNotice::Added(_))));
        // Off: a list goes after its time as before.
        let mut q = QuestMessages::default();
        q.push(objectives(&[("Follow Doc Mitchell", false)]));
        q.step(0.0);
        q.step(60.0);
        assert!(q.showing().is_none());
    }

    #[test]
    fn a_banner_is_the_label_over_the_name_in_capitals() {
        let (l, t) = fonts();
        let p = layout(&QuestNotice::Added("Come Fly With Me".into()), &l, &t);
        let Piece::Text { set, title: false, from } = &p[0] else { panic!("{p:?}") };
        assert_eq!((set.line.glyphs.len(), set.x, set.base, *from), (10, TEXT_X, LABEL_BASE, 0.0));
        // One piece a letter, coming from left to right.
        assert_eq!(p.len(), 1 + 13);
        let letters: Vec<(u8, f32, f32)> = p[1..]
            .iter()
            .map(|q| match q {
                Piece::Text { set, title: true, from } => (set.line.glyphs[0].code, set.x, *from),
                _ => panic!("{q:?}"),
            })
            .collect();
        assert_eq!(letters[0], (b'C', TEXT_X + TITLE_X, 0.0));
        assert_eq!(letters[1].0, b'O');
        assert_eq!((letters[12].0, letters[12].1), (b'E', TEXT_X + TITLE_X + 15.0 * 22.0));
        assert!((letters[12].2 - SWEEP * 15.0 / 16.0).abs() < 1e-6);
    }

    #[test]
    fn objectives_are_listed_with_their_boxes() {
        let (l, t) = fonts();
        let p = layout(&objectives(&[("Talk to Sunny Smiles in the Prospector Saloon.", true), ("Meet Sunny.", false)]), &l, &t);
        // "COMPLETED: " and the text, broken at the wrap width: 40 letters fit a line.
        let pitch = 36.0 + LEADING;
        assert_eq!(p[0], Piece::Box { completed: true, rect: [TEXT_X - 16.0, LIST_BASE - 21.0, 26.0, 26.0] });
        // Each line as its letters (spaces are not set) and the line it stands on.
        let texts: Vec<(String, f32)> = p
            .iter()
            .filter_map(|q| match q {
                Piece::Text { set, .. } => Some((set.line.glyphs.iter().map(|g| g.code as char).collect(), set.base)),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            vec![
                ("COMPLETED:TalktoSunnySmilesinthe".to_string(), LIST_BASE),
                ("ProspectorSaloon.".to_string(), LIST_BASE + pitch),
                ("MeetSunny.".to_string(), LIST_BASE + 2.0 * pitch),
            ]
        );
        assert_eq!(p[3], Piece::Box { completed: false, rect: [TEXT_X - 16.0, LIST_BASE + 2.0 * pitch - 21.0, 26.0, 26.0] });
    }

    #[test]
    fn kill_quest_updates_drops_the_lines_waiting() {
        let mut q = QuestMessages::default();
        q.push(QuestNotice::Added("Growing Up Fast".into()));
        q.step(0.0);
        q.push(QuestNotice::Completed("Growing Up Fast".into()));
        q.push(objectives(&[("Go to the party", false)]));
        assert_eq!(q.waiting(), 2);
        assert_eq!(q.kill_waiting(), 2);
        assert_eq!(q.waiting(), 0);
        // The line on screen stays for its time.
        assert!(matches!(q.showing(), Some(QuestNotice::Added(_))));
    }

    #[test]
    fn lines_come_one_at_a_time_and_objectives_join_a_waiting_list() {
        let mut q = QuestMessages::default();
        q.push(QuestNotice::Added("Back in the Saddle".into()));
        q.push(objectives(&[("Talk to Sunny.", false)]));
        q.push(objectives(&[("Talk to Sunny.", false), ("Follow Sunny.", false)]));
        q.push(objectives(&[("Talk to Sunny.", true)]));
        q.push(objectives(&[("Talk to Sunny.", false)]));
        q.push(QuestNotice::Completed("Back in the Saddle".into()));
        q.push(QuestNotice::Completed("Back in the Saddle".into()));
        assert_eq!(q.waiting(), 3);
        assert!(q.step(0.0));
        assert_eq!(q.showing(), Some(&QuestNotice::Added("Back in the Saddle".into())));
        assert_eq!(q.strength(0.0), 0.0);
        assert!(!q.step(0.125));
        assert_eq!((q.strength(0.0), q.strength(0.125)), (0.5, 0.0));
        assert!(!q.step(BANNER_SECONDS - 0.125));
        assert_eq!(q.strength(SWEEP), 1.0);
        assert!(q.step(FADE));
        // The two objectives in one list, which stays longer.
        assert_eq!(q.showing(), Some(&objectives(&[("Talk to Sunny.", false), ("Follow Sunny.", false)])));
        assert!(!q.step(LIST_SECONDS + LIST_SECONDS_MORE));
        assert!(q.step(FADE));
        assert_eq!(q.showing(), Some(&QuestNotice::Completed("Back in the Saddle".into())));
        // Another quest's objectives wait in a list of their own.
        let other = QuestNotice::Objectives(vec![Objective { quest: 2, text: "Find the caravan master.".into(), completed: false }]);
        q.push(other.clone());
        q.push(objectives(&[("Go and meet Trudy.", false)]));
        assert_eq!(q.waiting(), 2);
        assert!(q.step(BANNER_SECONDS + FADE));
        assert_eq!(q.showing(), Some(&other));
        // An objective of the list on screen gets a list of its own.
        q.push(other.clone());
        assert_eq!(q.waiting(), 2);
        assert!(q.step(LIST_SECONDS + FADE));
        assert_eq!(q.showing(), Some(&objectives(&[("Go and meet Trudy.", false)])));
        assert!(q.step(LIST_SECONDS + FADE));
        assert_eq!(q.showing(), Some(&other));
        assert!(q.step(LIST_SECONDS + FADE));
        assert!(q.showing().is_none());
    }

    #[test]
    fn log_lines() {
        assert_eq!(describe(&QuestNotice::Added("Ring-a-Ding-Ding!".into())), "Quest added: RING-A-DING-DING!");
        assert_eq!(describe(&objectives(&[("A.", true), ("B.", false)])), "COMPLETED: A. | B.");
    }
}
