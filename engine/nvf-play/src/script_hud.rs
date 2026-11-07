//! Script messages on screen. The subtitles are drawn in the interface's colour and the
//! HUD's font (docs/spec-notes/corner-messages.md §8); the message box is
//! drawn as the original's (`crate::message_menu`), and so are the short lines at the
//! top left (`crate::corner_messages`: messages without the message-box flag, objectives
//! shown or completed, quests completed, items a script gave or took).
//!
//! - **Message boxes**: the runtime's queue (`fnv_game::Game::boxes`, scripts.md §7.7),
//!   the first one in the middle of the view while the open menu is 1001, with the
//!   message's title and its buttons one under the other, the chosen one framed. A click
//!   picks a button; Up and Down (or the pointer) move the frame and Enter picks the
//!   framed one; number keys 1–9 pick a button by its place [design: the keys]. All end
//!   in `ScriptState` → `Runtime::answer`, which ends in `Game::close_menu` after the
//!   frame's MenuMode 1001 ran, so a `MenuMode 1001` block still reads
//!   `GetButtonPressed`. A box without buttons has an OK button, which Space also
//!   presses. The buttons are clickable as the original's are [observed by the
//!   coordinator playing the original, 2026-10-09]; the cursor is free while a box is
//!   open (`PlayState::Menu`). The next queued box then opens (the runtime's rule).
//! - **Menus scripts open** for character creation: the name, S.P.E.C.I.A.L., tag-skill
//!   and trait menus are drawn by `crate::chargen`, the race menu (1036) by
//!   `crate::creator` when the engine gave it its data; otherwise the race menu is a
//!   placeholder in the same box that names the menu, and Enter or a click on Done
//!   closes it as the player's Done would (`Game::close_menu`); nothing is chosen there.
//! - While a script's menu or box is open the player cannot move or look
//!   (`PlayState::Menu`, `script::hold_player`), and script time stops (the runtime's
//!   `Game::menu`).
//! - **Subtitles**: the dialogue lines being said (`Game::speech`), bottom centre, the
//!   text alone on screen and "Speaker: text" in the log, one response of the INFO at a time in response-number order, each
//!   for as long as it lasts (`crate::voice::speech_response`: its voice file's length,
//!   or the length fitted to its text without one). The response being said in the
//!   conversation with the player (`Game::dialogue`, `crate::dialogue::subtitle`) is in
//!   the log's `subtitle:` lines too, but drawn by the dialogue menu
//!   (`crate::dialogue_menu`). A radio's talk lines have none (`crate::radio`). Every
//!   text is shown without its acting notes in
//!   braces (`crate::dialogue::shown_text`). A line said where the player does not hear
//!   it (another cell, or beyond `crate::voice::DIALOG_MAX_DISTANCE`) has no subtitle.
//!   While the player is in a conversation no other line is drawn: the actors' lines to
//!   each other stay in the log only [owner's playtest list 5, item 74:
//!   Joe Cobb's line to Trudy over the player's conversation with her].

use crate::interface::{amber, bc_supported, face, plain_font, set_line, u, Face};
use crate::message_menu::{MessagePanel, PanelView};
use crate::script::{ScriptState, MENU_MESSAGE};
use crate::script_events::BoxMessage;
use crate::sim::SimState;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use crate::ui_pointer::PointerState;
use fnv_font::{Font, Line};
use fnv_game::game::menus;
use fnv_game::trace::EventKind;
use fnv_sim::FormId;
use nvf_assets::GameData;
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct ScriptHud {
    /// The text and buttons each message was last shown with (the trace's
    /// `message_shown`), for the box queue, which holds only FormIDs.
    shown: HashMap<u32, BoxMessage>,
    /// The form-less boxes (MessageBox, MessageBoxEx: FormId 0) relayed, oldest first, the
    /// last [`FORMLESS_KEPT`]: the queue holds no text for them and they share one FormId,
    /// so each queued one is told by its place ([`formless_front`]).
    formless: Vec<BoxMessage>,
    /// The response texts (NAM1) of the dialogue lines said, by INFO.
    lines: HashMap<u32, Vec<String>>,
    /// The message box on screen (as last drawn).
    on_screen: Option<BoxMessage>,
    /// The response the companion wheel's companion says now, drawn by the wheel
    /// (`crate::companion_wheel`) instead of here.
    pub wheel_line: Option<String>,
}

