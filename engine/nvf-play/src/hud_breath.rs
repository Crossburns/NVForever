//! The breath meter (docs/spec-notes/drowning.md §3): `BreathMeter` of
//! `menus\main\hud_main_menu.xml`, the model `Interface\HUD\AirTimer01.NIF` in a box of
//! 64 × 64 in the HUD's colour (`&hudmain;`), its animation `Forward` [documented: the
//! menu file].
//!
//! - **The model** ([`MeterModel::read`]): flat squares cut from
//!   `textures\interface\hud\air_meter.dds`, the word "O2" in the middle and sixteen ticks
//!   round it (`Tick01` to `Tick16`), all in the model's x–z plane, x to the right and z
//!   up [observed: the file; the word reads the right way round so]. Each is drawn as a
//!   picture turned to the square's angle; the model's bounds fill the menu's box
//!   [hypothesis].
//! - **The ticks** ([`MeterModel::lit`]): `Forward` (0 to 0.533 s, 16 frames of 1/30 s)
//!   switches `TickNN` off at frame NN, `Tick01` (just right of the top) first and on round
//!   to `Tick16` (the top) [observed: the vis keys]. The meter shows the sequence at the
//!   time of the breath used: all lit with a full breath, none with none [hypothesis].
//! - **When** ([`shown`]): while the player's head is under water, and after that until
//!   the breath is whole again [hypothesis].
//! - **Where**: the menu file gives no place (the game's code sets it); centred high on the
//!   screen, where the owner placed it ([`CENTRE_Y`]).
//! - **With `[UI] TileMenus=HUDMainMenu`** the tile HUD draws `BreathMeter` from its tile
//!   with this module's reading and pieces (`crate::tiles::model`, hud.md §14.12) and this
//!   meter is hidden.

use crate::drowning::PlayerBreath;
use crate::hud::{HudSet, HudView};
use crate::interface::{amber, bc_supported, texture, u};
// The root is hidden while the HUD is drawn from tiles (`crate::tiles::vanilla::hide_hand_built`).
use crate::tiles::vanilla::BreathRoot as MeterRoot;
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use nvf_assets::GameData;

/// The meter's model [documented: hud_main_menu.xml `BreathMeter`].
pub const MODEL: &str = "meshes\\interface\\hud\\airtimer01.nif";
/// The menu's box for the model, interface units [documented: the menu file's width and
/// height].
pub const BOX: f32 = 64.0;
/// The box's centre: across, the middle of the view; down, interface units from the top:
/// where the game's other breath meter stands, `breath_progress` 72 from the top and 64
/// high, centred across [documented: `menus\breath_meter_menu.xml`, a bar the HUD file does
/// not use], and where the owner placed this one [design: owner 2026-10-25, "needs placing
/// center high", a spot about 90 pixels down an 800-pixel view].
pub const CENTRE_Y: f32 = 72.0 + 32.0;
/// The sequence the meter plays [documented: the menu file's `animation`].
const SEQUENCE: &str = "Forward";

/// A flat piece of the model as drawn: a square of a texture turned on the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// The node it hangs under (`Tick01` …, or `O2`).
    pub node: String,
    /// Its centre, model units (x right, y up).
    pub centre: Vec2,
    /// Its width (along the texture's u) and height (along v), model units.
    pub size: Vec2,
    /// Its turn on the screen, radians anticlockwise.
    pub angle: f32,
    /// The part of the texture, 0 to 1 (min at the top left).
    pub uv: Rect,
    pub texture: String,
}

/// The meter's pieces and when each tick goes off.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeterModel {
    pub pieces: Vec<Piece>,
    /// By node: the sequence's vis keys (time, shown).
    pub keys: Vec<(String, Vec<(f32, bool)>)>,
    /// The sequence's length, seconds.
    pub length: f32,
    /// The pieces' bounds, model units.
    pub bounds: Rect,
}

