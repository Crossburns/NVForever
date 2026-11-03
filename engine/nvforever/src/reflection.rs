//! World reflections in water: a second camera, the main camera mirrored in the water
//! plane nearest the camera, renders the scene above that plane (an oblique near clip
//! plane cuts away everything below it) into a texture; the water shader samples it at
//! the fragment's screen position, offset by the wave normal. The same technique as
//! Bevy's `mirror` example. Water itself lives on [`WATER_LAYER`], which the reflection
//! camera does not draw.
//!
//! One plane at a time [hypothesis: enough for the Mojave, where lakes do not overlap
//! in height on screen]: water at another height reflects only the sky (water.wesl).
//!
//! Only when water is seen: the camera renders while water lies within [`SEARCH`] cells
//! and some water at the reflected height lay inside the main camera's view within the
//! last [`KEEP_UNSEEN`] seconds ([`water_on_screen`]: the wet part of each cell and placed
//! water objects, clipped against the view volume in the same frame). It draws the whole
//! scene again, and Bevy gives every active camera its own set of the sun's shadow
//! cascades, so it cost about as much as the main view's shadows: in Primm by day, with
//! water two cells off and out of sight, 19.5 ms a frame with it against 12.5 without
//! (RTX 4070, 1080p, fine shadows, 8x MSAA; 2026-10-30) [observed]. Bevy's own view
//! visibility of the water meshes would not do: its box test passes a 4,096-unit plane
//! that is nowhere in view, and every cell's plane, buried ones included, has one. Water
//! inside the view but hidden behind a hill or a building still counts [known gap].
//! Water that is not drawn does not count (#1059): a placed water object not visible (a
//! disabled reference, a hidden model) and any water while the console hides water.
//!
//! Nor does water hidden behind the scene (#1060): the water shader counts the pixels it
//! draws at the reflected height that no opaque surface hides (water.rs
//! `WaterMaterial::seen`), the count is read back from the GPU a few frames later
//! ([`WaterPixels`]), and water inside the view counts only while that count grew. In Primm
//! facing south-west (cell -16,-15's water behind the walls) the camera stays off. Turning
//! to water, the camera comes on a few frames after the water shows, which reflects only
//! the sky until then; the texture it shows is drawn in the frame it comes on (it renders
//! before the main camera, order -1), never an old one.
//!
//! Switched off by the `water-mirror` setting (graphics.rs `Graphics::water_mirror`;
//! #1380): water then reflects the sky, and the screen where screen-space reflections are
//! on.
//! At Hoover Dam by day (Lake Mead in view) the camera costs about 0.5 ms a frame, where
//! the CPU sets the frame: 7.6 ms with it, 7.1 without [observed 2026-11-02, #1380]. Its
//! own set of the sun's cascades stays: Bevy's renderer needs cascades for every view the
//! sun lights, and emptying their caster lists saved 0.12 ms of shadow-pass GPU time and
//! nothing of the frame there, so the reflected world keeps its shadows.

use crate::water::{DistantWater, WaterCache, WaterMaterial};
use crate::world::{bevy_to_game, Streamer, WorldData};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::camera::{Hdr, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::SimulationLightSystems;
use bevy::math::reflection_matrix;
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::transform::TransformSystems;
use bevy::window::{PrimaryWindow, WindowResized};
use fnv_world::cell_of;
use nvf_play::RefVisual;
use bevy::camera::primitives::Aabb;

/// Render layer of water surfaces: seen by the main camera, not by the reflection camera
/// (which would otherwise sample the texture it renders into).
pub const WATER_LAYER: usize = 1;

/// The reflection texture's size relative to the window.
const SCALE: u32 = 2;

/// Cells (in grid steps from the camera's cell) searched for the water to reflect in.
const SEARCH: i32 = 2;

/// Seconds the camera keeps rendering after water was last on screen, so turning
/// along the edge of a lake does not switch it on and off [engineering choice].
const KEEP_UNSEEN: f32 = 1.0;

/// Below the water plane by this much, so the water surface itself is clipped away.
const CLIP_MARGIN: f32 = 1.0;

#[derive(Component)]
pub struct ReflectionCamera;

#[derive(Resource)]
pub struct Reflection {
    pub image: Handle<Image>,
}

pub struct ReflectionPlugin;

impl Plugin for ReflectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaterPixels>().add_systems(Startup, setup_reflection).add_systems(
            PostUpdate,
            (resize_reflection, read_water_pixels, update_reflection)
                .chain()
                .after(TransformSystems::Propagate)
                .before(VisibilitySystems::UpdateFrusta)
                // Cascades are built only for active cameras: switch on before that, or
                // the renderer finds an active view without sun shadow cascades.
                .before(SimulationLightSystems::UpdateDirectionalLightCascades),
        );
    }
}