impl ScriptHud {
    /// A message box was shown: remember its text for when it reaches the front.
    pub fn message_box(&mut self, b: BoxMessage) {
        // One log line: the text's CR LF line ends (MESG DESC) would split it.
        info!("message box shown: {} {:?}", b.text.replace("\r\n", "\n").replace(['\r', '\n'], " / "), b.shown().iter().map(|(_, t)| *t).collect::<Vec<_>>());
        if b.message == 0 {
            self.formless.push(b);
            let extra = self.formless.len().saturating_sub(FORMLESS_KEPT);
            self.formless.drain(..extra);
        } else {
            self.shown.insert(b.message, b);
        }
    }

    /// The text of the front message box (`message` is `Game::boxes[0]`) as shown, else as
    /// the message record has it.
    pub fn box_view(&self, message: FormId, script: &ScriptState) -> BoxMessage {
        if let Some(b) = self.shown.get(&message.0) {
            return b.clone();
        }
        let rt = script.rt.as_ref();
        // A form-less box: its words from the relayed ones, or from the runtime's events
        // not relayed yet. Those reach this HUD in a script frame, and a box can open
        // before one: a Script Runner file's box under the main menu, where scripts run
        // no frame (#1548).
        if let (0, Some(rt)) = (message.0, rt) {
            let in_queue = rt.game.boxes.iter().filter(|m| m.0 == 0).count();
            let pending = rt.game.events.iter().filter_map(|e| match &e.kind {
                EventKind::MessageShown { message: FormId(0), text, buttons, hidden } if !buttons.is_empty() => {
                    Some(BoxMessage { message: 0, text: text.clone().unwrap_or_default(), buttons: buttons.clone(), hidden: hidden.clone() })
                }
                _ => None,
            });
            if let Some(b) = formless_front(&self.formless, pending, in_queue) {
                return b;
            }
        }
        let def = rt.and_then(|rt| rt.game.data.messages.get(&message));
        BoxMessage {
            message: message.0,
            text: def.and_then(|d| d.text.clone()).unwrap_or_else(|| format!("(message {message})")),
            buttons: def.map(|d| d.buttons.clone()).unwrap_or_default(),
            hidden: Vec::new(),
        }
    }

    /// The message box on screen, if any.
    pub fn front(&self) -> Option<&BoxMessage> {
        self.on_screen.as_ref()
    }

    pub fn knows_line(&self, info: FormId) -> bool {
        self.lines.contains_key(&info.0)
    }

    pub fn remember_line(&mut self, info: FormId, responses: Vec<String>) {
        self.lines.insert(info.0, responses);
    }
}

/// How many relayed form-less boxes [`ScriptHud`] keeps: more than ever wait at once.
const FORMLESS_KEPT: usize = 32;

/// The front one of the `in_queue` form-less boxes waiting in `Game::boxes`, from every
/// form-less box shown, oldest first (the relayed ones, then the events not relayed yet).
/// The queue is first in, first out, so the boxes waiting are the last `in_queue` shown and
/// the front one is the first of those [design]. `None` when fewer were seen than wait (a
/// save's boxes: their words are not saved).
fn formless_front(relayed: &[BoxMessage], pending: impl Iterator<Item = BoxMessage>, in_queue: usize) -> Option<BoxMessage> {
    let all: Vec<BoxMessage> = relayed.iter().cloned().chain(pending).collect();
    let i = all.len().checked_sub(in_queue.max(1))?;
    all.into_iter().nth(i)
}

/// A character-creation menu's name for the placeholder panel.
pub fn menu_name(code: u16) -> String {
    match code {
        menus::NAME => "Name".into(),
        menus::RACE => "Race and appearance".into(),
        menus::LOVE_TESTER => "S.P.E.C.I.A.L. (Vit-o-matic Vigor Tester)".into(),
        menus::TAG_SKILLS => "Tag skills".into(),
        menus::TRAITS => "Traits".into(),
        c => format!("Menu {c}"),
    }
}

