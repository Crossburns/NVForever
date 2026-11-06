//! The Vigor Tester's picture (`nvf_play::chargen::vigor`, docs/spec-notes/vigor-tester.md
//! §6): while its menu is open the machine is drawn by a camera and lights of its own, and
//! the room behind it is blurred and darkened. All of it ends when the menu closes.
//!
//! - **Its own camera** [design]: Bevy has no per-mesh light list; a view takes every light
//!   whose render layers meet its own (bevy_light 0.20.1 `cluster/assign.rs`). So the machine
//!   ([`VIGOR_LAYER`]), its camera and its lights share a layer no other view has: the room's
//!   lights do not reach the machine and the rig does not reach the room. The camera sits on
//!   the player's (a child, the same place) with its field of view, so the machine stands
//!   where `nvf_play::chargen::vigor::view` put it and clicks, which project through the
//!   player's camera, still land on it.
//! - **The picture** [design: as the lockpicking menu's, `lockpick_view.rs`]: the camera
//!   draws into an image the size of the window, clear where nothing is drawn, shown as a
//!   full-screen image under every other UI node. Being drawn after the main camera's image
//!   space, the machine stays sharp over the blurred room with no depth rule (the Pip-Boy's
//!   `sharp` is not needed). As with those menus' cameras, systems asking for the one
//!   `Camera3d` (level of detail, grass, streaming) wait while the menu is open.
//! - **The light** [design: tuned by eye against the original's shots, which show a warm
//!   shaded machine: cream poster, dark wood, brass]: a warm key above and to the left of the
//!   eye, a dimmer fill low to the right, a warm ambient light. Bevy's sum applies (the view
//!   has no display-value mark, lights.rs `DISPLAY_HELPERS`) with the game's attenuation, in
//!   the units of `lights::lumens`, not scaled by `--light-scale`: the machine looks the
//!   same in every room and setting.
//! - **The room** [design]: the image-space blur at [`ROOM_BLUR`] and black over it at
//!   [`ROOM_DARK`], both with the fade.
//! - **The fade** [documented: love_tester_menu.xml, a menu fade of 0.25 s]: the picture and
//!   the room's blur and darkening come in from [`FADE_FROM`] over [`FADE_SECONDS`] and go out
//!   to nothing over the same; the sequences are not touched. On closing the camera is gone at
//!   once and its image holds the machine's last frame.
//!
//! Log lines start `vigor view:`.

use crate::imagespace::{black_over, ImageSpaceGrade, LensGrade};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::AmbientLight;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use nvf_play::chargen::vigor::{VigorMenu, VIGOR_LAYER};
use nvf_play::pause::AidTime;
use nvf_play::PlayerCamera;

/// The ambient light on the machine (linear light on a white surface) and its colour
/// [design: to tune against the original's shots].
const AMBIENT: f32 = 0.14;
const AMBIENT_COLOR: Color = Color::srgb(1.0, 0.88, 0.7);
/// The key light: its brightness (`lights::lumens`), colour and place in the camera's space
/// (+x right, +y up, −z ahead; the board plane is about 11.6 ahead, 5.2 from the middle to
/// the top of the screen) [design: to tune against the original's shots].
const KEY: f32 = 0.66;
const KEY_COLOR: Color = Color::srgb(1.0, 0.9, 0.72);
const KEY_AT: Vec3 = Vec3::new(-2.0, 5.0, -5.0);
/// The fill light, as the key [design: to tune against the original's shots].
const FILL: f32 = 0.15;
const FILL_COLOR: Color = Color::srgb(1.0, 0.85, 0.7);
const FILL_AT: Vec3 = Vec3::new(4.0, -3.0, -3.0);
/// The lights' reach: short, so the machine falls off towards its edges as in the original's
/// shots [design: to tune against them].
const LIGHT_RANGE: f32 = 24.0;
/// The room behind the machine: the blur, in the modifiers' blur radius units [design: the
/// wait menu's, imagespace.rs `WAIT_MENU_BLUR`], and black over it [design: what the sheet
/// had, matched by eye to the margins of the owner's screenshots].
const ROOM_BLUR: f32 = 3.0;
const ROOM_DARK: f32 = 0.45;
/// The menu's fade, seconds [documented: love_tester_menu.xml].
const FADE_SECONDS: f32 = 0.25;
/// Where the fade in starts: not from nothing [design: the original's first frames show the
/// machine in place with no slow fade to speak of, sampled every 3 frames].
const FADE_FROM: f32 = 0.6;
/// Beyond the machine, game units: far enough for its back and base [design].
const CAMERA_FAR: f32 = 200.0;
/// Under every other UI node [design].
const PICTURE_Z: GlobalZIndex = GlobalZIndex(-100);

