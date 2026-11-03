//! Ripples on water: rain drops (while the weather is "Rainy") and things moving through
//! the surface (the camera, or the player with `--play`), as a height field around the
//! camera that the water shader adds to its wave normal.
//!
//! WATR DNAM has two sets of ripple settings, "rain" and "displacement" (starting size,
//! force, velocity, falloff, dampner) [documented: names]. Read here as [hypothesis]:
//! a damped wave simulation per set, with `force` the height a drop or a moving body
//! pushes down, `velocity` the wave-equation coefficient (c², kept at the 2-D stability
//! limit 0.5: NVCleanWater stores 0.6 for both) and `falloff` the damping per step (0.985
//! for both). Starting size and dampner are not used (unknown); the drop and wake radii,
//! rain rate and ripple height are tuned by eye. The game's own simulation (grid size,
//! step rate) is not known.

use crate::reflection::ReflectionCamera;
use crate::sky::SkyState;
use crate::water::{DistantWater, WaterCache, WaterMaterial};
use crate::world::{bevy_to_game, Streamer, WorldData};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fnv_world::cell_of;
use fnv_world::sky::{RippleParams, Water};
use std::collections::HashMap;

/// Grid cells per side.
pub const N: usize = 256;
/// Game units per grid cell [tuned].
const TEXEL: f32 = 4.0;
/// Simulation steps per second [hypothesis].
const STEP: f32 = 1.0 / 60.0;
/// Game units of surface height per unit of simulated height [tuned by eye].
const HEIGHT: f32 = 16.0;
/// Rain drops per second over the grid (1024 × 1024 units) [tuned].
const DROPS_PER_SECOND: f32 = 240.0;
/// Radii (game units) of a rain drop and of a body moving through the water [tuned].
const DROP_RADIUS: f32 = 5.0;
const WAKE_RADIUS: f32 = 24.0;
/// A body makes ripples while its reference point is this far below / above the surface:
/// the player's feet, or the free camera [tuned].
const WAKE_BELOW: f32 = 150.0;
const WAKE_ABOVE: f32 = 40.0;
/// Share of the displacement force a moving body applies per step at full speed
/// (300 units/s), and that a body crossing the surface applies once [tuned].
const WAKE_PER_STEP: f32 = 0.03;
const SPLASH: f32 = 1.0;

/// One damped wave simulation on the grid.
struct Sim {
    cur: Vec<f32>,
    prev: Vec<f32>,
    c2: f32,
    falloff: f32,
    at_rest: bool,
}

impl Sim {
    fn new() -> Sim {
        Sim { cur: vec![0.0; N * N], prev: vec![0.0; N * N], c2: 0.5, falloff: 0.985, at_rest: true }
    }

    fn configure(&mut self, p: &RippleParams) {
        self.c2 = p.velocity.clamp(0.0, 0.5);
        self.falloff = p.falloff.clamp(0.0, 1.0);
    }

    /// h' = (2h − h_prev + c²·∇²h) · falloff, edges held at 0.
    /// Returns whether the texture needs updating, including the last step to rest.
    fn step(&mut self) -> bool {
        if self.at_rest {
            return false;
        }
        let (c2, k) = (self.c2, self.falloff);
        for y in 1..N - 1 {
            for x in 1..N - 1 {
                let i = y * N + x;
                let h = self.cur[i];
                let lap = self.cur[i - 1] + self.cur[i + 1] + self.cur[i - N] + self.cur[i + N] - 4.0 * h;
                self.prev[i] = (2.0 * h - self.prev[i] + c2 * lap) * k;
            }
        }
        std::mem::swap(&mut self.cur, &mut self.prev);
        self.settle();
        true
    }

    /// [documented: f32::MIN_POSITIVE] Subnormal values can round back to themselves
    /// under damping. Once BOTH time buffers contain only magnitudes below the
    /// smallest normal f32 (2^-126), clear them together. Above this cutoff the
    /// recurrence is unchanged; zeroing cur alone could revive a wave from prev.
    fn settle(&mut self) {
        self.at_rest = self.cur.iter().chain(&self.prev).all(|h| h.abs() < f32::MIN_POSITIVE);
        if self.at_rest {
            self.cur.fill(0.0);
            self.prev.fill(0.0);
        }
    }