fn texture_size(window: &Window) -> Extent3d {
    Extent3d {
        width: (window.physical_width() / SCALE).max(1),
        height: (window.physical_height() / SCALE).max(1),
        depth_or_array_layers: 1,
    }
}

fn setup_reflection(mut commands: Commands, mut images: ResMut<Assets<Image>>, window: Query<&Window, With<PrimaryWindow>>) {
    let size = window.single().map(texture_size).unwrap_or(Extent3d { width: 640, height: 400, depth_or_array_layers: 1 });
    // Linear HDR, not tonemapped: the water shader mixes it with lit colours and
    // tonemaps the result.
    let mut image = Image::new_uninit(
        size,
        TextureDimension::D2,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    commands.spawn((
        Name::new("Reflection camera"),
        ReflectionCamera,
        Camera3d::default(),
        Camera { order: -1, invert_culling: true, is_active: false, ..default() },
        RenderTarget::Image(image.clone().into()),
        Hdr,
        Tonemapping::Linear,
        Msaa::Off,
        Projection::Perspective(PerspectiveProjection::default()),
        crate::sky::initial_fog(),
        // The world without the water; the player's first-person arms and seen body as
        // before they had layers of their own.
        RenderLayers::from_layers(&[0, crate::sky::ARMS_LAYER, crate::sky::BODY_LAYER]),
    ));
    commands.insert_resource(Reflection { image });
}

fn resize_reflection(
    mut resized: MessageReader<WindowResized>,
    window: Query<&Window, With<PrimaryWindow>>,
    reflection: Option<Res<Reflection>>,
    mut images: ResMut<Assets<Image>>,
) {
    if resized.read().last().is_none() {
        return;
    }
    let (Ok(window), Some(r)) = (window.single(), reflection) else { return };
    let size = texture_size(window);
    if let Some(mut img) = images.get_mut(&r.image) {
        if img.texture_descriptor.size != size {
            img.resize(size);
        }
    }
}

/// What the GPU's count of drawn water pixels (#1060, module notes) last said.
#[derive(Resource, Default, Debug)]
pub struct WaterPixels {
    /// The counter's last value read back.
    last: Option<u32>,
    /// Whether it grew between the last two readbacks: water at the reflected height was
    /// drawn, not hidden by the scene. `None` before two readbacks.
    drawn: Option<bool>,
    /// When the readback was asked for (real seconds), once it has been.
    started: Option<f32>,
}

/// Seconds without any readback after which the GPU check is taken to be broken and
/// left out (#1047's view check alone) [engineering choice].
const READBACK_WAIT: f32 = 2.0;

impl WaterPixels {
    /// Take in a value read back.
    fn read(&mut self, v: u32) {
        if let Some(l) = self.last {
            self.drawn = Some(v != l);
        }
        self.last = Some(v);
    }

    /// Whether water may be on screen as far as the GPU knows, at real time `now`. Before
    /// two readbacks: not yet (no camera for the frames the world fades in), unless none
    /// came for [`READBACK_WAIT`] seconds.
    fn may_be_drawn(&self, now: f32) -> bool {
        self.drawn.unwrap_or_else(|| self.started.is_some_and(|t| now - t > READBACK_WAIT))
    }
}

/// Ask for the water-pixel counter to be read back every frame, once a water material
/// made it.
fn read_water_pixels(mut commands: Commands, cache: Res<WaterCache>, mut px: ResMut<WaterPixels>, time: Res<Time<Real>>) {
    if px.started.is_some() {
        return;
    }
    let Some(counter) = cache.seen_counter() else { return };
    px.started = Some(time.elapsed_secs());
    commands.spawn((Name::new("Water pixel readback"), Readback::buffer(counter.clone()))).observe(
        |ev: On<ReadbackComplete>, mut px: ResMut<WaterPixels>| {
            if ev.data.len() >= 4 {
                px.read(u32::from_le_bytes([ev.data[0], ev.data[1], ev.data[2], ev.data[3]]));
            }
        },
    );
}

/// Placed water objects (models of a placed reference with the water material): where
/// they are, their bounds and whether they are drawn.
type PlacedWater<'w, 's> = Query<
    'w,
    's,
    (&'static GlobalTransform, &'static Aabb, &'static RefVisual, &'static InheritedVisibility),
    (With<MeshMaterial3d<WaterMaterial>>, Without<ReflectionCamera>),
>;

/// What put water on screen (module notes, "Only when water is seen").
#[derive(Debug, Clone, Copy, PartialEq)]
enum SeenWater {
    /// A cell's water surface (its grid position).
    Cell((i32, i32)),
    /// A placed water object (its reference's FormID).
    Placed(u32),
}

/// Height tolerance (game units) for water counting as at the reflected plane's height.
const SAME_HEIGHT: f32 = 1.0;

/// The first water at height `h` (game z) that some part of lies inside the main
/// camera's view (`clip_from_world`, Bevy space): the part of every loaded cell and every
/// distant-water cell where the water stands above the land ([`DistantWater::wet_area`],
/// so planes under the ground never count), and placed water
/// objects whose bounds reach that height. Water at another height reflects only the sky
/// and does not need the camera. What hides it from view (a hill, a wall) is not taken
/// into account.
fn water_on_screen(
    world: &WorldData,
    streamer: &Streamer,
    dw: &mut DistantWater,
    placed: &PlacedWater,
    clip_from_world: Mat4,
    h: f32,
    cam: Vec3,
) -> Option<SeenWater> {
    // Nearest first, so the water named is the same every run (the maps' order is not).
    let g = bevy_to_game(cam);
    let cells = nearest_first(streamer.loaded.keys().copied().chain(dw.plane_cells()), cell_of(g.x, g.y));
    for c in cells {
        let Some((height, _)) = dw.surface(world, c) else { continue };
        if (height - h).abs() > SAME_HEIGHT {
            continue;
        }
        let Some(wet) = dw.wet_area(world, c) else { continue };
        if polygon_in_view(clip_from_world, &wet_quad(wet, height)) {
            return Some(SeenWater::Cell(c));
        }
    }
    placed_water_on_screen(placed.iter().map(|(t, aabb, r, v)| (t.affine(), aabb, r.0, v.get())), clip_from_world, h, cam)
}

/// The nearest placed water object (to `cam`, Bevy space; the lower FormID on a tie) that
/// is drawn, reaches height `h` and lies partly inside the view: each as its transform,
/// bounds, reference and whether it is visible (an object hidden or on a disabled
/// reference never counts, #1059).
fn placed_water_on_screen<'a>(
    placed: impl Iterator<Item = (bevy::math::Affine3A, &'a Aabb, u32, bool)>,
    clip_from_world: Mat4,
    h: f32,
    cam: Vec3,
) -> Option<SeenWater> {
    let mut best: Option<(f32, u32)> = None;
    for (m, aabb, r, visible) in placed {
        if !visible {
            continue;
        }
        let (c, e) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        let corners: [Vec3; 8] = std::array::from_fn(|i| {
            let s = Vec3::new(
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { -1.0 } else { 1.0 },
                if i & 4 == 0 { -1.0 } else { 1.0 },
            );
            m.transform_point3(c + e * s)
        });
        // Bevy y is game z.
        let (lo, hi) = corners.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        if h < lo - SAME_HEIGHT || h > hi + SAME_HEIGHT {
            continue;
        }
        if box_in_view(clip_from_world, &corners) {
            // The nearest one seen, the lower FormID on a tie.
            let d = m.transform_point3(c).distance_squared(cam);
            if best.is_none_or(|(bd, bf)| (d, r) < (bd, bf)) {
                best = Some((d, r));
            }
        }
    }
    best.map(|(_, f)| SeenWater::Placed(f))
}

/// `cells` without repeats, nearest `centre` first (by grid steps both ways, then the
/// cell's own coordinates), so the order does not depend on the maps they came from.
fn nearest_first(cells: impl Iterator<Item = (i32, i32)>, centre: (i32, i32)) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = cells.collect();
    v.sort_unstable_by_key(|&(x, y)| ((x - centre.0).abs().max((y - centre.1).abs()), (x - centre.0).abs() + (y - centre.1).abs(), x, y));
    v.dedup();
    v
}

