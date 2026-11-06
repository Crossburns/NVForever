//! Drawing the character creator (docs/spec-notes/character-creator.md §4): a panel on
//! the right over the world, the player's face in the clear part of the picture at the
//! left. That arrangement, the frame, the controls and the pages along the bottom are
//! proposed to the maintainer: closer to the original's portrait and terminal, on a
//! draft [design; the original's arrangement observed, the note's §4]. The menu's own
//! look stands [owner 2026-10-16: "something new in design without having to add new
//! assets"]: the game menus' own fonts, pieces and plain drawn shapes, in the
//! interface's amber [owner 2026-10-17: "can we also change the colour scheme to match
//! the UI amber look im not a fan of the light blue trim"; it was bone white with a
//! turquoise accent before]. The fonts are the menus' [owner 2026-10-17: "if we can also
//! get the font to match our game"; it was Futura for titles, tabs and buttons before]:
//! the titles' glowing Monofonto (font 6, as the tag-skill and trait menus' headlines,
//! chargen.md §3) for the title, the tabs and the buttons, Monofonto (font 2) for the
//! rest.
//!
//! Lengths are the interface's units (960 to the view's height), from the panel's left
//! edge and the view's top. [`spawn`] returns where each thing the pointer can use lies
//! in the view ([`Hit`]), so the pointer is tested against those rectangles rather than
//! UI interaction state (the panel is rebuilt while a slider is dragged).

use super::model::{Model, RowView, ACTIONS};
use crate::interface::{amber, at, face, line_anchor, plain_font, u, Face, Picture, Pictures};
use bevy::prelude::*;
use fnv_font::Line;
use nvf_assets::GameData;

/// Font 6, the menus' titles, and font 2, their body text [documented:
/// Fallout_default.ini, docs/spec-notes/fonts.md; the same files as `chargen::draw`].
const HEAD_FONT: &str = "textures\\fonts\\glow_monofonto_vl_dialogs.fnt";
const BODY_FONT: &str = "textures\\fonts\\monofonto_large.fnt";

/// The colours: all the interface's amber (`crate::interface::AMBER`) [owner
/// 2026-10-17], at the brightnesses the other menus use for them [design]: text at 215
/// (and fainter by alpha where it is a hint or not chosen), the frame, the chosen row's
/// square and band, the slider's fill and knob, Done's outline at full brightness, values at
/// full brightness too.
pub fn bone(alpha: f32) -> Color {
    amber(215.0).with_alpha(alpha)
}
pub fn accent(alpha: f32) -> Color {
    amber(255.0).with_alpha(alpha)
}
pub fn sand(alpha: f32) -> Color {
    amber(255.0).with_alpha(alpha)
}
/// The chosen row's band and the chosen button's fill: the menus' highlight fill, amber
/// at brightness 128 and alpha 40 of 255 [documented: box.xml `_fill_alpha` 40, as
/// `chargen::draw::Ctx::highlight`; design: a little stronger over the dark panel].
pub fn highlight() -> Color {
    amber(128.0).with_alpha(0.25)
}

/// The dark backing when the game's picture cannot be read (the text must stay readable
/// over a bright room).
const PANEL: Color = Color::srgba(0.0, 0.0, 0.0, 0.8);
/// The panel's backing: the Pip-Boy and terminal screens' picture, a grey vignette, in
/// amber at the Pip-Boy page's brightness (`crate::pipboy::device_screen`), nearly opaque
/// so that the text reads over a bright room [design; how it reads not yet seen].
const BACKING: &str = "interface\\shared\\background\\pipboy.dds";
const BACKING_BRIGHTNESS: f32 = 120.0;
const BACKING_ALPHA: f32 = 0.9;