    /// Move the contents by (−dx, −dy) cells (the grid moved by (dx, dy)).
    fn shift(&mut self, dx: i32, dy: i32) {
        if self.at_rest {
            return;
        }
        for buf in [&mut self.cur, &mut self.prev] {
            let old = std::mem::replace(buf, vec![0.0; N * N]);
            for y in 0..N as i32 {
                let sy = y + dy;
                if !(0..N as i32).contains(&sy) {
                    continue;
                }
                for x in 0..N as i32 {
                    let sx = x + dx;
                    if (0..N as i32).contains(&sx) {
                        buf[(y as usize) * N + x as usize] = old[(sy as usize) * N + sx as usize];
                    }
                }
            }
            clear_border(buf);
        }
        self.settle();
    }

    /// Push the surface down by `amount` (cone of `radius` cells) at grid point (x, y),
    /// raising a ring out to twice the radius by the same volume (so rain does not
    /// lower the water on average).
    fn impulse(&mut self, x: f32, y: f32, radius: f32, amount: f32) -> bool {
        let r = radius.max(1.0);
        let (x0, x1) = ((x - 2.0 * r).floor().max(1.0) as usize, ((x + 2.0 * r).ceil() as usize).min(N - 2));
        let (y0, y1) = ((y - 2.0 * r).floor().max(1.0) as usize, ((y + 2.0 * r).ceil() as usize).min(N - 2));
        let shape = |d: f32| {
            if d < r {
                -(1.0 - d / r)
            } else if d < 2.0 * r {
                1.0 - (d - 1.5 * r).abs() / (0.5 * r)
            } else {
                0.0
            }
        };
        let cells = || (y0..=y1).flat_map(move |gy| (x0..=x1).map(move |gx| (gx, gy)));
        let dist = |gx: usize, gy: usize| ((gx as f32 - x).powi(2) + (gy as f32 - y).powi(2)).sqrt();
        let (mut down, mut up) = (0.0, 0.0);
        for (gx, gy) in cells() {
            let s = shape(dist(gx, gy));
            if s < 0.0 { down -= s } else { up += s }
        }
        let ring = if up > 0.0 { down / up } else { 0.0 };
        let mut changed = false;
        for (gx, gy) in cells() {
            let s = shape(dist(gx, gy));
            let h = &mut self.cur[gy * N + gx];
            let old = *h;
            *h += amount * if s < 0.0 { s } else { s * ring };
            changed |= *h != old;
        }
        if changed {
            self.at_rest = false;
        }
        changed
    }
}

/// [design] Zero the one-cell border of a grid buffer. `Sim::step` updates only the
/// interior and reads the border as the fixed 0 edge, so the border is not simulated: a
/// value `Sim::shift` moved into it would stay there forever (across every buffer swap),
/// show as a frozen value at the edge of the texture and keep the grid from coming to rest.
fn clear_border(buf: &mut [f32]) {
    buf[..N].fill(0.0);
    buf[(N - 1) * N..].fill(0.0);
    for y in 1..N - 1 {
        buf[y * N] = 0.0;
        buf[y * N + N - 1] = 0.0;
    }
}

#[derive(Resource)]
pub struct Ripples {
    pub image: Handle<Image>,
    rain: Sim,
    wake: Sim,
    /// Game-space grid cell of index (0, 0).
    origin: IVec2,
    acc: f32,
    drops: f32,
    rng: u64,
    /// The body's last position and its height relative to the water.
    last: Option<(Vec3, f32)>,
    types: HashMap<u32, Option<Water>>,
    configured: Option<u32>,
    texture_dirty: bool,
}

impl Ripples {
    fn new(image: Handle<Image>) -> Self {
        Self {
            image,
            rain: Sim::new(),
            wake: Sim::new(),
            origin: IVec2::ZERO,
            acc: 0.0,
            drops: 0.0,
            rng: 0x9E37_79B9_7F4A_7C15,
            last: None,
            types: HashMap::new(),
            configured: None,
            texture_dirty: false,
        }
    }

    fn follow(&mut self, want: IVec2) {
        let d = want - self.origin;
        if d == IVec2::ZERO {
            return;
        }
        let was_active = !self.rain.at_rest || !self.wake.at_rest;
        if d.x.abs() >= N as i32 || d.y.abs() >= N as i32 {
            self.rain = Sim::new();
            self.wake = Sim::new();
            self.configured = None;
        } else {
            self.rain.shift(d.x, d.y);
            self.wake.shift(d.x, d.y);
        }
        self.origin = want;
        // Active waves retain the existing upload timing (the next fixed step).
        // If moving the grid removed the last wave, no step will dirty it again.
        self.texture_dirty |= was_active && self.rain.at_rest && self.wake.at_rest;
    }

    fn step(&mut self) {
        let rain = self.rain.step();
        let wake = self.wake.step();
        self.texture_dirty |= rain || wake;
    }

