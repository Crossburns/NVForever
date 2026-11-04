//! [design] Pure tile layout; drawing interpretations are listed in
//! docs/spec-notes/menu-xml.md §3.1.1. No textures, fonts or Bevy state are loaded here.

use fnv_ui::{Tile, TileId, Ui, Value};

/// The private trait of a tile that takes no `alpha` from the tiles above it. `tutorial_menu.xml`
/// nests all its content under its background picture [documented: the file]; under that
/// picture at alpha 193.8 the text and lines read full strength (255, 182, 66) at 1280 x 720
/// [observed: shot 091 of hud_menus]. [observed] In the original's container menu the
/// `StrengthReqInfo` card reads `alpha` 0 and its top line and side line still show at full
/// strength (launchtest shot, 1280 x 720; barter-menu.md §8), the scrollbar image reads `alpha` 0
/// with its arrows shown, and the text buttons, an image and a rect at `alpha` 0, show their
/// words. Whether pictures pass alpha on elsewhere, or this holds outside these two menus, is
/// not known (observer check 83; barter-menu.md §8) [hypothesis]. It drops every alpha from
/// above, an ancestor's fade too. [design] An engine-written mark, in no game file, written by
/// a menu's model so nothing else changes: an ordinary trait to the tree, so generic trait dumps
/// list it and a file naming it would be read too.
pub const OWN_ALPHA: &str = "_NVFOwnAlpha";

/// The private trait of a hotrect that draws its own box filled at its `alpha` and
/// `brightness` (the list highlight's faint fill, [documented: `list_box.xml`'s `_fill_alpha`
/// 40 through `box.xml`]; hotrects draw nothing otherwise). [design] Written by a menu's
/// model, so the other T15 menus' hotrects stay as they are.
pub const HOTRECT_FILL: &str = "_NVFHotrectFill";

/// [design] Rectangle in the tree's screen units, without viewport scaling.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn intersect(self, other: Self) -> Self {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        Self {
            left,
            top,
            width: ((self.left + self.width).min(other.left + other.width) - left).max(0.0),
            height: ((self.top + self.height).min(other.top + other.height) - top).max(0.0),
        }
    }
}

/// [design] Unmapped local colour inputs; named system colours retain their names
/// because their numbers are unknown (docs/spec-notes/menu-xml.md §2.3).
#[derive(Clone, Debug, PartialEq)]
pub struct Colour {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub brightness: f64,
    pub systemcolor: Option<f64>,
    pub systemcolor_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub filename: String,
    pub zoom: f64,
    pub tile: Option<f64>,
    pub cropx: Option<f64>,
    pub cropy: Option<f64>,
    /// [design] Retained for the renderer's atlas lookup; no atlas IO during layout.
    pub texatlas: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub string: String,
    /// The `zoom` trait as read, a fraction (1 without one): not applied to the text's
    /// box or glyphs [hypothesis: menu-xml.md §3.1.3, the Hardcore words' zoom 75 left
    /// them as tall and their tiles as wide as at 100 on the original, T12e].
    pub zoom: f64,
    pub font: f64,
    /// [design] Numeric view retained unchanged; named constants also set alignment.
    pub justify: f64,
    pub alignment: Alignment,
    pub wrapwidth: Option<f64>,
}

/// A `nif` tile's model and the sequence it plays [documented: hud_main_menu.xml's
/// `BreathMeter`, `filename` and `animation`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Nif {
    pub filename: String,
    /// The sequence's name; `None` without one or with an empty one.
    pub animation: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Rect,
    Image(Image),
    Text(Text),
    Hotrect,
    Nif(Nif),
    Radial,
    Window,
    Menu,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawItem {
    pub tile: TileId,
    pub kind: Kind,
    pub rect: Rect,
    pub depth: f64,
    /// [hypothesis] Parent fraction products, kept unrounded in the range 0..=255.
    pub alpha: f64,
    pub visible: bool,
    pub clip: Option<Rect>,
    pub colour: Colour,
}

/// [design] Font number, string and optional wrap width in; measured screen units out.
pub type Measure<'a> = dyn Fn(f64, &str, Option<f64>) -> (f64, f64) + 'a;

