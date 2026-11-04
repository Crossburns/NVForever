//! The Pip-Boy device's screen treatment (docs/spec-notes/pipboy.md §3), one owner around
//! the shared image ([`PipBoyScreen`]), so that whatever paints the pages into it gets the
//! same screen: the undercoat (the screen camera's clear colour), the background picture
//! under the pages, the scanlines over them, and the coefficients the body's material
//! (nvforever `pipboy_arm.rs`) puts the image and the glare on the model with.
//!
//! A provisional static fit. Every coefficient here is
//! [hypothesis: fitted to the two measurements in pipboy.md §3], the empty screen and the
//! solid amber, unless it says otherwise; a capture of the original's screen is to refine
//! them, and they are all in this file so that its numbers can replace them. Flicker,
//! vertical hold and shudder are not built.
//!
//! The path from the image to the frame [design: `pipboy_arm.rs`, `nvf_assets::nif_spawn`
//! `unlit_vertex_colors`, `imagespace.wesl`]: image × [`SCREEN_TINT`] ×
//! [`SHAPE_VERTEX_COLOUR`], plus the glare's texel × [`GLARE_LINEAR_SCALE`] ×
//! [`SHAPE_VERTEX_COLOUR`], through the world camera's tone mapping and image-space grade.
//! The screen menu (`Frame::Screen`) has none of this: no undercoat, no background, no
//! scanlines, no glass (docs/design/screen-menu.md §2).
//!
//! Tuning on the `pipboy` shot, 1280 × 720, noon in Goodsprings, 10 × 10 medians,
//! remeasuring after each change, in this order:
//! - [`EMPTY_SCENE`], on the empty middle away from ink and patch: scale a channel by
//!   linear(target) / linear(measured).
//! - [`GLARE_LINEAR_SCALE`], on the top right patch: scale a channel by (linear(target) −
//!   linear(empty)) / (linear(measured) − linear(empty)).
//! - [`SCANLINE_DEPTH`], on a solid bar's rows: scale by the target dip over the measured
//!   dip, as parts of the bright row's display value.
//! - [`SCREEN_TINT`] stays unless the solid amber has moved from (220, 180, 64).

use super::frame::{scanline_column, Ctx, Kit};
use super::{Frame, PipBoy, PipBoyScreen, CANVAS};
use crate::interface::{at, AMBER_RGB};
use crate::terminal::screen::{lifted, scanline_column as baked_column, scanline_pixels};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use nvf_assets::GameData;

/// What the screen multiplies the menus' picture by (linear): so that their brightest
/// amber comes out as the original's on a 1280 × 720 frame at noon in Goodsprings
/// [observed: fitted; the owner's ITEMS screenshot's solid CND bar is about (220, 180,
/// 64), and with this the menus' brightest text is (221, 179, 59); unscaled it was
/// (237, 167, 53), the view's tone mapping squeezing the brightest values].
pub const SCREEN_TINT: [f32; 3] = [0.83, 1.17, 1.27];

/// The vertex colour both the screen's and the glare's shapes carry, in linear light: 0.81
/// in `pipboyarm.nif` [observed: `fnvtool nif decode`], taken as a display value on an
/// unlit shape (`nif_spawn::unlit_vertex_colors`).
pub const SHAPE_VERTEX_COLOUR: f32 = 0.624;

/// The light in the view (before tone mapping and grade) that comes out as the empty
/// screen's (31, 27, 15) [hypothesis: fitted to the two measurements in pipboy.md §3:
/// Bevy's TonyMcMapface table and an image-space grade fitted to our own three readings,
/// (237, 167, 53), (221, 179, 59) and (15, 9, 0), all within about 2 units; through the same
/// model the amber with this floor under it stays (220, 180, 63)]. Noon in Goodsprings:
/// the grade changes with the place and the hour.
pub const EMPTY_SCENE: [f32; 3] = [0.00909, 0.00774, 0.0043];