pub struct VigorViewPlugin;

impl Plugin for VigorViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fade>().add_systems(Update, show.after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync)).add_systems(PostUpdate, room);
    }
}

/// How far the machine and what it does to the room are in (0 … 1).
#[derive(Resource, Default)]
struct Fade(f32);

#[derive(Component)]
struct VigorCamera;

#[derive(Default)]
struct Drawn {
    /// The camera with the lights below it, while the menu is open.
    camera: Option<Entity>,
    /// The full-screen node and the image it shows, until the fade out is over.
    picture: Option<(Entity, Handle<Image>)>,
    /// When the menu opened, and when it closed.
    opened: f32,
    closed: Option<f32>,
}

/// The fade in, `since` seconds after opening.
fn fade_in(since: f32) -> f32 {
    FADE_FROM + (1.0 - FADE_FROM) * (since / FADE_SECONDS).clamp(0.0, 1.0)
}

/// The fade out, `since` seconds after closing, from the fade in's `from` at the time.
fn fade_out(since: f32, from: f32) -> f32 {
    from.min(1.0 - (since / FADE_SECONDS).clamp(0.0, 1.0))
}

/// The room's blur and fade colour with the machine in by `fade`: the blur on screen stays if
/// stronger, the fade is laid under the black.
fn room_grade(blur: f32, fade_color: [f32; 4], fade: f32) -> (f32, [f32; 4]) {
    if fade <= 0.0 {
        return (blur, fade_color);
    }
    (blur.max(ROOM_BLUR * fade), black_over(fade_color, ROOM_DARK * fade))
}

/// The window's size in pixels, the image's.
fn image_size(window: &Query<&Window, With<PrimaryWindow>>) -> Extent3d {
    window.single().map(|w| Extent3d { width: w.physical_width().max(1), height: w.physical_height().max(1), depth_or_array_layers: 1 }).unwrap_or(Extent3d { width: 1280, height: 720, depth_or_array_layers: 1 })
}

/// The camera, the lights and the full-screen node.
fn spawn(commands: &mut Commands, images: &mut Assets<Image>, size: Extent3d, player: Entity, projection: &PerspectiveProjection) -> (Entity, Entity, Handle<Image>) {
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    let layer = RenderLayers::layer(VIGOR_LAYER);
    let unit = crate::sky::light_unit();
    let camera = commands
        .spawn((
            Name::new("Vigor Tester camera"),
            VigorCamera,
            Camera3d::default(),
            Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(ImageRenderTarget::from(image.clone())),
            Projection::Perspective(PerspectiveProjection { fov: projection.fov, near: projection.near, far: CAMERA_FAR, ..default() }),
            Tonemapping::Linear,
            Msaa::Sample4,
            AmbientLight { color: AMBIENT_COLOR, brightness: AMBIENT * unit, ..default() },
            Transform::IDENTITY,
            layer.clone(),
            ChildOf(player),
        ))
        .id();
    for (name, brightness, color, at) in [("Vigor Tester key light", KEY, KEY_COLOR, KEY_AT), ("Vigor Tester fill light", FILL, FILL_COLOR, FILL_AT)] {
        commands.spawn((
            Name::new(name),
            PointLight { color, intensity: crate::lights::lumens(brightness, unit), range: LIGHT_RANGE, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(at),
            layer.clone(),
            ChildOf(camera),
        ));
    }
    let node = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    let picture = commands.spawn((Name::new("Vigor Tester picture"), node, ImageNode { color: Color::WHITE.with_alpha(0.0), ..ImageNode::new(image.clone()) }, PICTURE_Z)).id();
    (camera, picture, image)
}

#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    menu: Option<Res<VigorMenu>>,
    window: Query<&Window, With<PrimaryWindow>>,
    player: Query<(Entity, &Projection), (With<PlayerCamera>, Without<VigorCamera>)>,
    mut cameras: Query<&mut Projection, (With<VigorCamera>, Without<PlayerCamera>)>,
    mut nodes: Query<&mut ImageNode>,
    mut images: ResMut<Assets<Image>>,
    time: AidTime,
    mut fade: ResMut<Fade>,
    mut drawn: Local<Drawn>,
) {
    let now = time.elapsed_secs();
    let open = menu.as_deref().is_some_and(VigorMenu::is_open);
    if open && drawn.camera.is_none() {
        let Ok((entity, Projection::Perspective(p))) = player.single() else { return };
        // A menu opened again during the last one's fade out.
        if let Some((node, _)) = drawn.picture.take() {
            commands.entity(node).despawn();
        }
        let (camera, node, image) = spawn(&mut commands, &mut images, image_size(&window), entity, p);
        drawn.camera = Some(camera);
        drawn.picture = Some((node, image));
        drawn.opened = now;
        drawn.closed = None;
        info!("vigor view: on (key {KEY}, fill {FILL}, ambient {AMBIENT}, room blur {ROOM_BLUR}, dark {ROOM_DARK})");
    }
    if !open {
        if let Some(camera) = drawn.camera.take() {
            commands.entity(camera).despawn();
            drawn.closed = Some(now);
        }
    }
    let Some((node, image)) = drawn.picture.clone() else { return };
    let level = match drawn.closed {
        Some(at) => fade_out(now - at, fade.0),
        None => fade_in(now - drawn.opened),
    };
    fade.0 = level;
    if let Ok(mut n) = nodes.get_mut(node) {
        let color = Color::WHITE.with_alpha(level);
        if n.color != color {
            n.color = color;
        }
    }
    if drawn.closed.is_some() && level <= 0.0 {
        commands.entity(node).despawn();
        drawn.picture = None;
        info!("vigor view: off");
        return;
    }
    if !open {
        return;
    }
    // The field of view the menu keeps on the player's camera, and the window's size.
    if let (Ok((_, Projection::Perspective(from))), Some(camera)) = (player.single(), drawn.camera) {
        if let Ok(mut to) = cameras.get_mut(camera) {
            if let Projection::Perspective(to) = to.as_mut() {
                if to.fov != from.fov {
                    to.fov = from.fov;
                }
            }
        }
    }
    let size = image_size(&window);
    if images.get(&image).is_some_and(|i| i.width() != size.width || i.height() != size.height) {
        if let Some(mut i) = images.get_mut(&image) {
            i.resize(size);
        }
    }
}