/// The placeholder panel of a menu a script opened: its name, a text and its one button.
pub fn menu_panel(code: u16) -> PanelView {
    PanelView {
        title: Some(menu_name(code)),
        text: format!("Menu {code}.\n\nCharacter creation is not built yet (Layer 7); nothing is chosen here."),
        buttons: vec!["Done".into()],
        icon: None,
    }
}

pub struct ScriptHudPlugin;

impl Plugin for ScriptHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptHud>()
            .add_plugins((crate::message_menu::MessageMenuPlugin, crate::corner_messages::CornerMessagesPlugin, crate::quest_messages::QuestMessagesPlugin))
            .add_systems(PostStartup, spawn.after(crate::settings::fix_ui_scale))
            .add_systems(Update, draw.in_set(crate::PlaySet::Ui).before(crate::message_menu::DrawBox));
    }
}

/// The subtitles' area, a column as wide as the text may be, centred in the view.
// The marker lives with the tile HUD's, which hides it while the HUD is drawn from tiles
// and the tile renderer draws something (not in a menu; `crate::tiles::vanilla::hide`).
use crate::tiles::vanilla::SubtitlesArea as SubtitleArea;

// ---- the subtitles (corner-messages.md §8) -----------------------------------------------

/// The text may be as wide as the HUD's Subtitles rect [documented:
/// menus\main\hud_main_menu.xml, `Subtitles` width 800].
pub(crate) const SUBTITLE_WIDTH: f32 = 800.0;
/// The first line's base under the view's top [observed: the owner's screenshot].
const SUBTITLE_BASE: f32 = 633.0;
/// Lines are the font's size and this much apart [observed: 38 units on the screenshot].
const SUBTITLE_LEADING: f32 = 2.0;
/// Font 7, the HUD's text templates' [documented: menus\prefabs\hudtemplates.xml].
const SUBTITLE_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";

/// The subtitles' lines, each centred in the area: the line, its start and its base, from
/// the area's top left. Every text starts a line of its own.
pub fn subtitle_layout(texts: &[String], font: &Font) -> Vec<(Line, f32, f32)> {
    let pitch = font.size + SUBTITLE_LEADING;
    let lines = texts.iter().flat_map(|t| font.lines(t.trim(), Some(SUBTITLE_WIDTH)));
    lines
        .enumerate()
        .map(|(i, l)| {
            let x = ((SUBTITLE_WIDTH - l.width) / 2.0).max(0.0);
            (l, x, pitch * i as f32)
        })
        .collect()
}

/// A clickable button of the message box (0-based).
#[derive(Component, Clone, Copy, Debug)]
pub struct BoxButton(pub usize);

/// The area's frame keeps the subtitles' distance from the view's middle at any UI scale,
/// as the activation prompt's (`crate::reticle`) and the tile HUD's (`fnv_game::ui_hud`)
/// (`crate::interface::frame_top`; at 100% its top is the view's, as before) [design: #1631].
fn spawn(mut commands: Commands) {
    let _hud = crate::interface::hud_units();
    commands.spawn((
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), top: crate::interface::frame_top(0.5), justify_content: JustifyContent::Center, ..default() },
        children![(Node { width: u(SUBTITLE_WIDTH), ..default() }, SubtitleArea)],
    ));
}

/// The box's buttons as drawn: its own whose conditions held, or OK for a box without
/// any.
pub fn box_buttons(b: &BoxMessage) -> Vec<String> {
    if b.buttons.is_empty() {
        vec!["OK".into()]
    } else {
        b.shown().into_iter().map(|(_, t)| t.to_string()).collect()
    }
}

/// The perks' choice list as a box: the target's name, the choices' labels, then the
/// default action (Search on a corpse, else Talk) [design: fnv_game::activate_choices].
pub fn activate_panel(g: &fnv_game::Game, sim: &SimState, m: &fnv_game::activate_choices::ActivateMenu) -> PanelView {
    let name = sim.ref_name(Some(g), m.target.0).map(str::to_string).unwrap_or_else(|| g.data.name(m.target));
    let default = if g.is_dead(m.target) { "Search" } else { "Talk" };
    let mut buttons: Vec<String> = m.choices.iter().map(|c| c.label.clone()).collect();
    buttons.push(default.into());
    PanelView { title: None, text: name, buttons, icon: None }
}

