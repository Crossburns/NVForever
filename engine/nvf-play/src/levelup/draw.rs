//! Drawing the level-up menu from a [`View`]: the numbers of `menus\levelup_menu.xml`
//! [documented, docs/spec-notes/levelup.md §2], in the interface's units from the top
//! left of the menu's 1024 × 800 rectangle, centred on the screen; the pieces are the
//! character-creation menus' (`crate::chargen::draw`).

use super::model::{Page, Part, Row, View, PERK_ROWS, SKILL_ROWS};
use crate::chargen::draw::{Align, Ctx, Which, BODY_H, THICK};
use crate::chargen::lists::{marker, MAIN, ROW_H};
use crate::interface::{amber, at};
use bevy::prelude::*;
use crate::ui_pointer::Button;

/// The files' dark picture behind (1144 × 1070, 50 left of and 125 above the rectangle)
/// is replaced by the whole screen darkened ([`crate::interface::top_menu_backing`]; the
/// original also blurs the world behind, not built).
/// The headline: a line 50 long, the title 20 after it and 10 above the line; the lines
/// fading down from its ends 60 long (vertical_fade_line.xml).
const HEAD_LINE: f32 = 50.0;
const TITLE_X: f32 = 70.0;
const TITLE_Y: f32 = -10.0;
const HEAD_CORNER: f32 = 60.0;
/// The skill list: x -15, y 35, 470 wide; the perk list: y 60.
const LIST_X: f32 = -15.0;
const LIST_W: f32 = 470.0;
const SKILL_LIST_Y: f32 = 35.0;
const PERK_LIST_Y: f32 = 60.0;
/// The rows start after the list's scroll bar, 32 wide, and their highlight box ends 5
/// short of the list's right side (list_box.xml) [observed: the box from 403 to 890
/// pixels on the owner's 1920 × 1080 screenshots].
const BAR_W: f32 = 32.0;
const BAR_ARROW: f32 = 16.0;
const BOX_SHORT: f32 = 5.0;
/// A skill row: its name at 15, 20, its value centred at 340, the "−" at 245, −5 and the
/// "+" at 370, −5, each 64 square.
const SKILL_TEXT_X: f32 = 15.0;
const TEXT_Y: f32 = 20.0;
const VALUE_X: f32 = 340.0;
const MINUS_X: f32 = 245.0;
const PLUS_X: f32 = 370.0;
const ARROW_Y: f32 = -5.0;
const ARROW: f32 = 64.0;
/// A perk row: the square at 15, 17.5 (25 square) when taken, its name at 45, 20
/// wrapping at 360.
const MARK: Vec2 = Vec2::new(15.0, 17.5);
const MARK_SIZE: f32 = 25.0;
const PERK_TEXT_X: f32 = 45.0;
const PERK_WRAP: f32 = 360.0;
/// The chosen row's picture: 400 square at 520, 0.
const ICON: [f32; 3] = [520.0, 0.0, 400.0];
/// The info rectangle: 10 right of the list, 385 down, to the rectangle's right side;
/// its second line at 335 (less its thickness); the text 20 down wrapping 40 short of
/// the width; the counter 20 under the second line in the title font.
const INFO_Y: f32 = 385.0;
const INFO_B: f32 = 335.0;
/// Reset, Continue / Done and Back: right-aligned 10 inside the right side, from 60
/// down, each under the one before; a button is its text and 10 each side.
const BUTTONS: Vec2 = Vec2::new(1014.0, 60.0);
const BUTTON_PAD: f32 = 10.0;
/// A button the player cannot use: brightness 127 of 255 (LUM_ContinueButton
/// `_line_alpha`).
const DIM: f32 = 127.0 / 255.0;

/// A perk that cannot be taken now: its name at about half brightness [observed: 119
/// against 206 at the letters' brightest on the owner's 3840 × 2160 screenshots, over a
/// background near 30].
const DIM_ROW: f32 = 128.0;

