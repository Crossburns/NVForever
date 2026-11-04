//! The Pip-Boy on the first-person arms (docs/spec-notes/pipboy.md §1): the model's
//! screen shows the menus' image while the Pip-Boy is up (`nvf_play::pipboy`), the
//! button of the page shown is lit, and the window's pointer is followed onto the screen
//! and the three buttons.
//!
//! The raise itself is the arms rig's clip (`player_body.rs`, `ClipKey::PipBoy`).
//!
//! - The parts, by shape name in `pipboy3000\pipboyarm.nif` [observed]: the screen
//!   `pipboyscreen:0`; the buttons `PipBoyButton01`, `02` and `03` (each two shapes), at
//!   the glows `StatsGlow:0`, `ItemsGlow:0` and `DataGlow:0` (the same places, a little
//!   above them): STATS, ITEMS, DATA from the elbow out.
//! - The screen's texture coordinates run from 0 to about 0.75 both ways (u −0.010 to
//!   0.753, v 0.002 to 0.762) over a 4:3 shape [observed]. The canvas fills 0.8 of the
//!   texture each way, as a 1024 × 768 canvas in the top left of a 1280 × 960 image
//!   [observed: fitted to the owner's ITEMS screenshot, where the menus are 1.07 times
//!   as wide and 1.10 times as high as a fill of 0.75 gave]; so the screen shows the
//!   canvas up to about 964 across and 731 down, and the image the menus are drawn into
//!   (`nvf_play::pipboy::TARGET`) is laid out the same way.
//! - The screen with the menus is drawn unlit, as a lit display [hypothesis], in the
//!   original's amber ([`SCREEN_TINT`], fitted). The game's `fBrightenPipboy=1.3`
//!   [documented: `Fallout_default.ini` `[InterfaceFX]`] is not applied as such: what it
//!   brightens is not known, and the fit already matches the screenshot.
//! - The model's glass is not drawn while the menus are on the screen: `ScreenLit:8`, a
//!   lit and glossy copy of the screen in the same place, and `glare:0`, a sheet added
//!   over it with `ScreenGlare.dds` [observed: their properties]. Drawn, the glare lay
//!   over the menus as a pale grey-blue haze the owner's screenshots do not have
//!   [observed]; those show a faint lighter patch at the top right and an empty screen
//!   of about (31, 27, 15) where ours is (15, 9, 0), so the original draws the glare far
//!   more weakly than NVForever does [hypothesis; open, docs/owner-review.md]. The
//!   screen's own material and the glass come back when the Pip-Boy is down.
//! - While the Pip-Boy is up only the page's button glows [observed: the owner's
//!   screenshots: the button of the page shown is bright, the other two dimmer]; the
//!   pointer lights none (a player's comparison with the original, #1516). Down, all
//!   three glow, as before (owner, 2026-10-10).
//! - Under a Stealth Boy (`stealth_field.rs`) the screen's own material is the field's
//!   see-through copy; the menus are built from the screen's real material instead, and
//!   the field leaves the screen alone while they are on it, so the menus stay readable
//!   (owner, 2026-10-23). They used to inherit the refraction and the screen was blank.
//! - The pointer is followed onto the shapes as the arm's bones pose them, from CPU
//!   copies of their meshes ([`PartMesh`], [`Posed`]): the drawn meshes are kept on the
//!   GPU only and are skinned, so Bevy's mesh ray casts found nothing and clicks on the
//!   screen and the buttons did nothing (the owner's playtest, 2026-10-11).

use crate::player_body::PlayerBody;
use crate::stealth_field::StealthField;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use nvf_assets::nif_material::NifMaterial;
use nvf_play::pipboy::{AidTarget, Page, PipBoy, PipBoyAid, PipBoyInput, PipBoyScreen, CANVAS, TARGET};
use nvf_play::PlayerCamera;
use std::sync::Arc;

/// The part of the screen's texture the canvas fills [observed, module docs].
pub const UV_EXTENT: f32 = 0.8;

/// What the screen multiplies the menus' picture by (linear): so that their brightest
/// amber comes out as the original's on a 1280 × 720 frame at noon in Goodsprings
/// [observed: fitted; the owner's ITEMS screenshot's solid CND bar is about (220, 180,
/// 64), and with this the menus' brightest text is (221, 179, 59); unscaled it was
/// (237, 167, 53), the view's tone mapping squeezing the brightest values].
const SCREEN_TINT: [f32; 3] = [0.83, 1.17, 1.27];