/// The glare's linear scale on `ScreenGlare.dds` [hypothesis: fitted to the two
/// measurements in pipboy.md §3, which do not tell glare from undercoat: strong enough for
/// the faint lighter patch at the top right of the owner's screenshots, about 10 units
/// over the empty screen, while the amber moves by under one]. The terminal's
/// `GLARE_LINEAR_SCALE` is measured for its own glass and is not this.
pub const GLARE_LINEAR_SCALE: [f32; 3] = [0.25, 0.25, 0.25];

/// The glare texture's median texel in linear light [observed: `ScreenGlare.dds`, 512 ×
/// 512 DXT1, alpha 1 throughout, read 2026-11-04]: what the sheet adds away from its
/// streak, taken off the undercoat.
const GLARE_MEDIAN_TEXEL: [f32; 3] = [0.00243, 0.00368, 0.00304];

/// The background picture, at brightness 90 over the whole canvas [documented: the menu
/// files, as `frame.rs`]; its centre texel is (76, 77, 76), alpha 1 [observed: the file,
/// hacking.md §7].
const BACKGROUND: &str = "Interface\\Shared\\Background\\pipboy.dds";
const BACKGROUND_BRIGHTNESS: f32 = 90.0;
const BACKGROUND_MIDDLE: f32 = 76.0 / 255.0;
/// How much of the undercoat the picture covers. What it adds stays as at brightness 90
/// over black, so its light and its vignette are as they were; the undercoat shows through
/// [design: the terminal's way, `terminal::screen::BACKGROUND_ALPHA`; the value is its].
const BACKGROUND_ALPHA: f32 = 0.4;

/// The scanlines: the band of `pipboyscanlines.dds`, again every 9.5 of the canvas, dimming
/// the display values by about 15% at its dark end [observed: pipboy.md §3, the solid
/// bar's rows on the owner's ITEMS screenshot].
const SCANLINES: &str = "textures\\pipboy3000\\pipboyscanlines.dds";
pub const SCANLINE_PITCH: f32 = 9.5;
pub const SCANLINE_DEPTH: f32 = 0.15;

fn linear(v: f32) -> f32 {
    Color::srgb(v, 0.0, 0.0).to_linear().red
}

/// The background picture's colour over black: amber at its brightness, in linear light.
fn background_light(rgb: [u8; 3]) -> Vec3 {
    Vec3::from_array(rgb.map(|c| linear(f32::from(c) / 255.0 * BACKGROUND_BRIGHTNESS / 255.0)))
}

/// What the glare adds to the view away from its streak.
fn glare_floor() -> Vec3 {
    Vec3::from_array(GLARE_MEDIAN_TEXEL) * Vec3::from_array(GLARE_LINEAR_SCALE) * SHAPE_VERTEX_COLOUR
}

/// What the image must average over the empty middle of the screen, before scanlines.
fn glass_target() -> Vec3 {
    (Vec3::from_array(EMPTY_SCENE) - glare_floor()) / (Vec3::from_array(SCREEN_TINT) * SHAPE_VERTEX_COLOUR)
}

/// [design] Solve target = picture × alpha + undercoat × (1 − alpha) in the middle, at the
/// default amber: the undercoat is the glass, which does not follow the interface's colour
/// [hypothesis]. Before the scanlines' lift.
fn glass_undercoat() -> Vec3 {
    let picture = background_light(AMBER_RGB) * linear(BACKGROUND_MIDDLE);
    ((glass_target() - picture) / (1.0 - BACKGROUND_ALPHA)).max(Vec3::ZERO)
}

/// The undercoat, the screen camera's clear colour on the device: raised by what the
/// scanlines take away on average (`kept`), as the terminal's is.
pub fn undercoat(kept: f32) -> Color {
    let u = glass_undercoat();
    lifted(Color::linear_rgb(u.x, u.y, u.z), kept)
}

/// The picture's colour and alpha over the undercoat: what it adds stays the amber's at its
/// brightness, raised as the undercoat is. Follows the interface's colour.
fn picture_tint(rgb: [u8; 3], kept: f32) -> Color {
    let p = background_light(rgb) / BACKGROUND_ALPHA;
    lifted(Color::linear_rgba(p.x, p.y, p.z, BACKGROUND_ALPHA), kept)
}

