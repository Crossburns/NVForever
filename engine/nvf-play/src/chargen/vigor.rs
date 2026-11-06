//! The Vigor Tester's S.P.E.C.I.A.L. menu (menu 1074, `ShowLoveTesterMenuParams`), drawn
//! as the original: the machine's 3D model fills the screen in front of the player's
//! camera, its flip boards turn between pages by the model's own sequences, its bulbs,
//! digits and arrows show the sheet (docs/spec-notes/vigor-tester.md).
//!
//! - The model: `nv_vitomaticvigortester_cabinet.nif` (the cabinet and the bulbs with
//!   their glows) with `nv_vitomaticvigortester_activate.nif` (the label boards, the
//!   posters, the lever, the buttons and the 16 sequences) in one frame [observed: the
//!   two files share it; the placed activator's own model is `…_cabinet02.nif`, with a
//!   fixed poster]. Lit, on a render layer of its own ([`VIGOR_LAYER`]), by a camera and
//!   lights of its own (nvforever `vigor_view.rs`), so the menu looks the same in a dark
//!   room and at noon and the room's lighting does not change; each part's colour is
//!   tinted by `model::shade`. It stays below the main camera, whose field of view is the
//!   original menu's while the menu is open and is given back when it closes ([`view`]);
//!   the view's camera takes that field of view, and the room behind is blurred and
//!   darkened there.
//! - Pages ([`model::Page`]): Attract (the model's stored pose), the seven attributes,
//!   Review. A turn plays the sequence of [`model::sequence`] (about 1.3 s).
//! - Keys [hypothesis: the original's PC keys are not documented; the menu file binds the
//!   pad's left/right to increase/decrease, up/down to the row, RT/LT to the pages and
//!   A/X to exit]: Left/Right or A/D lower/raise the page's attribute (on the review page
//!   the chosen row's), Up/Down or W/S choose the row, Tab or Page Down the next page,
//!   Shift+Tab or Page Up the previous one, Enter or E Done; the → button on the review
//!   page is Done too. Clicks on the model's buttons, found by their outlines on the
//!   screen.
//! - Done only with every point given out (`SpecialSheet::complete`); then the sheet is
//!   written to the player and the menu answered (`chargen::finish`).
//!
//! Test aid `NVF_PLAY_TEST_VIGOR=<seconds>:<op>[,…]` (seconds after the menu opened;
//! steps wait for it to open): `raise[:<attribute>]`, `lower[:<attribute>]`,
//! `select:<attribute>`, `page:next`, `page:prev`, `page:<page>` (turning through the
//! pages between), `done`, `click:<shape>` (a click at the middle of the shape's outline
//! on the screen, e.g. `click:P1_RT_Btn:0`). Attributes by number 0–6, name or letter;
//! pages by name (`attract`, `review`, an attribute) or number 0–8.

pub mod model;
pub mod view;

use self::model::{Action, AidOp, Outcome, Page, Vigor, STATS};
use crate::pause::AidTime;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
use crate::{PlayerCamera, PlaySet};
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_game::chargen::SpecialSheet;
use fnv_game::game::menus;
use nvf_assets::anim_play::{Clip, Playing};
use nvf_assets::{GameData, NifMaterial, SpawnCtx};
use std::collections::HashMap;
use std::sync::Arc;

const CABINET: &str = "meshes\\architecture\\goodsprings\\nv_vitomaticvigortester_cabinet.nif";
const ACTIVATE: &str = "meshes\\architecture\\goodsprings\\nv_vitomaticvigortester_activate.nif";

/// The render layer of the machine, its lights and its camera (nvforever `vigor_view.rs`);
/// no other view has it, so the machine takes no light from the room and gives it none.
pub const VIGOR_LAYER: usize = 14;

pub struct VigorPlugin;

impl Plugin for VigorPlugin {
    fn build(&self, app: &mut App) {
        let aid = match std::env::var("NVF_PLAY_TEST_VIGOR").map(|v| model::parse_aid(&v)) {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => {
                warn!("NVF_PLAY_TEST_VIGOR: {e}");
                Vec::new()
            }
            Err(_) => Vec::new(),
        };
        app.init_resource::<VigorMenu>()
            .init_resource::<VigorStats>()
            .insert_resource(VigorAid(aid))
            .add_systems(
                Update,
                (open_close, input).chain().in_set(PlaySet::Scripts).before(crate::script::run_scripts).run_if(resource_exists::<GameData>),
            )
            .add_systems(Update, draw.in_set(PlaySet::Ui).run_if(resource_exists::<GameData>))
            .add_systems(Last, report);
    }
}

