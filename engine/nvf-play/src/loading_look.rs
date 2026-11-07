//! What a loading screen shows (`crate::loading` brings it up and takes it down): the tip
//! on its band and the turning emblem (docs/spec-notes/loading-screens.md §4).
//!
//! - **The tip** ([`tip_place`], [`band`], [`tip_lines`]): the screen's LSCT gives the box,
//!   font, colour and alignment. x and width are pixels of a window 1280 wide, y and the
//!   text's size parts of 960 of its height [observed: 1280x720 and 1280x1024]. The band is
//!   `loading_text_bar.dds` across the whole window, its own clear sides leaving the dark
//!   part about over the box [observed]; how far it reaches round the lines is
//!   [`BAND_ABOVE`] and [`BAND_BELOW`], how dark it is [`BAND_LAYERS`]. No text, no band.
//! - **The emblem** ([`Emblem`]): the model's flat pieces, each a picture turned to its
//!   angle (`crate::hud_breath::spawn_pieces`). Unlike the breath meter's they lie in the
//!   model's x-y plane, x to the right and y down, seen from below z [observed: the file's
//!   normals and texture coordinates; the wheel's numbers read the right way round so in the
//!   original]. The sequence's rotation keys turn the nodes they drive about their pivots;
//!   the clock behind the screen is stopped, so they run on real time.

use crate::hud_breath::{place_in, quad, shape_texture, spawn_pieces, to_root, MeterModel, Piece};
use crate::interface::{amber, bc_supported, face, file_name, plain_font, set_line, u, Face, Pictures, FONT_FILES};
use crate::tiles::draw::{line_pitch, text_lines};
use crate::tiles::layout::Alignment;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_font::{Font, Line};
use fnv_nif::anim::{TrackData, TransformTrack};
use fnv_world::loadscreen::TipStyle;
use nvf_assets::GameData;
use std::collections::HashMap;
use std::sync::Arc;

/// The tip's band [documented: `menus\loading_menu.xml`, `tip_matte_01`].
pub const BAND_TEXTURE: &str = "textures\\interface\\loading\\loading_text_bar.dds";
/// The dark part's top edge stands this far over the box, interface units [observed: fitted
/// to the two shots of two-line tips, 1280x720 and 1280x1024, within a pixel].
pub const BAND_ABOVE: f32 = 7.5;
/// Its bottom edge stands this far under the box's top plus the lines' pitches, interface
/// units [observed: as [`BAND_ABOVE`]; hypothesis: the same for other numbers of lines].
pub const BAND_BELOW: f32 = 3.0;
/// Where the texture's dark part begins and ends down its height: the rows at half of its
/// alpha, 8 and 115 of 128 [observed: `loading_text_bar.dds`].
const BAND_DARK: (f32, f32) = (8.0 / 128.0, 115.0 / 128.0);
/// The band's texture is drawn this many times, at these alphas. The original lets through
/// 0.26 of a picture's value, which is the texture's own alpha of 192 of 255 blended on
/// gamma values; this interface blends in linear light, where the same alpha once lets
/// through 0.54 of the value [observed: both, against the pictures]. Black over a picture
/// needs about the alpha's complement to the power 2.2, and layers do that along the
/// texture's soft edges too, which raising one alpha would not [fitted: 225, 211, 194
/// comes out about 60, 55, 50 where the original has 57, 53, 49].
pub const BAND_LAYERS: [f32; 3] = [1.0, 1.0, 0.2];

/// The emblem's model and sequence [documented: `menus\loading_menu.xml`, `loading_pinwheel`].
pub const EMBLEM_MODEL: &str = "meshes\\interface\\circular loading\\loading01.nif";
pub const EMBLEM_SEQUENCE: &str = "Idle";
/// The texture of the piece the emblem is placed and sized by.
const EMBLEM_WHEEL_TEXTURE: &str = "load_roulette_wheel.dds";
/// The wheel's centre as parts of the window's width and height [observed: 1280x720 and
/// 1280x1024, about +-5 px].
pub const EMBLEM_CENTRE: Vec2 = Vec2::new(0.5, 0.794);
/// The wheel's outer ring across, as a part of the window's height [observed: 124 of 720
/// and 176 of 1024].
pub const EMBLEM_WHEEL: f32 = 0.172;
/// That ring across, as a part of the wheel's square in the model [observed: the same
/// measure on NVForever's frame, 132 px for a square of 137.6].
const EMBLEM_RING: f32 = 0.96;