/// The band's darkening rescaled to `depth` at its dark end, in display values. A band with
/// no dark end stays as it is.
pub fn band_at_depth(band: &[f32], depth: f32) -> Vec<f32> {
    let dip = 1.0 - band.iter().copied().fold(1.0, f32::min);
    if dip <= 0.0 {
        return band.to_vec();
    }
    band.iter().map(|&l| 1.0 - depth * (1.0 - l) / dip).collect()
}

/// The treatment's parts made once.
#[derive(Resource)]
pub struct ScreenFx {
    scanlines: Option<Handle<Image>>,
    /// How much of the light the scanlines keep on average, in linear light (1 without).
    pub kept: f32,
    baked: bool,
}

impl Default for ScreenFx {
    fn default() -> Self {
        ScreenFx { scanlines: None, kept: 1.0, baked: false }
    }
}

impl ScreenFx {
    /// One baked column over the whole canvas, a value per row, as the terminal's: no tile
    /// per line, whose fractional pitch rounded to whole rows. A broken file gives no
    /// scanlines, not a GPU error.
    fn bake(&mut self, data: Option<&GameData>, images: &mut Assets<Image>) {
        if self.baked {
            return;
        }
        self.baked = true;
        let Some(band) = data.and_then(|d| d.read(SCANLINES)).and_then(|b| scanline_column(&b)).filter(|b| !b.is_empty()) else { return };
        let column = baked_column(&band_at_depth(&band, SCANLINE_DEPTH), CANVAS.y as usize, SCANLINE_PITCH);
        let (rgba, kept) = scanline_pixels(&column);
        let format = bevy::render::render_resource::TextureFormat::Rgba8Unorm;
        let Ok(mut image) = nvf_assets::texture::checked_image(1, column.len() as u32, 1, format, rgba).map_err(|e| warn!("pipboy: {SCANLINES}: {e}")) else { return };
        image.asset_usage = bevy::asset::RenderAssetUsages::default();
        image.sampler = bevy::image::ImageSampler::linear();
        self.scanlines = Some(images.add(image));
        self.kept = kept;
    }
}

/// A layer of the treatment in the image; they outlive the pages' redraws.
#[derive(Component)]
pub struct ScreenFxLayer;

/// Under every page and over every page, in the image's own stack.
const UNDER_PAGES: GlobalZIndex = GlobalZIndex(-1);
const OVER_PAGES: GlobalZIndex = GlobalZIndex(10);

/// The treatment shows with the device's frame, not in the screen menu.
pub fn wanted(pip: &PipBoy) -> bool {
    pip.shown() && pip.frame == Frame::Device
}

/// Keep the layers: the picture under the pages and the scanlines over them, while the
/// device's frame shows. The pages are drawn between, by whichever painter draws them.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    pip: Res<PipBoy>,
    screen: Option<Res<PipBoyScreen>>,
    layers: Query<Entity, With<ScreenFxLayer>>,
    mut fx: ResMut<ScreenFx>,
    mut kit: ResMut<Kit>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
) {
    let Some(screen) = screen else { return };
    let want = wanted(&pip);
    if want == !layers.is_empty() {
        return;
    }
    if !want {
        for e in &layers {
            commands.entity(e).despawn();
        }
        info!("pipboy: screen treatment off");
        return;
    }
    let mut c = Ctx::new(&mut kit, data.as_deref(), &mut images, formats.as_deref());
    fx.bake(c.data, &mut *c.images);
    let (rgb, kept) = (crate::interface::pipboy_rgb(), fx.kept);
    let whole = || at(0.0, 0.0, CANVAS.x, CANVAS.y);
    commands.spawn((ScreenFxLayer, whole(), UiTargetCamera(screen.camera), UNDER_PAGES, Pickable::IGNORE, Name::new("Pip-Boy screen background"))).with_children(|r| {
        if c.image(r, BACKGROUND, whole(), picture_tint(rgb, kept)).is_none() {
            r.spawn((whole(), BackgroundColor(crate::interface::pipboy_amber(BACKGROUND_BRIGHTNESS).with_alpha(0.1))));
        }
    });
    if let Some(lines) = fx.scanlines.clone() {
        commands.spawn((
            ScreenFxLayer,
            whole(),
            ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(lines) },
            UiTargetCamera(screen.camera),
            OVER_PAGES,
            Pickable::IGNORE,
            Name::new("Pip-Boy screen scanlines"),
        ));
    }
    let shown = undercoat(kept).to_srgba().to_u8_array();
    info!("pipboy: screen treatment on (scanlines {}, keeping {kept:.2} of the light; undercoat ({}, {}, {}))", if fx.scanlines.is_some() { "baked" } else { "none" }, shown[0], shown[1], shown[2]);
}

