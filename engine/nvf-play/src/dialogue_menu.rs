//! The dialogue menu, drawn as the original draws it (docs/spec-notes/dialogue-menu.md).
//!
//! `crate::dialogue` says what the menu holds ([`DialogueMenu`]: the actor's name, the
//! line being said or the choices offered, the selected choice); this module sets it out
//! by the game's own interface file and draws it with the game's fonts and pictures, read
//! from the owner's archives at run time. Lengths are in the interface's units, 960 to
//! the height of the view.
//!
//! - [`layout`] places everything (pure, tested without the game's files).
//! - While the actor speaks: the line, without a frame, on a dark band. While the player
//!   chooses: the choices between two brackets on the band, the selected one framed, a
//!   scroll bar at the left when not all of them fit.
//! - The actor's name stands at the top right throughout.
//! - While a Goodbye choice is offered, "TAB) Goodbye" stands at the left of the band,
//!   level with the bottom bracket (owner, list 5 item 62; the interface file has no
//!   such hint [observed: dialog_menu.xml]): Tab picks that choice (`crate::dialogue`).
//! - The menu is rebuilt when what it shows changes; the pointer chooses the choice it
//!   is on (`crate::dialogue` reads the clicks).

use crate::dialogue::{ChoiceButton, ListId};
use crate::interface::{amber, at, bc_supported, face, plain_font, set_line, Face, Picture, Pictures};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use crate::ui_pointer::Button;
use bevy::window::PrimaryWindow;
use fnv_font::{Font, Line};
use nvf_assets::GameData;

// ---- the interface file's numbers [documented: menus\dialog\dialog_menu.xml, §2] ------

/// The dark band's width, centred on the screen.
const BAND_WIDTH: f32 = 1080.0;
/// The safe zone at each edge [documented: Fallout_default.ini `iSafeZoneY`, hud.md §3].
const CROP_Y: f32 = 15.0;
/// The line and the list are centred on this part of the height inside the safe zone,
/// and kept this far above its bottom.
const CENTRE: f32 = 0.8;
const BOTTOM_ROOM: f32 = 40.0;
/// The line starts this far into the band and wraps this much narrower than it.
const TEXT_IN: f32 = 60.0;
/// The band reaches this far above the line's top (less 5) and is this much taller.
const TEXT_BAND_UP: f32 = 45.0;
const TEXT_BAND_MORE: f32 = 80.0;
/// The list: this far into the band and this much narrower; at least this high.
const LIST_IN: f32 = 20.0;
const LIST_LESS: f32 = 70.0;
const MIN_LIST: f32 = 110.0;
/// The brackets: this far right of the list, the top one this far above it, the bottom
/// one this far below it; their upright ends are this long.
const BRACKET_IN: f32 = 15.0;
const BRACKET_ABOVE: f32 = 20.0;
const BRACKET_BELOW: f32 = 10.0;
const BRACKET_ENDS: f32 = 40.0;
/// The band reaches this far above the top bracket's line and below the bottom one's.
const LIST_BAND_UP: f32 = 30.0;
const LIST_BAND_DOWN: f32 = 30.0;
/// The scroll bar's width at the list's left; the choices start right of it and end
/// this far short of the list's right.
const SCROLL_WIDTH: f32 = 32.0;
const CHOICE_SHORT: f32 = 5.0;
/// A choice's text: this far into its row and down from its top; wrapped this much
/// narrower than the row. The row is the text's height and this much more.
const CHOICE_TEXT: Vec2 = Vec2::new(25.0, 15.0);
const CHOICE_WRAP_LESS: f32 = 40.0;
const CHOICE_PAD: f32 = 20.0;
/// The arrows of the scroll bar.
const ARROW: Vec2 = Vec2::new(32.0, 16.0);
const MARKER_WIDTH: f32 = 6.0;
const MARKER_X: f32 = 12.0;
/// The name: this far left of the safe zone's right edge, and its top this far under the
/// safe zone's top (with the safe zone counted twice, as the file does).
const NAME_RIGHT: f32 = 50.0;
const NAME_TOP: f32 = 170.0;
/// The thickness of every line [documented: menus\globals.xml, on a PC].
const THICK: f32 = 2.0;

// ---- measured on the owner's screenshots [observed, §3] ------------------------------