/// Interface units a window's height holds.
const HEIGHT_UNITS: f32 = 960.0;
/// The width the LSCT's x and width count in, pixels [observed: the box's place and the
/// lines' wrapping at both window sizes].
const WIDTH_PIXELS: f32 = 1280.0;

/// The tip's text box, interface units from the window's top left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TipPlace {
    pub left: f32,
    pub width: f32,
    pub top: f32,
}

/// Where a style's box stands in a window.
pub fn tip_place(style: &TipStyle, window: Vec2) -> TipPlace {
    let across = window.x / WIDTH_PIXELS * HEIGHT_UNITS / window.y.max(1.0);
    TipPlace { left: style.x * across, width: style.width * across, top: style.y }
}

/// The band's texture's top and height for a text of `lines` lines `pitch` apart under
/// `top`, interface units; `None` with no text. The dark part goes from [`BAND_ABOVE`] over
/// the box to [`BAND_BELOW`] under the lines, so the text stands about 3 units over its
/// middle, as in the original [observed].
pub fn band(top: f32, lines: usize, pitch: f32) -> Option<(f32, f32)> {
    (lines > 0).then(|| {
        let height = (BAND_ABOVE + lines as f32 * pitch + BAND_BELOW) / (BAND_DARK.1 - BAND_DARK.0);
        (top - BAND_ABOVE - BAND_DARK.0 * height, height)
    })
}

/// A style's font number; one that names no font file is the fallback's [design].
pub fn font_number(style: &TipStyle) -> u8 {
    u8::try_from(style.font).ok().filter(|n| (1..=FONT_FILES.len() as u8).contains(n)).unwrap_or(TipStyle::FALLBACK.font as u8)
}

/// LSCT's alignment: 1 left, 2 centre, 4 right [documented: fopdoc FalloutNV/Records/LSCT];
/// any other number centred [design].
pub fn alignment(style: &TipStyle) -> Alignment {
    match style.alignment {
        1 => Alignment::Left,
        4 => Alignment::Right,
        _ => Alignment::Center,
    }
}

/// The tip's lines: each line, where it starts and the line its letters stand on, interface
/// units from the window's top left.
pub fn tip_lines(font: &Font, text: &str, style: &TipStyle, window: Vec2) -> Vec<(Line, f32, f32)> {
    let place = tip_place(style, window);
    let width = f64::from(place.width);
    text_lines(font, font_number(style), text, Some(width), alignment(style), width).into_iter().map(|(line, x, base)| (line, place.left + x, place.top + base)).collect()
}

/// The tip of a screen on the screen's node; [`fit_tip`] draws it for the window's size.
#[derive(Component)]
pub struct Tip {
    pub text: String,
    pub style: TipStyle,
    pub drawn_for: Option<Vec2>,
}

/// What the screens read once and keep: the emblem, its textures, the band and the font.
#[derive(Resource, Default)]
pub struct Look {
    /// `None` until read; the inner `None` for a model that could not be read.
    emblem: Option<Option<Arc<Emblem>>>,
    pictures: Pictures,
    faces: HashMap<u8, Arc<Face>>,
}

impl Look {
    /// The emblem, read once (and told once).
    fn emblem(&mut self, data: &GameData) -> Option<Arc<Emblem>> {
        self.emblem
            .get_or_insert_with(|| {
                let read = data.read(EMBLEM_MODEL).ok_or_else(|| "not found".to_string()).and_then(|bytes| nvf_assets::guard::guarded(|| Emblem::read(&bytes, &data.schema)));
                match read {
                    Ok(e) => {
                        info!("loading screen: emblem read: {} pieces, {} turning, sequence {:.3} s", e.model.pieces.len(), e.turns.len(), e.model.length);
                        Some(Arc::new(e))
                    }
                    Err(e) => {
                        warn!("loading screen: no emblem: {EMBLEM_MODEL}: {e}");
                        None
                    }
                }
            })
            .clone()
    }
}