/// The panel's place: this far in from the view's right and 480 wide, narrower than the
/// 640 it was, so that the player has the picture [design]. The longest words fit
/// [observed: measured with the two font files against `fnvtool actor creator`'s names:
/// "Face forehead-sellion-nose ratio" and its value 399 of the 424, "Facial hair 47/47"
/// 165 and "Shaggy Sideburns" 184].
const MARGIN: f32 = 32.0;
const PANEL_Y: f32 = 32.0;
const PANEL_W: f32 = 480.0;
const PANEL_H: f32 = 896.0;
const LEFT: f32 = 32.0;
const RIGHT: f32 = PANEL_W - 24.0;
const ROWS_TOP: f32 = 146.0;
const ROWS_BOTTOM: f32 = 758.0;
/// The pages along the bottom, over the buttons: their row's top line and the letters'
/// top [design: six fit at this scale, the words at most 59 wide in 70].
const TABS_Y: f32 = 776.0;
const TAB_TEXT_Y: f32 = 782.0;
const TAB_SCALE: f32 = 0.6;
const TAB_H: f32 = 44.0;
const ACTIONS_Y: f32 = 836.0;
/// The slider's bar stands inside its two arrows, and the knob's reach inside the bar.
const BAR_INSET: f32 = 6.0;
const ARROW: Vec2 = Vec2::new(10.0, 20.0);
/// The slider track's ends.
pub const TRACK_X0: f32 = LEFT + 16.0;
pub const TRACK_X1: f32 = RIGHT - 16.0;
/// The square on the chosen row, in the margin left of the labels [design: the size for
/// the panel; the menus' list marker, `chargen::lists::MARK`, is larger].
const MARK_X: f32 = 12.0;
const MARK_SIZE: f32 = 14.0;
/// The frame's top and bottom lines and how long the lines fading from their ends are
/// [documented: menus\globals.xml, top_bracket.xml, bottom_bracket.xml, as
/// `chargen::draw`].
const THICK: f32 = 2.0;
const TOP_CORNER: f32 = 50.0;
const BOTTOM_CORNER: f32 = 40.0;

/// The panel's left edge in the view's units, for a view `view_w` wide. On whole units:
/// the text's anchors round to pixels from there.
pub fn panel_left(view_w: f32) -> f32 {
    (view_w - MARGIN - PANEL_W).max(0.0).round()
}

/// What the pointer can use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitWhat {
    Tab(usize),
    /// A row (its index among the tab's rows).
    Row(usize),
    /// A row's left or right arrow.
    Arrow(usize, i32),
    /// A slider's track: the pointer's place along it sets it.
    Track(usize),
    Swatch(usize, usize),
    Action(usize),
    /// The panel itself (the wheel zooms nowhere on it).
    Panel,
    /// The rows' part of the panel: the wheel scrolls the rows (not the buttons).
    RowsArea,
    /// The scrollbar beside the rows: a click or drag scrolls to that place.
    Scrollbar,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub rect: [f32; 4],
    pub what: HitWhat,
}

impl Hit {
    pub fn contains(&self, p: Vec2) -> bool {
        let [x, y, w, h] = self.rect;
        p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h
    }
}

/// The thing under the pointer: the last (topmost) rectangle holding it.
pub fn hit_at(hits: &[Hit], p: Vec2) -> Option<HitWhat> {
    hits.iter().rev().find(|h| h.contains(p)).map(|h| h.what)
}

/// A slider track's fraction under the pointer, `x` from the panel's left edge.
pub fn track_fraction(x: f32) -> f64 {
    f64::from(((x - TRACK_X0) / (TRACK_X1 - TRACK_X0)).clamp(0.0, 1.0))
}

/// The game's menu pieces the panel is drawn with (`Interface\Shared`), each `None`
/// when it cannot be read.
struct Pieces {
    fade_down: Option<Picture>,
    fade_up: Option<Picture>,
    arrow_left: Option<Picture>,
    arrow_right: Option<Picture>,
    square: Option<Picture>,
    backing: Option<Picture>,
}

pub struct Kit {
    pub head: Face,
    pub body: Face,
    pieces: Pieces,
}