#[cfg(test)]
mod tests {
    use super::super::{Page, PageBody, PipBoyRoot, Phase};
    use super::*;

    /// The band of the file as pipboy.md §3 describes it: white at the top, 0.8 at the foot.
    fn band() -> Vec<f32> {
        (0..64).map(|i| 1.0 - 0.2 * i as f32 / 63.0).collect()
    }

    fn over(top: Color, under: LinearRgba) -> LinearRgba {
        let t = top.to_linear();
        LinearRgba::rgb(t.red * t.alpha + under.red * (1.0 - t.alpha), t.green * t.alpha + under.green * (1.0 - t.alpha), t.blue * t.alpha + under.blue * (1.0 - t.alpha))
    }

    /// The layers in order at one row of the column: undercoat, picture × tint, what is on
    /// top of it (a page's opaque ink, if any), the scanline's bytes.
    fn image_row(texel: f32, ink: Option<Color>, alpha_byte: u8, kept: f32) -> LinearRgba {
        let tint = picture_tint(AMBER_RGB, kept).to_linear();
        let t = linear(texel);
        let picture = Color::linear_rgba(tint.red * t, tint.green * t, tint.blue * t, tint.alpha);
        let mut c = over(picture, undercoat(kept).to_linear());
        if let Some(ink) = ink {
            c = over(ink, c);
        }
        over(Color::srgba_u8(0, 0, 0, alpha_byte), c)
    }

    #[test]
    fn the_column_has_a_row_per_canvas_row_the_pitch_and_the_depth() {
        let column = baked_column(&band_at_depth(&band(), SCANLINE_DEPTH), CANVAS.y as usize, SCANLINE_PITCH);
        assert_eq!(column.len(), 768);
        let (lo, hi) = column.iter().fold((1.0f32, 0.0f32), |(lo, hi), &k| (lo.min(k), hi.max(k)));
        // In linear light; `linear(0.85)` is a dip of 15% in the display values.
        assert!((hi - 1.0).abs() < 0.03, "{hi}");
        assert!((lo - linear(1.0 - SCANLINE_DEPTH)).abs() < 0.03, "{lo}");
        // Two lines on is 19 rows, a whole number: the same place in the band.
        for y in 0..700 {
            assert!((column[y] - column[y + 19]).abs() < 1e-3, "row {y}");
        }
    }

    #[test]
    fn a_band_is_rescaled_to_the_depth_and_a_white_one_stays() {
        let b = band_at_depth(&[1.0, 0.9, 0.8], 0.15);
        assert!((b[0] - 1.0).abs() < 1e-6 && (b[1] - 0.925).abs() < 1e-5 && (b[2] - 0.85).abs() < 1e-5, "{b:?}");
        assert_eq!(band_at_depth(&[1.0; 4], 0.15), vec![1.0; 4]);
    }