/// A part of the Pip-Boy model the menus use.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PipBoyPart {
    Screen,
    /// The screen's glass (`ScreenLit:8`, `glare:0`).
    Glass,
    Glow(Page),
    Button(Page),
}

/// The part a shape of the Pip-Boy model is, by its name.
pub fn part(name: &str) -> Option<PipBoyPart> {
    let n = name.trim().to_ascii_lowercase();
    let page = |s: &str| match s {
        "01" => Some(Page::Stats),
        "02" => Some(Page::Items),
        "03" => Some(Page::Data),
        _ => None,
    };
    match n.as_str() {
        "pipboyscreen:0" => Some(PipBoyPart::Screen),
        "screenlit:8" | "glare:0" => Some(PipBoyPart::Glass),
        "statsglow:0" => Some(PipBoyPart::Glow(Page::Stats)),
        "itemsglow:0" => Some(PipBoyPart::Glow(Page::Items)),
        "dataglow:0" => Some(PipBoyPart::Glow(Page::Data)),
        _ => n.strip_prefix("pipboybutton").and_then(|r| page(r.get(..2)?)).map(PipBoyPart::Button),
    }
}

/// Where a spot of the screen's texture lies on the canvas.
pub fn canvas_point(uv: Vec2) -> Option<Vec2> {
    let p = uv / UV_EXTENT * CANVAS;
    (p.x >= 0.0 && p.y >= 0.0 && p.x < CANVAS.x && p.y < CANVAS.y).then_some(p)
}

pub struct PipBoyArmPlugin;

impl Plugin for PipBoyArmPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (follow_pointer, show_screen).chain().after(nvf_play::player::BodySet).before(nvf_play::PlaySet::Ui));
    }
}

/// The screen's own material while the menus are on it.
#[derive(Default)]
pub(crate) struct Swapped {
    screen: Option<(Entity, Handle<NifMaterial>)>,
    menus: Option<Handle<NifMaterial>>,
    /// The menus' missing place was logged for this raise.
    warned: bool,
    /// The buttons last logged as lit.
    lit: Option<String>,
}