/// The height of a line of font 6, and how far its letters stand under its top.
const LINE: f32 = 38.0;
const TEXT_BASE: f32 = 23.3;
/// How far font 7's letters stand under its top.
const NAME_BASE: f32 = 19.5;
/// The choices stand this much higher in the list than the file puts them.
const LIFT: f32 = 7.2;
/// The tallest the list grows before it scrolls [hypothesis, §3].
const MAX_LIST: f32 = 290.0;
/// Where the arrows' middles stand from the list's top and bottom, and the length of the
/// marker between them.
const UP_MIDDLE: f32 = 31.0;
const DOWN_MIDDLE: f32 = 35.0;
const TRACK_TOP: f32 = 60.0;
const TRACK_BOTTOM: f32 = 52.0;
const MARKER: f32 = 40.0;
/// How dark the band makes what is behind it.
const SHADE: f32 = 0.87;
/// Between a check's tag and the line.
pub const TAG_GAP: &str = "  ";

// ---- NVForever's own [design, owner list 5 item 62] -------------------------------------

/// The hint shown while a Goodbye choice is offered.
pub const GOODBYE_HINT: &str = "TAB) Goodbye";
/// The hint's right end stands this far left of the band; its own dark box reaches this
/// far round the text.
const HINT_GAP: f32 = 20.0;
const HINT_PAD: f32 = 10.0;

/// Whether the Tab hint stands on `view`, for both views: `setting` is the player's switch for
/// it (#1517), and a Goodbye choice has to be offered [owner, list 5 item 62].
pub fn hint_shown(view: &MenuView, setting: bool) -> bool {
    setting && view.goodbye && !view.choices.is_empty()
}

/// A choice as the menu shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChoiceRow {
    /// The tag of a skill or perk check and the line.
    pub label: String,
    /// Said before: drawn dim.
    pub darkened: bool,
    /// A check the player falls short of: its frame is red.
    pub short: bool,
}

/// What the menu shows: the actor's name, and the line being said or the choices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MenuView {
    pub speaker: String,
    pub line: Option<String>,
    pub choices: Vec<ChoiceRow>,
    /// One of the choices is the Goodbye one: Tab says it.
    pub goodbye: bool,
}

/// The menu to draw (set by `crate::dialogue`), its selected choice and the first choice
/// shown ([`window`]).
#[derive(Resource, Default)]
pub struct DialogueMenu {
    pub view: Option<MenuView>,
    pub selected: usize,
    first: usize,
    /// The screen of the conversation this stands for, which its buttons carry.
    pub list: Option<ListId>,
}

// ---- layout --------------------------------------------------------------------------

/// A line of text: where it starts and the line its letters stand on.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub line: Line,
    pub x: f32,
    pub base: f32,
}

/// A rectangle: left, top, width, height.
pub type Area = [f32; 4];