    fn upload(&mut self, images: &mut Assets<Image>) -> bool {
        if !self.texture_dirty {
            return false;
        }
        let Some(mut img) = images.get_mut(&self.image) else { return false };
        let bytes = img.data.get_or_insert_with(|| vec![0; N * N * 4]);
        bytes.resize(N * N * 4, 0);
        for ((a, b), pixel) in self.rain.cur.iter().zip(&self.wake.cur).zip(bytes.as_chunks_mut::<4>().0) {
            pixel.copy_from_slice(&(a + b).to_le_bytes());
        }
        self.texture_dirty = false;
        true
    }

    /// Shader uniform: game x, y of the grid's corner, units per cell, height scale (0:
    /// no ripples).
    pub fn uniform(&self, active: bool) -> Vec4 {
        let o = self.origin.as_vec2() * TEXEL;
        Vec4::new(o.x, o.y, TEXEL, if active { HEIGHT } else { 0.0 })
    }

    fn random(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        ((self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }
}

pub struct RipplesPlugin;

impl Plugin for RipplesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_ripples).add_systems(Update, update_ripples.after(crate::world::stream));
    }
}

fn setup_ripples(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; N * N * 4],
        TextureFormat::R32Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    commands.insert_resource(Ripples::new(images.add(image)));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_ripples(
    time: Res<Time>,
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    sky: Res<SkyState>,
    player: Option<Res<nvf_play::player::Player>>,
    mut dw: ResMut<DistantWater>,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<ReflectionCamera>)>,
    ripples: Option<ResMut<Ripples>>,
    mut images: ResMut<Assets<Image>>,
    cache: Res<WaterCache>,
    mut materials: ResMut<Assets<WaterMaterial>>,
) {
    let (Ok(cam), Some(mut rp)) = (camera.single(), ripples) else { return };
    let cam_game = bevy_to_game(cam.translation());
    // The moving body: the player's feet when playing, else the camera.
    let body = player.as_ref().map_or(cam_game, |p| Vec3::from_array(p.feet));
    let surface = if streamer.interior.is_none() { dw.surface(&world, cell_of(body.x, body.y)) } else { None };

    // Follow the camera.
    let want = IVec2::new((cam_game.x / TEXEL).floor() as i32, (cam_game.y / TEXEL).floor() as i32) - IVec2::splat(N as i32 / 2);
    rp.follow(want);

    let params = surface.and_then(|(_, wid)| {
        rp.types.entry(wid).or_insert_with(|| fnv_world::sky::water(world.lo, world.schema, wid)).clone().map(|w| (wid, w))
    });
    if let Some((wid, w)) = &params {
        if rp.configured != Some(*wid) {
            rp.rain.configure(&w.rain);
            rp.wake.configure(&w.displacement);
            rp.configured = Some(*wid);
        }
    }

    rp.acc = (rp.acc + time.delta_secs()).min(STEP * 4.0);
    let raining = sky.interior.is_none() && sky.weather.rainy && params.is_some();
    let mut stepped = false;
    while rp.acc >= STEP {
        rp.acc -= STEP;
        stepped = true;
        if raining {
            let force = params.as_ref().map_or(0.0, |(_, w)| w.rain.force);
            rp.drops += DROPS_PER_SECOND * STEP;
            while rp.drops >= 1.0 {
                rp.drops -= 1.0;
                let (x, y) = (rp.random() * N as f32, rp.random() * N as f32);
                rp.rain.impulse(x, y, DROP_RADIUS / TEXEL, force);
            }
        }
        rp.step();
    }

    // A body at the surface: ripples while it moves, a splash when it crosses.
    if let (Some((h, _)), Some((_, w))) = (surface, &params) {
        let rel = body.z - h;
        if let Some((last, last_rel)) = rp.last {
            let dt = time.delta_secs().max(1e-4);
            let speed = (body.truncate() - last.truncate()).length() / dt;
            let crossing = (rel < 0.0) != (last_rel < 0.0);
            let near = (-WAKE_BELOW..=WAKE_ABOVE).contains(&rel);
            let mut amount = 0.0;
            if near {
                amount += w.displacement.force * WAKE_PER_STEP * (speed / 300.0).min(1.0) * dt / STEP;
            }
            if crossing {
                amount += w.displacement.force * SPLASH * ((body.z - last.z).abs() / dt / 300.0).clamp(0.25, 1.0);
            }
            if amount > 0.0 {
                let g = (body.truncate() / TEXEL) - rp.origin.as_vec2();
                let changed = rp.wake.impulse(g.x, g.y, WAKE_RADIUS / TEXEL, amount);
                // Keep the existing timing: body impulses follow the fixed steps,
                // and reach the texture this frame only if a fixed step was due.
                rp.texture_dirty |= changed && stepped;
            }
        }
        rp.last = Some((body, rel));
    } else {
        rp.last = None;
    }

    rp.upload(&mut images);
    cache.set_ripples(&mut materials, rp.uniform(streamer.interior.is_none()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Ripples, Assets<Image>) {
        let mut images = Assets::default();
        let image = images.add(Image::new(
            Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
            TextureDimension::D2,
            vec![0; N * N * 4],
            TextureFormat::R32Float,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        ));
        (Ripples::new(image), images)
    }

    #[test]
    fn rest_checks_both_time_buffers() {
        let mut s = Sim::new();
        assert!(s.at_rest);
        assert!(!s.step());
        s.prev[N + 1] = f32::MIN_POSITIVE;
        s.settle();
        assert!(!s.at_rest, "a nonzero previous height can revive the wave");
        s.prev[N + 1] = f32::MIN_POSITIVE * 0.5;
        s.cur[N + 2] = -f32::MIN_POSITIVE * 0.5;
        s.settle();
        assert!(s.at_rest);
        assert!(s.cur.iter().chain(&s.prev).all(|&h| h == 0.0));
    }

    #[test]
    fn impulse_resumes_each_grid_and_idle_does_not_upload() {
        let (mut rp, mut images) = fixture();
        rp.step();
        assert!(!rp.upload(&mut images));
        assert!(!rp.rain.impulse(128.0, 128.0, 2.0, 0.0));
        assert!(!rp.rain.impulse(N as f32 + 20.0, 128.0, 2.0, 0.1));
        assert!(rp.rain.at_rest);
        assert!(rp.rain.impulse(128.0, 128.0, 2.0, 0.1));
        assert!(!rp.rain.at_rest);
        rp.step();
        assert!(rp.upload(&mut images));
        assert!(rp.wake.at_rest);
        assert!(!rp.upload(&mut images));
        assert!(rp.wake.impulse(100.0, 100.0, 6.0, 0.1));
        rp.step();
        assert!(!rp.wake.at_rest);
        assert!(rp.upload(&mut images));
    }

    #[test]
    fn decay_uploads_one_final_clear_and_reuses_the_buffer() {
        let (mut rp, mut images) = fixture();
        let allocation = images.get(&rp.image).unwrap().data.as_ref().unwrap().as_ptr();
        // Start near the cutoff to test the decay transition without thousands
        // of steps. Both grids must finish before the texture can stay clear.
        for s in [&mut rp.rain, &mut rp.wake] {
            s.c2 = 0.0;
            s.falloff = 0.5;
            s.impulse(128.0, 128.0, 2.0, f32::MIN_POSITIVE * 8.0);
        }
        rp.wake.falloff = 0.25;
        rp.step();
        assert!(rp.upload(&mut images));
        assert!(images.get(&rp.image).unwrap().data.as_ref().unwrap().iter().any(|&b| b != 0));
        for _ in 0..64 {
            rp.step();
            assert!(rp.upload(&mut images));
            if rp.rain.at_rest && rp.wake.at_rest {
                break;
            }
        }
        assert!(rp.rain.at_rest && rp.wake.at_rest);
        let bytes = images.get(&rp.image).unwrap().data.as_ref().unwrap();
        assert!(bytes.iter().all(|&b| b == 0));
        assert_eq!(bytes.as_ptr(), allocation);
        rp.step();
        assert!(!rp.upload(&mut images));
        assert!(!rp.texture_dirty);
        // The same grids can be woken again after the final clear.
        rp.rain.impulse(128.0, 128.0, 2.0, 0.1);
        rp.step();
        assert!(rp.upload(&mut images));
    }

    #[test]
    fn moving_the_grid_clears_stale_texture_even_without_a_step() {
        for distance in [8, N as i32] {
            let (mut rp, mut images) = fixture();
            rp.rain.impulse(2.0, 128.0, 1.0, 0.1);
            rp.step();
            assert!(rp.upload(&mut images));
            rp.follow(IVec2::new(distance, 0));
            assert!(rp.rain.at_rest && rp.wake.at_rest);
            assert!(rp.upload(&mut images));
            assert!(images.get(&rp.image).unwrap().data.as_ref().unwrap().iter().all(|&b| b == 0));
            rp.follow(IVec2::new(distance + 1, 0));
            rp.step();
            assert!(!rp.upload(&mut images));
        }
    }

    #[test]
    fn active_steps_match_the_original_recurrence() {
        let mut s = Sim::new();
        s.impulse(128.0, 128.0, DROP_RADIUS / TEXEL, 0.1);
        s.impulse(100.3, 80.7, WAKE_RADIUS / TEXEL, 0.2);
        let (mut cur, mut prev) = (s.cur.clone(), s.prev.clone());
        for _ in 0..60 {
            for y in 1..N - 1 {
                for x in 1..N - 1 {
                    let i = y * N + x;
                    let h = cur[i];
                    let lap = cur[i - 1] + cur[i + 1] + cur[i - N] + cur[i + N] - 4.0 * h;
                    prev[i] = (2.0 * h - prev[i] + s.c2 * lap) * s.falloff;
                }
            }
            std::mem::swap(&mut cur, &mut prev);
            assert!(s.step());
            assert_eq!(s.cur, cur);
            assert_eq!(s.prev, prev);
        }
    }

    fn sum(s: &Sim) -> f32 {
        s.cur.iter().sum()
    }

    #[test]
    fn drops_keep_the_mean_level() {
        let mut s = Sim::new();
        s.impulse(100.3, 80.7, 3.0, 0.1);
        assert!(s.cur.iter().any(|&h| h < -0.05));
        assert!(sum(&s).abs() < 1e-4, "volume {}", sum(&s));
    }

    #[test]
    fn waves_spread_and_die_out() {
        let mut s = Sim::new();
        s.impulse(128.0, 128.0, 2.0, 0.1);
        let energy = |s: &Sim| s.cur.iter().map(|h| h * h).sum::<f32>();
        let e0 = energy(&s);
        for _ in 0..60 {
            s.step();
        }
        // A ring has moved outwards: the centre is no longer the only disturbed place.
        assert!(s.cur[128 * N + 128 + 20].abs() > 1e-6 || s.cur[(128 + 20) * N + 128].abs() > 1e-6);
        for _ in 0..600 {
            s.step();
        }
        assert!(energy(&s) < e0 * 1e-3, "energy {} of {}", energy(&s), e0);
        assert!(s.cur.iter().all(|h| h.is_finite()));
    }

    #[test]
    fn shift_moves_the_contents() {
        let mut s = Sim::new();
        s.cur[10 * N + 10] = 1.0;
        s.at_rest = false;
        s.shift(3, -2);
        assert_eq!(s.cur[12 * N + 7], 1.0);
        assert_eq!(s.cur.iter().filter(|&&h| h != 0.0).count(), 1);
    }

    fn border_is_zero(buf: &[f32]) -> bool {
        (0..N).all(|i| buf[i] == 0.0 && buf[(N - 1) * N + i] == 0.0 && buf[i * N] == 0.0 && buf[i * N + N - 1] == 0.0)
    }

    #[test]
    fn shift_keeps_the_border_zero_in_every_direction() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (3, -2)] {
            let mut s = Sim::new();
            for y in 1..N - 1 {
                for x in 1..N - 1 {
                    s.cur[y * N + x] = 1.0;
                    s.prev[y * N + x] = -1.0;
                }
            }
            s.at_rest = false;
            s.shift(dx, dy);
            assert!(border_is_zero(&s.cur) && border_is_zero(&s.prev), "shift ({dx}, {dy})");
            // The interior keeps the moved contents: a cell whose source was interior
            // still holds it.
            assert_eq!(s.cur[128 * N + 128], 1.0);
            assert_eq!(s.prev[128 * N + 128], -1.0);
            assert!(!s.at_rest);
        }
    }

    #[test]
    fn a_wave_shifted_against_the_edge_still_comes_to_rest() {
        let (mut rp, mut images) = fixture();
        // Production damping and wave speed (Sim::new).
        assert_eq!((rp.rain.c2, rp.rain.falloff), (0.5, 0.985));
        rp.rain.impulse(2.0, 128.0, 1.0, 0.1);
        rp.step();
        assert!(rp.upload(&mut images));
        // Move the grid two cells: values from x = 2 would land in the border at x = 0.
        rp.follow(IVec2::new(2, 0));
        assert!(!rp.rain.at_rest, "interior values remain after the shift");
        assert!(border_is_zero(&rp.rain.cur) && border_is_zero(&rp.rain.prev));
        // sqrt(0.985) per step from about 0.1 to below 2^-126 takes about 11 000 steps.
        let mut steps = 0;
        while !(rp.rain.at_rest && rp.wake.at_rest) {
            assert!(steps < 20_000, "the grid never came to rest");
            rp.step();
            steps += 1;
        }
        assert!(rp.upload(&mut images), "the last step to rest uploads a final clear");
        assert!(images.get(&rp.image).unwrap().data.as_ref().unwrap().iter().all(|&b| b == 0));
        rp.step();
        assert!(!rp.upload(&mut images));
    }
}
