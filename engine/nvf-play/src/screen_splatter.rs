//! Blood on the screen: the splats scripts put there (TriggerScreenSplatter,
//! SetScreenSplatterFade, ClearScreenSplatter; docs/spec-notes/screen-splatter.md).
//!
//! The opening is the only caller in the shipped plugins (`QUST 0x00102037` stages 36–39
//! and 50, cleared at stage 100) [observed: decoded scripts]: Benny's shots leave blood
//! over the player's view. Each call puts `count` splats on the screen
//! (`fnv_game::functions::ui::SplatterTrigger`), each at a random place, with a random
//! size and opacity from the game settings (`fBloodSplatter*`) [observed: the values in
//! FalloutNV.esm; documented: their names], fitted to the original's pictures of the
//! capture job `fx_screen_splatter_look` (screen-splatter.md §5, 2026-10-18):
//!
//! - count: that many splats; 100 darkened the screen about seven times as much as 10, so
//!   `iBloodSplatterMaxCount` (25) does not cap a call [observed]; what that setting
//!   bounds is not known. At most [`MAX_SPLATS`] here [engineering choice];
//! - size: between `fBloodSplatterMinSize` (0.12) and `fBloodSplatterMaxSize` (0.18)
//!   times the call's size factor, as a share of the screen's **width** (a square): the
//!   original's splats are about a quarter of the screen height across at factor 1
//!   [observed]; the darkening grew about as the square of the size factor [observed];
//! - opacity: between `fBloodSplatterMinOpacity` (0.5) and `fBloodSplatterMaxOpacity`
//!   (1.0), or with the chance `fBloodSplatterOpacityChance` (0.3) between
//!   `fBloodSplatterMinOpacity2` (0.3) and `fBloodSplatterMaxOpacity2` (0.6)
//!   [hypothesis], times the call's opacity factor with no cap at 1: factor 2 darkened
//!   about two and a half times as much as 1 [observed]. A texel's alpha is the shape's
//!   alpha times the opacity, at most 1, so a strong splat's soft edge spreads;
//! - time: the call's duration (else `fBloodSplatterDuration`, 20 s); the splat fades
//!   from its start to nothing at its end: stage 50's call (100 20 2 2) darkened the
//!   screen less at 5.5, 10.5 and 15.5 s and not at all at 21 s [observed]. Whether
//!   SetScreenSplatterFade changes that cannot be seen, since the call quits the original
//!   (§5); its share and switch are used as [hypothesis], and `fBloodSplatterFadeStart`
//!   (0.4) is not used (the pictures show fading before 40 % of the time).
//!
//! The picture is the game's own textures: the shape is the alpha of a cell of
//! `textures\gore\screenbloodalpha01opt.dds`, the colour the same cell of
//! `textures\gore\screenbloodcolor01opt.dds` (both 2×2 atlases of four splats) [observed:
//! Fallout - Textures2.bsa; hypothesis: the atlas pair, not the single-splat
//! `screenbloodalpha01`–`03` / `screenbloodcolor01`, is the one New Vegas draws]: dark
//! soft blobs with a red core, as the original's [observed]. The splats lie over the
//! world and under every menu and the HUD.
//!
//! **Blood when the player is hurt** (screen-splatter.md §4) [hypothesis throughout: the
//! wiki says only that the original puts blood on the screen "after taking damage"]:
//! each hit that takes Health from the player (`crate::combat::HitFeedback::player_hurt`;
//! not DamageActorValue, which put none there [observed], not god mode) adds
//! `ceil(lost / maximum × iBloodSplatterMaxCount)` splats, at least one, at the call
//! factors 1 and the setting's duration, while the screen holds fewer than
//! `iBloodSplatterMaxCount` (25) splats: the setting's use here.
//!
//! **The flare** (§4): each splat carries a small glint, `textures\gore\screenblooddot02.dds`
//! (a red glow on black [observed]) drawn as a lightening over the splat,
//! `fBloodSplatterFlareSize` (0.065) of the screen's width times the call's size factor
//! across, off the splat's centre by up to `fBloodSplatterFlareOffsetScale` (0.45) of its
//! half-side, as strong as the splat's opacity now beyond `fBloodSplatterFlareMult`
//! (0.95) [hypothesis, fitted: the original showed glints at the cores only with the
//! opacity factor 2, §5].
//!
//! The splats age with the game's frames while nothing holds the game (the pause menu,
//! a loading screen). `[ScreenSplatter] bScreenSplatterEnabled=1` is the original's INI
//! default [documented: Fallout_default.ini]; there is no switch to turn it off here.
//!
//! Log: `screen splatter: <n> splats on the screen (TriggerScreenSplatter <count>
//! <duration> <size> <opacity>)`, `screen splatter: cleared`, `screen splatter: the last
//! splat has faded`, `screen splatter: <n> splats on the screen (hurt: <lost> of <max>
//! health)`, `screen splatter: <texture>: <why>` when the textures cannot be read (then
//! nothing is drawn; without the flare texture only the flares are not).