/// Whether the glow of `page`'s button shows: the page's alone while the Pip-Boy shows on
/// the arm, all three otherwise (module docs).
pub(crate) fn glow_lit(pip: &PipBoy, page: Page) -> bool {
    !pip.on_arm() || page == pip.page
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn show_screen(
    pip: Res<PipBoy>,
    screen: Option<ResMut<PipBoyScreen>>,
    body: Res<PlayerBody>,
    mut field: ResMut<StealthField>,
    mut parts: Query<(Entity, &PipBoyPart, &mut MeshMaterial3d<NifMaterial>)>,
    mut visibility: Query<&mut Visibility>,
    mut materials: ResMut<Assets<NifMaterial>>,
    mut swapped: Local<Swapped>,
) {
    let Some(mut screen) = screen else { return };
    let shown = pip.on_arm() && body.first_person_drawn();
    let model_screen = parts.iter().find(|(_, p, _)| **p == PipBoyPart::Screen).map(|(e, _, m)| (e, m.0.clone()));
    // Give back the screen's own material when the menus leave it (or the rig was rebuilt).
    if let Some((e, own)) = swapped.screen.clone() {
        if !shown || model_screen.as_ref().is_none_or(|(s, _)| *s != e) {
            // Set at once, not by command: the Stealth Boy's field runs next and must see
            // the screen's own material to make it see-through again.
            if let Ok((_, _, mut m)) = parts.get_mut(e) {
                m.0 = own;
            }
            swapped.screen = None;
        }
    }
    screen.on_model = shown && model_screen.is_some();
    // Said once each time the Pip-Boy comes up when the menus have nowhere to show, so a
    // player's log says why the arm came up blank or not at all (#1330).
    if !pip.on_arm() {
        swapped.warned = false;
    } else if !swapped.warned && pip.clip_time >= nvf_play::pipboy::HOLD_SECONDS {
        let why = if !body.first_person_drawn() {
            Some("the first-person arms are not drawn")
        } else if model_screen.is_none() {
            Some("the arm's model has no screen shape (pipboyscreen:0)")
        } else {
            None
        };
        if let Some(why) = why {
            warn!("pipboy: up, but the menus are not on the arm: {why}");
            swapped.warned = true;
        }
    }
    if let (true, Some((e, mut own))) = (shown, model_screen) {
        if swapped.screen.is_none() {
            // Under a Stealth Boy the screen wears the field's see-through copy: take the
            // real material back, so neither the menus nor the screen afterwards refract.
            if let Some(original) = field.take_original(e, &own, &mut materials) {
                own = original;
                info!("pipboy: the menus are on the screen drawn plain under the Stealth Boy");
            }
            let menus = swapped.menus.get_or_insert_with(|| {
                let mut m = materials.get(&own).cloned().unwrap_or_default();
                let scale = (CANVAS / TARGET.as_vec2()) / UV_EXTENT;
                let [r, g, b] = SCREEN_TINT;
                m.base.base_color = Color::LinearRgba(LinearRgba::rgb(r, g, b));
                m.base.base_color_texture = Some(screen.image.clone());
                m.base.emissive_texture = None;
                m.base.normal_map_texture = None;
                m.base.unlit = true;
                m.base.alpha_mode = AlphaMode::Opaque;
                m.base.uv_transform = bevy::math::Affine2::from_scale(scale);
                materials.add(m)
            });
            if let Ok((_, _, mut m)) = parts.get_mut(e) {
                m.0 = menus.clone();
            }
            swapped.screen = Some((e, own));
            info!("pipboy: the menus are on the model's screen");
        }
    }
    // The glows and the glass (not drawn while the menus are on the screen).
    let mut lit: [bool; 3] = default();
    for (e, p, _) in &parts {
        let shown = match *p {
            PipBoyPart::Glow(page) => {
                let on = glow_lit(&pip, page);
                lit[page.index()] |= on;
                on
            }
            PipBoyPart::Glass => !screen.on_model,
            _ => continue,
        };
        let want = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if let Ok(mut v) = visibility.get_mut(e) {
            if *v != want {
                *v = want;
            }
        }
    }
    // Said on each change, so a log can tell which glows are drawn. Nothing while no rig
    // holds a glow.
    if lit.contains(&true) {
        let names = Page::ALL.into_iter().filter(|p| lit[p.index()]).map(|p| p.title()).collect::<Vec<_>>().join(", ");
        if swapped.lit.as_deref() != Some(names.as_str()) {
            info!("pipboy: buttons lit: {names}");
            swapped.lit = Some(names);
        }
    }
}

/// A Pip-Boy part's mesh as loaded (positions, texture coordinates, joints), kept on the
/// CPU: the drawn mesh lives on the GPU only, and Bevy's mesh ray casts do not skin.
#[derive(Component, Clone)]
pub struct PartMesh(pub Arc<Mesh>);

/// A shape posed in the world: its triangles' corners and texture coordinates.
pub struct Posed {
    pub points: Vec<Vec3>,
    pub uvs: Vec<Vec2>,
    pub triangles: Vec<[usize; 3]>,
}

impl Posed {
    /// Pose `mesh`: skinned by `joints` (each joint's world matrix times its inverse bind
    /// pose, as Bevy's skinning does), else placed by `world`.
    pub fn new(mesh: &Mesh, joints: Option<&[Mat4]>, world: Mat4) -> Option<Posed> {
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
        let uvs: Vec<Vec2> = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(uv)) => uv.iter().map(|u| Vec2::from_array(*u)).collect(),
            _ => return None,
        };
        let skin = joints.and_then(|j| match (mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX), mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)) {
            (Some(VertexAttributeValues::Uint16x4(ji)), Some(VertexAttributeValues::Float32x4(jw))) => Some((j, ji, jw)),
            _ => None,
        });
        let points = pos
            .iter()
            .enumerate()
            .map(|(v, p)| {
                let p = Vec3::from_array(*p);
                match skin {
                    Some((j, ji, jw)) => {
                        let mut m = Mat4::ZERO;
                        for k in 0..4 {
                            if let Some(joint) = j.get(ji[v][k] as usize) {
                                m += *joint * jw[v][k];
                            }
                        }
                        m.transform_point3(p)
                    }
                    None => world.transform_point3(p),
                }
            })
            .collect();
        let triangles = match mesh.indices() {
            Some(i) => {
                let i: Vec<usize> = i.iter().collect();
                i.as_chunks::<3>().0.to_vec()
            }
            None => (0..pos.len() / 3).map(|t| [3 * t, 3 * t + 1, 3 * t + 2]).collect(),
        };
        Some(Posed { points, uvs, triangles })
    }

    /// The nearest triangle the ray meets, both faces: the distance along the ray and the
    /// texture coordinates there.
    pub fn hit(&self, origin: Vec3, dir: Vec3) -> Option<(f32, Vec2)> {
        let mut best: Option<(f32, Vec2)> = None;
        for &[a, b, c] in &self.triangles {
            let (pa, pb, pc) = (self.points[a], self.points[b], self.points[c]);
            let (e1, e2) = (pb - pa, pc - pa);
            let h = dir.cross(e2);
            let det = e1.dot(h);
            if det.abs() < 1e-9 {
                continue;
            }
            let s = origin - pa;
            let u = s.dot(h) / det;
            let q = s.cross(e1);
            let v = dir.dot(q) / det;
            let t = e2.dot(q) / det;
            if u < 0.0 || v < 0.0 || u + v > 1.0 || t <= 0.0 || best.is_some_and(|(d, _)| d <= t) {
                continue;
            }
            best = Some((t, self.uvs[a] * (1.0 - u - v) + self.uvs[b] * u + self.uvs[c] * v));
        }
        best
    }

    /// The point of the shape at texture coordinates `uv`, if some triangle covers them.
    pub fn at_uv(&self, uv: Vec2) -> Option<Vec3> {
        self.triangles.iter().find_map(|&[a, b, c]| {
            let (ta, tb, tc) = (self.uvs[a], self.uvs[b], self.uvs[c]);
            let (e1, e2, p) = (tb - ta, tc - ta, uv - ta);
            let det = e1.perp_dot(e2);
            if det.abs() < 1e-12 {
                return None;
            }
            let u = p.perp_dot(e2) / det;
            let v = e1.perp_dot(p) / det;
            (u >= 0.0 && v >= 0.0 && u + v <= 1.0).then(|| self.points[a] * (1.0 - u - v) + self.points[b] * u + self.points[c] * v)
        })
    }

    /// The middle of the shape's corners.
    pub fn middle(&self) -> Option<Vec3> {
        (!self.points.is_empty()).then(|| self.points.iter().sum::<Vec3>() / self.points.len() as f32)
    }
}