#[derive(Debug, Clone, PartialEq)]
pub struct ChoicePlace {
    pub index: usize,
    pub area: Area,
    pub lines: Vec<Placed>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollBar {
    pub up: Area,
    pub down: Area,
    pub marker: Area,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListPlace {
    pub area: Area,
    /// The left end, width and height of the brackets' lines.
    pub top_line: f32,
    pub bottom_line: f32,
    pub bracket_x: f32,
    pub bracket_width: f32,
    pub choices: Vec<ChoicePlace>,
    pub scroll: Option<ScrollBar>,
}

/// The menu set out on a screen `width` units wide (960 high).
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub name: Option<Placed>,
    pub band: Option<Area>,
    pub text: Vec<Placed>,
    pub list: Option<ListPlace>,
    /// "TAB) Goodbye" and its dark box, while a Goodbye choice is offered.
    pub hint: Option<(Placed, Area)>,
    /// The first choice shown.
    pub first: usize,
}

/// The height of a text of `lines` lines in font 6.
fn text_height(lines: usize) -> f32 {
    LINE * lines as f32
}

/// Where a block `height` high starts: centred on the centring line, but not lower than
/// the room the safe zone leaves under it.
fn centred(height: f32) -> f32 {
    let inside = 960.0 - 2.0 * CROP_Y;
    (inside * CENTRE - height / 2.0).min(inside - height - BOTTOM_ROOM)
}

/// The first choice shown so that `selected` is among the shown ones, moving as little
/// as possible: as many whole choices as fit [`MAX_LIST`], at least one.
pub fn window(first: usize, selected: usize, heights: &[f32]) -> (usize, usize) {
    let n = heights.len();
    if n == 0 {
        return (0, 0);
    }
    let selected = selected.min(n - 1);
    let fits = |from: usize| {
        let mut sum = 0.0;
        let mut end = from;
        while end < n && (end == from || sum + heights[end] <= MAX_LIST) {
            sum += heights[end];
            end += 1;
        }
        end
    };
    let mut first = first.min(selected);
    while fits(first) <= selected {
        first += 1;
    }
    // Show as much as fits: no room left empty under the last choice when earlier ones
    // could be shown.
    while first > 0 && fits(first - 1) == n && fits(first) == n {
        first -= 1;
    }
    (first, fits(first))
}

const LIST_WIDTH: f32 = BAND_WIDTH - LIST_LESS;
const CHOICE_WIDTH: f32 = LIST_WIDTH - SCROLL_WIDTH - CHOICE_SHORT;

/// Each choice's lines.
fn choice_lines(view: &MenuView, body: &Font) -> Vec<Vec<Line>> {
    view.choices.iter().map(|c| body.lines(c.label.trim(), Some(CHOICE_WIDTH - CHOICE_WRAP_LESS))).collect()
}

/// Each choice's row height: its text's and the room under it.
fn choice_heights(lines: &[Vec<Line>]) -> Vec<f32> {
    lines.iter().map(|l| text_height(l.len().max(1)) + CHOICE_PAD).collect()
}

/// The safe zone at the sides: as the height's in a 4:3 screen, narrower in units on a
/// wider one [hypothesis, §3].
fn crop_x(width: f32) -> f32 {
    CROP_Y * 1280.0 / width
}

pub fn layout(view: &MenuView, first: usize, selected: usize, width: f32, body: &Font, name_font: &Font) -> Layout {
    let band_x = (width - BAND_WIDTH) / 2.0;
    let name = Some(view.speaker.trim()).filter(|s| !s.is_empty()).map(|s| {
        let line = name_font.lines(s, None).remove(0);
        let right = width - 2.0 * crop_x(width) - NAME_RIGHT;
        Placed { x: right - line.width, base: 2.0 * CROP_Y + NAME_TOP + NAME_BASE, line }
    });
    let mut out = Layout { name, band: None, text: Vec::new(), list: None, hint: None, first: 0 };
    if view.choices.is_empty() {
        let Some(text) = view.line.as_deref().map(str::trim).filter(|t| !t.is_empty()) else { return out };
        let lines = body.lines(text, Some(BAND_WIDTH - 2.0 * TEXT_IN));
        let height = text_height(lines.len());
        let top = centred(height);
        out.band = Some([band_x, top - TEXT_BAND_UP, BAND_WIDTH, height + TEXT_BAND_MORE]);
        out.text = lines.into_iter().enumerate().map(|(i, line)| Placed { line, x: band_x + TEXT_IN, base: top + TEXT_BASE + LINE * i as f32 }).collect();
        return out;
    }

    let list_x = band_x + LIST_IN;
    let choice_x = list_x + SCROLL_WIDTH;
    let lines = choice_lines(view, body);
    let heights = choice_heights(&lines);
    let (first, end) = window(first, selected, &heights);
    let height = heights[first..end].iter().sum::<f32>().max(MIN_LIST);
    let top = centred(height);
    let mut y = top - LIFT;
    let mut choices = Vec::new();
    for (i, ls) in lines.iter().enumerate().take(end).skip(first) {
        let placed = ls
            .iter()
            .enumerate()
            .map(|(j, l)| Placed { line: l.clone(), x: choice_x + CHOICE_TEXT.x, base: y + CHOICE_TEXT.y + TEXT_BASE + LINE * j as f32 })
            .collect();
        choices.push(ChoicePlace { index: i, area: [choice_x, y, CHOICE_WIDTH, heights[i]], lines: placed });
        y += heights[i];
    }
    let bottom = top + height;
    let scroll = (end - first < view.choices.len()).then(|| {
        let (track_top, track_bottom) = (top + TRACK_TOP, bottom - TRACK_BOTTOM);
        let hidden = view.choices.len() - (end - first);
        let along = (track_bottom - track_top - MARKER).max(0.0) * first as f32 / hidden as f32;
        ScrollBar {
            up: [list_x, top + UP_MIDDLE - ARROW.y / 2.0, ARROW.x, ARROW.y],
            down: [list_x, bottom - DOWN_MIDDLE - ARROW.y / 2.0, ARROW.x, ARROW.y],
            marker: [list_x + MARKER_X, track_top + along, MARKER_WIDTH, MARKER],
        }
    });
    let top_line = top - BRACKET_ABOVE;
    let bottom_line = bottom + BRACKET_BELOW;
    out.band = Some([band_x, top_line - LIST_BAND_UP, BAND_WIDTH, bottom_line - top_line + LIST_BAND_UP + LIST_BAND_DOWN]);
    out.list = Some(ListPlace { area: [list_x, top, LIST_WIDTH, height], top_line, bottom_line, bracket_x: list_x + BRACKET_IN, bracket_width: LIST_WIDTH, choices, scroll });
    out.first = first;
    if hint_shown(view, true) {
        let line = body.lines(GOODBYE_HINT, None).remove(0);
        // Left of the band where the screen has room, else at the safe zone's edge.
        let x = (band_x - HINT_GAP - line.width).max(crop_x(width) + HINT_PAD);
        let top = bottom_line - LINE / 2.0;
        let area = [x - HINT_PAD, top - HINT_PAD / 2.0, line.width + 2.0 * HINT_PAD, LINE + HINT_PAD];
        out.hint = Some((Placed { x, base: top + TEXT_BASE, line }, area));
    }
    out
}

// ---- the game's fonts and pictures ---------------------------------------------------

/// Font 6 (the line and the choices) and font 7 (the name) [documented: the interface
/// file names them by number; Fallout_default.ini `sFontFile_6`, `_7`].
const BODY_FONT: &str = "textures\\fonts\\glow_monofonto_vl_dialogs.fnt";
const NAME_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";

struct Kit {
    body: Face,
    name: Face,
    band: Option<Picture>,
    fade_down: Option<Picture>,
    fade_up: Option<Picture>,
    arrow_up: Option<Picture>,
    arrow_down: Option<Picture>,
    marker: Option<Picture>,
}

fn load_kit(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
    let mut pictures = Pictures::load(data);
    let mut picture = |path: &str| pictures.picture(path, data, images, bc);
    let band = picture("interface\\shared\\background\\solid_black.dds");
    let fade_down = picture("interface\\shared\\line\\fade_to_bottom.dds");
    let fade_up = picture("interface\\shared\\line\\fade_to_top.dds");
    let arrow_up = picture("interface\\shared\\scrollbar\\arrow_up.dds");
    let arrow_down = picture("interface\\shared\\scrollbar\\arrow_down.dds");
    let marker = picture("interface\\shared\\scrollbar\\vert_marker.dds");
    Kit {
        body: face(BODY_FONT, plain_font(34.0, 17.0), data, images, bc),
        name: face(NAME_FONT, plain_font(36.0, 16.0), data, images, bc),
        band,
        fade_down,
        fade_up,
        arrow_up,
        arrow_down,
        marker,
    }
}

// ---- drawing ---------------------------------------------------------------------------

pub struct DialogueMenuPlugin;

impl Plugin for DialogueMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DialogueMenu>().add_systems(Update, draw.in_set(crate::PlaySet::Ui).in_set(DrawMenu));
    }
}