/// The arrows' pictures, and lit under the pointer [documented: LUM_Template_LeftArrow
/// and RightArrow, `_Filename_1` and `_Filename_2` by `mouseover`].
const MINUS_PIC: [&str; 2] = ["interface\\shared\\arrow\\adjust_decrease.dds", "interface\\shared\\arrow\\adjust_decrease_over.dds"];
const PLUS_PIC: [&str; 2] = ["interface\\shared\\arrow\\adjust_increase.dds", "interface\\shared\\arrow\\adjust_increase_over.dds"];

/// Spawn the menu under `root` (a full-screen node).
pub fn spawn(root: &mut ChildSpawnerCommands, v: &View, c: &mut Ctx) {
    let bright = amber(255.0);
    root.spawn(Node { width: crate::interface::u(MAIN.x), height: crate::interface::u(MAIN.y), ..default() }).with_children(|p| {
        c.line(p, 0.0, 0.0, HEAD_LINE, THICK, bright);
        c.fade(p, 0.0, 0.0, HEAD_CORNER, false, bright);
        let w = c.text(p, Which::Title, &v.title, TITLE_X, TITLE_Y, Align::Left, bright);
        let h2 = TITLE_X + w + 20.0;
        c.line(p, h2, 0.0, MAIN.x - h2, THICK, bright);
        c.fade(p, MAIN.x - THICK, 0.0, HEAD_CORNER, false, bright);
        list(p, c, v);
        if let Some(icon) = v.icon.as_deref() {
            let [x, y, size] = ICON;
            c.image(p, icon, at(x, y, size, size), bright, ());
        }
        info(p, c, v);
        buttons(p, c, v);
        c.bottom_bracket(p, 0.0, MAIN.y, MAIN.x, bright);
    });
}

fn list(p: &mut ChildSpawnerCommands, c: &mut Ctx, v: &View) {
    let bright = amber(255.0);
    let (y, shown) = match v.page {
        Page::Skills => (SKILL_LIST_Y, SKILL_ROWS),
        Page::Perks => (PERK_LIST_Y, PERK_ROWS),
    };
    let h = shown as f32 * ROW_H;
    p.spawn(at(LIST_X, y, LIST_W, h)).with_children(|l| {
        if let Some((my, mh)) = marker(v.rows.len(), shown, v.scroll, h) {
            c.image(l, "arrow_up.dds", at(0.0, 0.0, BAR_W, BAR_ARROW), bright, (Button, Part::ScrollUp));
            c.image(l, "arrow_down.dds", at(0.0, h - BAR_ARROW, BAR_W, BAR_ARROW), bright, (Button, Part::ScrollDown));
            if c.image(l, "vert_marker.dds", at(12.0, my, 6.0, mh), bright, ()).is_none() {
                l.spawn((at(12.0, my, 6.0, mh), BackgroundColor(bright)));
            }
        }
        let row_w = LIST_W - BAR_W - BOX_SHORT;
        for (k, i) in (v.scroll..v.rows.len()).take(shown).enumerate() {
            let r = &v.rows[i];
            l.spawn((Button, Row(i), at(BAR_W, k as f32 * ROW_H, row_w, ROW_H))).with_children(|row| {
                if i == v.selected {
                    c.highlight(row, 0.0, 0.0, row_w, ROW_H - 1.0, 1.0);
                }
                match v.page {
                    Page::Skills => {
                        c.text(row, Which::Body, &r.text, SKILL_TEXT_X, TEXT_Y, Align::Left, bright);
                        if let Some(val) = &r.value {
                            c.text(row, Which::Body, val, VALUE_X, TEXT_Y, Align::Centre, bright);
                        }
                        // The chosen row's "+" is drawn lit [observed: the owner's
                        // screenshots, the chosen row's "+" filled, the others hollow]; an
                        // arrow under the pointer too.
                        if r.minus {
                            let lit = v.hover == Some(Part::Minus(i));
                            arrow(row, c, MINUS_PIC[usize::from(lit)], MINUS_X, "-", Part::Minus(i));
                        }
                        if r.plus {
                            let lit = v.hover == Some(Part::Plus(i)) || (i == v.selected && v.hover != Some(Part::Minus(i)));
                            arrow(row, c, PLUS_PIC[usize::from(lit)], PLUS_X, "+", Part::Plus(i));
                        }
                    }
                    Page::Perks => {
                        let color = if r.dim { amber(DIM_ROW) } else { bright };
                        if r.marked {
                            c.image(row, "square_filled.dds", at(MARK.x, MARK.y, MARK_SIZE, MARK_SIZE), color, ());
                        }
                        c.wrapped(row, &r.text, PERK_TEXT_X, TEXT_Y, PERK_WRAP, color);
                    }
                }
            });
        }
    });
}