/// The open menu, if any.
#[derive(Resource, Default)]
pub struct VigorMenu {
    open: Option<OpenMenu>,
    /// The activate model's sequences by name, read on the first opening.
    clips: Option<Arc<HashMap<String, Clip>>>,
}

impl VigorMenu {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

struct OpenMenu {
    vigor: Vigor,
    /// The two models' roots (children of the camera).
    roots: [Entity; 2],
    /// Named objects of both models (the activate model's first).
    nodes: HashMap<String, Entity>,
    /// The turn playing, or the last one (held at its end).
    flip: Option<Playing>,
    /// Shapes given their own material, by entity.
    materials: HashMap<Entity, Handle<NifMaterial>>,
    /// The texture each changed shape shows now.
    shown: HashMap<Entity, String>,
    /// Up/Down chose a review row: its arrows show it.
    keyed_row: bool,
    /// The camera's field of view before the menu set the original's.
    fov_before: Option<f32>,
    /// When the test aid's clock started: the first frame the menu could take input.
    aid_from: Option<f32>,
}

#[derive(Resource, Default)]
struct VigorStats {
    opened: u32,
    changes: u32,
    turns: u32,
    clicks: u32,
    done: u32,
}

#[derive(Resource, Default)]
struct VigorAid(Vec<(f32, AidOp)>);

/// The machine's parts below the camera.
#[derive(Component)]
struct VigorPart;

#[allow(clippy::too_many_arguments)]
fn open_close(
    mut menu: ResMut<VigorMenu>,
    mut script: ResMut<ScriptState>,
    mut sim: ResMut<SimState>,
    mut ctx: SpawnCtx,
    mut camera: Query<(Entity, &mut Projection), With<PlayerCamera>>,
    mut stats: ResMut<VigorStats>,
) {
    let code = crate::chargen::open_code(&script);
    if code != Some(menus::LOVE_TESTER) {
        if let Some(o) = menu.open.take() {
            for e in o.roots {
                ctx.commands.entity(e).despawn();
            }
            if let (Some(fov), Ok((_, mut proj))) = (o.fov_before, camera.single_mut()) {
                if let Projection::Perspective(p) = proj.as_mut() {
                    p.fov = fov;
                }
            }
            info!("vigor tester: closed");
        }
        return;
    }
    if menu.open.is_some() {
        return;
    }
    let Ok((cam, _)) = camera.single() else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let sheet = crate::script::with_world(rt, &mut sim, |rt| SpecialSheet::for_menu(&mut rt.game));
    if menu.clips.is_none() {
        menu.clips = Some(Arc::new(read_clips(&ctx.data)));
    }
    let (act, cab) = match (ctx.spawn_nif(ACTIVATE, None), ctx.spawn_nif(CABINET, None)) {
        (Ok(a), Ok(c)) => (a, c),
        (a, c) => {
            warn!("vigor tester: model not drawn: {:?} {:?}", a.err(), c.err());
            return;
        }
    };
    let (Some(ra), Some(rc)) = (act.root, cab.root) else { return };
    let mut nodes = act.nodes;
    for (k, v) in cab.nodes {
        nodes.entry(k).or_insert(v);
    }
    // Hidden until `draw` has put the shapes on the machine's layer, or the main camera
    // would draw them for a frame in the room's light.
    for r in [ra, rc] {
        ctx.commands.entity(r).insert((VigorPart, Visibility::Hidden, view::model_in_camera(1.0)));
        ctx.commands.entity(cam).add_child(r);
    }
    let vigor = Vigor::new(sheet);
    // The Attract page: the stored pose, as the end of the turn back to it holds it.
    let clips = menu.clips.clone().unwrap_or_default();
    let flip = clips.get(model::ATTRACT_POSE).map(|c| Playing::new(ACTIVATE, c, |n| nodes.get(n).copied()).at_phase(1.0));
    info!(
        "vigor tester: open, S.P.E.C.I.A.L. {}, {} points remaining ({} sequences, {} parts)",
        vigor.values(),
        vigor.sheet.remaining(),
        clips.len(),
        nodes.len()
    );
    stats.opened += 1;
    menu.open = Some(OpenMenu { vigor, roots: [ra, rc], nodes, flip, materials: HashMap::new(), shown: HashMap::new(), keyed_row: false, fov_before: None, aid_from: None });
}

/// The machine's clips by name. A broken file (a panic reading it) gives none, and a clip
/// that cannot be played ([`Clip::check`]) is left out; each is logged.
fn read_clips(data: &GameData) -> HashMap<String, Clip> {
    let Some(bytes) = data.read(ACTIVATE) else { return HashMap::new() };
    let read = nvf_assets::guard::guarded(|| {
        let Ok(f) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else { return Ok(Vec::new()) };
        let dec = fnv_nif::scene::Decoded::new(&f, &bytes, &data.schema);
        Ok(fnv_nif::anim::sequences(&f, &dec).iter().map(Clip::from_sequence).collect())
    });
    let clips: Vec<Clip> = read.unwrap_or_else(|e| {
        warn!("vigor tester: {ACTIVATE} not read: {e}");
        Vec::new()
    });
    clips
        .into_iter()
        .filter(|c| c.check().map_err(|e| warn!("vigor tester: clip {} not played: {e}", c.name)).is_ok())
        .map(|c| (c.name.clone(), c))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    shapes: Query<(&Aabb, &GlobalTransform, &InheritedVisibility)>,
    time: AidTime,
    mut aid: ResMut<VigorAid>,
    mut menu: ResMut<VigorMenu>,
    mut script: ResMut<ScriptState>,
    mut sim: ResMut<SimState>,
    mut stats: ResMut<VigorStats>,
    mut sounds: MessageWriter<UiSound>,
) {
    use KeyCode::*;
    let clips = menu.clips.clone().unwrap_or_default();
    let Some(o) = menu.open.as_mut() else { return };
    let mut acts: Vec<(Action, &'static str)> = Vec::new();
    let shift = keys.any_pressed([ShiftLeft, ShiftRight]);
    if keys.any_just_pressed([ArrowLeft, KeyA]) {
        acts.push((Action::Lower(None), "key"));
    }
    if keys.any_just_pressed([ArrowRight, KeyD]) {
        acts.push((Action::Raise(None), "key"));
    }
    if keys.any_just_pressed([ArrowUp, KeyW]) {
        acts.push((Action::Step(-1), "key"));
    }
    if keys.any_just_pressed([ArrowDown, KeyS]) {
        acts.push((Action::Step(1), "key"));
    }
    if keys.just_pressed(PageDown) || (keys.just_pressed(Tab) && !shift) {
        acts.push((Action::Next, "key"));
    }
    if keys.just_pressed(PageUp) || (keys.just_pressed(Tab) && shift) {
        acts.push((Action::Prev, "key"));
    }
    if keys.any_just_pressed([Enter, NumpadEnter, KeyE]) {
        acts.push((Action::Done, "key"));
    }
    if acts.iter().any(|(a, _)| matches!(a, Action::Step(_))) {
        o.keyed_row = true;
    }
    let cam = camera.single().ok();
    let pick = |at: Vec2, o: &OpenMenu| -> Option<Action> {
        let (camera, cam_t) = cam?;
        let mut best: Option<(f32, Action)> = None;
        for name in model::live_buttons(o.vigor.page) {
            let Some(&e) = o.nodes.get(&name) else { continue };
            let Some(rect) = screen_rect(e, camera, cam_t, &shapes) else { continue };
            let area = rect.size().x * rect.size().y;
            if rect.contains(at) && best.is_none_or(|(a, _)| area < a) {
                if let Some(a) = model::button_action(&name, o.vigor.page) {
                    best = Some((area, a));
                }
            }
        }
        best.map(|(_, a)| a)
    };
    if mouse.just_pressed(MouseButton::Left) {
        if let Some(a) = windows.single().ok().and_then(|w| w.cursor_position()).and_then(|at| pick(at, o)) {
            acts.push((a, "click"));
            stats.clicks += 1;
        }
    }
    // The test aid's steps due now.
    // Its seconds count from the menu's first frame of input, so that a slow start
    // (cells loading) cannot run every step in the frame the model appears.
    let now = time.elapsed_secs();
    let since = now - *o.aid_from.get_or_insert(now);
    let mut due: Vec<(f32, AidOp)> = aid.0.iter().filter(|(t, _)| *t <= since).cloned().collect();
    due.sort_by(|a, b| a.0.total_cmp(&b.0));
    aid.0.retain(|(t, _)| *t > since);
    for (_, step) in due {
        info!("test aid NVF_PLAY_TEST_VIGOR: {step:?}");
        match step {
            AidOp::Act(a) => acts.push((a, "test aid")),
            AidOp::Page(p) => {
                let (from, to) = (o.vigor.page.index(), p.index());
                let a = if to > from { Action::Next } else { Action::Prev };
                for _ in 0..from.abs_diff(to) {
                    acts.push((a, "test aid"));
                }
            }
            AidOp::Click(name) => {
                let at = cam.and_then(|(c, ct)| o.nodes.get(&name).and_then(|&e| screen_rect(e, c, ct, &shapes))).map(|r| r.center());
                match at.and_then(|at| pick(at, o).map(|a| (at, a))) {
                    Some((at, a)) => {
                        info!("vigor tester: click at ({:.0}, {:.0}) on {name}", at.x, at.y);
                        acts.push((a, "click"));
                        stats.clicks += 1;
                    }
                    None => info!("vigor tester: click on {name}: nothing there to press"),
                }
            }
        }
    }
    let mut sound = |event| {
        sounds.write(UiSound { menu: UiMenu::Vigor, event });
    };
    for (a, how) in acts {
        match o.vigor.act(a) {
            Outcome::Changed { stat, value, remaining } => {
                sound(UiEvent::Value { up: matches!(a, Action::Raise(_)) });
                stats.changes += 1;
                info!("vigor tester: {} {value}, {remaining} points remaining ({how})", STATS[stat]);
            }
            Outcome::Turned { from, to } => {
                sound(UiEvent::Switch);
                stats.turns += 1;
                let seq = model::sequence(from, to).unwrap_or("?");
                info!("vigor tester: page {} (sequence {seq}, {how})", to.name());
                match clips.get(seq) {
                    Some(c) => o.flip = Some(Playing::new(ACTIVATE, c, |n| o.nodes.get(n).copied())),
                    None => warn!("vigor tester: no sequence {seq}: the boards turn at once"),
                }
            }
            Outcome::Selected(i) => {
                sound(UiEvent::Focus);
                info!("vigor tester: row {} chosen ({how})", STATS[i]);
            }
            Outcome::Refused(why) => {
                sound(UiEvent::Refused);
                info!("vigor tester: {a:?} refused: {why} ({how})");
            }
            Outcome::Done => {
                let sheet = o.vigor.sheet;
                let values = o.vigor.values();
                if crate::chargen::finish(&mut script, &mut sim, menus::LOVE_TESTER, how, |g| sheet.apply(g)) {
                    sound(UiEvent::Accepted);
                    stats.done += 1;
                    info!("vigor tester: done, S.P.E.C.I.A.L. {values} ({how})");
                }
                break;
            }
        }
    }
}

/// A shape's outline on the screen (logical pixels), from its bounds; `None` while it is
/// hidden or behind the camera.
fn screen_rect(e: Entity, camera: &Camera, cam_t: &GlobalTransform, shapes: &Query<(&Aabb, &GlobalTransform, &InheritedVisibility)>) -> Option<Rect> {
    let (aabb, t, vis) = shapes.get(e).ok()?;
    if !vis.get() {
        return None;
    }
    let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
    let mut rect: Option<Rect> = None;
    for k in 0..8 {
        let s = Vec3::new(if k & 1 == 0 { -1.0 } else { 1.0 }, if k & 2 == 0 { -1.0 } else { 1.0 }, if k & 4 == 0 { -1.0 } else { 1.0 });
        let p = camera.world_to_viewport(cam_t, t.transform_point(c + h * s)).ok()?;
        rect = Some(rect.map_or(Rect::from_corners(p, p), |r| r.union_point(p)));
    }
    rect
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut menu: ResMut<VigorMenu>,
    time: AidTime,
    mut camera: Query<&mut Projection, With<PlayerCamera>>,
    children: Query<&Children>,
    meshes: Query<&MeshMaterial3d<NifMaterial>>,
    names: Query<&Name>,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
    mut ctx: SpawnCtx,
) {
    let Some(o) = menu.open.as_mut() else { return };
    // The same place in front of the camera every frame.
    // The original's field of view while the menu is open (the world behind is only a
    // background), given back when it closes.
    let vfov = match camera.single_mut().as_deref_mut() {
        Ok(Projection::Perspective(p)) => {
            let fit = view::FIT_VFOV_DEG.to_radians();
            if o.fov_before.is_none() {
                o.fov_before = Some(p.fov);
            }
            if p.fov != fit {
                p.fov = fit;
            }
            fit
        }
        _ => view::FIT_VFOV_DEG.to_radians(),
    };
    let place = view::model_in_camera(vfov);
    for r in o.roots {
        if let Ok(mut t) = transforms.get_mut(r) {
            t.set_if_neq(place);
        }
    }
    // Once the shapes exist: each its own material and tint, on the machine's layer,
    // casting no shadow. The model says which shapes are unlit; the glows always are.
    if o.materials.is_empty() {
        for r in o.roots {
            for e in children.iter_descendants(r) {
                let Ok(m) = meshes.get(e) else { continue };
                let Some(mut mat) = ctx.materials.get(&m.0).cloned() else { continue };
                let name = names.get(e).map(|n| n.as_str()).unwrap_or("");
                mat.base.unlit |= model::is_glow(name);
                let [r, g, b] = model::shade(name, r == o.roots[1]);
                let c = mat.base.base_color.to_srgba();
                mat.base.base_color = Color::srgba(c.red * r, c.green * g, c.blue * b, c.alpha);
                mat.base.fog_enabled = false;
                let h = ctx.materials.add(mat);
                ctx.commands.entity(e).insert((MeshMaterial3d(h.clone()), NotShadowCaster, RenderLayers::layer(VIGOR_LAYER)));
                o.materials.insert(e, h);
            }
        }
        if !o.materials.is_empty() {
            debug!("vigor tester: {} shapes drawn", o.materials.len());
            for r in o.roots {
                if let Ok(mut vis) = visibility.get_mut(r) {
                    vis.set_if_neq(Visibility::Inherited);
                }
            }
        }
    }
    if let Some(f) = o.flip.as_mut() {
        f.step(time.delta_secs_f64() as f32, false, &mut transforms);
    }
    let v = &o.vigor;
    // Bulbs.
    let lit = model::lit_bulbs(v.page, &v.sheet);
    for n in 1..=10 {
        for s in model::glow_shapes(n) {
            if let Some(&e) = o.nodes.get(&s) {
                if let Ok(mut vis) = visibility.get_mut(e) {
                    vis.set_if_neq(if n <= lit { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
        }
    }
    // The review page's arrows and values, the points remaining, the arrow buttons.
    let mut want: Vec<(String, String)> = Vec::new();
    for (i, s) in STATS.iter().enumerate() {
        let (down, up) = model::review_arrows(&v.sheet, i);
        let chosen = o.keyed_row && v.row == i && v.page == Page::Review;
        for (shape, shown, raise) in [(format!("Index_{s}Decrease_Btn:0"), down, false), (format!("Index_{s}Increase_Btn:0"), up, true)] {
            if let Some(&e) = o.nodes.get(&shape) {
                if let Ok(mut vis) = visibility.get_mut(e) {
                    vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
            want.push((shape, model::press_texture(raise, chosen).to_string()));
        }
        want.push((format!("Index_{s}PointVal:0"), model::number_texture(v.sheet.values[i])));
    }
    let (tens, ones) = model::two_digits(v.sheet.remaining());
    want.push((model::TENS_SHAPE.into(), model::number_texture(tens)));
    want.push((model::ONES_SHAPE.into(), model::number_texture(ones)));
    want.push(("P1_RT_Btn:0".into(), model::rt_texture(model::next_enabled(v.page, &v.sheet)).into()));
    want.push(("LookInside_Btn:0".into(), model::rt_texture(true).into()));
    want.push(("P1_LT_Btn:0".into(), model::LT_TEXTURE.into()));
    for (shape, tex) in want {
        let Some(&e) = o.nodes.get(&shape) else { continue };
        let Some(h) = o.materials.get(&e) else { continue };
        if o.shown.get(&e) == Some(&tex) {
            continue;
        }
        let img = ctx.textures.get(&tex, true, &ctx.data, &mut ctx.images, ctx.formats.as_deref());
        if let (Some(img), Some(mut m)) = (img, ctx.materials.get_mut(h)) {
            m.base.base_color_texture = Some(img);
        }
        o.shown.insert(e, tex);
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<VigorStats>) {
    if exit.read().next().is_none() || stats.opened == 0 {
        return;
    }
    info!(
        "vigor tester summary: opened {} times, {} changes, {} page turns, {} clicks on its buttons, done {} times",
        stats.opened, stats.changes, stats.turns, stats.clicks, stats.done
    );
}