/// What the overlay shows this frame: the panel (a message box or a menu placeholder),
/// the box on screen, and the subtitles.
fn view(hud: &ScriptHud, script: &ScriptState, sim: &SimState, player: Option<&crate::player::Player>, creator: bool) -> (Option<PanelView>, Option<BoxMessage>, Subtitles) {
    let Some(rt) = script.rt.as_ref() else { return (None, None, Subtitles::default()) };
    let g = &rt.game;
    let front = match g.menu {
        Some(MENU_MESSAGE) => g.boxes.first().map(|&m| hud.box_view(m, script)),
        _ => None,
    };
    let panel = match g.menu {
        // The perks' choice list (`fnv_game::activate_choices`).
        Some(MENU_MESSAGE) if g.activate_menu.is_some() => g.activate_menu.as_ref().map(|m| activate_panel(g, sim, m)),
        // A reputation's box (`crate::reputation`).
        Some(MENU_MESSAGE) if g.boxes.first().is_some_and(|&m| crate::reputation::is_reputation(g, m)) => g.boxes.first().map(|&m| crate::reputation::panel(g, sim, m)),
        // The title is the message's own (MESG FULL).
        Some(MENU_MESSAGE) => front.as_ref().map(|b| PanelView {
            title: g.data.messages.get(&FormId(b.message)).and_then(|d| d.title.clone()),
            text: b.text.trim_end().to_string(),
            buttons: box_buttons(b),
            icon: None,
        }),
        // The name, S.P.E.C.I.A.L., tag-skill, trait and level-up menus draw themselves,
        // and the race menu with the creator's data (`crate::creator`).
        // The tutorial menu draws itself (`crate::tutorial_menu`).
        Some(code) if crate::script::is_script_menu(code) && code != menus::TUTORIAL && !crate::chargen::has_own_menu(code) && !crate::levelup::draws(code) && !crate::creator::draws(code, creator) => {
            Some(menu_panel(code))
        }
        _ => None,
    };
    (panel, front, subtitles(hud, g, sim, player))
}

/// The adapter reads the drawing's subtitle selection before scripts [design].
pub(crate) fn hud_subtitles_shown(hud: &ScriptHud, game: &fnv_game::Game<'_>, sim: &SimState, player: Option<&crate::player::Player>) -> bool {
    !hud_subtitle_texts(hud, game, sim, player).is_empty()
}

/// The subtitles drawn, one text each, for the tile-drawn HUD (`crate::tiles::notices`)
/// [design: the drawing's own selection].
pub(crate) fn hud_subtitle_texts(hud: &ScriptHud, game: &fnv_game::Game<'_>, sim: &SimState, player: Option<&crate::player::Player>) -> Vec<String> {
    subtitles(hud, game, sim, player).on_screen
}

fn subtitles(hud: &ScriptHud, g: &fnv_game::Game<'_>, sim: &SimState, player: Option<&crate::player::Player>) -> Subtitles {
    let mut subs = Vec::new();
    let mut on_screen = Vec::new();
    // In a conversation with the player only the dialogue menu's own line is on screen.
    let talking = g.dialogue.is_some();
    let mut wheel = None;
    for s in &g.speech {
        // A line the player does not hear has no subtitle (`crate::voice::audible`),
        // unless a script asked for the speaker's subtitles always
        // (AlwaysShowActorSubtitles), or for this line's (Say's ForceSubtitles), or said
        // it as a 2D sound heard anywhere (Say's is2D).
        if !crate::voice::audible(sim, player, s.speaker) && !g.always_subtitles(s.speaker) && !s.force_subtitles && !s.two_d {
            continue;
        }
        let Some(texts) = hud.lines.get(&s.info.0) else { continue };
        let i = crate::voice::speech_response(g, sim, s);
        let Some(text) = texts.get(i).or_else(|| texts.last()) else { continue };
        let who = sim.ref_name(Some(g), s.speaker.0).map(str::to_string).unwrap_or_else(|| g.data.name(s.speaker));
        let text = crate::dialogue::shown_text(&g.expand_codes(text));
        if text.is_empty() {
            continue;
        }
        // The companion wheel's companion: drawn in the wheel (`crate::companion_wheel`).
        if g.wheel_companion() == Some(s.speaker) {
            subs.push(format!("{who}: {text} (in the companion wheel)"));
            wheel = Some(text);
            continue;
        }
        subs.push(format!("{who}: {text}{}", if talking { " (not drawn: in dialogue)" } else { "" }));
        if !talking {
            on_screen.push(text);
        }
    }
    // The response being said in the conversation with the player: drawn by the dialogue
    // menu (`crate::dialogue_menu`), logged with the others.
    subs.extend(crate::dialogue::subtitle(g, sim));
    Subtitles { logged: subs.join("\n"), on_screen, wheel }
}