/// Draw each tip for the window, again when its size changes: how the text wraps follows
/// the window's shape. In the frame the screen comes up too (`crate::loading`'s chain).
pub fn fit_tip(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    data: Option<Res<GameData>>,
    images: Option<ResMut<Assets<Image>>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut look: ResMut<Look>,
    mut tips: Query<(Entity, &mut Tip)>,
) {
    // The loading screen fills the view: at the original's size whatever the UI scale (#1631).
    let _view = crate::interface::original_units();
    let Ok(window) = windows.single() else { return };
    let Some(mut images) = images else { return };
    let size = Vec2::new(window.width(), window.height());
    let bc = bc_supported(formats.as_deref());
    let data = data.as_deref();
    let look = &mut *look;
    for (entity, mut tip) in &mut tips {
        if tip.drawn_for == Some(size) {
            continue;
        }
        tip.drawn_for = Some(size);
        let number = font_number(&tip.style);
        let font = look.faces.entry(number).or_insert_with(|| {
            // The stand-in's size and advance are about those of the game's font 6.
            Arc::new(face(FONT_FILES[usize::from(number) - 1], plain_font(34.0, 17.0), data, &mut images, bc))
        });
        let font = font.clone();
        let image = data.and_then(|d| look.pictures.file(BAND_TEXTURE, d, &mut images, bc)).map(|(image, _)| image);
        let lines = tip_lines(&font.font, &tip.text, &tip.style, size);
        let at = band(tip.style.y, lines.len(), line_pitch(number, &font.font));
        let [r, g, b] = tip.style.colour.map(|c| c / 255.0);
        commands.entity(entity).despawn_related::<Children>().with_children(|p| {
            if let (Some((top, height)), Some(image)) = (at, image) {
                for alpha in BAND_LAYERS {
                    let node = Node { position_type: PositionType::Absolute, left: Val::Px(0.0), width: Val::Percent(100.0), top: u(top), height: u(height), ..default() };
                    p.spawn((node, ImageNode { color: Color::WHITE.with_alpha(alpha), image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) }, Pickable::IGNORE));
                }
            }
            for (line, x, base) in &lines {
                set_line(p, &font, line, *x, *base, Color::srgb(r, g, b));
            }
        });
    }
}

/// A node of the rotation keys: it turns what hangs under it about its pivot.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub node: String,
    /// The node's origin, model units (x right, y up).
    pub pivot: Vec2,
    /// The rotation from the node's parent to the model's root.
    parent: Mat3,
    /// The node's own rotation in the file.
    bind: Mat3,
    track: TransformTrack,
}

impl Turn {
    /// How far the node has turned at the sequence's time `t`: radians anticlockwise.
    pub fn at(&self, t: f32) -> f32 {
        self.track.rotation_at(t).map_or(0.0, |[w, x, y, z]| screen_turn(self.parent, Quat::from_xyzw(x, y, z, w), self.bind))
    }
}

/// A point of the model's root as the screen sees it: x right, y up.
fn flat(root: Vec3) -> Vec2 {
    Vec2::new(root.x, -root.y)
}

/// The turn on the screen, radians anticlockwise, of a node standing at `rotation` where the
/// file's rest is `bind`, under a parent turned `parent` from the root.
pub fn screen_turn(parent: Mat3, rotation: Quat, bind: Mat3) -> f32 {
    let change = parent * Mat3::from_quat(rotation) * bind.inverse() * parent.inverse();
    let x = flat(change * Vec3::X);
    x.y.atan2(x.x)
}

/// A piece's centre and angle after its node has turned `turn` about `pivot`.
pub fn turned(centre: Vec2, angle: f32, pivot: Vec2, turn: f32) -> (Vec2, f32) {
    (pivot + Vec2::from_angle(turn).rotate(centre - pivot), angle + turn)
}

/// The sequence's time `elapsed` real seconds after it began, going round (cycle type 0 in
/// the file, "loop"): seconds of a long frame are simply passed over.
pub fn sequence_time(elapsed: f64, start: f32, length: f32) -> f32 {
    if length > 0.0 {
        start + elapsed.rem_euclid(f64::from(length)) as f32
    } else {
        start
    }
}

/// The emblem as read: its pieces back to front, the nodes that turn and how it stands in
/// the window.
#[derive(Debug, Clone, PartialEq)]
pub struct Emblem {
    pub model: MeterModel,
    pub turns: Vec<Turn>,
    pub start: f32,
    /// The model's point at the emblem's centre.
    pub anchor: Vec2,
    /// Interface units a model unit.
    pub zoom: f32,
}