use crate::sim::SimState;
use crate::PlaySet;
use bevy::asset::RenderAssetUsages;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use fnv_game::functions::ui::SplatterTrigger;
use nvf_assets::GameData;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// The splat shapes (alpha) and colours [observed: Fallout - Textures2.bsa].
pub const ALPHA_ATLAS: &str = "textures\\gore\\screenbloodalpha01opt.dds";
pub const COLOR_ATLAS: &str = "textures\\gore\\screenbloodcolor01opt.dds";
/// The flare: a red glow on black, the opaque one of the two dots (the other a dark
/// red with soft alpha) [observed: Fallout - Textures2.bsa; hypothesis: the flare's].
pub const FLARE_DOT: &str = "textures\\gore\\screenblooddot02.dds";
/// The most splats on the screen; beyond it a new splat takes the oldest's place
/// [engineering choice: one picture per splat].
pub const MAX_SPLATS: usize = 1000;
/// Cells per side of the atlases (2×2) [observed: four splats in each].
const ATLAS_SIDE: usize = 2;
/// Steps of the alpha gain baked into a strong splat's picture (a quarter).
const GAIN_STEPS: f32 = 4.0;

/// The game settings that shape the splats, with the values FalloutNV.esm gives them
/// [observed 2026-10-18] as defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplatterSettings {
    pub min_size: f32,
    pub max_size: f32,
    pub min_opacity: f32,
    pub max_opacity: f32,
    pub min_opacity2: f32,
    pub max_opacity2: f32,
    pub opacity_chance: f32,
    pub duration: f32,
    /// `iBloodSplatterMaxCount`: the most splats a hurt adds up to (hurt splats).
    pub max_count: f32,
    /// The flare: side (share of the width), strength threshold, offset (share of the
    /// splat's half-side).
    pub flare_size: f32,
    pub flare_mult: f32,
    pub flare_offset_scale: f32,
}

impl Default for SplatterSettings {
    fn default() -> Self {
        SplatterSettings { min_size: 0.12, max_size: 0.18, min_opacity: 0.5, max_opacity: 1.0, min_opacity2: 0.3, max_opacity2: 0.6, opacity_chance: 0.3, duration: 20.0, max_count: 25.0, flare_size: 0.065, flare_mult: 0.95, flare_offset_scale: 0.45 }
    }
}

impl SplatterSettings {
    /// From the load order's game settings; a missing one keeps its default.
    pub fn from_settings(get: impl Fn(&str) -> Option<f32>) -> SplatterSettings {
        let d = SplatterSettings::default();
        let f = |name: &str, v: f32| get(name).filter(|x| x.is_finite()).unwrap_or(v);
        SplatterSettings {
            min_size: f("fBloodSplatterMinSize", d.min_size),
            max_size: f("fBloodSplatterMaxSize", d.max_size),
            min_opacity: f("fBloodSplatterMinOpacity", d.min_opacity),
            max_opacity: f("fBloodSplatterMaxOpacity", d.max_opacity),
            min_opacity2: f("fBloodSplatterMinOpacity2", d.min_opacity2),
            max_opacity2: f("fBloodSplatterMaxOpacity2", d.max_opacity2),
            opacity_chance: f("fBloodSplatterOpacityChance", d.opacity_chance),
            duration: f("fBloodSplatterDuration", d.duration),
            max_count: f("iBloodSplatterMaxCount", d.max_count),
            flare_size: f("fBloodSplatterFlareSize", d.flare_size),
            flare_mult: f("fBloodSplatterFlareMult", d.flare_mult),
            flare_offset_scale: f("fBloodSplatterFlareOffsetScale", d.flare_offset_scale),
        }
    }
}

