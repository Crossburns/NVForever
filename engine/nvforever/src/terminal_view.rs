//! The terminal's monitor (`nvf_play::terminal`, docs/spec-notes/hacking.md §7): the
//! game's model `meshes\terminals\terminalinterface01.nif`, drawn by a camera of its own
//! into a picture shown over the darkened world ([`nvf_play::terminal::MonitorView`]),
//! with the terminal's screen on its glass and its power button leaving the terminal.
//!
//! - The parts, by shape name [observed: the file's nodes]: the case
//!   `TerminalInterface01:0`; the glass `screen:0` (textured `pipboy3000\pipboy.dds`, the
//!   rendered screen's stand-in) and over it `glare:0` (`TerminalScreenGlare.dds`, alpha
//!   blended); the power button `PowerButton:0` and its glow `Plane01:0`, under the screen
//!   on the right; three point lights (`Omni02`, `03`, `05`), not read.
//! - The screen's picture (`nvf_play::terminal::TerminalImage`) goes on `screen:0` unlit,
//!   as a lit display [hypothesis], spanning `screen::TEXTURE_UNITS` of the canvas.
//!   [design] Keep `glare:0`, added in linear light at a per-channel scale (§7), which
//!   gives the top-right streak [observed: owner's 2026-10-31 reference] without the
//!   floor the display-value add puts over the textured glass.
//! - The camera looks at the front (along +Y) with the field of view
//!   `fRenderedTerminalFOV` 0.15 [documented: Fallout_default.ini `[RenderedTerminal]`;
//!   hypothesis: radians, vertical], looking `fRenderedTerminalVPos` 0.38 below the
//!   model's origin [documented; hypothesis: model units; observed: the owner's
//!   screenshot puts the origin 0.37 above the middle of the view], from [`DISTANCE`] in
//!   front [observed: fitted to the same screenshot, where the glass is 0.735 of the view's
//!   height; `fRenderedTerminalZoom` 36 is not that distance as such]. Lights of its
//!   own on a render layer of its own [design], as the lockpicking view's
//!   (`lockpick_view.rs`).
//! - The window's pointer is followed onto the glass and the button on CPU copies of
//!   their shapes: where it lies on the glass goes to the screen
//!   (`nvf_play::terminal::TerminalPointer`), and a left click on the button leaves the
//!   terminal (the owner, 2026-10-16).

use crate::pipboy_arm::Posed;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::AmbientLight;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use nvf_assets::model::ModelCtx;
use nvf_play::terminal::screen::{CANVAS, TEXTURE_UNITS};
use nvf_play::terminal::{MonitorView, PowerAid, TerminalImage, TerminalInput, TerminalPointer, TerminalScreen, TARGET};

pub const MONITOR_MODEL: &str = "meshes\\terminals\\terminalinterface01.nif";
/// The render layer of the monitor, its lights and its camera.
pub const MONITOR_LAYER: usize = 8;
/// Where the monitor is put, far from the world and the lockpicking lock (Bevy space)
/// [design].
const PLACE: Vec3 = Vec3::new(0.0, -210_000.0, 0.0);
/// The camera's field of view, radians, and how far below the model's origin it looks
/// [documented: `[RenderedTerminal]`; hypothesis: their units, module docs].
const FOV: f32 = 0.15;
const DROP: f32 = 0.38;
/// The camera's distance in front of the model's origin, model units [observed, module
/// docs].
pub const DISTANCE: f32 = 38.5;

/// [design] The glare is added in linear light, as a plain unlit additive material on
/// the model's own texture, not through `nif_material.wesl`'s display-value add. The
/// shape is additive in the model (NiAlphaProperty flags 4109: blend on, SRC_ALPHA /
/// ONE; `TerminalScreenGlare.dds` DXT1, alpha 1 throughout, 4 to 15 in its dark parts
/// and (90, 110, 112) at the streak's peak [observed: fnvtool nif decode, PIL,
/// 2026-10-31]). [observed: in game, 1280 x 720, 2026-10-31, hacking.md §7] Through the
/// display-value add the dark texels put a floor of (13-26, 13-24, 7-22) over the whole
/// glass and (+30, 0, +25) on the attempt blocks, and the original shows no such floor
/// (corners (8, 11, 5), blocks (59-65, 255, 148-152) under a streak of +80); added in
/// linear light the same texels put 1 to 4 units on the glass and the streak where the
/// original has it. [hypothesis] A counter-example to nif.md §6's display-value add
/// for this one shape; the engine's rule for every other additive shape is untouched.
/// The linear scale on the texture, per channel: the original's peak (100, 125, 95)
/// less its glass (21, 44, 26), over the texture's peak, in linear light.
/// Recipe: on the hacking smoke shot measure the 10x10 streak peak; scale a channel
/// by (linear(target) - linear(glass beside it)) / (linear(measured) - linear(glass)).
const GLARE_LINEAR_SCALE: Vec3 = Vec3::new(1.19, 1.14, 0.64);

