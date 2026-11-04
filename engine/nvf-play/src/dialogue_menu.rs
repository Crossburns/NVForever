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
//!   level with the bottom bracket, in the band's own text on a backing in its shade (owner, list 5 item 62; the
//!   interface file has no such hint [observed: dialog_menu.xml]): Tab picks that choice
//!   (`crate::dialogue`). The settings page's "Goodbye hint" switch ([`GoodbyeHint`])
//!   hides only the words; Tab works as before.
//! - The menu is rebuilt when what it shows changes; the pointer chooses the choice it
//!   is on (`crate::dialogue` reads the clicks).

use crate::dialogue::ChoiceButton;
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
/// The hint's backing (the band's shade, flush against the band's left edge) reaches this
/// far left of the words and this far above and below their letters; the words' right end
/// stands as far from the band [design].
const HINT_PAD_X: f32 = 10.0;
const HINT_PAD_Y: f32 = 8.0;
/// Where the screen has no room at the side, the band grows this much under the bottom
/// bracket and the hint stands in that strip, this far in from the band's edge (as the
/// line of text starts) [design].
const HINT_ROW: f32 = 20.0;
const HINT_IN: f32 = TEXT_IN;
/// How tall font 6's capital letters stand (the line is [`LINE`] high and its letters stand
/// [`TEXT_BASE`] under its top); the hint's letters are centred on the bottom bracket's
/// line, or in the strip of band under it, by this [design].
const HINT_CAP: f32 = 17.0;

/// The settings page's "Goodbye hint" switch (key `dialogue-goodbye-hint`): on by
/// default [owner, list 5 item 62]; off hides the words only, Tab still says goodbye.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoodbyeHint(pub bool);