/// The point at the emblem's centre and the interface units of a model unit: the wheel's
/// centre and its ring at [`EMBLEM_WHEEL`] of the height; without a wheel (another model in
/// its place) its bounds' [design].
pub fn anchor_and_zoom(model: &MeterModel) -> (Vec2, f32) {
    let wheel = model.pieces.iter().find(|p| file_name(&p.texture) == EMBLEM_WHEEL_TEXTURE);
    let (centre, across) = wheel.map_or((model.bounds.center(), model.bounds.width().max(model.bounds.height())), |p| (p.centre, p.size.x));
    (centre, EMBLEM_WHEEL / EMBLEM_RING * HEIGHT_UNITS / across.max(1e-3))
}

impl Emblem {
    /// Read the emblem's model. A shape flagged hidden, or under a hidden node, is left out
    /// (bit 0 of an object's flags, as `nvf_assets::actor`): the model's large
    /// `loading_brighten01` plate is one, and the original shows no such plate [observed].
    pub fn read(bytes: &[u8], schema: &fnv_nif::Schema) -> Result<Emblem, String> {
        use fnv_nif::scene::{mesh_data, scene_objects, Decoded};
        let f = fnv_nif::NifFile::parse(bytes, Some(schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&f, bytes, schema);
        let objects = scene_objects(&f, &dec);
        let by_block: HashMap<usize, usize> = objects.iter().enumerate().map(|(i, o)| (o.block, i)).collect();
        let hidden = |mut i: usize| loop {
            let o = &objects[i];
            if dec.get(o.block as i32).and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0 {
                return true;
            }
            match o.parent.and_then(|p| by_block.get(&p)) {
                Some(&p) => i = p,
                None => return false,
            }
        };
        // With the depth along z: the view looks up it, so the largest is the furthest.
        let mut pieces: Vec<(f32, Piece)> = Vec::new();
        for (i, o) in objects.iter().enumerate().filter(|(_, o)| !o.is_node) {
            let Some(shape) = dec.get(o.block as i32) else { continue };
            let Some(data) = shape.block_ref("data").and_then(|d| dec.get(d)) else { continue };
            if hidden(i) {
                continue;
            }
            let mesh = mesh_data(data);
            let Some(uvs) = mesh.uv_sets.first() else { continue };
            let (m, t) = to_root(&objects, &by_block, i);
            let points: Vec<Vec3> = mesh.positions.iter().map(|p| m * Vec3::from_array(*p) + t).collect();
            let corners: Vec<[f32; 2]> = points.iter().map(|&p| flat(p).to_array()).collect();
            let Some((centre, size, angle, uv)) = quad(&corners, uvs) else { continue };
            let Some(texture) = shape_texture(shape, &dec) else { continue };
            let node = o.parent.and_then(|p| by_block.get(&p)).and_then(|&p| objects[p].name.clone()).unwrap_or_default();
            // A few of the file's corners lie a hair outside the texture.
            let uv = Rect::from_corners(uv.min.clamp(Vec2::ZERO, Vec2::ONE), uv.max.clamp(Vec2::ZERO, Vec2::ONE));
            let depth = points.iter().map(|p| p.z).sum::<f32>() / points.len().max(1) as f32;
            pieces.push((depth, Piece { node, centre, size, angle, uv, texture }));
        }
        if pieces.is_empty() {
            return Err("no flat pieces".into());
        }
        pieces.sort_by(|a, b| b.0.total_cmp(&a.0));
        let pieces: Vec<Piece> = pieces.into_iter().map(|(_, p)| p).collect();
        let seq = fnv_nif::anim::sequences(&f, &dec).into_iter().find(|s| s.name.as_deref() == Some(EMBLEM_SEQUENCE)).ok_or_else(|| format!("no sequence {EMBLEM_SEQUENCE}"))?;
        let mut turns = Vec::new();
        for track in seq.tracks {
            let (Some(node), TrackData::Transform(track)) = (track.node_name, track.data) else { continue };
            let Some(i) = objects.iter().position(|o| o.name.as_deref() == Some(node.as_str())) else { continue };
            let o = &objects[i];
            let parent = o.parent.and_then(|p| by_block.get(&p)).map_or(Mat3::IDENTITY, |&p| to_root(&objects, &by_block, p).0);
            let bind = Mat3::from_cols_array(&o.transform.rotation).transpose();
            turns.push(Turn { pivot: flat(to_root(&objects, &by_block, i).1), node, parent, bind, track });
        }
        let bounds = pieces.iter().fold(Rect::EMPTY, |b, p| b.union(Rect::from_center_size(p.centre, p.size)));
        let model = MeterModel { pieces, keys: Vec::new(), length: seq.stop_time - seq.start_time, bounds };
        let (anchor, zoom) = anchor_and_zoom(&model);
        Ok(Emblem { model, turns, start: seq.start_time, anchor, zoom })
    }
}

/// The emblem's node: where its pieces hang, and when it began turning.
#[derive(Component)]
pub struct EmblemBox {
    emblem: Arc<Emblem>,
    began: Option<f64>,
}

/// One of the emblem's pieces: its centre and angle in the file, and the turn it follows.
#[derive(Component)]
pub struct EmblemPart {
    centre: Vec2,
    angle: f32,
    turn: Option<usize>,
}

/// Put the emblem on the screen `root`; nothing when its model cannot be read.
pub fn spawn_emblem(commands: &mut Commands, root: Entity, look: &mut Look, data: &GameData, images: &mut Assets<Image>, bc: bool) {
    // The loading screen fills the view: at the original's size whatever the UI scale (#1631).
    let _view = crate::interface::original_units();
    let Some(emblem) = look.emblem(data) else { return };
    let mut read = |path: &str| look.pictures.file(path, data, images, bc);
    let node = Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(EMBLEM_CENTRE.x * 100.0),
        top: Val::Percent(EMBLEM_CENTRE.y * 100.0),
        width: Val::Px(0.0),
        height: Val::Px(0.0),
        ..default()
    };
    let place = |piece: &Piece| place_in(piece, emblem.anchor, Vec2::ZERO, emblem.zoom);
    let mark = |piece: &Piece| EmblemPart { centre: piece.centre, angle: piece.angle, turn: emblem.turns.iter().position(|t| t.node == piece.node) };
    commands.spawn((EmblemBox { emblem: emblem.clone(), began: None }, node, Pickable::IGNORE, ChildOf(root))).with_children(|p| {
        spawn_pieces(p, &emblem.model, place, amber(255.0), &mut read, mark, |_| true);
    });
}