    #[test]
    fn the_empty_screen_averages_the_fit_through_every_layer() {
        // The same bytes and mean as `bake`, then the layers as `draw` and `camera_on` lay
        // them, and the material's product and the glare's floor (`pipboy_arm.rs`).
        let column = baked_column(&band_at_depth(&band(), SCANLINE_DEPTH), CANVAS.y as usize, SCANLINE_PITCH);
        let (rgba, kept) = scanline_pixels(&column);
        assert!(kept > 0.7 && kept < 1.0, "{kept}");
        let mut sum = Vec3::ZERO;
        for p in rgba.as_chunks::<4>().0 {
            let c = image_row(BACKGROUND_MIDDLE, None, p[3], kept);
            sum += Vec3::new(c.red, c.green, c.blue);
        }
        let image = sum / column.len() as f32;
        let scene = image * Vec3::from_array(SCREEN_TINT) * SHAPE_VERTEX_COLOUR + glare_floor();
        let want = Vec3::from_array(EMPTY_SCENE);
        assert!((scene - want).abs().max_element() < 2e-5, "{scene} {want}");
        // The undercoat stays a colour of its own: nothing is clipped away in the solve.
        assert!(glass_undercoat().min_element() > 0.0, "{}", glass_undercoat());
    }

    #[test]
    fn opaque_amber_keeps_its_value_on_the_bright_rows() {
        // The band's white rows take nothing from the ink, and the undercoat is behind it.
        let amber = crate::interface::tint(AMBER_RGB, 255.0);
        let c = image_row(BACKGROUND_MIDDLE, Some(amber), 0, 0.8);
        let a = amber.to_linear();
        assert!((c.red - a.red).abs() < 1e-5 && (c.green - a.green).abs() < 1e-5 && (c.blue - a.blue).abs() < 1e-5);
        // The glare's floor is small beside it (so the amber moves by under a unit).
        let scene = Vec3::new(a.red, a.green, a.blue) * Vec3::from_array(SCREEN_TINT) * SHAPE_VERTEX_COLOUR;
        assert!(glare_floor().x < 0.01 * scene.x && glare_floor().y < 0.01 * scene.y && glare_floor().z < 0.05 * scene.z);
    }

    fn app(frame: Frame) -> App {
        let mut app = App::new();
        app.init_resource::<PipBoy>()
            .init_resource::<Kit>()
            .init_resource::<ScreenFx>()
            .init_resource::<Assets<Image>>()
            .insert_resource(PipBoyScreen { image: Handle::default(), camera: Entity::PLACEHOLDER, on_model: true, pointer: None, scale: 1.0 })
            .add_systems(Update, (super::super::frame::draw, draw).chain());
        let mut pip = app.world_mut().resource_mut::<PipBoy>();
        pip.phase = Phase::Up;
        pip.frame = frame;
        app
    }

    fn layers(app: &mut App) -> Vec<Entity> {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<ScreenFxLayer>>().iter(world).collect()
    }

    #[test]
    fn the_layers_are_made_once_and_outlive_the_pages() {
        let mut app = app(Frame::Device);
        app.update();
        let first = layers(&mut app);
        assert_eq!(first.len(), 1, "the background; no scanlines without the game's data");
        // A change of page redraws the page, not the treatment, and only one page root is left.
        app.world_mut().resource_mut::<PipBoy>().page = Page::Stats;
        app.update();
        app.update();
        assert_eq!(layers(&mut app), first);
        let world = app.world_mut();
        assert_eq!(world.query_filtered::<Entity, With<PipBoyRoot>>().iter(world).count(), 1);
        assert_eq!(world.query::<&PageBody>().iter(world).count(), 1);
        // Down: gone, and back when it comes up again.
        app.world_mut().resource_mut::<PipBoy>().phase = Phase::Down;
        app.update();
        app.update();
        assert!(layers(&mut app).is_empty());
        app.world_mut().resource_mut::<PipBoy>().phase = Phase::Up;
        app.update();
        app.update();
        assert_eq!(layers(&mut app).len(), 1);
    }

    #[test]
    fn the_screen_menu_has_no_treatment() {
        let mut app = app(Frame::Screen);
        app.update();
        app.update();
        assert!(layers(&mut app).is_empty(), "flat: no background, no scanlines");
        assert!(!wanted(&PipBoy { frame: Frame::Screen, phase: Phase::Up, ..default() }));
        assert!(!wanted(&PipBoy { frame: Frame::Device, phase: Phase::Down, ..default() }));
        assert!(wanted(&PipBoy { frame: Frame::Device, phase: Phase::Raising, ..default() }));
    }
}
