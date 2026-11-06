//! Drawing the character creator (docs/spec-notes/character-creator.md §4): a panel on
//! the left over the world, the player standing in the clear part of the picture. Its
//! own look [owner 2026-10-16: "something new in design without having to add new
//! assets"]: the game menus' own fonts and plain drawn shapes, in the
//! interface's amber [owner 2026-10-17: "can we also change the colour scheme to match
//! the UI amber look im not a fan of the light blue trim"; it was bone white with a
//! turquoise accent before]. The fonts are the menus' [owner 2026-10-17: "if we can also
//! get the font to match our game"; it was Futura for titles, tabs and buttons before]:
//! the titles' glowing Monofonto (font 6, as the tag-skill and trait menus' headlines,
//! chargen.md §3) for the title, the tabs and the buttons, Monofonto (font 2) for the
//! rest.
//!
//! Lengths are the interface's units (960 to the view's height) from the view's top
//! left. [`spawn`] returns where each thing the pointer can use lies ([`Hit`]), so the
//! pointer is tested against those rectangles rather than UI interaction state (the
//! panel is rebuilt while a slider is dragged).

use super::model::{Model, RowView, ACTIONS};
use crate::interface::{amber, at, face, line_anchor, plain_font, u, Face};
use bevy::prelude::*;
use fnv_font::Line;
use nvf_assets::GameData;

/// Font 6, the menus' titles, and font 2, their body text [documented:
/// Fallout_default.ini, docs/spec-notes/fonts.md; the same files as `chargen::draw`].
const HEAD_FONT: &str = "textures\\fonts\\glow_monofonto_vl_dialogs.fnt";
const BODY_FONT: &str = "textures\\fonts\\monofonto_large.fnt";

/// The colours: all the interface's amber (`crate::interface::AMBER`) [owner
/// 2026-10-17], at the brightnesses the other menus use for them [design]: text at 215
/// (and fainter by alpha where it is a hint or not chosen), the trim, the chosen row's
/// bar and band, the slider's fill and knob, Done's outline at full brightness, values at
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

/// The dark backing behind the panel (the text must stay readable over a bright room).
const PANEL: Color = Color::srgba(0.0, 0.0, 0.0, 0.8);

/// The panel's place.
pub const PANEL_X: f32 = 32.0;
pub const PANEL_Y: f32 = 32.0;
pub const PANEL_W: f32 = 640.0;
pub const PANEL_H: f32 = 896.0;
const LEFT: f32 = 64.0;
const RIGHT: f32 = PANEL_X + PANEL_W - 24.0;
const ROWS_TOP: f32 = 226.0;
const ROWS_BOTTOM: f32 = 812.0;
const ACTIONS_Y: f32 = 836.0;
/// The slider track's ends.
pub const TRACK_X0: f32 = LEFT;
pub const TRACK_X1: f32 = RIGHT;

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

/// A slider track's fraction under the pointer.
pub fn track_fraction(x: f32) -> f64 {
    f64::from(((x - TRACK_X0) / (TRACK_X1 - TRACK_X0)).clamp(0.0, 1.0))
}

pub struct Kit {
    pub head: Face,
    pub body: Face,
}