/// The wet part of a cell (`[x0, y0, x1, y1]`, game units) at game height `height`, its
/// corners in Bevy space.
fn wet_quad(wet: [f32; 4], height: f32) -> [Vec3; 4] {
    let [x0, y0, x1, y1] = wet;
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| game_point(Vec3::new(x, y, height)))
}

/// A game-space point in Bevy space (the inverse of [`bevy_to_game`]).
fn game_point(g: Vec3) -> Vec3 {
    Vec3::new(g.x, g.z, -g.y)
}

/// The sides of Bevy's view volume in clip space, each as `n · p >= 0`: left, right,
/// bottom, top, near (`z <= w`; reverse z puts the near plane at 1) and far (`z >= 0`;
/// the far plane is at infinity).
const VIEW_SIDES: [Vec4; 6] = [
    Vec4::new(1.0, 0.0, 0.0, 1.0),
    Vec4::new(-1.0, 0.0, 0.0, 1.0),
    Vec4::new(0.0, 1.0, 0.0, 1.0),
    Vec4::new(0.0, -1.0, 0.0, 1.0),
    Vec4::new(0.0, 0.0, -1.0, 1.0),
    Vec4::new(0.0, 0.0, 1.0, 0.0),
];

/// Whether any part of the convex polygon `points` (world space) lies inside the view:
/// the polygon clipped against each side of the view volume in clip space
/// (Sutherland-Hodgman) keeps something.
fn polygon_in_view(clip_from_world: Mat4, points: &[Vec3]) -> bool {
    let mut poly: Vec<Vec4> = points.iter().map(|p| clip_from_world * p.extend(1.0)).collect();
    for side in VIEW_SIDES {
        if poly.is_empty() {
            return false;
        }
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (da, db) = (side.dot(a), side.dot(b));
            if da >= 0.0 {
                out.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                out.push(a + (b - a) * (da / (da - db)));
            }
        }
        poly = out;
    }
    !poly.is_empty()
}