#[derive(Clone, Copy)]
struct Context {
    rect: Rect,
    depth: f64,
    alpha: f64,
    visible: bool,
    clip: Option<Rect>,
    /// The crop without the parent's own window, for a tile marked outside it.
    clip_above: Option<Rect>,
    t15: bool,
}

impl Default for Context {
    fn default() -> Self {
        Self { rect: Rect::default(), depth: 0.0, alpha: 255.0, visible: true, clip: None, clip_above: None, t15: false }
    }
}

fn number(tile: &Tile, name: &str, default: f64) -> f64 {
    tile.get(name).map_or(default, Value::num)
}

fn optional(tile: &Tile, name: &str) -> Option<f64> {
    tile.get(name).map(Value::num)
}

fn symbol(tile: &Tile, name: &str) -> Option<String> {
    match tile.get(name) {
        Some(Value::Sym(name)) => Some(name.clone()),
        _ => None,
    }
}

fn text_zoom(tile: &Tile) -> f64 {
    let zoom = number(tile, "zoom", 100.0);
    if zoom > 0.0 && zoom.is_finite() { zoom / 100.0 } else { 1.0 }
}

fn kind(tile: &Tile) -> Kind {
    match tile.kind.as_str() {
        "image" => Kind::Image(Image {
            filename: tile.get("filename").map(Value::text).unwrap_or_default(),
            zoom: number(tile, "zoom", 100.0),
            tile: optional(tile, "tile"),
            cropx: optional(tile, "cropx"),
            cropy: optional(tile, "cropy"),
            texatlas: tile.get("texatlas").cloned(),
        }),
        "text" => Kind::Text(Text {
            string: tile.get("string").map(Value::text).unwrap_or_default(),
            zoom: text_zoom(tile),
            font: number(tile, "font", 0.0),
            justify: number(tile, "justify", 0.0),
            alignment: match symbol(tile, "justify").as_deref() {
                Some("center") => Alignment::Center,
                Some("right") => Alignment::Right,
                _ => Alignment::Left,
            },
            wrapwidth: optional(tile, "wrapwidth"),
        }),
        "hotrect" => Kind::Hotrect,
        "nif" => Kind::Nif(Nif {
            filename: tile.get("filename").map(Value::text).unwrap_or_default(),
            animation: tile.get("animation").map(|v| v.text().trim().to_string()).filter(|a| !a.is_empty()),
        }),
        "radial" => Kind::Radial,
        "window" => Kind::Window,
        "menu" => Kind::Menu,
        _ => Kind::Rect,
    }
}

fn place(tile: &Tile, kind: &Kind, parent: Context, measure: &Measure<'_>) -> Context {
    let mut rect = Rect {
        left: parent.rect.left + number(tile, "x", 0.0),
        top: parent.rect.top + number(tile, "y", 0.0),
        width: number(tile, "width", 0.0),
        height: number(tile, "height", 0.0),
    };
    if let Kind::Text(text) = kind {
        if tile.get("width").is_none() || tile.get("height").is_none() {
            let (w, h) = measure(text.font, &text.string, text.wrapwidth);
            rect.width = number(tile, "width", w);
            rect.height = number(tile, "height", h);
        }
        rect.left -= match text.alignment {
            Alignment::Left => 0.0,
            Alignment::Center => rect.width / 2.0,
            Alignment::Right => rect.width,
        };
    }
    Context {
        rect,
        depth: parent.depth + number(tile, "depth", 0.0),
        alpha: match kind {
            // A hotrect's own `alpha` is not passed to its children [hypothesis: every
            // vanilla text button is a hotrect of alpha 0 (text_box.xml) whose text sets
            // its own alpha, 128 or 255 by `target`, and the original shows those texts
            // (the message box's OK, the prompt's Ok and Cancel, the HUD prompt's verb on
            // the owner's screenshots); the list rows (list_box_template.xml) and the
            // list itself are alpha-0 hotrects over shown texts; the highlight box, alpha
            // 40, draws its lines at full strength (message-menu.md §2); T15a,
            // menu-xml.md §3.1.1].
            // [design] Gate this hypothesis with `tiles::interface_screen`: default
            // play, including mod HUD tiles, retains its pre-T15 alpha products.
            // Applying it unconditionally is a candidate for a separate PR.
            _ if tile.get(OWN_ALPHA).is_some_and(Value::truthy) => number(tile, "alpha", 255.0).clamp(0.0, 255.0),
            Kind::Hotrect if parent.t15 => parent.alpha,
            _ => parent.alpha * number(tile, "alpha", 255.0).clamp(0.0, 255.0) / 255.0,
        },
        visible: parent.visible && tile.get("visible").is_none_or(Value::truthy),
        clip: if tile.get(fnv_ui::input::OUTSIDE_CLIP).is_some_and(Value::truthy) { parent.clip_above } else { parent.clip },
        clip_above: parent.clip_above,
        t15: parent.t15,
    }
}