impl Kit {
    pub fn load(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
        Kit { head: face(HEAD_FONT, plain_font(34.0, 14.0), data, images, bc), body: face(BODY_FONT, plain_font(31.0, 12.0), data, images, bc) }
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
            n.spawn((Text::new(s.to_string()), TextFont { font_size: crate::interface::font_units(face.font.size * 0.75 * scale), ..default() }, TextColor(color)));
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

/// The scrollbar beside the rows, in the panel's right margin.
const BAR_X: f32 = PANEL_X + PANEL_W - 14.0;
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
    let mut hits = vec![Hit { rect: [PANEL_X, PANEL_Y, PANEL_W, PANEL_H], what: HitWhat::Panel }];
    fill(p, PANEL_X, PANEL_Y, PANEL_W, PANEL_H, PANEL);
    fill(p, PANEL_X, PANEL_Y, 4.0, PANEL_H, accent(0.9));
    // The title and what the look is.
    text(p, &kit.head, m.title(), LEFT, 52.0, 1.0, Align::Left, bone(1.0));
    text(p, &kit.body, &m.caption(), RIGHT, 60.0, 0.8, Align::Right, sand(0.9));
    // The tabs.
    let tabs = m.tabs();
    let tab_w = (RIGHT - LEFT) / tabs.len() as f32;
    for (i, t) in tabs.iter().enumerate() {
        let x = LEFT + tab_w * i as f32;
        let on = *t == m.tab;
        text(p, &kit.head, t.title(), x + tab_w / 2.0, 120.0, 0.75, Align::Centre, if on { bone(1.0) } else { bone(0.45) });
        if on {
            fill(p, x + 10.0, 154.0, tab_w - 20.0, 4.0, accent(1.0));
        }
        hits.push(Hit { rect: [x, 112.0, tab_w, 50.0], what: HitWhat::Tab(i) });
    }
    fill(p, LEFT, 162.0, RIGHT - LEFT, 2.0, bone(0.18));
    text(p, &kit.body, m.tab.blurb(), LEFT, 176.0, 0.72, Align::Left, bone(0.55));
    // The rows.
    let rows = m.rows();
    let views: Vec<RowView> = rows.iter().map(|&r| m.view(r)).collect();
    let heights: Vec<f32> = views.iter().map(height).collect();
    let chosen = m.chosen_row().map(|_| m.row());
    let first = m.scroll().min(max_first(&heights));
    hits.push(Hit { rect: [PANEL_X, ROWS_TOP, PANEL_W, ROWS_BOTTOM - ROWS_TOP], what: HitWhat::RowsArea });
    let mut y = ROWS_TOP;
    for (i, v) in views.iter().enumerate().skip(first) {
        let h = heights[i];
        if y + h > ROWS_BOTTOM + 0.5 {
            break;
        }
        let on = chosen == Some(i);
        if on {
            fill(p, PANEL_X + 4.0, y, PANEL_W - 4.0, h - 6.0, highlight());
            fill(p, PANEL_X + 4.0, y, 4.0, h - 6.0, accent(1.0));
        }
        hits.push(Hit { rect: [PANEL_X, y, PANEL_W, h], what: HitWhat::Row(i) });
        let label_colour = if on { bone(1.0) } else { bone(0.78) };
        match v {
            RowView::Choice { label, value, at: n, of } => {
                text(p, &kit.body, label, LEFT, y + 10.0, 0.85, Align::Left, label_colour);
                let count = if *of > 0 { format!("{n}/{of}") } else { String::new() };
                let lw = width(&kit.body, label, 0.85);
                text(p, &kit.body, &count, LEFT + lw + 14.0, y + 14.0, 0.62, Align::Left, bone(0.4));
                let (ax0, ax1) = (RIGHT - 270.0, RIGHT - 20.0);
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
                fill(p, TRACK_X0, ty, span, 2.0, bone(0.25));
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
                text(p, &kit.body, low, TRACK_X0, ty + 12.0, 0.55, Align::Left, bone(0.45));
                text(p, &kit.body, high, TRACK_X1, ty + 12.0, 0.55, Align::Right, bone(0.45));
                hits.push(Hit { rect: [TRACK_X0 - 12.0, ty - 16.0, span + 24.0, 34.0], what: HitWhat::Track(i) });
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
    // How to look round, at the bottom of the picture.
    // Over the picture: how to look round (the mouse, then the controller).
    for (k, line) in ["Drag: turn    Right-drag or Shift-drag: pan    Wheel over the picture: zoom    C: reset view", "Right stick: turn    Left stick: pan    Triggers: zoom    Click right stick: reset view"].iter().enumerate() {
        text(p, &kit.body, line, view_w - 40.0, 872.0 + 28.0 * k as f32, 0.6, Align::Right, bone(0.6));
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
    fn the_list_scrolls_no_further_than_its_last_rows() {
        // 586 units of room hold 7 rows of 82: from row 8 on, the last 7 fit.
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
        // 586 units of room hold 7 rows of 82.
        assert_eq!(first_shown(&h, 7), 1);
        assert_eq!(first_shown(&h, 14), 8);
    }
}