/// Whether any part of the box with these 8 corners (bit 0: its first axis, bit 1: its
/// second, bit 2: its third) lies inside the view: one of its faces does, or the camera
/// is inside it.
fn box_in_view(clip_from_world: Mat4, corners: &[Vec3; 8]) -> bool {
    const FACES: [[usize; 4]; 6] = [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 3, 7, 6], [0, 2, 6, 4], [1, 3, 7, 5]];
    if FACES.iter().any(|f| polygon_in_view(clip_from_world, &f.map(|i| corners[i]))) {
        return true;
    }
    // The camera inside the box (swimming in it): every face may lie outside the view.
    let eye = clip_from_world.inverse().project_point3(Vec3::new(0.0, 0.0, 1.0));
    let (o, ex, ey, ez) = (corners[0], corners[1] - corners[0], corners[2] - corners[0], corners[4] - corners[0]);
    let d = eye - o;
    [ex, ey, ez].iter().all(|axis| {
        let t = d.dot(*axis);
        t >= 0.0 && t <= axis.length_squared()
    })
}

/// The water surface (Bevy y) to reflect in: the nearest cell around the camera with
/// visible water below the camera.
fn reflection_height(world: &WorldData, dw: &mut DistantWater, cam_game: Vec3) -> Option<f32> {
    let c = cell_of(cam_game.x, cam_game.y);
    (0..=SEARCH).find_map(|r| {
        (-r..=r)
            .flat_map(|dx| (-r..=r).map(move |dy| (dx, dy)))
            .filter(|&(dx, dy)| dx.abs().max(dy.abs()) == r)
            .filter_map(|(dx, dy)| dw.surface(world, (c.0 + dx, c.1 + dy)))
            .map(|(h, _)| h)
            .filter(|&h| h < cam_game.z)
            .min_by(|a, b| (cam_game.z - a).total_cmp(&(cam_game.z - b)))
    })
}