/// Turn the emblem's pieces by the real clock (module notes).
pub fn turn_emblem(real: Res<Time<Real>>, mut boxes: Query<&mut EmblemBox>, mut parts: Query<(&EmblemPart, &mut UiTransform)>) {
    // The loading screen fills the view: at the original's size whatever the UI scale (#1631).
    let _view = crate::interface::original_units();
    let Some(mut emblem_box) = boxes.iter_mut().next() else { return };
    let now = real.elapsed_secs_f64();
    let began = *emblem_box.began.get_or_insert(now);
    let emblem = &emblem_box.emblem;
    let t = sequence_time(now - began, emblem.start, emblem.model.length);
    let turns: Vec<f32> = emblem.turns.iter().map(|turn| turn.at(t)).collect();
    for (part, mut transform) in &mut parts {
        let Some(i) = part.turn else { continue };
        let (centre, angle) = turned(part.centre, part.angle, emblem.turns[i].pivot, turns[i]);
        let moved = (centre - part.centre) * emblem.zoom;
        transform.translation = Val2::new(u(moved.x), u(-moved.y));
        // As `spawn_pieces`: the screen's y runs down.
        transform.rotation = Rot2::radians(-angle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    /// Interface units as pixels of a window `height` high.
    fn px(units: f32, height: f32) -> f32 {
        units * height / HEIGHT_UNITS
    }

    const WIDE: Vec2 = Vec2::new(1280.0, 720.0);
    const TALL: Vec2 = Vec2::new(1280.0, 1024.0);

    fn close(a: f32, b: f32, within: f32) {
        assert!((a - b).abs() <= within, "{a} is not within {within} of {b}");
    }

    /// Every letter 17 wide with a space 14, as font 6 about is.
    fn font() -> Font {
        let mut f = plain_font(34.0, 17.0);
        f.glyphs[b' ' as usize].after = 14.0;
        f
    }

    #[test]
    fn the_box_follows_the_windows_width_across_and_its_height_down() {
        let style = TipStyle::FALLBACK;
        // Both windows are 1280 wide: the same pixels across, in interface units that differ.
        for window in [WIDE, TALL] {
            let place = tip_place(&style, window);
            close(px(place.left, window.y), 210.0, 0.01);
            close(px(place.width, window.y), 850.0, 0.01);
        }
        // Down, a part of the height: 75 at 720, 107 at 1024 [observed: about 75 and 107].
        close(px(tip_place(&style, WIDE).top, WIDE.y), 75.0, 0.01);
        close(px(tip_place(&style, TALL).top, TALL.y), 106.7, 0.05);
        // Twice as wide, twice the pixels.
        let double = Vec2::new(2560.0, 720.0);
        close(px(tip_place(&style, double).left, double.y), 420.0, 0.01);
    }

    #[test]
    fn the_band_follows_the_text_and_goes_with_it() {
        assert_eq!(band(100.0, 0, 38.0), None);
        let dark = |lines: usize, height: f32| {
            let (top, whole) = band(100.0, lines, 38.0).unwrap();
            (px(top + BAND_DARK.0 * whole, height), px(top + BAND_DARK.1 * whole, height))
        };
        // Two lines: the dark part where the shots have it, y 70-134.5 and 99-191 [observed].
        let (top, bottom) = dark(2, WIDE.y);
        close(top, 69.4, 0.1);
        close(bottom, 134.25, 0.1);
        let (top, bottom) = dark(2, TALL.y);
        close(top, 98.7, 0.1);
        close(bottom, 190.9, 0.1);
        // Measured from the lines' letters' middles, 114 units down and one pitch on: as
        // much room at both ends but for 5.5 units, the original's, at any number of lines.
        for (lines, last) in [(1, 114.0), (2, 152.0), (3, 190.0)] {
            let (top, bottom) = dark(lines, HEIGHT_UNITS);
            close((bottom - last) - (114.0 - top), 5.5, 0.01);
        }
    }

    /// The layers let through what the original's one draw does on gamma values (module notes).
    #[test]
    fn the_bands_layers_darken_as_the_original_blends() {
        let through = |alpha: f32| BAND_LAYERS.iter().map(|layer| 1.0 - alpha * layer).product::<f32>();
        // The texture's alpha as it comes out at its middle: 0.262 let through by one draw.
        let middle = through(1.0 - 0.262);
        assert!((0.05..0.065).contains(&middle), "{middle}");
        // Along its soft edge, half the alpha: about the complement to the power 2.2.
        close(through(0.37), 0.63f32.powf(2.2), 0.03);
        assert_eq!(through(0.0), 1.0);
    }

    #[test]
    fn the_tip_wraps_in_the_box_and_is_centred_in_it() {
        let style = TipStyle::FALLBACK;
        let text = "aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff";
        let all = font().width(text);
        // In the tall window the box holds fewer of the wider units' letters.
        let wide = tip_lines(&font(), text, &style, WIDE);
        let tall = tip_lines(&font(), text, &style, TALL);
        assert!(all > tip_place(&style, TALL).width && all < tip_place(&style, WIDE).width, "{all}");
        assert_eq!((wide.len(), tall.len()), (1, 2));
        let place = tip_place(&style, TALL);
        for (line, x, _) in &tall {
            close(*x + line.width / 2.0, place.left + place.width / 2.0, 0.01);
            assert!(line.width <= place.width);
        }
        // Font 6's line under the box's top, the next one pitch on (fonts.md §3.1).
        assert_eq!((tall[0].2, tall[1].2), (122.0, 160.0));
    }

    #[test]
    fn the_style_picks_alignment_and_font() {
        let style = |font, side| TipStyle { font, alignment: side, ..TipStyle::FALLBACK };
        assert_eq!(alignment(&style(6, 1)), Alignment::Left);
        assert_eq!(alignment(&style(6, 2)), Alignment::Center);
        assert_eq!(alignment(&style(6, 4)), Alignment::Right);
        assert_eq!(alignment(&style(6, 3)), Alignment::Center);
        assert_eq!(font_number(&style(7, 2)), 7);
        assert_eq!(font_number(&style(0, 2)), 6);
        assert_eq!(font_number(&style(70, 2)), 6);
    }

    fn square(texture: &str, centre: Vec2, across: f32) -> Piece {
        Piece { node: "n".into(), centre, size: Vec2::splat(across), angle: 0.0, uv: Rect::new(0.0, 0.0, 1.0, 1.0), texture: texture.into() }
    }

    #[test]
    fn the_wheel_stands_at_the_emblems_centre_at_its_share_of_the_height() {
        let wheel = square("textures\\interface\\loading\\load_roulette_wheel.dds", Vec2::new(-80.0, 75.0), 86.0);
        let bars = square("textures\\interface\\loading\\load_roulette_bars.dds", Vec2::new(-6.5, 75.0), 86.0);
        let model = MeterModel { pieces: vec![bars.clone(), wheel.clone()], keys: Vec::new(), length: 1.0, bounds: Rect::new(-200.0, 30.0, 36.0, 120.0) };
        let (anchor, zoom) = anchor_and_zoom(&model);
        assert_eq!(anchor, wheel.centre);
        // The ring about 124 px at 720, 176 at 1024 [observed: 124 and 176].
        close(px(wheel.size.x * EMBLEM_RING * zoom, WIDE.y), 123.84, 0.01);
        close(px(wheel.size.x * EMBLEM_RING * zoom, TALL.y), 176.13, 0.01);
        // The bars, about 200.6 model units from tip to tip on NVForever's frame, keep to
        // the original's 0.42 of the height with no change of the model [observed].
        close(200.6 * zoom / HEIGHT_UNITS, 0.418, 0.002);
        // The window's y 572 and 813 [observed: 0.794 of 720 and 1024].
        close(EMBLEM_CENTRE.y * WIDE.y, 571.7, 0.05);
        close(EMBLEM_CENTRE.y * TALL.y, 813.1, 0.05);
        // In the box: the wheel on the node, a bar to its right, the model's y up.
        assert_eq!(place_in(&wheel, anchor, Vec2::ZERO, zoom).0, Vec2::ZERO);
        assert!(place_in(&bars, anchor, Vec2::ZERO, zoom).0.x > 0.0);
        // Another model: its bounds fill the wheel's place.
        let other = MeterModel { pieces: vec![bars], keys: Vec::new(), length: 1.0, bounds: Rect::new(0.0, 0.0, 200.0, 100.0) };
        let (anchor, zoom) = anchor_and_zoom(&other);
        assert_eq!(anchor, Vec2::new(100.0, 50.0));
        close(200.0 * zoom * EMBLEM_RING, EMBLEM_WHEEL * HEIGHT_UNITS, 0.01);
    }

    /// The nodes' rotation keys as the file has them: a half turn about x, then about z.
    /// The wheel's z goes down, the pointer's up, and on the screen the wheel goes clockwise
    /// and the pointer anticlockwise [observed: frames 0.12 s apart in the original].
    #[test]
    fn the_keys_turn_the_wheel_clockwise_and_the_pointer_the_other_way() {
        let half = Mat3::from_rotation_x(PI);
        let at = |z: f32| {
            let [w, x, y, z] = fnv_nif::anim::euler_xyz_to_quat([PI, 0.0, z]);
            screen_turn(half, Quat::from_xyzw(x, y, z, w), half)
        };
        close(at(0.0), 0.0, 1e-4);
        close(at(-FRAC_PI_2), -FRAC_PI_2, 1e-4);
        close(at(FRAC_PI_2), FRAC_PI_2, 1e-4);
    }

    #[test]
    fn a_piece_goes_round_its_nodes_pivot() {
        let pivot = Vec2::new(-80.0, 75.0);
        // A pointer's piece straight up from the pivot, a quarter turn clockwise: to the right.
        let (centre, angle) = turned(pivot + Vec2::new(0.0, 20.0), 0.5, pivot, -FRAC_PI_2);
        assert!((centre - (pivot + Vec2::new(20.0, 0.0))).length() < 1e-4, "{centre}");
        close(angle, 0.5 - FRAC_PI_2, 1e-6);
        // On the pivot: only its angle changes.
        assert_eq!(turned(pivot, 0.0, pivot, 1.0), (pivot, 1.0));
    }

    #[test]
    fn the_sequence_goes_round_and_a_long_frame_jumps() {
        close(sequence_time(0.0, 0.0, 5.0), 0.0, 1e-6);
        close(sequence_time(1.25, 0.0, 5.0), 1.25, 1e-6);
        close(sequence_time(6.0, 0.0, 5.0), 1.0, 1e-5);
        close(sequence_time(1006.5, 1.0, 5.0), 2.5, 1e-3);
        assert_eq!(sequence_time(3.0, 2.0, 0.0), 2.0);
    }
}