/// Follow the window's pointer onto the model: where it lies on the screen goes to the
/// menus (`PipBoyScreen::pointer`); a click on a button shows its page. The pointer on a
/// button does nothing else: the button's light follows the page shown. The shapes are
/// posed as the arm's bones hold them this frame. The test aid's target (`PipBoyAid`)
/// stands in for the window's pointer.
#[allow(clippy::too_many_arguments)]
fn follow_pointer(
    pip: Res<PipBoy>,
    screen: Option<ResMut<PipBoyScreen>>,
    cams: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    parts: Query<(&PipBoyPart, &PartMesh, &GlobalTransform, Option<&SkinnedMesh>)>,
    globals: Query<&GlobalTransform>,
    binds: Res<Assets<SkinnedMeshInverseBindposes>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut aid: ResMut<PipBoyAid>,
    mut aid_over: Local<Option<Page>>,
    mut out: MessageWriter<PipBoyInput>,
) {
    let Some(mut screen) = screen else { return };
    let was_over = aid_over.take();
    if !screen.on_model {
        return;
    }
    screen.pointer = None;
    if !pip.open() {
        return;
    }
    let (Ok((cam, at)), Ok(window)) = (cams.single(), windows.single()) else { return };
    let posed: Vec<(PipBoyPart, Posed)> = parts
        .iter()
        .filter(|(p, ..)| matches!(p, PipBoyPart::Screen | PipBoyPart::Button(_)))
        .filter_map(|(p, mesh, gt, skin)| {
            let joints: Option<Vec<Mat4>> = skin.and_then(|s| {
                let inv = binds.get(&s.inverse_bindposes)?;
                s.joints.iter().zip(inv.iter()).map(|(j, ib)| globals.get(*j).ok().map(|g| g.to_matrix() * *ib)).collect()
            });
            Some((*p, Posed::new(&mesh.0, joints.as_deref(), gt.to_matrix())?))
        })
        .collect();
    // Where the pointer is: the test aid's target, else the window's pointer.
    let target = aid.target.and_then(|t| {
        let world = match t {
            AidTarget::Button(page) => posed.iter().find(|(p, _)| *p == PipBoyPart::Button(page)).and_then(|(_, s)| s.middle()),
            AidTarget::Canvas(c) => posed.iter().find(|(p, _)| *p == PipBoyPart::Screen).and_then(|(_, s)| s.at_uv(c / CANVAS * UV_EXTENT)),
        }?;
        cam.world_to_viewport(at, world).ok()
    });
    if aid.target.is_some() && target.is_none() {
        warn!("test aid: pipboy target {:?} is not in view", aid.target);
    }
    let cursor = if aid.target.is_some() { target } else { window.cursor_position() };
    let Some(ray) = cursor.and_then(|c| cam.viewport_to_world(at, c).ok()) else { return };
    let (origin, dir) = (ray.origin, Vec3::from(ray.direction));
    let hit = posed.iter().filter_map(|(p, s)| s.hit(origin, dir).map(|(t, uv)| (t, *p, uv))).min_by(|a, b| a.0.total_cmp(&b.0));
    let clicked = mouse.just_pressed(MouseButton::Left) || (aid.click == 3 && matches!(aid.target, Some(AidTarget::Button(_))));
    match hit {
        Some((_, PipBoyPart::Screen, uv)) => screen.pointer = canvas_point(uv),
        Some((_, PipBoyPart::Button(page), _)) => {
            if aid.target.is_some() {
                // What the hover-only smoke step needs said: the pointer did reach it.
                if was_over != Some(page) {
                    info!("test aid: the pointer on the {} button", page.title());
                }
                *aid_over = Some(page);
            }
            if clicked {
                info!("pipboy: the {} button pressed", page.title());
                out.write(PipBoyInput::Page(page));
                if aid.click == 3 {
                    aid.click = 0;
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parts_are_known_by_their_shapes() {
        assert_eq!(part("pipboyscreen:0"), Some(PipBoyPart::Screen));
        assert_eq!(part("ItemsGlow:0"), Some(PipBoyPart::Glow(Page::Items)));
        assert_eq!(part("PipBoyButton01:1"), Some(PipBoyPart::Button(Page::Stats)));
        assert_eq!(part("PipBoyButton03:0"), Some(PipBoyPart::Button(Page::Data)));
        assert_eq!(part("ScreenLit:8"), Some(PipBoyPart::Glass));
        assert_eq!(part("glare:0"), Some(PipBoyPart::Glass));
        assert_eq!(part("PipBoyArm:0"), None);
    }

    #[test]
    fn a_posed_shape_is_hit_and_found_by_its_texture_coordinates() {
        // A square 2 x 2 at z = 5 facing the origin, uv 0..1, moved up by `world`.
        let mut m = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]]);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        m.insert_indices(bevy::mesh::Indices::U16(vec![0, 1, 2, 0, 2, 3]));
        let s = Posed::new(&m, None, Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0))).unwrap();
        let (t, uv) = s.hit(Vec3::ZERO, Vec3::new(0.5, 0.0, 5.0).normalize()).unwrap();
        assert!((uv - Vec2::new(0.75, 0.5)).length() < 1e-4, "{uv}");
        assert!((t - Vec3::new(0.5, 0.0, 5.0).length()).abs() < 1e-4);
        assert!(s.hit(Vec3::ZERO, Vec3::new(3.0, 0.0, 5.0).normalize()).is_none());
        let p = s.at_uv(Vec2::new(0.75, 0.5)).unwrap();
        assert!((p - Vec3::new(0.5, 0.0, 5.0)).length() < 1e-4);
        // Skinned: one joint moving everything 2 to the side.
        m.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(vec![[0, 0, 0, 0]; 4]));
        m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[1.0f32, 0.0, 0.0, 0.0]; 4]);
        let joint = [Mat4::from_translation(Vec3::new(2.0, 0.0, 5.0))];
        let s = Posed::new(&m, Some(&joint), Mat4::IDENTITY).unwrap();
        assert!((s.middle().unwrap() - Vec3::new(2.0, 0.0, 5.0)).length() < 1e-4);
    }

    #[test]
    fn the_screen_maps_onto_the_canvas() {
        assert_eq!(canvas_point(Vec2::ZERO), Some(Vec2::ZERO));
        let p = canvas_point(Vec2::new(0.4, 0.4)).unwrap();
        assert!((p - CANVAS / 2.0).length() < 1e-3);
        assert_eq!(canvas_point(Vec2::new(0.81, 0.1)), None);
    }

    /// A square 2 x 2 facing +Z about its origin, uv 0..1.
    fn square() -> Arc<Mesh> {
        let mut m = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]]);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        m.insert_indices(bevy::mesh::Indices::U16(vec![0, 1, 2, 0, 2, 3]));
        Arc::new(m)
    }

    /// The pointer systems as the game chains them, then the menus' taking of the input
    /// (`PlaySet::Ui`, after them), over a camera, three buttons and their glows.
    fn pointer_app() -> (App, [Entity; 3]) {
        let mut app = App::new();
        app.init_resource::<PipBoy>()
            .init_resource::<PipBoyAid>()
            .init_resource::<PlayerBody>()
            .init_resource::<StealthField>()
            .init_resource::<Assets<NifMaterial>>()
            .init_resource::<Assets<SkinnedMeshInverseBindposes>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_message::<PipBoyInput>()
            .insert_resource(PipBoyScreen { image: Handle::default(), camera: Entity::PLACEHOLDER, on_model: true, pointer: None, scale: 1.0 })
            .add_systems(
                Update,
                (follow_pointer, show_screen, |mut input: MessageReader<PipBoyInput>, mut pip: ResMut<PipBoy>| {
                    for &i in input.read() {
                        pip.apply(i);
                    }
                })
                    .chain(),
            );
        let world = app.world_mut();
        world.spawn((Window::default(), PrimaryWindow));
        let mut camera = Camera::default();
        camera.computed.clip_from_view = bevy::camera::CameraProjection::get_clip_from_view(&PerspectiveProjection::default());
        camera.computed.target_info = Some(bevy::camera::RenderTargetInfo { physical_size: UVec2::new(1280, 960), scale_factor: 1.0 });
        world.spawn((camera, GlobalTransform::IDENTITY, PlayerCamera));
        let mesh = square();
        let glows = Page::ALL.map(|page| {
            let x = page.index() as f32 * 3.0 - 3.0;
            world.spawn((PipBoyPart::Button(page), PartMesh(mesh.clone()), GlobalTransform::from_translation(Vec3::new(x, 0.0, -10.0))));
            world.spawn((PipBoyPart::Glow(page), MeshMaterial3d::<NifMaterial>::default(), Visibility::Inherited)).id()
        });
        world.resource_mut::<PipBoy>().page = Page::Items;
        world.resource_mut::<PipBoy>().phase = nvf_play::pipboy::Phase::Up;
        (app, glows)
    }

    /// A frame; the arms are not built here, so the screen is said to be on the model.
    fn frame(app: &mut App) {
        app.world_mut().resource_mut::<PipBoyScreen>().on_model = true;
        app.update();
    }

    fn lit(app: &App, glows: [Entity; 3]) -> [bool; 3] {
        glows.map(|g| *app.world().get::<Visibility>(g).unwrap() != Visibility::Hidden)
    }

    #[test]
    fn the_pointer_lights_no_button_a_click_shows_its_page_and_down_all_glow() {
        let (mut app, glows) = pointer_app();
        frame(&mut app);
        assert_eq!(lit(&app, glows), [false, true, false], "only the page shown");
        // The pointer on another page's button: neither the page nor the lights change.
        app.world_mut().resource_mut::<PipBoyAid>().target = Some(AidTarget::Button(Page::Stats));
        for _ in 0..3 {
            frame(&mut app);
        }
        assert_eq!(app.world().resource::<PipBoy>().page, Page::Items);
        assert_eq!(lit(&app, glows), [false, true, false], "hovering lights nothing");
        // A click on it shows its page, whose button lights the frame after.
        app.world_mut().resource_mut::<PipBoyAid>().click = 3;
        frame(&mut app);
        assert_eq!(app.world().resource::<PipBoy>().page, Page::Stats);
        frame(&mut app);
        assert_eq!(lit(&app, glows), [true, false, false], "the page clicked");
        // Down: all three.
        app.world_mut().resource_mut::<PipBoy>().phase = nvf_play::pipboy::Phase::Down;
        frame(&mut app);
        assert_eq!(lit(&app, glows), [true, true, true], "down, all glow");
    }
}