impl Default for GoodbyeHint {
    fn default() -> Self {
        GoodbyeHint(true)
    }
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
/// shown.
#[derive(Resource, Default)]
pub struct DialogueMenu {
    pub view: Option<MenuView>,
    pub selected: usize,
    first: usize,
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
    /// "TAB) Goodbye", while a Goodbye choice is offered and the switch is on.
    pub hint: Option<Placed>,
    /// Its backing in the band's shade, when it stands at the band's side.
    pub hint_back: Option<Area>,
    /// The first choice shown.
    pub first: usize,
}

/// The height of a text of `lines` lines in font 6.
fn text_height(lines: usize) -> f32 {
    LINE * lines as f32
}

/// Where a block `height` high starts: centred on the centring line, but not lower than
/// the room the safe zone leaves under it.
fn centred(height: f32, extra: f32) -> f32 {
    let inside = 960.0 - 2.0 * CROP_Y;
    (inside * CENTRE - height / 2.0).min(inside - height - BOTTOM_ROOM - extra)
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

/// Whether "TAB) Goodbye", `words` wide, stands left of the band with its backing inside
/// the safe zone.
fn hint_at_side(words: f32, width: f32, band_x: f32) -> bool {
    band_x - HINT_PAD_X - words - HINT_PAD_X >= crop_x(width)
}

/// Where "TAB) Goodbye" stands, its letters on `base`, and its backing. At the band's side:
/// the words' right end [`HINT_PAD_X`] from the band's edge, their letters centred on the
/// bottom bracket's line, on a backing flush against the band. Else (a 4:3 screen) inside
/// the band, in the strip [`HINT_ROW`] taller than the band's own under the bottom bracket,
/// [`HINT_IN`] from the band's left edge (kept inside the safe zone), its letters centred
/// in the strip: clear of the bracket's corner, under the choices and the scroll bar
/// [design].
fn hint_place(line: Line, width: f32, band_x: f32, bottom_line: f32) -> (Placed, Option<Area>) {
    if hint_at_side(line.width, width, band_x) {
        let x = band_x - HINT_PAD_X - line.width;
        let base = bottom_line + (THICK + HINT_CAP) / 2.0;
        let back = [x - HINT_PAD_X, base - HINT_CAP - HINT_PAD_Y, line.width + 2.0 * HINT_PAD_X, HINT_CAP + 2.0 * HINT_PAD_Y];
        return (Placed { x, base, line }, Some(back));
    }
    let right = band_x + BAND_WIDTH - HINT_IN - line.width;
    let x = (band_x + HINT_IN).min(right).max(crop_x(width));
    (Placed { x, base: bottom_line + (THICK + LIST_BAND_DOWN + HINT_ROW + HINT_CAP) / 2.0, line }, None)
}

/// `hint`: whether the "Goodbye hint" switch is on.
pub fn layout(view: &MenuView, first: usize, selected: usize, width: f32, hint: bool, body: &Font, name_font: &Font) -> Layout {
    let band_x = (width - BAND_WIDTH) / 2.0;
    let name = Some(view.speaker.trim()).filter(|s| !s.is_empty()).map(|s| {
        let line = name_font.lines(s, None).remove(0);
        let right = width - 2.0 * crop_x(width) - NAME_RIGHT;
        Placed { x: right - line.width, base: 2.0 * CROP_Y + NAME_TOP + NAME_BASE, line }
    });
    let mut out = Layout { name, band: None, text: Vec::new(), list: None, hint: None, hint_back: None, first: 0 };
    if view.choices.is_empty() {
        let Some(text) = view.line.as_deref().map(str::trim).filter(|t| !t.is_empty()) else { return out };
        let lines = body.lines(text, Some(BAND_WIDTH - 2.0 * TEXT_IN));
        let height = text_height(lines.len());
        let top = centred(height, 0.0);
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
    // The hint's own words, and the room under the band they need where they stand in it.
    let hint_line = (view.goodbye && hint).then(|| body.lines(GOODBYE_HINT, None).remove(0));
    let room = hint_line.as_ref().map_or(0.0, |l| if hint_at_side(l.width, width, band_x) { 0.0 } else { HINT_ROW });
    let top = centred(height, room);
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
    out.band = Some([band_x, top_line - LIST_BAND_UP, BAND_WIDTH, bottom_line - top_line + LIST_BAND_UP + LIST_BAND_DOWN + room]);
    out.list = Some(ListPlace { area: [list_x, top, LIST_WIDTH, height], top_line, bottom_line, bracket_x: list_x + BRACKET_IN, bracket_width: LIST_WIDTH, choices, scroll });
    out.first = first;
    if let Some(line) = hint_line {
        let (placed, back) = hint_place(line, width, band_x, bottom_line);
        (out.hint, out.hint_back) = (Some(placed), back);
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
        app.init_resource::<DialogueMenu>().init_resource::<GoodbyeHint>().add_systems(Update, draw.in_set(crate::PlaySet::Ui).in_set(DrawMenu));
        crate::settings::add_setting(
            app,
            crate::settings::Setting {
                key: "dialogue-goodbye-hint",
                group: crate::pause::GROUP_MENUS,
                label: "Goodbye hint",
                note: "On: while a conversation offers a Goodbye choice, \"TAB) Goodbye\" stands at the left of the dialogue box. Off: the words are hidden; Tab still says goodbye.",
                get: |w| w.resource::<GoodbyeHint>().0,
                set: |w, on| w.resource_mut::<GoodbyeHint>().0 = on,
                slider: None,
            },
        );
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

fn spawn_menu(commands: &mut Commands, view: &MenuView, l: &Layout, kit: &Kit, selected: usize) {
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
        if let Some([x, y, w, h]) = l.hint_back {
            let shade = Color::srgba(0.0, 0.0, 0.0, SHADE);
            match image(kit.band.as_ref(), shade) {
                Some(i) => s.spawn((at(x, y, w, h), i)),
                None => s.spawn((at(x, y, w, h), BackgroundColor(shade))),
            };
        }
        if let Some(hint) = &l.hint {
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
                w.spawn((Button, ChoiceButton(c.index), at(x, y - clip_top, cw, ch))).with_children(|r| {
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

/// What the drawn menu was made from: the menu's view, the first choice shown, the width
/// in units and the "Goodbye hint" switch. The menu is rebuilt when it changes, so a
/// flip of the switch shows at once.
type MenuKey = (MenuView, usize, u32, bool);

fn menu_key(view: Option<&MenuView>, creating: bool, first: usize, width: f32, hint: bool) -> Option<MenuKey> {
    view.filter(|_| !creating).map(|v| (v.clone(), first, width.round() as u32, hint))
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
    mut shown: Local<Option<MenuKey>>,
    creator: Option<Res<crate::creator::CreatorMenu>>,
    hint: Res<GoodbyeHint>,
) {
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
    let key = menu_key(menu.view.as_ref(), creating, first, width, hint.0);
    if key != *shown {
        for e in &roots {
            commands.entity(e).despawn();
        }
        if let Some((view, first, _, hint_on)) = &key {
            let l = layout(view, *first, selected, width, *hint_on, &kit.body.font, &kit.name.font);
            // One line when the hint appears, or when the switch holds it back.
            let was = shown.as_ref().and_then(|s| s.0.goodbye.then_some(s.3));
            match (view.goodbye.then_some(*hint_on), was) {
                (Some(true), was) if was != Some(true) => info!("dialogue menu: {GOODBYE_HINT} shown at the left of the choices"),
                (Some(false), was) if was != Some(false) => info!("dialogue menu: {GOODBYE_HINT} hidden by the Goodbye hint setting"),
                _ => {}
            }
            spawn_menu(&mut commands, view, &l, kit, selected);
        }
        *shown = key;
        return;
    }
    for (frame, mut v) in &mut frames {
        v.set_if_neq(if frame.0 == selected { Visibility::Inherited } else { Visibility::Hidden });
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

    fn lay(v: &MenuView, width: f32, hint: bool) -> Layout {
        layout(v, 0, 0, width, hint, &font(34.0), &font(36.0))
    }

    /// The hint's letters' box: left, top, right, bottom.
    fn hint_box(h: &Placed) -> [f32; 4] {
        [h.x, h.base - HINT_CAP, h.x + h.line.width, h.base]
    }

    fn meets(a: [f32; 4], b: Area) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[2] && a[1] < b[1] + b[3] && b[1] < a[3]
    }

    fn goodbye(labels: &[&str]) -> MenuView {
        MenuView { goodbye: true, ..choices(labels) }
    }

    #[test]
    fn the_goodbye_hint_stands_left_of_the_band_on_a_backing() {
        let mut v = choices(&["What happened here?", "Goodbye."]);
        assert!(lay(&v, 1707.0, true).hint.is_none(), "no hint without a Goodbye choice");
        v.goodbye = true;
        let l = lay(&v, 1707.0, true);
        let hint = l.hint.expect("the hint");
        let (band, list, back) = (l.band.unwrap(), l.list.unwrap(), l.hint_back.expect("its backing"));
        assert!(hint.line.width > 0.0);
        // The words end HINT_PAD_X from the band, their letters centred on the bottom
        // bracket's line.
        assert!((hint.x + hint.line.width - (band[0] - HINT_PAD_X)).abs() < 0.01);
        assert!((hint.base - HINT_CAP / 2.0 - (list.bottom_line + THICK / 2.0)).abs() < 0.01);
        // The backing is flush against the band, reaches HINT_PAD_X and HINT_PAD_Y round the
        // letters, and stays inside the safe zone.
        assert!((back[0] + back[2] - band[0]).abs() < 0.01);
        let b = hint_box(&hint);
        assert!((b[0] - back[0] - HINT_PAD_X).abs() < 0.01 && (b[1] - back[1] - HINT_PAD_Y).abs() < 0.01);
        assert!((back[1] + back[3] - b[3] - HINT_PAD_Y).abs() < 0.01);
        assert!(back[0] >= crop_x(1707.0));
        // The band is the height it is without the hint.
        assert_eq!(l.band, lay(&v, 1707.0, false).band);
    }

    #[test]
    fn the_goodbye_hint_moves_into_the_band_on_a_4_3_screen_clear_of_the_bracket() {
        let v = goodbye(&["One", "Two", "Goodbye."]);
        // 1280 units wide is 4:3 (1024x768 too); 1200 is 5:4. The band's left edge is 100
        // and 60 from the screen's, the words 200 wide: no room at the side.
        for width in [1280.0, 1200.0, 1100.0] {
            let l = lay(&v, width, true);
            let hint = l.hint.expect("the hint");
            let (band, list) = (l.band.unwrap(), l.list.unwrap());
            assert!(l.hint_back.is_none(), "width {width}: no backing inside the band");
            let [left, top, right, bottom] = hint_box(&hint);
            // In the strip the band grows under the bottom bracket, HINT_IN from its left edge
            // (or the safe zone), well clear of the bracket's line and corner.
            assert!((left - (band[0] + HINT_IN).max(crop_x(width))).abs() < 0.01, "width {width}");
            assert!(top - (list.bottom_line + THICK) >= 10.0, "width {width}: clear of the bracket");
            assert!(bottom <= band[1] + band[3] - 10.0, "width {width}: inside the band's bottom");
            assert!(bottom <= 960.0 - CROP_Y, "width {width}: inside the safe zone");
            assert!(left >= crop_x(width) && right <= width - crop_x(width), "width {width}: inside the safe zone");
            assert_eq!(band[3], lay(&v, width, false).band.unwrap()[3] + HINT_ROW, "width {width}: the band grew for it");
        }
    }

    #[test]
    fn the_goodbye_hint_never_covers_a_choice_or_the_scroll_bar() {
        // Six long choices scroll (the list at its tallest); 5:4, 4:3, wider screens, the
        // selection at both ends.
        let long = "Tell me everything you know about the people who attacked the town last night";
        let v = goodbye(&[long, long, long, long, long, "Goodbye."]);
        for width in [1200.0, 1280.0, 1440.0, 1707.0, 2560.0] {
            for selected in [0, 5] {
                let l = layout(&v, 0, selected, width, true, &font(34.0), &font(36.0));
                let hint = l.hint.expect("the hint");
                let b = hint_box(&hint);
                let (band, list) = (l.band.unwrap(), l.list.unwrap());
                assert!(list.scroll.is_some());
                for c in &list.choices {
                    assert!(!meets(b, c.area), "width {width}: over choice {}", c.index);
                }
                let bar = list.scroll.as_ref().unwrap();
                for a in [bar.up, bar.down, bar.marker] {
                    assert!(!meets(b, a), "width {width}: over the scroll bar");
                }
                if let Some(back) = l.hint_back {
                    assert!(!meets(b, band) && back[0] >= crop_x(width), "width {width}: the side's backing is outside the band");
                }
                assert!(b[0] >= crop_x(width) && b[2] <= width - crop_x(width), "width {width}: inside the safe zone");
                assert!(band[1] + band[3] <= 960.0 - CROP_Y, "width {width}: the band ends inside the safe zone");
            }
        }
    }

    #[test]
    fn the_goodbye_hint_switch_hides_only_the_words() {
        let v = goodbye(&["What happened here?", "Goodbye."]);
        // At the side nothing else moves.
        let (on, off) = (lay(&v, 1707.0, true), lay(&v, 1707.0, false));
        assert!(on.hint.is_some() && on.hint_back.is_some());
        assert!(off.hint.is_none() && off.hint_back.is_none());
        assert_eq!((on.band, on.list, on.name), (off.band, off.list, off.name));
        // Inside the band, the room made for the words goes with them; the list and name stay.
        let (on, off) = (lay(&v, 1280.0, true), lay(&v, 1280.0, false));
        assert!(on.hint.is_some() && off.hint.is_none());
        assert_eq!((on.list, on.name), (off.list, off.name));
    }

    #[test]
    fn a_flip_of_the_switch_rebuilds_the_menu() {
        let mut v = choices(&["Hello.", "Goodbye."]);
        v.goodbye = true;
        let on = menu_key(Some(&v), false, 0, 1707.0, true);
        assert_eq!(on, menu_key(Some(&v), false, 0, 1707.2, true));
        assert_ne!(on, menu_key(Some(&v), false, 0, 1707.0, false), "the drawing's cache sees the switch");
        assert_eq!(menu_key(Some(&v), true, 0, 1707.0, true), None, "nothing over the character creator");
    }

    #[test]
    fn the_goodbye_hint_is_a_setting_on_by_default() {
        let mut app = App::new();
        app.add_plugins(DialogueMenuPlugin);
        let world = app.world_mut();
        assert!(world.resource::<GoodbyeHint>().0);
        let settings = world.resource::<crate::settings::Settings>();
        let s = settings.list()[settings.index("dialogue-goodbye-hint").expect("listed")];
        assert_eq!((s.group, s.label), (crate::pause::GROUP_MENUS, "Goodbye hint"));
        assert!((s.get)(world));
        (s.set)(world, false);
        assert!(!world.resource::<GoodbyeHint>().0 && !(s.get)(world));
    }

    #[test]
    fn the_line_is_centred_on_its_band() {
        // Two lines on a 16:9 screen, as the owner's screenshot of Bruce Isaac.
        let text = format!("{} {}", "a".repeat(55), "b".repeat(5));
        let v = MenuView { speaker: "Bruce Isaac".into(), line: Some(text), choices: Vec::new(), goodbye: false };
        let width = 960.0 * 16.0 / 9.0;
        let l = layout(&v, 0, 0, width, true, &font(34.0), &font(36.0));
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
        let l = layout(&v, 0, 1, 1280.0, true, &font(34.0), &font(36.0));
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
        let l = layout(&choices(&["Goodbye."]), 0, 0, 1280.0, true, &font(34.0), &font(36.0));
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