/// A quad's four corners (model x–z) and texture coordinates as a turned square: its
/// centre, size, angle and texture part. `None` for anything but four corners.
pub fn quad(corners: &[[f32; 2]], uvs: &[[f32; 2]]) -> Option<(Vec2, Vec2, f32, Rect)> {
    if corners.len() != 4 || uvs.len() != 4 {
        return None;
    }
    let p: Vec<Vec2> = corners.iter().map(|c| Vec2::from_array(*c)).collect();
    let t: Vec<Vec2> = uvs.iter().map(|c| Vec2::from_array(*c)).collect();
    let centre = p.iter().copied().sum::<Vec2>() / 4.0;
    let (lo, hi) = (t.iter().copied().fold(Vec2::INFINITY, Vec2::min), t.iter().copied().fold(Vec2::NEG_INFINITY, Vec2::max));
    // Along the texture's u: from the corner of least u to the one of most u with about the
    // same v; the screen's "right" of the picture.
    let corner = |u: f32, v: f32| (0..4).min_by(|&a, &b| ((t[a] - Vec2::new(u, v)).length()).total_cmp(&(t[b] - Vec2::new(u, v)).length()));
    let (tl, tr, bl) = (corner(lo.x, lo.y)?, corner(hi.x, lo.y)?, corner(lo.x, hi.y)?);
    let right = p[tr] - p[tl];
    let down = p[bl] - p[tl];
    Some((centre, Vec2::new(right.length(), down.length()), right.y.atan2(right.x), Rect::from_corners(lo, hi)))
}

/// Whether a node is shown at `t` by its step keys: the last key at or before `t`, else
/// the first.
pub fn shown_at(keys: &[(f32, bool)], t: f32) -> bool {
    keys.iter().rev().find(|(k, _)| *k <= t).or(keys.first()).is_none_or(|(_, v)| *v)
}

/// The sequence time the meter shows for a share of breath left (1 whole, 0 none).
pub fn time_at(left: f32, length: f32) -> f32 {
    (1.0 - left.clamp(0.0, 1.0)) * length
}

/// Whether the meter is up: the head under water, or the breath not yet whole.
pub fn shown(b: &PlayerBreath) -> bool {
    b.under() || b.share() < 1.0
}

impl MeterModel {
    /// Read the meter's model with its sequence [`SEQUENCE`].
    pub fn read(bytes: &[u8], schema: &fnv_nif::Schema) -> Result<MeterModel, String> {
        MeterModel::read_sequence(bytes, schema, Some(SEQUENCE))
    }