impl Kit {
    pub fn load(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
        let (head, body) = (face(HEAD_FONT, plain_font(34.0, 14.0), data, images, bc), face(BODY_FONT, plain_font(31.0, 12.0), data, images, bc));
        let mut pictures = Pictures::load(data);
        let mut piece = |path: &str| pictures.picture(path, data, images, bc);
        let pieces = Pieces {
            fade_down: piece("fade_to_bottom.dds"),
            fade_up: piece("fade_to_top.dds"),
            arrow_left: piece("arrow_left.dds"),
            arrow_right: piece("arrow_right.dds"),
            square: piece("square_filled.dds"),
            backing: piece(BACKING),
        };
        Kit { head, body, pieces }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Right,
    Centre,
}

fn first_line(face: &Face, s: &str) -> Line {
    face.font.lines(s, None).into_iter().next().unwrap_or_default()
}

pub fn width(face: &Face, s: &str, scale: f32) -> f32 {
    first_line(face, s).width * scale
}

/// One line of text, its top at `top`, `scale` times the font's size. Its width.
#[allow(clippy::too_many_arguments)]
pub fn text(p: &mut ChildSpawnerCommands, face: &Face, s: &str, x: f32, top: f32, scale: f32, align: Align, color: Color) -> f32 {
    let line = first_line(face, s);
    let w = line.width * scale;
    let x = match align {
        Align::Left => x,
        Align::Right => x - w,
        Align::Centre => x - w / 2.0,
    };
    // The letters stand on a line three quarters of the font's size down [design: as
    // the menus' body text, chargen::draw::BODY_BASE for 31].
    let base = top + face.font.size * 0.75 * scale;
    let Some((image, size)) = &face.image else {
        p.spawn(Node { position_type: PositionType::Absolute, left: u(x), top: u(top), ..default() }).with_children(|n| {
            n.spawn((Text::new(s.to_string()), TextFont { font_size: FontSize::Vh(face.font.size * 0.75 * scale / 9.6), ..default() }, TextColor(color)));
        });
        return w;
    };
    p.spawn(line_anchor(base)).with_children(|p| {
        for placed in &line.glyphs {
            let g = face.font.glyph(placed.code);
            if !g.drawn() {
                continue;
            }
            let rect = Rect::new(g.uv[0] * size.x, g.uv[1] * size.y, g.uv[2] * size.x, g.uv[3] * size.y);
            p.spawn((
                at(x + (placed.x + g.before) * scale, -g.ascent * scale, g.width * scale, g.height * scale),
                ImageNode { rect: Some(rect), color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) },
                LayoutConfig { use_rounding: false },
            ));
        }
    });
    w
}

pub fn fill(p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, color: Color) {
    p.spawn((at(x, y, w, h), BackgroundColor(color)));
}

fn outline(p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, t: f32, color: Color) {
    fill(p, x, y, w, t, color);
    fill(p, x, y + h - t, w, t, color);
    fill(p, x, y + t, t, h - 2.0 * t, color);
    fill(p, x + w - t, y + t, t, h - 2.0 * t, color);
}

/// A piece of the game's, tinted `color` and stretched, or a plain fill without it.
fn piece(p: &mut ChildSpawnerCommands, picture: &Option<Picture>, [x, y, w, h]: [f32; 4], color: Color, stand_in: Color) {
    match picture {
        Some((image, rect)) => {
            p.spawn((at(x, y, w, h), ImageNode { rect: *rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) }));
        }
        None => fill(p, x, y, w, h, stand_in),
    }
}

/// The panel's frame, as the menus' boxes have it (`chargen::draw`'s brackets): a line
/// along the top and the bottom, and a line fading away from each of their ends.
fn frame(p: &mut ChildSpawnerCommands, pieces: &Pieces) {
    let color = accent(1.0);
    let bottom = PANEL_Y + PANEL_H - THICK;
    fill(p, 0.0, PANEL_Y, PANEL_W, THICK, color);
    fill(p, 0.0, bottom, PANEL_W, THICK, color);
    for x in [0.0, PANEL_W - THICK] {
        piece(p, &pieces.fade_down, [x, PANEL_Y, THICK, TOP_CORNER], color, color.with_alpha(0.5));
        piece(p, &pieces.fade_up, [x, bottom - BOTTOM_CORNER + THICK, THICK, BOTTOM_CORNER], color, color.with_alpha(0.5));
    }
}

/// The scrollbar beside the rows, in the panel's right margin.
const BAR_X: f32 = PANEL_W - 14.0;
const BAR_W: f32 = 6.0;