/// One splat on the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Splat {
    /// Its centre, as shares of the screen's width and height from the top left.
    pub center: Vec2,
    /// Its side, as a share of the screen's width.
    pub size: f32,
    /// Its opacity; above 1 its soft edge spreads.
    pub opacity: f32,
    pub duration: f32,
    pub age: f32,
    /// Its atlas cell (0..4).
    pub cell: usize,
    /// Its flare's centre off the splat's, as shares of the splat's side.
    pub flare_offset: Vec2,
    /// Its flare's side, as a share of the screen's width.
    pub flare_side: f32,
}

impl Splat {
    /// Its opacity now: full until the fade starts (`fade`: the share of its time, or
    /// none), then down to 0 at its end.
    pub fn alpha(&self, fade: Option<f32>) -> f32 {
        if self.age >= self.duration {
            return 0.0;
        }
        let Some(start) = fade.map(|s| s.clamp(0.0, 1.0) * self.duration) else { return self.opacity };
        if self.age <= start {
            return self.opacity;
        }
        self.opacity * (1.0 - (self.age - start) / (self.duration - start).max(1e-6))
    }
}

/// A flare's strength for its splat's opacity now: what the opacity has beyond
/// `fBloodSplatterFlareMult`, at most 1 [hypothesis, fitted: glints only at the
/// opacity factor 2 in the original's pictures].
pub fn flare_strength(alpha: f32, flare_mult: f32) -> f32 {
    (alpha - flare_mult).clamp(0.0, 1.0)
}

/// How a splat of opacity `alpha` is drawn: the gain baked into its picture's alpha (in
/// quarters, 4 = none) and the opacity left for the picture as a whole (at most 1).
pub fn gain_and_rest(alpha: f32) -> (u16, f32) {
    if alpha <= 1.0 {
        return (GAIN_STEPS as u16, alpha.max(0.0));
    }
    let q = (alpha * GAIN_STEPS).ceil().min(f32::from(u16::MAX));
    (q as u16, alpha * GAIN_STEPS / q)
}