/// Test aid `NVF_PLAY_TEST_NO_REFLECTION=1`: the camera stays off (water reflects the sky),
/// for measuring what it costs where water is seen (docs/design/frame-rate-audit.md §1).
fn no_reflection() -> bool {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| std::env::var("NVF_PLAY_TEST_NO_REFLECTION").is_ok_and(|v| v == "1"))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_reflection(
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    mut dw: ResMut<DistantWater>,
    main: Query<(&GlobalTransform, &Projection, &DistanceFog), (With<Camera3d>, Without<ReflectionCamera>)>,
    mut refl: Query<
        (&mut Camera, &mut Transform, &mut GlobalTransform, &mut Projection, &mut DistanceFog),
        With<ReflectionCamera>,
    >,
    cache: Res<WaterCache>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    graphics: Option<Res<crate::graphics::Graphics>>,
    placed: PlacedWater,
    time: Res<Time<Real>>,
    hidden: Option<Res<crate::console_toggles::Hidden>>,
    pixels: Res<WaterPixels>,
    mut unseen_for: Local<f32>,
) {
    let (Ok((main_gt, main_proj, main_fog)), Ok((mut camera, mut t, mut gt, mut proj, mut fog))) = (main.single(), refl.single_mut())
    else {
        return;
    };
    let cam = main_gt.translation();
    let mirror = graphics.as_ref().is_none_or(|g| g.water_mirror);
    let near_water = match (streamer.interior, main_proj) {
        (None, Projection::Perspective(_)) if mirror => reflection_height(&world, &mut dw, bevy_to_game(cam)),
        _ => None,
    };
    // Only while water is on screen (module notes, "Only when water is seen"), and never
    // while the console hides water (#1059).
    let water_drawn = !hidden.is_some_and(|h| h.water);
    // The shader counts the water at the reflected height (#1060).
    cache.set_seen_plane(&mut materials, near_water.map_or(Vec4::ZERO, |h| Vec4::new(h, 1.0, 0.0, 0.0)));
    let seen = near_water.filter(|_| water_drawn && pixels.may_be_drawn(time.elapsed_secs())).and_then(|h| {
        let clip_from_world = main_proj.get_clip_from_view() * main_gt.to_matrix().inverse();
        water_on_screen(&world, &streamer, &mut dw, &placed, clip_from_world, h, cam)
    });
    if seen.is_some() {
        *unseen_for = 0.0;
    } else {
        *unseen_for += time.delta_secs();
    }
    let height = near_water.filter(|_| seen.is_some() || (camera.is_active && *unseen_for <= KEEP_UNSEEN));
    let height = height.filter(|_| !no_reflection());
    // z: screen-space reflections stand in where this plane is not (graphics.rs).
    let ssr = if graphics.is_some_and(|g| g.water_reflections) { 1.0 } else { 0.0 };
    let plane = match height {
        Some(h) => Vec4::new(h, 1.0, ssr, 0.0),
        None => Vec4::new(0.0, 0.0, ssr, 0.0),
    };
    cache.set_reflection_plane(&mut materials, plane);
    if camera.is_active != height.is_some() {
        camera.is_active = height.is_some();
        match (height, seen) {
            (Some(_), Some(SeenWater::Cell(c))) => info!("water reflection: on (the water of cell {},{} on screen)", c.0, c.1),
            (Some(_), Some(SeenWater::Placed(r))) => info!("water reflection: on (placed water {r:08X} on screen)"),
            (Some(_), None) => {}
            (None, _) if !mirror => info!("water reflection: off (mirrored water switched off)"),
            (None, _) if near_water.is_some() => info!("water reflection: off (no water on screen for {KEEP_UNSEEN} s)"),
            (None, _) => info!("water reflection: off (no water near)"),
        }
    }
    let (Some(h), Projection::Perspective(p)) = (height, main_proj) else { return };

    // Mirror the camera in the plane y = h.
    let mirror = Mat4::from_translation(Vec3::Y * h)
        * Mat4::from_mat3a(reflection_matrix(Vec3::Y))
        * Mat4::from_translation(Vec3::NEG_Y * h);
    *t = Transform::from_matrix(mirror * main_gt.to_matrix());
    *gt = GlobalTransform::from(*t);
    // Keep only what lies above the plane: in the main camera's view space the mirrored
    // scene must lie below it (see Bevy's mirror example).
    let normal = (main_gt.compute_transform().rotation.inverse() * Vec3::NEG_Y).normalize();
    *proj = Projection::Perspective(PerspectiveProjection {
        near_clip_plane: normal.extend(h - CLIP_MARGIN - cam.y),
        ..p.clone()
    });
    *fog = main_fog.clone();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clip-from-world matrix of a camera at `eye` (Bevy space) turned `pitch` degrees
    /// up from looking along -z, with the game's 75° view and Bevy's reverse-z projection.
    fn view(eye: Vec3, pitch: f32) -> Mat4 {
        let proj = PerspectiveProjection { fov: 75f32.to_radians(), aspect_ratio: 16.0 / 9.0, near: 5.0, ..default() };
        let world_from_view = Transform::from_translation(eye).with_rotation(Quat::from_rotation_x(pitch.to_radians())).to_matrix();
        Projection::Perspective(proj).get_clip_from_view() * world_from_view.inverse()
    }

    /// A horizontal square of half-width `r` at Bevy height `y`, centred under `(x, z)`.
    fn square(x: f32, z: f32, y: f32, r: f32) -> [Vec3; 4] {
        [Vec3::new(x - r, y, z - r), Vec3::new(x + r, y, z - r), Vec3::new(x + r, y, z + r), Vec3::new(x - r, y, z + r)]
    }

    #[test]
    fn water_ahead_and_below_is_seen() {
        let v = view(Vec3::new(0.0, 100.0, 0.0), -10.0);
        assert!(polygon_in_view(v, &square(0.0, -3000.0, 0.0, 500.0)));
    }

    #[test]
    fn water_behind_is_not_seen() {
        let v = view(Vec3::new(0.0, 100.0, 0.0), -10.0);
        assert!(!polygon_in_view(v, &square(0.0, 3000.0, 0.0, 500.0)));
    }

    #[test]
    fn water_off_to_the_side_is_not_seen() {
        let v = view(Vec3::new(0.0, 100.0, 0.0), 0.0);
        assert!(!polygon_in_view(v, &square(6000.0, 0.0, 0.0, 500.0)));
    }

    #[test]
    fn a_huge_lake_under_a_camera_looking_at_the_sky_is_not_seen() {
        // Bevy's bounding-box test passes such a plane; the clip test does not.
        let v = view(Vec3::new(0.0, 100.0, 0.0), 70.0);
        assert!(!polygon_in_view(v, &square(0.0, 0.0, 0.0, 20_000.0)));
        // Looking level, the far part of the same lake is on screen below the horizon.
        assert!(polygon_in_view(view(Vec3::new(0.0, 100.0, 0.0), 0.0), &square(0.0, 0.0, 0.0, 20_000.0)));
    }

    #[test]
    fn water_crossing_the_near_plane_is_seen() {
        // Half the square lies behind the camera (negative w), the rest in view.
        let v = view(Vec3::new(0.0, 10.0, 0.0), -60.0);
        assert!(polygon_in_view(v, &square(0.0, 0.0, 0.0, 50.0)));
    }

    #[test]
    fn water_above_the_camera_is_seen_only_looking_up() {
        let s = square(0.0, -500.0, 300.0, 400.0);
        assert!(polygon_in_view(view(Vec3::ZERO, 45.0), &s));
        assert!(!polygon_in_view(view(Vec3::ZERO, -60.0), &s));
    }

    #[test]
    fn a_box_around_the_camera_counts_as_seen() {
        let corners: [Vec3; 8] = std::array::from_fn(|i| {
            Vec3::new(if i & 1 == 0 { -1000.0 } else { 1000.0 }, if i & 2 == 0 { -1000.0 } else { 1000.0 }, if i & 4 == 0 { -1000.0 } else { 1000.0 })
        });
        assert!(box_in_view(view(Vec3::ZERO, 0.0), &corners));
        let far: [Vec3; 8] = corners.map(|c| c * 0.1 + Vec3::new(0.0, 0.0, 9000.0));
        assert!(!box_in_view(view(Vec3::ZERO, 0.0), &far));
    }

    #[test]
    fn cells_come_nearest_first_whatever_their_order() {
        let a = nearest_first([(5, 5), (0, 1), (2, 0), (0, 0), (1, 1), (0, 1)].into_iter(), (0, 0));
        let b = nearest_first([(1, 1), (0, 1), (5, 5), (2, 0), (0, 0)].into_iter(), (0, 0));
        assert_eq!(a, vec![(0, 0), (0, 1), (1, 1), (2, 0), (5, 5)]);
        assert_eq!(a, b);
    }

    /// #1059: placed water that is not drawn (a disabled reference, a hidden model) never
    /// switches the camera on; the same object drawn does. Of two drawn, the nearer one is
    /// named (eb618878's choice).
    #[test]
    fn hidden_placed_water_is_not_seen() {
        let eye = Vec3::new(0.0, 100.0, 0.0);
        let v = view(eye, -10.0);
        let aabb = Aabb::from_min_max(Vec3::new(-500.0, -10.0, -500.0), Vec3::new(500.0, 0.0, 500.0));
        let near = bevy::math::Affine3A::from_translation(Vec3::new(0.0, 0.0, -3000.0));
        let far = bevy::math::Affine3A::from_translation(Vec3::new(0.0, 0.0, -6000.0));
        let one = |visible| placed_water_on_screen(std::iter::once((near, &aabb, 0x1234, visible)), v, 0.0, eye);
        assert_eq!(one(true), Some(SeenWater::Placed(0x1234)));
        assert_eq!(one(false), None);
        // At another height: not the reflected plane's water.
        assert_eq!(placed_water_on_screen(std::iter::once((near, &aabb, 0x1234, true)), v, 200.0, eye), None);
        // The nearer of two drawn; a hidden nearer one is passed over for the farther.
        let two = |near_visible| placed_water_on_screen([(far, &aabb, 0x1, true), (near, &aabb, 0x2, near_visible)].into_iter(), v, 0.0, eye);
        assert_eq!(two(true), Some(SeenWater::Placed(0x2)));
        assert_eq!(two(false), Some(SeenWater::Placed(0x1)));
    }

    /// #1060: the counter only grows; water was drawn between two readbacks when it
    /// changed, wrapping included. Before two readbacks it is not, unless the readback
    /// never answers.
    #[test]
    fn water_pixels_drawn_when_the_count_changed() {
        let mut px = WaterPixels { started: Some(10.0), ..default() };
        assert!(!px.may_be_drawn(10.5));
        px.read(10);
        assert!(!px.may_be_drawn(10.5));
        px.read(10);
        assert!(!px.may_be_drawn(10.5));
        px.read(25);
        assert!(px.may_be_drawn(10.5));
        px.read(25);
        px.read(u32::MAX);
        px.read(3);
        assert!(px.may_be_drawn(10.5));
        // No readback for READBACK_WAIT seconds: the GPU check is left out.
        assert!(WaterPixels { started: Some(10.0), ..default() }.may_be_drawn(12.5));
        assert!(!WaterPixels::default().may_be_drawn(100.0));
    }

    #[test]
    fn game_points_map_to_bevy_and_back() {
        let g = Vec3::new(10.0, 20.0, 30.0);
        assert_eq!(bevy_to_game(game_point(g)), g);
    }
}