/// A row's height.
fn height(v: &RowView) -> f32 {
    match v {
        RowView::Choice { .. } => 52.0,
        RowView::Slider { .. } => 82.0,
        RowView::Swatches { .. } => 76.0,
    }
}

/// The open tab's rows' heights.
pub fn heights(m: &Model) -> Vec<f32> {
    m.rows().iter().map(|&r| height(&m.view(r))).collect()
}

/// The last first row worth scrolling to: from it on, the rest fit.
pub fn max_first(heights: &[f32]) -> usize {
    let room = ROWS_BOTTOM - ROWS_TOP;
    let mut first = heights.len();
    let mut used = 0.0;
    while first > 0 && used + heights[first - 1] <= room + 0.5 {
        used += heights[first - 1];
        first -= 1;
    }
    first.min(heights.len().saturating_sub(1))
}

/// The first row shown after the chosen row moved: unchanged when the chosen row is in
/// view, else the least scroll that shows it.
pub fn reveal(heights: &[f32], first: usize, chosen: usize) -> usize {
    let first = if chosen < first { chosen } else { first.max(first_shown(heights, chosen)) };
    first.min(max_first(heights))
}

/// The first row for a place along the scrollbar (0 top, 1 bottom).
pub fn first_at(heights: &[f32], y: f32) -> usize {
    let f = ((y - ROWS_TOP) / (ROWS_BOTTOM - ROWS_TOP)).clamp(0.0, 1.0);
    let last = max_first(heights);
    (f * last as f32).round() as usize
}

/// The first row drawn, so that the chosen one is in view.
pub fn first_shown(heights: &[f32], chosen: usize) -> usize {
    let room = ROWS_BOTTOM - ROWS_TOP;
    let mut first = 0;
    loop {
        let used: f32 = heights[first..=chosen.min(heights.len().saturating_sub(1)).max(first)].iter().sum();
        if used <= room || first >= chosen {
            return first;
        }
        first += 1;
    }
}

/// Draw the creator into `p` (a full-view node). `view_w`: the view's width in units.
pub fn spawn(p: &mut ChildSpawnerCommands, m: &Model, kit: &Kit, view_w: f32) -> Vec<Hit> {
    let left = panel_left(view_w);
    let mut hits = Vec::new();
    p.spawn(at(left, 0.0, PANEL_W, 960.0)).with_children(|panel| hits = spawn_panel(panel, m, kit));
    for h in &mut hits {
        h.rect[0] += left;
    }
    // Over the picture, at its bottom left: how to look round (the mouse, then the
    // controller), in the form the menus' buttons give their keys ("Done A)").
    for (k, line) in ["Turn Drag)    Pan Right-drag or Shift-drag)    Zoom Wheel)    Reset View C)", "Turn Right Stick)    Pan Left Stick)    Zoom Triggers)    Reset View RS)"].iter().enumerate() {
        text(p, &kit.body, line, 40.0, 872.0 + 28.0 * k as f32, 0.6, Align::Left, bone(0.6));
    }
    hits
}