/// The drawing of the menu, for what fills [`DialogueMenu`] to run before.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrawMenu;

#[derive(Component)]
struct MenuRoot;

/// The frame of a choice, shown while it is the selected one.
#[derive(Component)]
struct Frame(usize);

/// The line of the red frame of a check the player falls short of [observed: §3].
fn short_line() -> Color {
    Color::srgb_u8(100, 18, 16)
}

fn image(p: Option<&Picture>, color: Color) -> Option<ImageNode> {
    p.map(|(image, rect)| ImageNode { rect: *rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) })
}

fn spawn_menu(commands: &mut Commands, view: &MenuView, l: &Layout, kit: &Kit, selected: usize, on_screen: Option<&ListId>) {
    let screen = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    let bright = amber(255.0);
    commands.spawn((MenuRoot, screen, GlobalZIndex(90))).with_children(|s| {
        if let Some([x, y, w, h]) = l.band {
            let shade = Color::srgba(0.0, 0.0, 0.0, SHADE);
            match image(kit.band.as_ref(), shade) {
                Some(i) => s.spawn((at(x, y, w, h), i)),
                None => s.spawn((at(x, y, w, h), BackgroundColor(shade))),
            };
        }
        if let Some(n) = &l.name {
            set_line(s, &kit.name, &n.line, n.x, n.base, bright);
        }
        if let Some((hint, [x, y, w, h])) = &l.hint {
            let shade = Color::srgba(0.0, 0.0, 0.0, SHADE);
            match image(kit.band.as_ref(), shade) {
                Some(i) => s.spawn((at(*x, *y, *w, *h), i)),
                None => s.spawn((at(*x, *y, *w, *h), BackgroundColor(shade))),
            };
            set_line(s, &kit.body, &hint.line, hint.x, hint.base, bright);
        }
        for t in &l.text {
            set_line(s, &kit.body, &t.line, t.x, t.base, bright);
        }
        let Some(list) = &l.list else { return };
        // The brackets: a line with an upright at each end that fades out, down from the
        // top one and up from the bottom one.
        let (bx, bw) = (list.bracket_x, list.bracket_width);
        for (y, fade, ends_y) in [(list.top_line, &kit.fade_down, list.top_line), (list.bottom_line, &kit.fade_up, list.bottom_line + THICK - BRACKET_ENDS)] {
            s.spawn((at(bx, y, bw, THICK), BackgroundColor(bright)));
            for x in [bx, bx + bw - THICK] {
                if let Some(i) = image(fade.as_ref(), bright) {
                    s.spawn((at(x, ends_y, THICK, BRACKET_ENDS), i));
                }
            }
        }
        if let Some(bar) = &list.scroll {
            for (area, p) in [(bar.up, &kit.arrow_up), (bar.down, &kit.arrow_down), (bar.marker, &kit.marker)] {
                let [x, y, w, h] = area;
                match image(p.as_ref(), bright) {
                    Some(i) => s.spawn((at(x, y, w, h), i)),
                    None => s.spawn((at(x + w / 2.0 - THICK, y, 2.0 * THICK, h), BackgroundColor(bright))),
                };
            }
        }
        // The choices, cut at the brackets' lines.
        let clip_top = list.top_line + THICK;
        let mut window = at(0.0, clip_top, 0.0, list.bottom_line - clip_top);
        (window.width, window.overflow) = (Val::Percent(100.0), Overflow::clip());
        s.spawn(window).with_children(|w| {
            for c in &list.choices {
                let [x, y, cw, ch] = c.area;
                let row = &view.choices[c.index];
                w.spawn((Button, ChoiceButton(c.index, on_screen.cloned()), at(x, y - clip_top, cw, ch))).with_children(|r| {
                    // [documented: the list's highlight box] One unit short of the row; a
                    // faint fill and a line around it, red for a check the player falls
                    // short of [observed].
                    let h = ch - 1.0;
                    let (line, fill) = if row.short { (short_line(), Color::srgba_u8(62, 0, 0, 82)) } else { (amber(200.0), amber(128.0).with_alpha(0.1)) };
                    let shown = if c.index == selected { Visibility::Inherited } else { Visibility::Hidden };
                    r.spawn((Frame(c.index), at(0.0, 0.0, cw, h), shown, BackgroundColor(fill))).with_children(|f| {
                        for fy in [0.0, h - THICK] {
                            f.spawn((at(0.0, fy, cw, THICK), BackgroundColor(line)));
                        }
                        for fx in [0.0, cw - THICK] {
                            f.spawn((at(fx, THICK, THICK, h - 2.0 * THICK), BackgroundColor(line)));
                        }
                    });
                    // Said before: as bright as the interface's disabled text (128 of 255).
                    let color = if row.darkened { amber(128.0) } else { bright };
                    for t in &c.lines {
                        set_line(r, &kit.body, &t.line, t.x - x, t.base - y, color);
                    }
                });
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    mut menu: ResMut<DialogueMenu>,
    data: Option<Res<GameData>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    roots: Query<Entity, With<MenuRoot>>,
    mut frames: Query<(&Frame, &mut Visibility)>,
    mut kit: Local<Option<Kit>>,
    mut shown: Local<Option<(MenuView, usize, u32)>>,
    creator: Option<Res<crate::creator::CreatorMenu>>,
    tiles: Option<Res<crate::tiles::dialog_menu::DialogTiles>>,
    mut buttons: Query<&mut ChoiceButton>,
) {
    // Drawn from its file: all of the menu, none of it here (`crate::tiles::dialog_menu`).
    if tiles.is_some_and(|t| t.by_tiles()) {
        for e in &roots {
            commands.entity(e).despawn();
        }
        *shown = None;
        return;
    }
    let width = windows.single().map_or(960.0 * 16.0 / 9.0, |w| 960.0 * w.width() / w.height().max(1.0));
    let selected = menu.selected;
    let kit = kit.get_or_insert_with(|| load_kit(data.as_deref(), &mut images, bc_supported(formats.as_deref())));
    // Where the list starts follows the selection: the menu is rebuilt when it scrolls.
    let first = match &menu.view {
        Some(v) if !v.choices.is_empty() => window(menu.first, selected, &choice_heights(&choice_lines(v, &kit.body.font))).0,
        _ => 0,
    };
    if menu.first != first {
        menu.first = first;
    }
    // Nothing over the character creator: the opening opens it at the end of a line of
    // Doc Mitchell's, still in the conversation [owner 2026-10-17].
    let creating = creator.is_some_and(|c| c.is_open());
    let key = menu.view.clone().filter(|_| !creating).map(|v| (v, first, width.round() as u32));
    if key != *shown {
        for e in &roots {
            commands.entity(e).despawn();
        }
        if let Some((view, first, _)) = &key {
            let l = layout(view, *first, selected, width, &kit.body.font, &kit.name.font);
            if l.hint.is_some() && shown.as_ref().is_none_or(|s| !s.0.goodbye) {
                info!("dialogue menu: {GOODBYE_HINT} shown at the left of the choices");
            }
            spawn_menu(&mut commands, view, &l, kit, selected, menu.list.as_ref());
        }
        *shown = key;
        return;
    }
    for (frame, mut v) in &mut frames {
        v.set_if_neq(if frame.0 == selected { Visibility::Inherited } else { Visibility::Hidden });
    }
    // The same words for the next screen (no rebuild): what stands is that screen's now.
    for mut b in &mut buttons {
        if b.1 != menu.list {
            b.1.clone_from(&menu.list);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character 17 on, spaces 14; sizes as the game's fonts 6 and 7.
    fn font(size: f32) -> Font {
        let mut f = plain_font(size, 17.0);
        f.glyphs[b' ' as usize].after = 14.0;
        f
    }

    fn choices(labels: &[&str]) -> MenuView {
        MenuView { speaker: "Trudy".into(), line: None, choices: labels.iter().map(|l| ChoiceRow { label: l.to_string(), ..default() }).collect(), goodbye: false }
    }

    #[test]
    fn the_goodbye_hint_stands_left_of_the_band() {
        let mut v = choices(&["What happened here?", "Goodbye."]);
        let l = layout(&v, 0, 0, 1707.0, &font(34.0), &font(36.0));
        assert!(l.hint.is_none(), "no hint without a Goodbye choice");
        v.goodbye = true;
        let l = layout(&v, 0, 0, 1707.0, &font(34.0), &font(36.0));
        let (hint, area) = l.hint.expect("the hint");
        let band = l.band.unwrap();
        let list = l.list.unwrap();
        assert!(hint.line.width > 0.0);
        // Its right end is left of the band, its line level with the bottom bracket.
        assert!(hint.x + hint.line.width <= band[0] - HINT_GAP + 0.01);
        assert!(area[1] < list.bottom_line && list.bottom_line < area[1] + area[3]);
        // On a 4:3 screen it keeps inside the safe zone.
        let l = layout(&v, 0, 0, 1280.0, &font(34.0), &font(36.0));
        assert!(l.hint.unwrap().0.x >= crop_x(1280.0));
    }

    #[test]
    fn the_line_is_centred_on_its_band() {
        // Two lines on a 16:9 screen, as the owner's screenshot of Bruce Isaac.
        let text = format!("{} {}", "a".repeat(55), "b".repeat(5));
        let v = MenuView { speaker: "Bruce Isaac".into(), line: Some(text), choices: Vec::new(), goodbye: false };
        let width = 960.0 * 16.0 / 9.0;
        let l = layout(&v, 0, 0, width, &font(34.0), &font(36.0));
        assert_eq!(l.text.len(), 2);
        // The text's top at 706, its first line standing 23.3 under it, the next 38 lower.
        assert!((l.text[0].base - 729.3).abs() < 1e-3);
        assert!((l.text[1].base - 767.3).abs() < 1e-3);
        // Starts 60 into the band, which is 1080 wide and centred.
        let band = l.band.unwrap();
        assert!((band[0] - (width - 1080.0) / 2.0).abs() < 1e-3);
        assert!((l.text[0].x - band[0] - 60.0).abs() < 1e-3);
        assert_eq!((band[1], band[3]), (661.0, 156.0));
        assert!(l.list.is_none());
        // The name stands right of the middle at the top, ending short of the edge.
        let n = l.name.unwrap();
        assert!((n.x + n.line.width - (width - 2.0 * crop_x(width) - 50.0)).abs() < 1e-3);
        assert_eq!(n.base, 219.5);
    }

    #[test]
    fn the_choices_stand_between_the_brackets() {
        // Three choices on a 4:3 screen, two of two lines and one of one, as the owner's
        // screenshot of Trudy; a fourth that does not fit.
        let long = "abcde ".repeat(12);
        let v = choices(&[&long, &long, "Do you know the robot?", &long]);
        let l = layout(&v, 0, 1, 1280.0, &font(34.0), &font(36.0));
        let list = l.list.unwrap();
        assert_eq!(list.choices.len(), 3);
        // The list is 250 high, its top at 619 and the brackets' lines 20 above and 10
        // below it.
        assert_eq!(list.area, [120.0, 619.0, 1010.0, 250.0]);
        assert_eq!((list.top_line, list.bottom_line, list.bracket_x), (599.0, 879.0, 135.0));
        // The rows: 96, 96 and 58 high, starting 7.2 above the list's top, from the right
        // of the scroll bar.
        let tops: Vec<f32> = list.choices.iter().map(|c| c.area[1]).collect();
        assert!((tops[0] - 611.8).abs() < 1e-3 && (tops[1] - 707.8).abs() < 1e-3 && (tops[2] - 803.8).abs() < 1e-3);
        assert_eq!(list.choices[0].area[0], 152.0);
        assert_eq!(list.choices[0].area[2], 973.0);
        let first = &list.choices[0].lines;
        assert!((first[0].base - (611.8 + 15.0 + 23.3)).abs() < 1e-3);
        assert_eq!(first[0].x, 177.0);
        // The scroll bar shows because the fourth choice does not fit.
        let bar = list.scroll.unwrap();
        assert_eq!(bar.up[0], 120.0);
        assert!((bar.up[1] + 8.0 - 650.0).abs() < 1e-3);
        // The band: 30 above and below the brackets.
        assert_eq!(l.band.unwrap(), [100.0, 569.0, 1080.0, 340.0]);
    }

    #[test]
    fn a_short_list_keeps_its_least_height() {
        let l = layout(&choices(&["Goodbye."]), 0, 0, 1280.0, &font(34.0), &font(36.0));
        let list = l.list.unwrap();
        assert_eq!(list.area[3], 110.0);
        assert!(list.scroll.is_none());
    }

    #[test]
    fn the_window_follows_the_selection() {
        let h = [58.0; 8];
        // Five rows of 58 fit in 290.
        assert_eq!(window(0, 0, &h), (0, 5));
        assert_eq!(window(0, 4, &h), (0, 5));
        assert_eq!(window(0, 5, &h), (1, 6));
        assert_eq!(window(3, 7, &h), (3, 8));
        // Going up moves the window only when the selection leaves it.
        assert_eq!(window(3, 4, &h), (3, 8));
        assert_eq!(window(3, 2, &h), (2, 7));
        // A window that could show more above when the list got shorter.
        assert_eq!(window(6, 7, &h[..7]), (2, 7));
        // One very long choice is shown alone.
        assert_eq!(window(0, 0, &[400.0, 58.0]), (0, 1));
        assert_eq!(window(0, 0, &[]), (0, 0));
    }
}