/// A uniform number in [0, 1).
fn unit(rng: &mut ChaCha8Rng) -> f32 {
    (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
}

/// The splats one call puts on the screen.
pub fn splats_for(t: &SplatterTrigger, s: &SplatterSettings, rng: &mut ChaCha8Rng) -> Vec<Splat> {
    let duration = t.duration.unwrap_or(s.duration).max(0.0);
    let n = (t.count as usize).min(MAX_SPLATS);
    (0..n)
        .map(|_| {
            let lerp = |a: f32, b: f32, x: f32| a + (b - a) * x;
            let center = Vec2::new(unit(rng), unit(rng));
            let size = lerp(s.min_size, s.max_size, unit(rng)) * t.size;
            let second = unit(rng) < s.opacity_chance;
            let base = if second { lerp(s.min_opacity2, s.max_opacity2, unit(rng)) } else { lerp(s.min_opacity, s.max_opacity, unit(rng)) };
            let cell = (rng.next_u32() as usize) % (ATLAS_SIDE * ATLAS_SIDE);
            let (turn, reach) = (unit(rng) * std::f32::consts::TAU, unit(rng) * s.flare_offset_scale * 0.5);
            let flare_offset = Vec2::new(turn.cos(), turn.sin()) * reach;
            Splat { center, size, opacity: (base * t.opacity).max(0.0), duration, age: 0.0, cell, flare_offset, flare_side: s.flare_size * t.size }
        })
        .collect()
}

/// The splats a hit that took `lost` of the player's `max` Health adds, with `on` splats
/// already on the screen: `ceil(lost / max × iBloodSplatterMaxCount)`, at least one,
/// while the screen holds fewer than that setting; factors 1, the setting's duration
/// [hypothesis].
pub fn hurt_splats(lost: f32, max: f32, on: usize, s: &SplatterSettings, rng: &mut ChaCha8Rng) -> Vec<Splat> {
    let cap = s.max_count.clamp(0.0, MAX_SPLATS as f32) as usize;
    if lost.is_nan() || lost <= 0.0 || on >= cap {
        return Vec::new();
    }
    let share = if max > 0.0 { (lost / max).min(1.0) } else { 1.0 };
    let n = ((share * s.max_count).ceil() as usize).max(1).min(cap - on);
    splats_for(&SplatterTrigger { count: n as u32, duration: None, size: 1.0, opacity: 1.0 }, s, rng)
}

/// Add new splats; beyond `max` the oldest go.
pub fn add_splats(on: &mut Vec<Splat>, new: Vec<Splat>, max: usize) {
    on.extend(new);
    if on.len() > max {
        on.drain(..on.len() - max);
    }
}

/// The four splat pictures: each atlas cell's colour with the shape's alpha (RGBA8).
pub fn compose(alpha: &[u8], color: &[u8], side: usize) -> Vec<Vec<u8>> {
    let cell = side / ATLAS_SIDE;
    (0..ATLAS_SIDE * ATLAS_SIDE)
        .map(|c| {
            let (cx, cy) = ((c % ATLAS_SIDE) * cell, (c / ATLAS_SIDE) * cell);
            let mut px = Vec::with_capacity(cell * cell * 4);
            for y in 0..cell {
                for x in 0..cell {
                    let i = ((cy + y) * side + cx + x) * 4;
                    px.extend_from_slice(&[color[i], color[i + 1], color[i + 2], alpha[i + 3]]);
                }
            }
            px
        })
        .collect()
}

/// A picture's alpha times `gain` quarters, at most 255.
pub fn with_gain(px: &[u8], gain: u16) -> Vec<u8> {
    let mut out = px.to_vec();
    for a in out.iter_mut().skip(3).step_by(4) {
        *a = (u32::from(*a) * u32::from(gain) / GAIN_STEPS as u32).min(255) as u8;
    }
    out
}

/// The flare's picture from the dot's colours: its red-glow colour at full strength,
/// with the glow (its brightest channel) as alpha, so that over the dark splat the
/// blend adds about the glow (RGBA8).
pub fn flare_picture(dot: &[u8]) -> Vec<u8> {
    let peak = dot.chunks(4).map(|t| t[0].max(t[1]).max(t[2])).max().unwrap_or(0).max(1);
    let mut out = Vec::with_capacity(dot.len());
    for t in dot.chunks(4) {
        let glow = t[0].max(t[1]).max(t[2]);
        let scale = |c: u8| if glow == 0 { 0 } else { (u32::from(c) * 255 / u32::from(glow)) as u8 };
        out.extend_from_slice(&[scale(t[0]), scale(t[1]), scale(t[2]), (u32::from(glow) * 255 / u32::from(peak)) as u8]);
    }
    out
}

/// The splat pictures: the composed cells, and the images made of them per gain; the
/// flare's, if its texture could be read.
struct Pictures {
    cells: Vec<Vec<u8>>,
    side: u32,
    images: HashMap<(usize, u16), Handle<Image>>,
    flare: Option<Handle<Image>>,
}

impl Pictures {
    fn image(&mut self, cell: usize, gain: u16, images: &mut Assets<Image>) -> Handle<Image> {
        let cell = cell % self.cells.len();
        let side = self.side;
        let px = &self.cells[cell];
        self.images
            .entry((cell, gain))
            .or_insert_with(|| {
                let px = if gain == GAIN_STEPS as u16 { px.clone() } else { with_gain(px, gain) };
                images.add(Image::new(Extent3d { width: side, height: side, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD))
            })
            .clone()
    }
}

#[derive(Resource)]
pub struct ScreenSplatter {
    pub splats: Vec<Splat>,
    /// SetScreenSplatterFade's share and switch.
    fade: Option<(f32, bool)>,
    settings: Option<SplatterSettings>,
    /// The splat pictures, once read (`Err`: they cannot be).
    pictures: Option<Result<Pictures, String>>,
    clears_seen: u64,
    rng: ChaCha8Rng,
    root: Option<Entity>,
}

impl Default for ScreenSplatter {
    fn default() -> Self {
        ScreenSplatter { splats: Vec::new(), fade: None, settings: None, pictures: None, clears_seen: 0, rng: ChaCha8Rng::seed_from_u64(0x5B1A_77E2), root: None }
    }
}

/// The share of a splat's time after which it fades, or none: from its start unless
/// SetScreenSplatterFade said otherwise [observed: the original's fade; hypothesis: the
/// call's meaning].
pub fn fade_share(fade: Option<(f32, bool)>) -> Option<f32> {
    match fade {
        Some((share, true)) => Some(share),
        Some((_, false)) => None,
        None => Some(0.0),
    }
}

#[derive(Component)]
struct SplatRoot;
#[derive(Component)]
struct SplatPicture(usize);
#[derive(Component)]
struct FlarePicture(usize);

pub struct ScreenSplatterPlugin;

impl Plugin for ScreenSplatterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScreenSplatter>().add_systems(Update, (take_requests, take_hurts, age, draw).chain().in_set(PlaySet::Ui));
    }
}