/// The panel, from its left edge; the rectangles it returns are too.
fn spawn_panel(p: &mut ChildSpawnerCommands, m: &Model, kit: &Kit) -> Vec<Hit> {
    let mut hits = vec![Hit { rect: [0.0, PANEL_Y, PANEL_W, PANEL_H], what: HitWhat::Panel }];
    piece(p, &kit.pieces.backing, [0.0, PANEL_Y, PANEL_W, PANEL_H], amber(BACKING_BRIGHTNESS).with_alpha(BACKING_ALPHA), PANEL);
    frame(p, &kit.pieces);
    // The title and what the look is, and what the page is for.
    text(p, &kit.head, m.title(), LEFT, 46.0, 1.0, Align::Left, bone(1.0));
    text(p, &kit.body, &m.caption(), RIGHT, 54.0, 0.8, Align::Right, sand(0.9));
    fill(p, LEFT, 86.0, RIGHT - LEFT, 2.0, bone(0.18));
    text(p, &kit.body, m.tab.blurb(), LEFT, 94.0, 0.66, Align::Left, bone(0.55));
    // The rows.
    let rows = m.rows();
    let views: Vec<RowView> = rows.iter().map(|&r| m.view(r)).collect();
    let heights: Vec<f32> = views.iter().map(height).collect();
    let chosen = m.chosen_row().map(|_| m.row());
    let first = m.scroll().min(max_first(&heights));
    hits.push(Hit { rect: [0.0, ROWS_TOP, PANEL_W, ROWS_BOTTOM - ROWS_TOP], what: HitWhat::RowsArea });
    let mut y = ROWS_TOP;
    for (i, v) in views.iter().enumerate().skip(first) {
        let h = heights[i];
        if y + h > ROWS_BOTTOM + 0.5 {
            break;
        }
        let on = chosen == Some(i);
        if on {
            fill(p, THICK, y, PANEL_W - 2.0 * THICK, h - 6.0, highlight());
            // The small square the game's lists mark a row with, level with the label.
            let mark_y = y + if matches!(v, RowView::Choice { .. }) { 14.0 } else { 10.0 };
            piece(p, &kit.pieces.square, [MARK_X, mark_y, MARK_SIZE, MARK_SIZE], accent(1.0), accent(1.0));
        }
        hits.push(Hit { rect: [0.0, y, PANEL_W, h], what: HitWhat::Row(i) });
        let label_colour = if on { bone(1.0) } else { bone(0.78) };
        match v {
            RowView::Choice { label, value, at: n, of } => {
                text(p, &kit.body, label, LEFT, y + 10.0, 0.85, Align::Left, label_colour);
                let count = if *of > 0 { format!("{n}/{of}") } else { String::new() };
                let lw = width(&kit.body, label, 0.85);
                text(p, &kit.body, &count, LEFT + lw + 10.0, y + 14.0, 0.62, Align::Left, bone(0.4));
                let (ax0, ax1) = (RIGHT - 228.0, RIGHT - 8.0);
                text(p, &kit.body, "<", ax0, y + 10.0, 0.85, Align::Centre, if on { accent(1.0) } else { bone(0.5) });
                text(p, &kit.body, ">", ax1, y + 10.0, 0.85, Align::Centre, if on { accent(1.0) } else { bone(0.5) });
                text(p, &kit.body, value, (ax0 + ax1) / 2.0, y + 10.0, 0.85, Align::Centre, if on { sand(1.0) } else { sand(0.8) });
                hits.push(Hit { rect: [ax0 - 24.0, y, 48.0, h], what: HitWhat::Arrow(i, -1) });
                hits.push(Hit { rect: [ax1 - 24.0, y, 48.0, h], what: HitWhat::Arrow(i, 1) });
            }
            RowView::Slider { label, low, high, value, fraction, race } => {
                text(p, &kit.body, label, LEFT, y + 6.0, 0.85, Align::Left, label_colour);
                text(p, &kit.body, value, RIGHT, y + 6.0, 0.85, Align::Right, if on { sand(1.0) } else { sand(0.8) });
                let ty = y + 42.0;
                let span = TRACK_X1 - TRACK_X0;
                // The bar between its two arrows, the original's way of drawing a slider
                // [observed: the note's §4]; the arrows step it, as a list row's do.
                let arrow = if on { accent(1.0) } else { bone(0.6) };
                let arrow_y = ty + 1.0 - ARROW.y / 2.0;
                fill(p, LEFT + BAR_INSET, ty, RIGHT - LEFT - 2.0 * BAR_INSET, 2.0, bone(0.35));
                piece(p, &kit.pieces.arrow_left, [LEFT, arrow_y, ARROW.x, ARROW.y], arrow, arrow);
                piece(p, &kit.pieces.arrow_right, [RIGHT - ARROW.x, arrow_y, ARROW.x, ARROW.y], arrow, arrow);
                let mid = TRACK_X0 + span / 2.0;
                fill(p, mid - 1.0, ty - 5.0, 2.0, 12.0, bone(0.35));
                if let Some(r) = race {
                    // Where the race alone puts the face.
                    let rx = TRACK_X0 + span * r;
                    fill(p, rx - 1.0, ty - 8.0, 2.0, 6.0, sand(0.8));
                }
                let kx = TRACK_X0 + span * fraction;
                let (a, b) = if kx < mid { (kx, mid) } else { (mid, kx) };
                fill(p, a, ty - 1.0, b - a, 4.0, accent(if on { 0.95 } else { 0.6 }));
                fill(p, kx - 5.0, ty - 10.0, 10.0, 22.0, if on { bone(1.0) } else { bone(0.75) });
                text(p, &kit.body, low, LEFT, ty + 12.0, 0.55, Align::Left, bone(0.45));
                text(p, &kit.body, high, RIGHT, ty + 12.0, 0.55, Align::Right, bone(0.45));
                hits.push(Hit { rect: [TRACK_X0 - 12.0, ty - 16.0, span + 24.0, 34.0], what: HitWhat::Track(i) });
                hits.push(Hit { rect: [LEFT - 8.0, ty - 16.0, ARROW.x + 9.0, 34.0], what: HitWhat::Arrow(i, -1) });
                hits.push(Hit { rect: [RIGHT - ARROW.x - 1.0, ty - 16.0, ARROW.x + 9.0, 34.0], what: HitWhat::Arrow(i, 1) });
            }
            RowView::Swatches { label, colours, at: sel } => {
                text(p, &kit.body, label, LEFT, y + 6.0, 0.85, Align::Left, label_colour);
                let size = ((RIGHT - LEFT) / colours.len().max(1) as f32 - 6.0).min(30.0);
                for (k, c) in colours.iter().enumerate() {
                    let x = LEFT + k as f32 * (size + 6.0);
                    let sy = y + 38.0;
                    if *sel == Some(k) {
                        outline(p, x - 4.0, sy - 4.0, size + 8.0, size + 8.0, 2.0, bone(1.0));
                    }
                    fill(p, x, sy, size, size, Color::srgb_u8(c[0], c[1], c[2]));
                    hits.push(Hit { rect: [x, sy, size, size], what: HitWhat::Swatch(i, k) });
                }
            }
        }
        y += h;
    }
    if first > 0 {
        text(p, &kit.body, "more above", RIGHT, ROWS_TOP - 22.0, 0.55, Align::Right, bone(0.4));
    }
    if y < ROWS_BOTTOM && views.len() > first && heights[first..].iter().sum::<f32>() > ROWS_BOTTOM - ROWS_TOP {
        text(p, &kit.body, "more below", RIGHT, ROWS_BOTTOM - 4.0, 0.55, Align::Right, bone(0.4));
    }
    // The scrollbar, when the rows do not all fit: the thumb is the part shown.
    let total: f32 = heights.iter().sum();
    let room = ROWS_BOTTOM - ROWS_TOP;
    if total > room + 0.5 {
        let above: f32 = heights[..first].iter().sum();
        let shown: f32 = heights[first..].iter().scan(0.0, |used, h| {
            *used += h;
            (*used <= room + 0.5).then_some(*h)
        }).sum();
        fill(p, BAR_X, ROWS_TOP, BAR_W, room, bone(0.15));
        let top = ROWS_TOP + room * above / total;
        let len = (room * shown / total).max(24.0);
        fill(p, BAR_X, top.min(ROWS_BOTTOM - len), BAR_W, len, accent(0.8));
        hits.push(Hit { rect: [BAR_X - 8.0, ROWS_TOP, BAR_W + 14.0, room], what: HitWhat::Scrollbar });
    }
    // The pages, along the bottom over the buttons (the original's page lamps stand along
    // its bottom [observed: the note's §4]; six tabs, the maintainer's structure).
    fill(p, LEFT, TABS_Y, RIGHT - LEFT, 2.0, bone(0.18));
    let tabs = m.tabs();
    let tab_w = (RIGHT - LEFT) / tabs.len() as f32;
    for (i, t) in tabs.iter().enumerate() {
        let x = LEFT + tab_w * i as f32;
        let on = *t == m.tab;
        text(p, &kit.head, t.title(), x + tab_w / 2.0, TAB_TEXT_Y, TAB_SCALE, Align::Centre, if on { bone(1.0) } else { bone(0.45) });
        if on {
            fill(p, x + 4.0, TABS_Y + 32.0, tab_w - 8.0, 3.0, accent(1.0));
        }
        hits.push(Hit { rect: [x, TABS_Y, tab_w, TAB_H], what: HitWhat::Tab(i) });
    }
    // The buttons.
    fill(p, LEFT, ACTIONS_Y - 14.0, RIGHT - LEFT, 2.0, bone(0.18));
    let bw = (RIGHT - LEFT - 3.0 * 10.0) / ACTIONS.len() as f32;
    for (i, a) in ACTIONS.iter().enumerate() {
        let x = LEFT + i as f32 * (bw + 10.0);
        let on = m.on_actions == Some(i);
        let done = i == ACTIONS.len() - 1;
        if on {
            fill(p, x, ACTIONS_Y, bw, 48.0, highlight());
        }
        outline(p, x, ACTIONS_Y, bw, 48.0, 2.0, if done || on { accent(1.0) } else { bone(0.35) });
        text(p, &kit.head, a.title(), x + bw / 2.0, ACTIONS_Y + 8.0, 0.7, Align::Centre, bone(if on || done { 1.0 } else { 0.8 }));
        text(p, &kit.body, a.hint(), x + bw / 2.0, ACTIONS_Y + 54.0, 0.5, Align::Centre, bone(0.4));
        hits.push(Hit { rect: [x, ACTIONS_Y, bw, 48.0], what: HitWhat::Action(i) });
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_topmost_rectangle_takes_the_pointer() {
        let hits = [Hit { rect: [0.0, 0.0, 100.0, 100.0], what: HitWhat::Panel }, Hit { rect: [10.0, 10.0, 20.0, 20.0], what: HitWhat::Row(2) }];
        assert_eq!(hit_at(&hits, Vec2::new(15.0, 15.0)), Some(HitWhat::Row(2)));
        assert_eq!(hit_at(&hits, Vec2::new(50.0, 50.0)), Some(HitWhat::Panel));
        assert_eq!(hit_at(&hits, Vec2::new(150.0, 50.0)), None);
    }

    #[test]
    fn the_track_reads_the_pointer() {
        assert_eq!(track_fraction(TRACK_X0 - 5.0), 0.0);
        assert_eq!(track_fraction(TRACK_X1 + 5.0), 1.0);
        assert!((track_fraction((TRACK_X0 + TRACK_X1) / 2.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_panel_stands_at_the_views_right() {
        // The same margin on any view; the rows and buttons stay inside the frame.
        assert_eq!(panel_left(1280.0), 768.0);
        assert_eq!(panel_left(1707.0) + PANEL_W + MARGIN, 1707.0);
        assert_eq!(panel_left(100.0), 0.0);
        const { assert!(RIGHT < BAR_X && BAR_X + BAR_W < PANEL_W) };
        const { assert!(ROWS_BOTTOM < TABS_Y && TABS_Y + TAB_H < ACTIONS_Y) };
    }

    #[test]
    fn the_list_scrolls_no_further_than_its_last_rows() {
        // 612 units of room hold 7 rows of 82: from row 8 on, the last 7 fit.
        let h = vec![82.0; 15];
        assert_eq!(max_first(&h), 8);
        assert_eq!(max_first(&[52.0; 3]), 0);
        // A row in view leaves the scroll alone; one out of view brings it just in.
        assert_eq!(reveal(&h, 3, 5), 3);
        assert_eq!(reveal(&h, 3, 1), 1);
        assert_eq!(reveal(&h, 0, 10), 4);
        assert_eq!(reveal(&h, 12, 14), 8);
        assert_eq!(first_at(&h, ROWS_TOP), 0);
        assert_eq!(first_at(&h, ROWS_BOTTOM), 8);
    }

    #[test]
    fn the_chosen_row_is_kept_in_view() {
        let h = vec![82.0; 15];
        assert_eq!(first_shown(&h, 0), 0);
        // 612 units of room hold 7 rows of 82.
        assert_eq!(first_shown(&h, 7), 1);
        assert_eq!(first_shown(&h, 14), 8);
    }
}