/// The subtitles: all of them for the log ("Speaker: text"), those drawn here for the
/// screen (the text alone).
#[derive(Default, PartialEq)]
struct Subtitles {
    logged: String,
    on_screen: Vec<String>,
    wheel: Option<String>,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    mut hud: ResMut<ScriptHud>,
    script: Res<ScriptState>,
    sim: Res<SimState>,
    mut panel: ResMut<MessagePanel>,
    mut shown: Local<Subtitles>,
    mut commands: Commands,
    areas: Query<Entity, With<SubtitleArea>>,
    player: Option<Res<crate::player::Player>>,
    (data, mut images, formats, mut font): (Option<Res<GameData>>, ResMut<Assets<Image>>, Option<Res<CompressedImageFormatSupport>>, Local<Option<Face>>),
    (creator, creator_menu): (Option<Res<crate::creator::CreatorData>>, Option<Res<crate::creator::CreatorMenu>>),
) {
    let (p, front, mut subs) = view(&hud, &script, &sim, player.as_deref(), creator.is_some());
    // No subtitles over the character creator; they are still logged [owner 2026-10-17].
    if creator_menu.is_some_and(|c| c.is_open()) {
        subs.on_screen.clear();
    }
    hud.on_screen = front;
    if hud.wheel_line != subs.wheel {
        hud.wheel_line = subs.wheel.clone();
    }
    // Which showing of a message box this is: a box replaced by one alike starts afresh.
    let showing = script.rt.as_ref().filter(|rt| rt.game.menu == Some(MENU_MESSAGE)).and_then(|rt| rt.game.front_box()).map(|b| b.serial);
    if panel.view != p || panel.showing != showing {
        panel.show(p, showing);
    }
    if subs != *shown {
        if subs.logged != shown.logged && !subs.logged.is_empty() {
            info!("subtitle: {}", subs.logged.replace('\n', " / "));
        }
        if subs.on_screen != shown.on_screen {
            let _hud = crate::interface::hud_units();
            let face = font.get_or_insert_with(|| face(SUBTITLE_FONT, plain_font(36.0, 17.0), data.as_deref(), &mut images, bc_supported(formats.as_deref())));
            let lines = subtitle_layout(&subs.on_screen, &face.font);
            for area in &areas {
                commands.entity(area).despawn_related::<Children>().with_children(|a| {
                    for (line, x, base) in &lines {
                        set_line(a, face, line, *x, SUBTITLE_BASE + *base, amber(255.0));
                    }
                });
            }
        }
        *shown = subs;
    }
}