fn children_context(tile: &Tile, mut context: Context) -> Context {
    context.clip_above = context.clip;
    // `clips`, or a `clipwindow` its menu turned on: the same rule as under the pointer.
    if fnv_ui::input::clips_children(tile) {
        context.clip = Some(context.clip.map_or(context.rect, |clip| clip.intersect(context.rect)));
    }
    context
}

/// [design] Lay out a live subtree in parent-first order, siblings stably sorted by
/// local depth. Ancestors outside root still contribute place, alpha, depth and clips.
/// Hidden items retain visible=false; menu/hotrect containers emit no item.
/// `t15` is `tiles::interface_screen`'s switch gate for the hotrect-alpha hypothesis.
pub fn layout(ui: &Ui, root: TileId, measure: &Measure<'_>, t15: bool) -> Vec<DrawItem> {
    let mut ancestors = Vec::new();
    let mut at = ui.tile(root).parent;
    while let Some(id) = at {
        ancestors.push(id);
        at = ui.tile(id).parent;
    }
    let mut context = Context { t15, ..Default::default() };
    for id in ancestors.into_iter().rev() {
        let tile = ui.tile(id);
        if !tile.live {
            return Vec::new();
        }
        context = children_context(tile, place(tile, &kind(tile), context, measure));
    }
    let mut out = Vec::new();
    let mut stack = vec![(root, context)];
    while let Some((id, parent)) = stack.pop() {
        let tile = ui.tile(id);
        if !tile.live {
            continue;
        }
        let kind = kind(tile);
        let context = place(tile, &kind, parent, measure);
        let fill = matches!(kind, Kind::Hotrect) && tile.get(HOTRECT_FILL).is_some_and(Value::truthy);
        if fill || !matches!(kind, Kind::Menu | Kind::Hotrect) {
            out.push(DrawItem {
                tile: id,
                kind,
                rect: context.rect,
                depth: context.depth,
                alpha: context.alpha,
                visible: context.visible,
                clip: context.clip,
                colour: Colour {
                    red: number(tile, "red", 255.0).clamp(0.0, 255.0),
                    green: number(tile, "green", 255.0).clamp(0.0, 255.0),
                    blue: number(tile, "blue", 255.0).clamp(0.0, 255.0),
                    brightness: number(tile, "brightness", 255.0),
                    systemcolor: optional(tile, "systemcolor"),
                    systemcolor_name: symbol(tile, "systemcolor"),
                },
            });
        }
        let parent = children_context(tile, context);
        let mut children = tile.children.clone();
        children.sort_by(|&a, &b| number(ui.tile(a), "depth", 0.0).total_cmp(&number(ui.tile(b), "depth", 0.0)));
        stack.extend(children.into_iter().rev().map(|id| (id, parent)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    fn tree(xml: &str) -> (Ui, TileId) {
        let src = MemSource::default().with("test.xml", xml);
        let mut ui = Ui::new();
        let root = ui.load_menu(&src, "test.xml").unwrap();
        ui.update();
        (ui, root)
    }

    fn unmeasured(_: f64, _: &str, _: Option<f64>) -> (f64, f64) {
        panic!("this tree needs no text measurement")
    }

    #[test]
    fn a_tile_outside_its_window_stays_inside_the_ones_above() {
        // [design] A window in a window: the mark skips the nearest only.
        let (mut ui, root) = tree(
            r#"<menu name="M"><rect name="Outer"><width>100</width><height>100</height><clips>&true;</clips>
              <rect name="Inner"><width>50</width><height>50</height><clips>&true;</clips>
                <image name="Beside"><x>50</x><width>20</width><height>20</height></image>
                <image name="In"><width>20</width><height>20</height></image>
              </rect></rect></menu>"#,
        );
        let beside = ui.find_tile("M/Outer/Inner/Beside").unwrap();
        let clip = |ui: &Ui, t: TileId| layout(ui, root, &unmeasured, true).into_iter().find(|i| i.tile == t).unwrap().clip.unwrap();
        assert_eq!(clip(&ui, beside).width, 50.0, "both windows");
        ui.set_trait(beside, fnv_ui::input::OUTSIDE_CLIP, Value::Num(1.0));
        ui.update();
        assert_eq!(clip(&ui, beside).width, 100.0, "the outer window still crops it");
        let inside = ui.find_tile("M/Outer/Inner/In").unwrap();
        assert_eq!(clip(&ui, inside).width, 50.0, "its neighbour keeps both");
    }

    #[test]
    fn a_hotrects_alpha_is_not_passed_to_its_children() {
        // [design] A text button's shape (text_box.xml): an alpha-0 hotrect over its text,
        // under a rect at half strength.
        let (ui, root) = tree(
            r#"<menu name="M"><rect name="R"><alpha>128</alpha>
          <hotrect name="H"><alpha>0</alpha><text name="T"><string>OK</string><width>20</width><height>10</height><alpha>255</alpha></text></hotrect>
          </rect></menu>"#,
        );
        let items = layout(&ui, root, &unmeasured, true);
        let text = items.iter().find(|i| matches!(i.kind, Kind::Text(_))).unwrap();
        assert_eq!(text.alpha, 128.0);
        // [design] With T15 off, the same alpha-0 ancestor still hides the text.
        assert_eq!(layout(&ui, root, &unmeasured, false)[1].alpha, 0.0);
        let child = ui.find_tile("M/R/H/T").unwrap();
        assert_eq!(layout(&ui, child, &unmeasured, false)[0].alpha, 0.0);
        assert_eq!(layout(&ui, child, &unmeasured, true)[0].alpha, 128.0);
        // An alpha-0 hotrect alone over its text: the text drawn at full strength, shown.
        let (ui, root) = tree(
            r#"<menu name="M"><hotrect name="H"><alpha>0</alpha><text name="T"><string>OK</string><width>20</width><height>10</height></text></hotrect></menu>"#,
        );
        let items = layout(&ui, root, &unmeasured, true);
        assert_eq!(items.len(), 1, "the hotrect itself draws nothing");
        assert_eq!((items[0].alpha, items[0].visible), (255.0, true));
    }

    #[test]
    fn places_and_depth_add_through_containers_and_subtree_roots() {
        let (ui, root) = tree(
            r#"<menu name="M"><x>10</x><y>20</y><depth>-50</depth>
          <hotrect name="H"><x>3</x><y>4</y><depth>2</depth>
            <rect name="R"><x>5</x><y>6</y><width>30</width><height>40</height><depth>1</depth></rect>
          </hotrect></menu>"#,
        );
        let items = layout(&ui, root, &unmeasured, false);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, Kind::Rect);
        assert_eq!(items[0].rect, Rect { left: 18.0, top: 30.0, width: 30.0, height: 40.0 });
        assert_eq!(items[0].depth, -47.0);
        assert_eq!(layout(&ui, items[0].tile, &unmeasured, false), items);
    }

    #[test]
    fn parents_precede_children_and_sibling_depth_ties_keep_file_order() {
        let (ui, root) = tree(
            r#"<menu name="M">
          <rect name="Later"><depth>2</depth></rect>
          <rect name="First"><depth>-1</depth><rect name="Child"><depth>100</depth></rect></rect>
          <rect name="Tie1"/><rect name="Tie2"/>
        </menu>"#,
        );
        let items = layout(&ui, root, &unmeasured, false);
        let names: Vec<_> = items.iter().map(|i| ui.tile(i.tile).name.as_str()).collect();
        assert_eq!(names, ["First", "Child", "Tie1", "Tie2", "Later"]);
        assert_eq!(items[1].depth, 99.0);
    }

    #[test]
    fn alpha_products_stay_unrounded_and_visibility_hides_descendants() {
        let (ui, root) = tree(
            r#"<menu name="M"><alpha>128</alpha><visible>&false;</visible>
          <rect name="R"><alpha>128</alpha><visible>&true;</visible>
            <image name="I"><alpha>128</alpha></image>
          </rect></menu>"#,
        );
        let items = layout(&ui, root, &unmeasured, false);
        assert!((items[0].alpha - 128.0 * 128.0 / 255.0).abs() < 1e-10);
        assert!((items[1].alpha - 128.0_f64.powi(3) / 255.0_f64.powi(2)).abs() < 1e-10);
        assert!(items.iter().all(|i| !i.visible));
        assert_eq!(layout(&ui, items[1].tile, &unmeasured, false), items[1..]);
    }

    #[test]
    fn clips_apply_to_children_and_intersect_through_nested_clippers() {
        let (ui, root) = tree(
            r#"<menu name="M"><x>10</x><y>20</y><width>100</width><height>80</height><clips>&true;</clips>
          <rect name="R"><x>50</x><y>-10</y><width>80</width><height>40</height><clips>&true;</clips>
            <image name="I"><width>200</width><height>200</height></image>
          </rect>
          <rect name="Outside"><x>200</x><width>10</width><height>10</height><clips>1</clips><rect name="Empty"/></rect>
        </menu>"#,
        );
        let items = layout(&ui, root, &unmeasured, false);
        assert_eq!(items[0].clip, Some(Rect { left: 10.0, top: 20.0, width: 100.0, height: 80.0 }));
        assert_eq!(items[1].clip, Some(Rect { left: 60.0, top: 20.0, width: 50.0, height: 30.0 }));
        assert_eq!(items[1].rect.width, 200.0);
        assert_eq!(items[3].clip.unwrap().width, 0.0);
        assert_eq!(layout(&ui, items[1].tile, &unmeasured, false), items[1..2]);
    }

    #[test]
    fn a_marked_tile_takes_no_alpha_from_above() {
        let xml = r#"<menu name="M"><image name="B"><alpha>51</alpha><width>9</width><height>9</height>
            <rect name="R"><image name="I"><alpha>127.5</alpha></image></rect></image></menu>"#;
        let (mut ui, root) = tree(xml);
        let alpha = |ui: &Ui, name: &str| layout(ui, root, &unmeasured, false).into_iter().find(|i| ui.tile(i.tile).name == name).unwrap().alpha;
        assert_eq!(alpha(&ui, "I"), 25.5, "unmarked, the product through the picture, 0 to 255");
        let r = ui.find_tile("M/B/R").unwrap();
        ui.set_trait(r, OWN_ALPHA, Value::Num(1.0));
        assert_eq!(alpha(&ui, "B"), 51.0, "the picture keeps its own");
        assert_eq!(alpha(&ui, "R"), 255.0, "the mark starts again at full strength");
        assert_eq!(alpha(&ui, "I"), 127.5, "under the mark, only what is below it");
    }

    #[test]
    fn scrollbar_children_are_visible() {
        // [design] A scrollbar as an image at alpha 0 over its pieces, the marker's alpha copied.
        let xml = r#"<menu name="M"><hotrect name="L"><alpha>0</alpha>
            <image name="lb_scrollbar"><alpha>0</alpha><_marker_alpha>255</_marker_alpha>
              <image name="scrollbar_vert_marker"><filename>m.dds</filename><alpha><copy src="parent()" trait="_marker_alpha"/></alpha></image>
            </image></hotrect></menu>"#;
        let (mut ui, root) = tree(xml);
        let marker = ui.find_tile("M/L/lb_scrollbar/scrollbar_vert_marker").unwrap();
        let alpha = |ui: &Ui| layout(ui, root, &unmeasured, true).into_iter().find(|i| i.tile == marker).unwrap().alpha;
        assert_eq!(alpha(&ui), 0.0, "unmarked, the image's alpha hides it");
        ui.set_trait(marker, OWN_ALPHA, Value::Num(1.0));
        assert_eq!(alpha(&ui), 255.0, "on the 0-255 scale, what `draw::pieces` divides by 255");
    }

    #[test]
    fn a_hotrect_draws_its_fill_only_when_marked() {
        let (mut ui, root) = tree(r#"<menu name="M"><hotrect name="H"><alpha>40</alpha><width>9</width><height>9</height><image name="I"/></hotrect></menu>"#);
        let h = ui.find_tile("M/H").unwrap();
        assert!(layout(&ui, root, &unmeasured, true).iter().all(|i| i.tile != h), "a hotrect draws nothing");
        ui.set_trait(h, HOTRECT_FILL, Value::Num(1.0));
        ui.set_trait(h, OWN_ALPHA, Value::Num(1.0));
        let items = layout(&ui, root, &unmeasured, true);
        let item = items.iter().find(|i| i.tile == h).unwrap();
        assert_eq!((item.alpha, &item.kind), (40.0, &Kind::Hotrect));
    }

    #[test]
    fn clipwindow_clips_hotrect_descendants() {
        let (mut ui, root) = tree(
            r#"<menu name="M"><hotrect name="W"><x>10</x><y>20</y><width>100</width><height>80</height><clipwindow>&true;</clipwindow>
            <text name="T"><string>t</string><y>-30</y><width>90</width><height>400</height></text>
          </hotrect></menu>"#,
        );
        // Not turned on: nothing is cropped, as in every menu that did not ask for it.
        let t = ui.find_tile("M/W/T").unwrap();
        let of = |ui: &Ui| layout(ui, root, &unmeasured, true).into_iter().find(|i| i.tile == t).unwrap();
        assert_eq!(of(&ui).clip, None);
        fnv_ui::input::mark_clip_windows(&mut ui, root);
        let item = of(&ui);
        assert_eq!(item.clip, Some(Rect { left: 10.0, top: 20.0, width: 100.0, height: 80.0 }));
        // The crop moves nothing: the text stands where its own `y` puts it.
        assert_eq!((item.rect.top, item.rect.height), (-10.0, 400.0));
    }

    #[test]
    fn clipwindow_intersects_clips_and_survives_subtree_layout() {
        let (mut ui, root) = tree(
            r#"<menu name="M"><rect name="C"><x>10</x><y>10</y><width>50</width><height>50</height><clips>&true;</clips>
            <hotrect name="W"><x>20</x><y>20</y><width>100</width><height>100</height><clipwindow>1</clipwindow>
              <image name="I"><width>500</width><height>500</height></image>
            </hotrect>
            <hotrect name="Off"><clipwindow>0</clipwindow><image name="J"><width>5</width><height>5</height></image></hotrect>
            <hotrect name="Apart"><x>500</x><width>9</width><height>9</height><clipwindow>1</clipwindow><image name="K"/></hotrect>
          </rect></menu>"#,
        );
        fnv_ui::input::mark_clip_windows(&mut ui, root);
        let items = layout(&ui, root, &unmeasured, true);
        let at = |name: &str| items.iter().find(|i| ui.tile(i.tile).name == name).unwrap();
        // The window's box less what the outer clip leaves of it.
        let both = Some(Rect { left: 30.0, top: 30.0, width: 30.0, height: 30.0 });
        assert_eq!(at("I").clip, both);
        // A false `clipwindow` adds nothing to the clip above it.
        assert_eq!(at("J").clip, Some(Rect { left: 10.0, top: 10.0, width: 50.0, height: 50.0 }));
        // A window outside the clip leaves nothing.
        assert_eq!(at("K").clip.unwrap().width, 0.0);
        // Laid out alone, the image still has both ancestors' crop.
        let i = at("I").tile;
        assert_eq!(layout(&ui, i, &unmeasured, true)[0].clip, both);
    }

    #[test]
    fn defaults_and_kind_inputs_are_preserved_without_loading_assets() {
        let (ui, root) = tree(
            r#"<menu name="M">
          <image name="Default"/>
          <image name="Set"><filename>pic.dds</filename><zoom>75</zoom><tile>2</tile><cropx>3</cropx><cropy>4</cropy>
            <texatlas>atlas.tai</texatlas><red>-1</red><green>128</green><blue>300</blue><brightness>160</brightness><systemcolor>7</systemcolor></image>
          <nif name="N"/><radial name="R"/><window name="W"><systemcolor>&hudmain;</systemcolor></window>
        </menu>"#,
        );
        let items = layout(&ui, root, &unmeasured, false);
        let item = &items[0];
        assert_eq!(item.rect, Rect::default());
        assert_eq!((item.depth, item.alpha, item.visible, item.clip), (0.0, 255.0, true, None));
        assert_eq!(item.colour, Colour { red: 255.0, green: 255.0, blue: 255.0, brightness: 255.0, systemcolor: None, systemcolor_name: None });
        assert_eq!(item.kind, Kind::Image(Image { filename: String::new(), zoom: 100.0, tile: None, cropx: None, cropy: None, texatlas: None }));
        assert_eq!(
            items[1].kind,
            Kind::Image(Image { filename: "pic.dds".into(), zoom: 75.0, tile: Some(2.0), cropx: Some(3.0), cropy: Some(4.0), texatlas: Some(Value::Str("atlas.tai".into())) })
        );
        assert_eq!(items[1].colour, Colour { red: 0.0, green: 128.0, blue: 255.0, brightness: 160.0, systemcolor: Some(7.0), systemcolor_name: None });
        assert_eq!(items[2].kind, Kind::Nif(Nif::default()));
        assert_eq!(items[3].kind, Kind::Radial);
        assert_eq!(items[4].kind, Kind::Window);
        assert_eq!(items[4].colour.systemcolor_name.as_deref(), Some("hudmain"));
    }

    #[test]
    fn text_measures_missing_dimensions_and_anchors_named_justification() {
        let (ui, root) = tree(
            r#"<menu name="M"><x>100</x><y>20</y>
          <text name="Left"><string>abc</string><font>3</font><wrapwidth>60</wrapwidth><justify>&left;</justify></text>
          <text name="Center"><string>abc</string><width>40</width><justify>&center;</justify></text>
          <text name="Right"><string>abc</string><height>12</height><justify>&right;</justify></text>
          <text name="Explicit"><string>abc</string><width>0</width><height>0</height><justify>17</justify></text>
        </menu>"#,
        );
        let calls = std::cell::RefCell::new(Vec::new());
        let measure = |font, text: &str, wrap| {
            calls.borrow_mut().push((font, text.to_string(), wrap));
            (30.0, 10.0)
        };
        let items = layout(&ui, root, &measure, false);
        assert_eq!(*calls.borrow(), [(3.0, "abc".into(), Some(60.0)), (0.0, "abc".into(), None), (0.0, "abc".into(), None)]);
        assert_eq!(items[0].rect, Rect { left: 100.0, top: 20.0, width: 30.0, height: 10.0 });
        assert_eq!(items[1].rect, Rect { left: 80.0, top: 20.0, width: 40.0, height: 10.0 });
        assert_eq!(items[2].rect, Rect { left: 70.0, top: 20.0, width: 30.0, height: 12.0 });
        assert_eq!(items[3].rect, Rect { left: 100.0, top: 20.0, width: 0.0, height: 0.0 });
        let Kind::Text(text) = &items[3].kind else { panic!("expected text") };
        assert_eq!((text.justify, text.alignment), (17.0, Alignment::Left));
    }

    #[test]
    fn unloaded_subtrees_emit_nothing() {
        let (mut ui, root) = tree(r#"<menu name="M"><rect name="R"><image name="I"/></rect></menu>"#);
        let rect = ui.find_tile("M/R").unwrap();
        ui.unload_tile(rect);
        assert!(layout(&ui, root, &unmeasured, false).is_empty());
        assert!(layout(&ui, rect, &unmeasured, false).is_empty());
    }
}