    /// Read a model of flat pieces, the vis keys of the sequence `sequence` names (an error
    /// when the model has no such sequence; none without a name). The tile HUD reads a
    /// `nif` tile's model so (`crate::tiles::model`).
    pub fn read_sequence(bytes: &[u8], schema: &fnv_nif::Schema, sequence: Option<&str>) -> Result<MeterModel, String> {
        use fnv_nif::scene::{mesh_data, scene_objects, Decoded};
        let f = fnv_nif::NifFile::parse(bytes, Some(schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&f, bytes, schema);
        let objects = scene_objects(&f, &dec);
        let by_block: std::collections::HashMap<usize, usize> = objects.iter().enumerate().map(|(i, o)| (o.block, i)).collect();
        let mut pieces = Vec::new();
        for (i, o) in objects.iter().enumerate().filter(|(_, o)| !o.is_node) {
            let Some(shape) = dec.get(o.block as i32) else { continue };
            let Some(data) = shape.block_ref("data").and_then(|d| dec.get(d)) else { continue };
            let mesh = mesh_data(data);
            let Some(uvs) = mesh.uv_sets.first() else { continue };
            let (m, t) = to_root(&objects, &by_block, i);
            let corners: Vec<[f32; 2]> = mesh.positions.iter().map(|p| m * Vec3::from_array(*p) + t).map(|w| [w.x, w.z]).collect();
            let Some((centre, size, angle, uv)) = quad(&corners, uvs) else { continue };
            let Some(texture) = shape_texture(shape, &dec) else { continue };
            let node = o.parent.and_then(|p| by_block.get(&p)).and_then(|&p| objects[p].name.clone()).unwrap_or_default();
            pieces.push(Piece { node, centre, size, angle, uv, texture });
        }
        if pieces.is_empty() {
            return Err("no flat pieces".into());
        }
        let (keys, length) = match sequence {
            Some(name) => {
                let seq = fnv_nif::anim::sequences(&f, &dec).into_iter().find(|s| s.name.as_deref() == Some(name)).ok_or_else(|| format!("no sequence {name}"))?;
                let keys = seq.tracks.iter().filter_map(|t| Some((t.node_name.clone()?, t.bools.as_ref()?.keys.clone()))).collect();
                (keys, seq.stop_time - seq.start_time)
            }
            None => (Vec::new(), 0.0),
        };
        let mut bounds = Rect::EMPTY;
        for p in &pieces {
            let half = Vec2::new(p.size.x * p.angle.cos().abs() + p.size.y * p.angle.sin().abs(), p.size.x * p.angle.sin().abs() + p.size.y * p.angle.cos().abs()) / 2.0;
            bounds = bounds.union(Rect::from_center_half_size(p.centre, half));
        }
        Ok(MeterModel { pieces, keys, length, bounds })
    }

    /// Whether `node` is lit at the sequence's time `t` (a node without keys always is).
    pub fn lit(&self, node: &str, t: f32) -> bool {
        self.keys.iter().find(|(n, _)| n == node).is_none_or(|(_, k)| shown_at(k, t))
    }
}

/// Object `i`'s transform to the model's root: v' = R v · s + t, rows stored.
pub(crate) fn to_root(objects: &[fnv_nif::scene::SceneObject], by_block: &std::collections::HashMap<usize, usize>, mut i: usize) -> (Mat3, Vec3) {
    let mut m = Mat3::IDENTITY;
    let mut t = Vec3::ZERO;
    loop {
        let o = &objects[i];
        let r = Mat3::from_cols_array(&o.transform.rotation).transpose() * o.transform.scale;
        t = r * t + Vec3::from_array(o.transform.translation);
        m = r * m;
        match o.parent.and_then(|p| by_block.get(&p)) {
            Some(&p) => i = p,
            None => return (m, t),
        }
    }
}

/// The texture a shape's properties name.
pub(crate) fn shape_texture(shape: &fnv_nif::Struct, dec: &fnv_nif::scene::Decoded) -> Option<String> {
    shape.refs("properties").unwrap_or_default().iter().filter_map(|&p| dec.get(p)).find_map(|p| p.text("file_name").filter(|s| !s.is_empty()).map(str::to_string))
}

/// A piece's centre and size in a box, box units with y down: the model's point `origin`
/// (model units) at `at`, `zoom` box units a model unit, the model's z up the box's y down.
pub fn place_in(piece: &Piece, origin: Vec2, at: Vec2, zoom: f32) -> (Vec2, Vec2) {
    (Vec2::new(piece.centre.x - origin.x, origin.y - piece.centre.y) * zoom + at, piece.size * zoom)
}

/// Reads a texture by its game path: the image and its size in texels.
pub(crate) type TextureRead<'a> = dyn FnMut(&str) -> Option<(Handle<Image>, Vec2)> + 'a;

/// Spawn the model's pieces under `p`: each a picture of its texture's part turned to its
/// angle, `place` giving its centre and size in the box ([`place_in`]), drawn in `colour`,
/// shown as `shown` says and carrying `mark`. `texture` reads a texture by its game path
/// (with its size in texels); a piece whose texture is not read is left out. Shared by the
/// hand-built meter and the tile HUD's `nif` tiles (`crate::tiles::model`).
pub(crate) fn spawn_pieces<M: Component>(
    p: &mut ChildSpawnerCommands,
    model: &MeterModel,
    place: impl Fn(&Piece) -> (Vec2, Vec2),
    colour: Color,
    texture: &mut TextureRead<'_>,
    mark: impl Fn(&Piece) -> M,
    shown: impl Fn(&Piece) -> bool,
) {
    for piece in &model.pieces {
        let Some((image, texels)) = texture(&piece.texture) else { continue };
        let (centre, size) = place(piece);
        let node = Node {
            position_type: PositionType::Absolute,
            left: u(centre.x - size.x / 2.0),
            top: u(centre.y - size.y / 2.0),
            width: u(size.x),
            height: u(size.y),
            ..default()
        };
        let rect = Rect::from_corners(piece.uv.min * texels, piece.uv.max * texels);
        let image = ImageNode { rect: Some(rect), color: colour, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) };
        let vis = if shown(piece) { Visibility::Inherited } else { Visibility::Hidden };
        // The screen's y runs down, so the model's anticlockwise turn is clockwise here.
        p.spawn((mark(piece), node, image, vis, UiTransform::from_rotation(Rot2::radians(-piece.angle)), Pickable::IGNORE));
    }
}

pub struct HudBreathPlugin;

impl Plugin for HudBreathPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw.after(HudSet).in_set(PlaySet::Ui));
    }
}

#[derive(Component)]
struct MeterPiece(String);