/// The room behind the machine. After `Update`: `update_grade` writes every grade afresh
/// there, so nothing stays when the menu goes. A 3D scope's lens keeps its own grade.
fn room(fade: Res<Fade>, mut grades: Query<&mut ImageSpaceGrade, Without<LensGrade>>) {
    if fade.0 <= 0.0 {
        return;
    }
    for mut grade in &mut grades {
        let (blur, color) = room_grade(grade.blur, grade.fade.to_array(), fade.0);
        grade.blur = blur;
        grade.fade = Vec4::from_array(color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_play::chargen::vigor::view;

    #[test]
    fn the_fade_comes_in_and_goes_out_in_the_files_time() {
        assert_eq!(fade_in(0.0), FADE_FROM);
        assert!(fade_in(FADE_SECONDS / 2.0) > FADE_FROM && fade_in(FADE_SECONDS / 2.0) < 1.0);
        assert_eq!(fade_in(FADE_SECONDS), 1.0);
        assert_eq!(fade_in(60.0), 1.0);
        assert_eq!(fade_out(0.0, 1.0), 1.0);
        assert_eq!(fade_out(FADE_SECONDS, 1.0), 0.0);
        // Closed before it was all in: no jump upwards.
        assert_eq!(fade_out(0.0, 0.4), 0.4);
    }

    #[test]
    fn the_room_goes_back_as_it_was() {
        let fade = [1.0, 1.0, 1.0, 0.25];
        assert_eq!(room_grade(0.5, fade, 0.0), (0.5, fade));
        let (blur, color) = room_grade(0.0, [0.0; 4], 1.0);
        assert_eq!(blur, ROOM_BLUR);
        assert!((color[3] - ROOM_DARK).abs() < 1e-6);
        // A stronger blur on the screen stays; a modifier's fade stays under the black.
        let (blur, color) = room_grade(ROOM_BLUR + 1.0, fade, 1.0);
        assert_eq!(blur, ROOM_BLUR + 1.0);
        assert!(color[3] > 0.25 && color[0] < 1.0);
    }

    #[test]
    fn the_lights_stand_before_the_board_and_reach_it() {
        let board = view::board_depth(view::FIT_VFOV_DEG.to_radians());
        for at in [KEY_AT, FILL_AT] {
            assert!(-at.z < board - 1.0, "{at}");
            assert!(at.distance(Vec3::new(0.0, 0.0, -board)) < LIGHT_RANGE, "{at}");
        }
    }
}