/// Keys and clicks for a script's open menu (before the frame's scripts run): a click
/// or a number key picks a button of the front message box, Up and Down move the frame
/// and Enter picks the framed button; Space also presses the OK of a box without
/// buttons; Enter or a click on Done closes a character-creation menu. All go through
/// `ScriptState::answer`.
#[allow(clippy::too_many_arguments)]
pub fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    clicks: Query<(&PointerState, &BoxButton), Changed<PointerState>>,
    hud: Res<ScriptHud>,
    mut panel: ResMut<MessagePanel>,
    mut script: ResMut<ScriptState>,
    creator: Option<Res<crate::creator::CreatorData>>,
    (tile_click, tiles): (Option<ResMut<crate::tiles::input::MessageClick>>, Option<Res<crate::tiles::vanilla::TileMenus>>),
) {
    use KeyCode::*;
    // A click on the box drawn from its tiles (T15a, `crate::tiles::input`), taken.
    let tile_click = tile_click.and_then(|mut c| c.0.take());
    let Some(code) = script.rt.as_ref().and_then(|rt| rt.game.menu) else { return };
    if !crate::script::is_script_menu(code) {
        return;
    }
    let clicked = clicks.iter().find(|(i, _)| **i == PointerState::Pressed).map(|(_, b)| b.0).or(tile_click);
    let enter = keys.any_just_pressed([Enter, NumpadEnter]);
    if code == menus::TUTORIAL || crate::chargen::has_own_menu(code) || crate::levelup::draws(code) || crate::creator::draws(code, creator.is_some()) {
        return;
    }
    if code != MENU_MESSAGE {
        if enter || clicked.is_some() {
            script.answer(None, if enter { "Enter" } else { "click" });
        }
        return;
    }
    // The box drawn from its tiles opens with no row chosen: the first Up or Down chooses
    // the first row (`MessagePanel::key`). Enter with none chosen answers the first button,
    // as the hand-built box, whose first row is chosen at open [design: what the original's
    // Enter does with no row chosen was not captured].
    let first_picks = tiles.as_ref().is_some_and(|m| m.message) && script.rt.as_ref().is_some_and(|rt| crate::tiles::menus::message_tiled(true, &rt.game));
    if keys.just_pressed(ArrowUp) {
        panel.key(-1, first_picks);
    }
    if keys.just_pressed(ArrowDown) {
        panel.key(1, first_picks);
    }
    let digits = [Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    let pads = [Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9];
    let picked = (0..9).find(|&i| keys.just_pressed(digits[i]) || keys.just_pressed(pads[i])).or(clicked).or(enter.then_some(panel.selected));
    // The perks' choice list: the n-th button (`Runtime::pick_activate_choice`).
    if script.rt.as_ref().is_some_and(|rt| rt.game.activate_menu.is_some()) {
        if let Some(p) = picked {
            script.answer(Some(p as i32), "key");
        }
        return;
    }
    let Some(front) = script.rt.as_ref().and_then(|rt| rt.game.boxes.first().copied()) else { return };
    // The drawn buttons are those whose conditions held: the n-th drawn one answers with
    // its index in the record (GetButtonPressed).
    let view = hud.box_view(front, &script);
    if view.buttons.is_empty() {
        if picked == Some(0) || keys.just_pressed(Space) {
            script.answer(None, "OK");
        }
    } else if let Some((i, _)) = picked.and_then(|p| view.shown().get(p).copied()) {
        script.answer(Some(i as i32), "key");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitles_are_centred_lines_of_the_hud_rect() {
        // Every character 17 wide: 47 fit the rect's 800.
        let font = plain_font(36.0, 17.0);
        let long = "a".repeat(30) + " " + &"b".repeat(30);
        let l = subtitle_layout(&[long, "Hi.".into()], &font);
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].0.width, l[0].1, l[0].2), (510.0, 145.0, 0.0));
        assert_eq!((l[1].1, l[1].2), (145.0, 38.0));
        // A second speaker's text starts a line of its own.
        assert_eq!((l[2].0.width, l[2].1, l[2].2), (51.0, 374.5, 76.0));
    }

    fn text_box(text: &str) -> BoxMessage {
        BoxMessage { message: 0, text: text.into(), buttons: vec!["OK".into()], hidden: Vec::new() }
    }

    #[test]
    fn a_formless_box_not_relayed_yet_shows_its_own_words() {
        // Two boxes of Script Runner files under the main menu: no script frame has
        // relayed either (#1548). The front is the first; once it is answered, the second.
        let pending = || [text_box("first"), text_box("second")].into_iter();
        assert_eq!(formless_front(&[], pending(), 2).unwrap().text, "first");
        assert_eq!(formless_front(&[], pending(), 1).unwrap().text, "second");
    }

    #[test]
    fn formless_boxes_relayed_in_one_frame_keep_their_order() {
        // Relayed boxes, an answered one first, then one still in the events.
        let relayed = [text_box("answered"), text_box("a"), text_box("b")];
        let pending = || std::iter::once(text_box("c"));
        assert_eq!(formless_front(&relayed, pending(), 3).unwrap().text, "a");
        assert_eq!(formless_front(&relayed, std::iter::empty(), 2).unwrap().text, "a");
        // More waiting than were seen (a save's boxes): not known.
        assert!(formless_front(&relayed, pending(), 5).is_none());
    }
}