/// Read and compose the splat pictures.
fn read_pictures(data: &GameData, images: &mut Assets<Image>) -> Result<Pictures, String> {
    let decode = |path: &str| -> Result<(usize, Vec<u8>), String> {
        let bytes = data.read(path).ok_or_else(|| format!("{path}: not in the game's files"))?;
        let dds = fnv_dds::Dds::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
        let (w, h) = (dds.header.width, dds.header.height);
        if w == 0 || w != h || !(w as usize).is_multiple_of(ATLAS_SIDE) {
            return Err(format!("{path}: {w}x{h}, not a square atlas"));
        }
        let rgba = fnv_dds::decode_blocks(dds.format, w, h, dds.levels[0][0]).or_else(|| fnv_dds::to_rgba8(dds.format, dds.levels[0][0])).ok_or_else(|| format!("{path}: its format is not decoded"))?;
        Ok((w as usize, rgba))
    };
    let (side, alpha) = decode(ALPHA_ATLAS)?;
    let (side2, color) = decode(COLOR_ATLAS)?;
    if side != side2 {
        return Err(format!("{COLOR_ATLAS}: {side2} wide, the shapes {side}"));
    }
    let flare = match decode(FLARE_DOT) {
        Ok((dot_side, dot)) => {
            let d = dot_side as u32;
            match nvf_assets::texture::checked_image(d, d, 1, TextureFormat::Rgba8UnormSrgb, flare_picture(&dot)) {
                Ok(mut img) => {
                    img.asset_usage = RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;
                    Some(images.add(img))
                }
                Err(e) => {
                    warn!("screen splatter: {FLARE_DOT}: {e}; no flares are drawn");
                    None
                }
            }
        }
        Err(e) => {
            warn!("screen splatter: {e}; no flares are drawn");
            None
        }
    };
    Ok(Pictures { cells: compose(&alpha, &color, side), side: (side / ATLAS_SIDE) as u32, images: HashMap::default(), flare })
}

/// The splats hits on the player add (`HitFeedback::player_hurt`).
fn take_hurts(mut splatter: ResMut<ScreenSplatter>, feedback: Option<ResMut<crate::combat::HitFeedback>>, sim: Option<Res<SimState>>, tweaks: Option<Res<crate::tweaks::TweakSettings>>) {
    use crate::tweaks::wears_power_armor;
    let Some(mut feedback) = feedback else { return };
    if feedback.player_hurt.is_empty() {
        return;
    }
    // The player's tweak: no blood on the screen while the player wears power armour on
    // the body [design: docs/design/tweaks.md].
    if tweaks.is_some_and(|t| t.power_armor_no_screen_blood) && sim.as_deref().is_some_and(|s| wears_power_armor(&s.world, fnv_sim::FormId::PLAYER_REF)) {
        feedback.player_hurt.clear();
        return;
    }
    let sp = &mut *splatter;
    let settings = *sp.settings.get_or_insert_with(|| settings_of(sim.as_deref()));
    for (lost, max) in std::mem::take(&mut feedback.player_hurt) {
        let new = hurt_splats(lost, max, sp.splats.len(), &settings, &mut sp.rng);
        if new.is_empty() {
            continue;
        }
        add_splats(&mut sp.splats, new, MAX_SPLATS);
        info!("screen splatter: {} splats on the screen (hurt: {lost:.1} of {max:.0} health)", sp.splats.len());
    }
}

/// The splat settings from the load order's game settings.
fn settings_of(sim: Option<&SimState>) -> SplatterSettings {
    sim.map_or_else(SplatterSettings::default, |s| SplatterSettings::from_settings(|n| s.forms.setting_f32(n)))
}

/// The scripts' calls (`UiRequests::splatter`): clear, fade, new splats.
fn take_requests(mut splatter: ResMut<ScreenSplatter>, script: Option<ResMut<crate::script::ScriptState>>, sim: Option<Res<SimState>>) {
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    let asked = &mut rt.game.ui.splatter;
    let sp = &mut *splatter;
    if asked.clears != sp.clears_seen {
        sp.clears_seen = asked.clears;
        if !sp.splats.is_empty() {
            info!("screen splatter: cleared");
        }
        sp.splats.clear();
    }
    sp.fade = asked.fade;
    if asked.triggers.is_empty() {
        return;
    }
    let settings = *sp.settings.get_or_insert_with(|| settings_of(sim.as_deref()));
    for t in std::mem::take(&mut asked.triggers) {
        let new = splats_for(&t, &settings, &mut sp.rng);
        add_splats(&mut sp.splats, new, MAX_SPLATS);
        info!(
            "screen splatter: {} splats on the screen (TriggerScreenSplatter {} {} {} {})",
            sp.splats.len(),
            t.count,
            t.duration.unwrap_or(settings.duration),
            t.size,
            t.opacity
        );
    }
}