/// What was read: the model, or why there is none (then the meter is not drawn).
enum Read {
    Model(MeterModel),
    Missing,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    view: Res<HudView>,
    breath: Option<Res<PlayerBreath>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut read: Local<Option<Read>>,
    mut root: Query<&mut Visibility, (With<MeterRoot>, Without<MeterPiece>)>,
    mut pieces: Query<(&MeterPiece, &mut Visibility), Without<MeterRoot>>,
    mut last: Local<Option<usize>>,
) {
    // The HUD's lengths at the UI scale (`crate::interface::hud_units`, #1631).
    let _hud = crate::interface::hud_units();
    let Some(b) = breath else { return };
    let wanted = view.shown && shown(&b);
    if read.is_none() {
        if !wanted {
            return;
        }
        let Some(data) = data.as_deref() else { return };
        let model = data.read(MODEL).ok_or_else(|| "not found".to_string()).and_then(|bytes| {
            nvf_assets::guard::guarded(|| MeterModel::read(&bytes, &data.schema))
        });
        let model = match model {
            Ok(m) => m,
            Err(e) => {
                warn!("hud: no breath meter: {MODEL}: {e}");
                *read = Some(Read::Missing);
                return;
            }
        };
        let bc = bc_supported(formats.as_deref());
        // The model's bounds in the box, its centre at the box's.
        let zoom = BOX / model.bounds.width().max(model.bounds.height()).max(1e-3);
        let mid = model.bounds.center();
        let root_node = Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            margin: UiRect::left(u(-BOX / 2.0)),
            top: u(CENTRE_Y - BOX / 2.0),
            width: u(BOX),
            height: u(BOX),
            ..default()
        };
        let mut textures: Vec<(String, Option<(Handle<Image>, Vec2)>)> = Vec::new();
        let mut read_texture = |path: &str| match textures.iter().find(|(n, _)| n == path) {
            Some((_, t)) => t.clone(),
            None => {
                let t = texture(path, data, &mut images, bc);
                textures.push((path.to_string(), t.clone()));
                t
            }
        };
        // Tagged so the tile HUD hides it while it draws `BreathMeter` itself.
        commands.spawn((MeterRoot, root_node, Visibility::Hidden, Pickable::IGNORE)).with_children(|p| {
            let place = |piece: &Piece| place_in(piece, mid, Vec2::splat(BOX / 2.0), zoom);
            spawn_pieces(p, &model, place, amber(255.0), &mut read_texture, |piece| MeterPiece(piece.node.clone()), |_| true);
        });
        info!("hud: breath meter read: {} pieces, sequence {:.3} s", model.pieces.len(), model.length);
        *read = Some(Read::Model(model));
        return;
    }
    let Some(Read::Model(model)) = read.as_ref() else { return };
    let show = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        v.set_if_neq(show(wanted));
    }
    if !wanted {
        *last = None;
        return;
    }
    let t = time_at(b.share(), model.length);
    let mut lit = 0;
    for (piece, mut vis) in &mut pieces {
        let on = model.lit(&piece.0, t);
        lit += usize::from(on && piece.0.starts_with("Tick"));
        vis.set_if_neq(show(on));
    }
    if *last != Some(lit) {
        debug!("hud: breath meter {lit} ticks lit (breath {:.1} of {:.1} s)", b.breath.left, b.rules.seconds);
        *last = Some(lit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_keeps_its_size_and_turn() {
        // The model's O2 square: u grows with x, v down the z axis [observed].
        let corners = [[19.6, -19.4], [19.6, 15.8], [-12.6, -19.4], [-12.6, 15.8]];
        let uvs = [[0.847, 0.566], [0.847, 0.058], [0.382, 0.566], [0.382, 0.058]];
        let (c, size, angle, uv) = quad(&corners, &uvs).expect("quad");
        assert!((c - Vec2::new(3.5, -1.8)).length() < 1e-3);
        assert!((size - Vec2::new(32.2, 35.2)).length() < 1e-3);
        assert!(angle.abs() < 1e-4);
        assert!((uv.min - Vec2::new(0.382, 0.058)).length() < 1e-6);
        // Tick02, turned 45° clockwise on the screen (right of the top).
        let corners = [[15.8, 6.6], [29.9, 20.7], [6.9, 15.6], [21.0, 29.6]];
        let uvs = [[0.292, 0.413], [0.292, 0.082], [0.082, 0.413], [0.082, 0.082]];
        let (_, size, angle, _) = quad(&corners, &uvs).expect("quad");
        assert!((angle.to_degrees() + 45.0).abs() < 1.0, "{}", angle.to_degrees());
        assert!((size.x - 12.66).abs() < 0.1 && (size.y - 19.9).abs() < 0.1, "{size}");
    }

    #[test]
    fn ticks_go_out_with_the_breath() {
        // Tick01's keys [observed]: on, off at frame 1.
        let k = [(0.0, true), (1.0 / 30.0, false), (0.533, false)];
        let len = 16.0 / 30.0;
        assert!(shown_at(&k, time_at(1.0, len)));
        assert!(!shown_at(&k, time_at(15.0 / 16.0, len)));
        // Tick16: on until frame 16, the end.
        let k = [(0.0, true), (0.5, true), (16.0 / 30.0, false)];
        assert!(shown_at(&k, time_at(0.01, len)));
        assert!(!shown_at(&k, time_at(0.0, len)));
    }
}