/// A row's "+" or "−": its picture (a sign in the body font without the game's
/// pictures), above the rows around it so the parts that reach over them stay
/// clickable.
fn arrow(row: &mut ChildSpawnerCommands, c: &mut Ctx, picture: &str, x: f32, sign: &str, part: Part) {
    let node = at(x, ARROW_Y, ARROW, ARROW);
    if c.image(row, picture, node.clone(), amber(255.0), (Button, part, GlobalZIndex(crate::interface::TOP_MENU_Z + 1))).is_none() {
        row.spawn((node, Button, part, GlobalZIndex(crate::interface::TOP_MENU_Z + 1))).with_children(|a| {
            c.text(a, Which::Body, sign, ARROW / 2.0, ARROW / 2.0 - BODY_H / 2.0, Align::Centre, amber(255.0));
        });
    }
}

fn info(p: &mut ChildSpawnerCommands, c: &mut Ctx, v: &View) {
    let bright = amber(255.0);
    let x = LIST_X + LIST_W + 10.0;
    let w = MAIN.x - x;
    let len = w - 3.0;
    let b = INFO_Y + INFO_B - THICK;
    for y in [INFO_Y, b] {
        c.line(p, x, y, len, THICK, bright);
        c.fade(p, x + w - THICK, y, HEAD_CORNER, false, bright);
    }
    c.wrapped(p, &v.info, x, INFO_Y + 20.0, w - 40.0, bright);
    if !v.counter.is_empty() {
        c.text(p, Which::Title, &v.counter, x, b + 20.0, Align::Left, bright);
    }
}

fn buttons(p: &mut ChildSpawnerCommands, c: &mut Ctx, v: &View) {
    let bh = BODY_H + 2.0 * BUTTON_PAD;
    // [observed: "Reset R)", "Done A)"; hypothesis: "Continue A)", "Back E)", the words
    // the menu file names (sContinue, sBack, sPCMenuHintE) not being in the masters]
    let h = |k: KeyCode| crate::pad::hint_as(&[k], crate::pad::at_menu(fnv_game::game::menus::LEVEL_UP), v.labels);
    let mut list = vec![(Part::Reset, "Reset", h(KeyCode::KeyR), true), (Part::Continue, v.next, h(KeyCode::KeyA), v.next_on)];
    if v.back {
        list.push((Part::Back, "Back", h(KeyCode::KeyE), true));
    }
    for (k, (part, label, hint, on)) in list.into_iter().enumerate() {
        let color = amber(255.0).with_alpha(if on { 1.0 } else { DIM });
        let y = BUTTONS.y + k as f32 * bh;
        let bw = c.width(Which::Body, label) + 2.0 * BUTTON_PAD;
        if v.hover == Some(part) && on {
            c.outline(p, BUTTONS.x - bw, y, bw, bh, amber(200.0));
        }
        c.text(p, Which::Body, label, BUTTONS.x - BUTTON_PAD, y + BUTTON_PAD, Align::Right, color);
        c.text(p, Which::Body, &hint, BUTTONS.x, y + BUTTON_PAD, Align::Left, color);
        p.spawn((Button, part, at(BUTTONS.x - bw, y, bw + 40.0, bh)));
    }
}