/// The splats age while nothing holds the game; spent ones go.
fn age(time: Res<Time>, paused: Res<crate::pause::Paused>, loading: Res<crate::loading::Loading>, mut splatter: ResMut<ScreenSplatter>) {
    if splatter.splats.is_empty() || paused.0 || loading.active() {
        return;
    }
    let dt = time.delta_secs();
    for s in &mut splatter.splats {
        s.age += dt;
    }
    splatter.splats.retain(|s| s.age < s.duration);
    if splatter.splats.is_empty() {
        info!("screen splatter: the last splat has faded");
    }
}

/// Draw the splats: one picture each over the screen, under the HUD and menus.
fn draw(
    mut commands: Commands,
    mut splatter: ResMut<ScreenSplatter>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut pictures: Query<(&SplatPicture, &mut Node, &mut ImageNode), Without<FlarePicture>>,
    mut flares: Query<(&FlarePicture, &mut Node, &mut ImageNode), Without<SplatPicture>>,
) {
    let sp = &mut *splatter;
    if sp.splats.is_empty() {
        if let Some(root) = sp.root.take() {
            commands.entity(root).despawn();
        }
        return;
    }
    let Some(data) = data else { return };
    let pics = match sp.pictures.get_or_insert_with(|| read_pictures(&data, &mut images)) {
        Ok(p) => p,
        Err(e) => {
            warn!("screen splatter: {e}; nothing is drawn");
            sp.splats.clear();
            return;
        }
    };
    let Ok(w) = windows.single() else { return };
    let (width, height) = (w.width(), w.height());
    let fade = fade_share(sp.fade);
    let settings = sp.settings.unwrap_or_default();
    // The pictures follow the splats: rebuilt when their number changed.
    let count = pictures.iter().count();
    if sp.root.is_none() || count != sp.splats.len() {
        if let Some(root) = sp.root.take() {
            commands.entity(root).despawn();
        }
        let laid: Vec<(Node, ImageNode)> = sp.splats.iter().map(|s| layout(s, pics, &mut images, fade, width, height)).collect();
        let flared: Vec<(Node, ImageNode)> = pics.flare.as_ref().map_or_else(Vec::new, |f| sp.splats.iter().map(|s| flare_layout(s, f, &settings, fade, width, height)).collect());
        let root = commands
            .spawn((SplatRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, GlobalZIndex(-1), Pickable::IGNORE))
            .with_children(|r| {
                for (i, (node, image)) in laid.into_iter().enumerate() {
                    r.spawn((SplatPicture(i), image, node, Pickable::IGNORE));
                }
                // The flares over every splat (later children draw on top).
                for (i, (node, image)) in flared.into_iter().enumerate() {
                    r.spawn((FlarePicture(i), image, node, Pickable::IGNORE));
                }
            })
            .id();
        sp.root = Some(root);
    }
    for (pic, mut node, mut image) in &mut pictures {
        let Some(s) = sp.splats.get(pic.0) else { continue };
        (*node, *image) = layout(s, pics, &mut images, fade, width, height);
    }
    if let Some(f) = pics.flare.as_ref() {
        for (pic, mut node, mut image) in &mut flares {
            let Some(s) = sp.splats.get(pic.0) else { continue };
            (*node, *image) = flare_layout(s, f, &settings, fade, width, height);
        }
    }
}

/// A splat's flare: place, size and strength.
fn flare_layout(s: &Splat, flare: &Handle<Image>, settings: &SplatterSettings, fade: Option<f32>, width: f32, height: f32) -> (Node, ImageNode) {
    let side = s.flare_side * width;
    let splat_side = s.size * width;
    let (x, y) = (s.center.x * width + s.flare_offset.x * splat_side, s.center.y * height + s.flare_offset.y * splat_side);
    let node = Node { position_type: PositionType::Absolute, width: Val::Px(side), height: Val::Px(side), left: Val::Px(x - side / 2.0), top: Val::Px(y - side / 2.0), ..default() };
    let strength = flare_strength(s.alpha(fade), settings.flare_mult);
    (node, ImageNode { image_mode: NodeImageMode::Stretch, color: Color::srgba(1.0, 1.0, 1.0, strength), ..ImageNode::new(flare.clone()) })
}