pub struct TerminalViewPlugin;

impl Plugin for TerminalViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (follow_pointer.before(nvf_play::PlaySet::Ui), show.after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync)));
    }
}

/// The monitor as drawn: its entities, its camera, and the glass and the button posed
/// where they are drawn.
#[derive(Default)]
struct Drawn {
    entities: Vec<Entity>,
    glass: Option<Posed>,
    button: Option<Posed>,
    warned: bool,
}

#[derive(Resource)]
struct Monitor {
    camera: Entity,
    glass: Posed,
    button: Option<Posed>,
}

/// A shape's name as one of the monitor's parts.
fn is_glass(name: &str) -> bool {
    name.eq_ignore_ascii_case("screen:0")
}

fn is_button(name: &str) -> bool {
    name.eq_ignore_ascii_case("PowerButton:0")
}

#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    screen: Res<TerminalScreen>,
    picture: Option<Res<TerminalImage>>,
    view: Option<Res<MonitorView>>,
    mut models: ModelCtx,
    mut plain: ResMut<Assets<StandardMaterial>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut drawn: Local<Drawn>,
) {
    let Some(picture) = picture.filter(|_| screen.0.is_some()) else {
        if !drawn.entities.is_empty() || view.is_some() {
            for e in drawn.entities.drain(..) {
                commands.entity(e).despawn();
            }
            *drawn = Drawn { warned: drawn.warned, ..default() };
            commands.remove_resource::<MonitorView>();
            commands.remove_resource::<Monitor>();
        }
        return;
    };
    if !drawn.entities.is_empty() {
        return;
    }
    let Some(model) = models.model_with_cpu(MONITOR_MODEL) else {
        if !drawn.warned {
            warn!("terminal view: {MONITOR_MODEL} not loaded; the screen is drawn flat");
            drawn.warned = true;
        }
        return;
    };
    let size = window.single().map(|w| Extent3d { width: w.physical_width().max(1), height: w.physical_height().max(1), depth_or_array_layers: 1 }).unwrap_or(Extent3d { width: 1280, height: 720, depth_or_array_layers: 1 });
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = models.images.add(image);
    let layer = RenderLayers::layer(MONITOR_LAYER);
    // Game space (Z up) into Bevy's (Y up), as the world root does.
    let place = Affine3A::from_translation(PLACE) * Affine3A::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let mut entities = Vec::new();
    for s in &model.shapes {
        let name = s.name.as_deref().unwrap_or("");
        let world = place * model.root * s.transform;
        let mut material = s.material.clone();
        let mut glare = None;
        if is_glass(name) {
            let mut m = models.materials.get(&s.material).cloned().unwrap_or_default();
            m.base.base_color = Color::WHITE;
            m.base.base_color_texture = Some(picture.image.clone());
            // [design] Preserve the measured texels instead of applying the model's
            // emissive colour to an already lit display (nif_material.wesl).
            m.base.emissive_texture = None;
            m.base.emissive = LinearRgba::BLACK;
            m.base.normal_map_texture = None;
            m.base.unlit = true;
            m.base.alpha_mode = AlphaMode::Opaque;
            m.base.uv_transform = bevy::math::Affine2::from_scale(TEXTURE_UNITS / TARGET.as_vec2());
            material = models.materials.add(m);
            drawn.glass = s.cpu.as_deref().and_then(|m| Posed::new(m, None, Mat4::from(world)));
        } else if name.eq_ignore_ascii_case("glare:0") {
            // [design] The model's texture on Bevy's own additive material: its colour,
            // texel × GLARE_LINEAR_SCALE, is added in linear light (GLARE_LINEAR_SCALE).
            let mut m = models.materials.get(&s.material).map(|m| m.base.clone()).unwrap_or_default();
            m.base_color = Color::linear_rgb(GLARE_LINEAR_SCALE.x, GLARE_LINEAR_SCALE.y, GLARE_LINEAR_SCALE.z);
            m.emissive = LinearRgba::BLACK;
            m.emissive_texture = None;
            m.normal_map_texture = None;
            m.unlit = true;
            m.alpha_mode = AlphaMode::Add;
            glare = Some(plain.add(m));
        } else if is_button(name) {
            drawn.button = s.cpu.as_deref().and_then(|m| Posed::new(m, None, Mat4::from(world)));
        }
        let mut shape = commands.spawn((Name::new(format!("Terminal monitor {name}")), Mesh3d(s.mesh.clone()), Transform::from_matrix(Mat4::from(world)), layer.clone()));
        match glare {
            Some(plain) => shape.insert(MeshMaterial3d(plain)),
            None => shape.insert(MeshMaterial3d(material)),
        };
        entities.push(shape.id());
    }
    let Some(glass) = drawn.glass.take() else {
        warn!("terminal view: {MONITOR_MODEL} has no screen:0 shape; the screen is drawn flat");
        for e in entities {
            commands.entity(e).despawn();
        }
        drawn.warned = true;
        return;
    };
    let shapes = entities.len();
    let to = |p: Vec3| place.transform_point3(p);
    let eye = Vec3::new(0.0, -DISTANCE, -DROP);
    let aim = Vec3::new(0.0, 0.0, -DROP);
    let camera = commands
        .spawn((
            Name::new("Terminal monitor camera"),
            Camera3d::default(),
            Camera { order: -4, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(ImageRenderTarget::from(image.clone())),
            Projection::Perspective(PerspectiveProjection { fov: FOV, near: 1.0, far: 200.0, ..default() }),
            Tonemapping::Linear,
            Msaa::Sample4,
            AmbientLight { color: Color::WHITE, brightness: 150.0, ..default() },
            Transform::from_translation(to(eye)).looking_at(to(aim), Vec3::Y),
            layer.clone(),
        ))
        .id();
    // A key light from the upper left in front and a fill from the right [design: as
    // bright as makes the case about the owner's screenshot's, (29, 28, 18)].
    let key = commands
        .spawn((DirectionalLight { illuminance: 2400.0, ..default() }, Transform::from_translation(to(Vec3::new(-20.0, -30.0, 25.0))).looking_at(to(Vec3::ZERO), Vec3::Y), layer.clone()))
        .id();
    let fill = commands
        .spawn((DirectionalLight { illuminance: 600.0, ..default() }, Transform::from_translation(to(Vec3::new(30.0, -20.0, -5.0))).looking_at(to(Vec3::ZERO), Vec3::Y), layer.clone()))
        .id();
    entities.extend([camera, key, fill]);
    info!("terminal view: the monitor drawn ({shapes} shapes, power button {}), the camera {DISTANCE} in front", if drawn.button.is_some() { "found" } else { "missing" });
    commands.insert_resource(Monitor { camera, glass, button: drawn.button.take() });
    commands.insert_resource(MonitorView { image });
    drawn.entities = entities;
}

/// Where a spot of the glass's texture lies on the canvas.
pub fn canvas_point(uv: Vec2) -> Option<Vec2> {
    let p = uv * TEXTURE_UNITS;
    (p.x >= 0.0 && p.y >= 0.0 && p.x < CANVAS.x && p.y < CANVAS.y).then_some(p)
}

/// Follow the window's pointer onto the monitor: on the glass it goes to the screen, a
/// left click on the power button leaves the terminal. The test aid `power` clicks the
/// button's middle.
fn follow_pointer(
    monitor: Option<Res<Monitor>>,
    cams: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut aid: ResMut<PowerAid>,
    mut pointer: ResMut<TerminalPointer>,
    mut out: MessageWriter<TerminalInput>,
) {
    let Some(m) = monitor else { return };
    let Ok((cam, at)) = cams.get(m.camera) else { return };
    let Ok(window) = windows.single() else { return };
    let aimed = aid.0.then(|| m.button.as_ref().and_then(Posed::middle).and_then(|p| cam.world_to_viewport(at, p).ok())).flatten();
    if aid.0 && aimed.is_none() {
        warn!("test aid: the terminal's power button is not in view");
        aid.0 = false;
    }
    // The camera draws a picture of the window's size, one pixel to a pixel.
    let cursor = aimed.or_else(|| window.physical_cursor_position());
    let ray = cursor.and_then(|c| cam.viewport_to_world(at, c).ok());
    pointer.canvas = None;
    let Some(ray) = ray else { return };
    let (origin, dir) = (ray.origin, Vec3::from(ray.direction));
    let glass = m.glass.hit(origin, dir);
    let button = m.button.as_ref().and_then(|b| b.hit(origin, dir));
    match (glass, button) {
        (Some((t, uv)), b) if b.is_none_or(|(bt, _)| t < bt) => pointer.canvas = canvas_point(uv),
        (_, Some(_)) if mouse.just_pressed(MouseButton::Left) || aid.0 => {
            info!("terminal: the power button pressed");
            out.write(TerminalInput::Exit);
            aid.0 = false;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canvas_starts_at_the_glass_corner() {
        assert_eq!(canvas_point(Vec2::ZERO), Some(Vec2::ZERO));
        // The glass's far corner (0.767, 0.755) is past the canvas's right edge.
        assert_eq!(canvas_point(Vec2::new(0.767, 0.755)), None);
        assert!(canvas_point(Vec2::new(0.7, 0.7)).is_some());
    }
}