/// A splat's place, size and picture on a window of `width` × `height`.
fn layout(s: &Splat, pics: &mut Pictures, images: &mut Assets<Image>, fade: Option<f32>, width: f32, height: f32) -> (Node, ImageNode) {
    let side = s.size * width;
    let node = Node { position_type: PositionType::Absolute, width: Val::Px(side), height: Val::Px(side), left: Val::Px(s.center.x * width - side / 2.0), top: Val::Px(s.center.y * height - side / 2.0), ..default() };
    let (gain, rest) = gain_and_rest(s.alpha(fade));
    let image = ImageNode { image_mode: NodeImageMode::Stretch, color: Color::srgba(1.0, 1.0, 1.0, rest), ..ImageNode::new(pics.image(s.cell, gain, images)) };
    (node, image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger(count: u32, duration: Option<f32>, size: f32, opacity: f32) -> SplatterTrigger {
        SplatterTrigger { count, duration, size, opacity }
    }

    #[test]
    fn a_call_puts_its_count_on_the_screen_within_the_settings_ranges() {
        let s = SplatterSettings::default();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let splats = splats_for(&trigger(12, Some(25.0), 2.0, 1.0), &s, &mut rng);
        assert_eq!(splats.len(), 12);
        for p in &splats {
            assert!(p.size >= s.min_size * 2.0 && p.size <= s.max_size * 2.0, "{p:?}");
            assert!(p.opacity >= s.min_opacity2 && p.opacity <= s.max_opacity, "{p:?}");
            assert!((0.0..1.0).contains(&p.center.x) && (0.0..1.0).contains(&p.center.y));
            assert_eq!(p.duration, 25.0);
            assert!(p.cell < 4);
        }
        // The opacity factor with no cap at 1 (the original's factor 2 darkened more than
        // 1); no duration: the setting's.
        let strong = splats_for(&trigger(50, None, 1.0, 2.0), &s, &mut rng);
        assert!(strong.iter().all(|p| p.opacity >= 0.6 && p.opacity <= 2.0 && p.duration == 20.0));
        assert!(strong.iter().any(|p| p.opacity > 1.0));
    }

    #[test]
    fn a_call_is_not_capped_at_the_max_count_setting() {
        let s = SplatterSettings::default();
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut on = Vec::new();
        // The original's 100 darkened about seven times as much as 10.
        add_splats(&mut on, splats_for(&trigger(100, Some(20.0), 2.0, 2.0), &s, &mut rng), MAX_SPLATS);
        assert_eq!(on.len(), 100);
        add_splats(&mut on, splats_for(&trigger(u32::MAX, Some(21.0), 1.0, 1.0), &s, &mut rng), MAX_SPLATS);
        assert_eq!(on.len(), MAX_SPLATS);
        assert!(on.iter().all(|p| p.duration == 21.0), "beyond the engine's bound the newest stay");
    }

    #[test]
    fn a_hurt_adds_splats_by_the_health_lost_up_to_the_max_count() {
        let s = SplatterSettings::default();
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        // A tenth of the health: 2.5 of 25, so 3 splats at the call factors 1.
        let new = hurt_splats(10.0, 100.0, 0, &s, &mut rng);
        assert_eq!(new.len(), 3);
        assert!(new.iter().all(|p| p.duration == s.duration && p.size <= s.max_size && p.opacity <= s.max_opacity));
        assert_eq!(hurt_splats(0.1, 100.0, 0, &s, &mut rng).len(), 1, "any hurt leaves a splat");
        assert_eq!(hurt_splats(80.0, 100.0, 0, &s, &mut rng).len(), 20);
        assert_eq!(hurt_splats(80.0, 100.0, 20, &s, &mut rng).len(), 5, "up to iBloodSplatterMaxCount on the screen");
        assert!(hurt_splats(80.0, 100.0, 25, &s, &mut rng).is_empty());
        assert!(hurt_splats(0.0, 100.0, 0, &s, &mut rng).is_empty() && hurt_splats(f32::NAN, 100.0, 0, &s, &mut rng).is_empty());
        assert_eq!(hurt_splats(500.0, 100.0, 0, &s, &mut rng).len(), 25, "more than the maximum counts as all of it");
    }

    #[test]
    fn a_flare_shows_only_beyond_the_flare_mult() {
        let s = SplatterSettings::default();
        assert_eq!(flare_strength(0.9, s.flare_mult), 0.0);
        assert!(flare_strength(1.0, s.flare_mult) < 0.06, "opacity factor 1: a glint too faint to see");
        assert!((flare_strength(1.5, s.flare_mult) - 0.55).abs() < 1e-6);
        assert_eq!(flare_strength(3.0, s.flare_mult), 1.0);
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        for p in splats_for(&trigger(40, None, 2.0, 2.0), &s, &mut rng) {
            assert!((p.flare_side - 0.13).abs() < 1e-6, "FlareSize x the size factor");
            assert!(p.flare_offset.length() <= s.flare_offset_scale * 0.5 + 1e-6, "{p:?}");
        }
        // The dot's red glow on black: full red, its glow as alpha.
        assert_eq!(flare_picture(&[0, 0, 0, 255, 0x7c, 0, 0, 255, 0xf8, 0, 0, 255]), vec![0, 0, 0, 0, 255, 0, 0, 127, 255, 0, 0, 255]);
    }

    #[test]
    fn splats_fade_from_their_start_unless_told_otherwise() {
        let mut p = Splat { center: Vec2::splat(0.5), size: 0.2, opacity: 0.8, duration: 10.0, age: 0.0, cell: 0, flare_offset: Vec2::ZERO, flare_side: 0.065 };
        let default = fade_share(None);
        assert_eq!(p.alpha(default), 0.8);
        p.age = 5.0;
        assert!((p.alpha(default) - 0.4).abs() < 1e-6, "half way, half the opacity");
        assert_eq!(p.alpha(fade_share(Some((0.5, true)))), 0.8, "SetScreenSplatterFade's share");
        p.age = 7.5;
        assert!((p.alpha(fade_share(Some((0.5, true)))) - 0.4).abs() < 1e-6);
        assert_eq!(p.alpha(fade_share(Some((0.5, false)))), 0.8, "fade off: full to the end");
        p.age = 10.0;
        assert_eq!(p.alpha(None), 0.0);
    }

    #[test]
    fn a_strong_splat_bakes_its_gain_and_keeps_the_rest_under_one() {
        assert_eq!(gain_and_rest(0.7), (4, 0.7));
        assert_eq!(gain_and_rest(1.0), (4, 1.0));
        let (g, r) = gain_and_rest(1.6);
        assert_eq!(g, 7);
        assert!((r - 1.6 * 4.0 / 7.0).abs() < 1e-6 && r <= 1.0);
        let (g, r) = gain_and_rest(2.0);
        assert_eq!((g, r), (8, 1.0));
        // Twice the gain doubles a texel's alpha, at most 255.
        let px = [10, 20, 30, 100, 1, 2, 3, 200];
        assert_eq!(with_gain(&px, 8), vec![10, 20, 30, 200, 1, 2, 3, 255]);
    }

    #[test]
    fn settings_come_from_the_game_settings() {
        let s = SplatterSettings::from_settings(|n| match n {
            "fBloodSplatterDuration" => Some(5.0),
            "fBloodSplatterMinSize" => Some(f32::NAN),
            _ => None,
        });
        assert_eq!(s.duration, 5.0);
        assert_eq!(s.min_size, 0.12, "a setting that is not a number keeps the default");
    }

    #[test]
    fn each_picture_is_a_cell_colour_with_the_shape_alpha() {
        // A 4×4 atlas: 2×2 cells of 2×2 texels; colour red = the cell index, alpha = 10 + cell.
        let side = 4;
        let mut alpha = vec![0u8; side * side * 4];
        let mut color = vec![0u8; side * side * 4];
        for y in 0..side {
            for x in 0..side {
                let c = (y / 2) * 2 + x / 2;
                let i = (y * side + x) * 4;
                color[i] = c as u8;
                color[i + 3] = 255;
                alpha[i] = 99;
                alpha[i + 3] = 10 + c as u8;
            }
        }
        let cells = compose(&alpha, &color, side);
        assert_eq!(cells.len(), 4);
        for (c, px) in cells.iter().enumerate() {
            assert_eq!(px.len(), 16);
            assert!(px.chunks(4).all(|t| t[0] == c as u8 && t[3] == 10 + c as u8));
        }
    }
}
