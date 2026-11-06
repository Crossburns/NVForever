//! The weather's sounds, precipitation and lightning (WTHR SNAM, MODL and DATA;
//! docs/spec-notes/world.md §6 "Weather sound, precipitation and lightning").
//!
//! - **Outdoors only**: indoors (the cell's own lighting) nothing here plays or shows.
//! - **Sounds**: a weather's Default and Wind sounds loop at the listener while it is
//!   the weather shown (from the start of the change to it); its Precipitation sounds
//!   loop while its precipitation falls. Through `nvf_play::sfx::LoopSound`, so they take
//!   the effects volume as every SOUN does.
//! - **Precipitation**: the weather's model, drawn at the camera's place and moved with
//!   it every frame, emitting its particle systems there; it falls once the change to the
//!   weather has passed its "precipitation begin fade in" byte (of 255) and, on a change
//!   away, until the change passes its "end fade out" byte [hypothesis from the names].
//! - **Lightning** in a weather with a lightning frequency that is Rainy or has a
//!   Thunder sound (`fnv_world::sky::Weather::lightning`), between its lightning fade
//!   bytes as above: flashes at random intervals set by the frequency, each lighting the
//!   scene and the sky in the lightning colour for a moment
//!   ([`crate::sky::SkyState::set_flash`]), and one of the weather's Thunder sounds
//!   follows after a delay.
//! - Test aid `NVF_PLAY_TEST_WEATHER_FX=precip=<model>;sound=<SOUN EDID>`: the weather
//!   shown has that precipitation model and Precipitation sound as well (no weather in the
//!   masters has either).
//!
//! The constants are guesses [hypothesis], listed in docs/owner-review.md.

use crate::fly::FlyCam;
use crate::sky::SkyState;
use crate::world::{bevy_to_game, WorldRoot};
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use fnv_sim::FormId;
use fnv_world::sky::{srgb_to_linear, Weather, WeatherSound, WeatherSoundKind};
use nvf_assets::model::ModelCtx;
use nvf_play::sfx::{LoopSound, PlaySound, Sfx};

/// The mean wait between flashes at lightning frequency 255 and at 1, real seconds
/// [hypothesis: the byte's scale is not documented; higher is read as more often].
pub const LIGHTNING_MEAN_MIN: f32 = 4.0;
pub const LIGHTNING_MEAN_MAX: f32 = 40.0;
/// How long a flash lights the scene, seconds [hypothesis, a look].
pub const FLASH_SECONDS: f32 = 0.35;
/// A flash's light at its peak, times the lightning colour (linear, added to the ambient
/// light and the sky's glow) [hypothesis, a look].
pub const FLASH_PEAK: f32 = 1.0;
/// Thunder follows a flash after this many seconds, drawn evenly [hypothesis: a strike
/// some hundreds of metres to over a kilometre away].
pub const THUNDER_DELAY: (f32, f32) = (1.0, 4.0);

/// Where a weather stands in a change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    /// Shown, no change in progress.
    Steady,
    /// Being changed to; the change's progress (0..1).
    Incoming(f32),
    /// Being changed away from; the change's progress (0..1).
    Outgoing(f32),
}

/// Whether an effect with fade bytes `(begin fade in, end fade out)` is on for a
/// weather in `role` (see the module notes).
pub fn fade_share_on(fades: (u8, u8), role: Role) -> bool {
    match role {
        Role::Steady => true,
        Role::Incoming(p) => p >= fades.0 as f32 / 255.0,
        Role::Outgoing(p) => p < fades.1 as f32 / 255.0,
    }
}

/// The wait to the next flash at lightning frequency `freq`, from a uniform draw `u`.
pub fn lightning_wait(freq: u8, u: f32) -> f32 {
    let mean = LIGHTNING_MEAN_MIN + (LIGHTNING_MEAN_MAX - LIGHTNING_MEAN_MIN) * (1.0 - freq as f32 / 255.0);
    mean * (0.5 + u.clamp(0.0, 1.0))
}

/// A flash's strength (0..1) `t` seconds after it began: a bright stroke falling off
/// over 0.12 s, then a weaker return stroke about 0.22 s in [hypothesis, a look].
pub fn flash_strength(t: f32) -> f32 {
    if !(0.0..FLASH_SECONDS).contains(&t) {
        return 0.0;
    }
    let first = (1.0 - t / 0.12).max(0.0);
    let second = 0.6 * (1.0 - (t - 0.22).abs() / 0.08).max(0.0);
    first.max(second)
}

/// The test aid's additions to the weather shown.
#[derive(Default)]
struct Aid {
    precip: Option<String>,
    sound: Option<String>,
    /// The aid's sound resolved (`Some(None)`: no SOUN of that name).
    sound_id: Option<Option<u32>>,
}

impl Aid {
    fn from_env() -> Aid {
        let Ok(v) = std::env::var("NVF_PLAY_TEST_WEATHER_FX") else { return Aid::default() };
        let mut aid = Aid::default();
        for part in v.split(';') {
            match part.split_once('=') {
                Some(("precip", m)) if !m.trim().is_empty() => aid.precip = Some(m.trim().to_string()),
                Some(("sound", s)) if !s.trim().is_empty() => aid.sound = Some(s.trim().to_string()),
                _ => warn!("test aid: NVF_PLAY_TEST_WEATHER_FX: {part:?} is not precip=<model> or sound=<SOUN>"),
            }
        }
        info!("test aid: weather fx: the weather shown gets precipitation {:?} and sound {:?}", aid.precip, aid.sound);
        aid
    }
}

#[derive(Resource)]
pub struct WeatherFx {
    /// Sounds looping now (SOUN FormIDs).
    loops: Vec<u32>,
    /// The precipitation model drawn and its holder.
    precip: Option<(String, Entity)>,
    /// Seconds to the next flash, while lightning is on.
    next_flash: Option<f32>,
    /// The flash going on: seconds since it began and its colour (linear).
    flash: Option<(f32, [f32; 3])>,
    /// Thunder to come: seconds left and the SOUN.
    thunder: Vec<(f32, u32)>,
    rng: SplitMix,
    aid: Aid,
    pub flashes: u32,
}

/// The holder of the precipitation model.
#[derive(Component)]
struct PrecipHolder;

pub struct WeatherFxPlugin;

impl Plugin for WeatherFxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WeatherFx {
            loops: Vec::new(),
            precip: None,
            next_flash: None,
            flash: None,
            thunder: Vec::new(),
            rng: SplitMix(0x7e47_4e3d_9b1f_0c25),
            aid: Aid::from_env(),
            flashes: 0,
        })
        .add_systems(Update, weather_fx.before(crate::sky::update_sky).before(nvf_play::sfx::SfxSet));
    }
}

/// A weather's precipitation model and sounds, with the test aid's for the one shown.
fn effective(w: &Weather, shown: bool, aid: &Aid) -> (Option<String>, Vec<WeatherSound>) {
    let mut sounds = w.sounds.clone();
    let mut model = w.precipitation_model.as_deref().map(fnv_actor::resolve::mesh_path);
    if shown {
        if let Some(Some(id)) = aid.sound_id {
            sounds.push(WeatherSound { sound: id, kind: WeatherSoundKind::Precipitation });
        }
        if model.is_none() {
            model = aid.precip.as_deref().map(fnv_actor::resolve::mesh_path);
        }
    }
    (model, sounds)
}

fn sound_name(sfx: Option<&Sfx>, id: u32) -> String {
    sfx.and_then(|s| s.forms.sounds.get(&FormId(id))).and_then(|s| s.edid.clone()).unwrap_or_else(|| format!("{id:08X}"))
}

#[allow(clippy::too_many_arguments)]
fn weather_fx(
    mut commands: Commands,
    mut fx: ResMut<WeatherFx>,
    mut sky: ResMut<SkyState>,
    sfx: Option<Res<Sfx>>,
    time: Res<Time>,
    cam: Query<&Transform, (With<FlyCam>, Without<PrecipHolder>)>,
    root: Query<Entity, With<WorldRoot>>,
    mut holders: Query<&mut Transform, With<PrecipHolder>>,
    mut models: ModelCtx,
    mut loops: MessageWriter<LoopSound>,
    mut plays: MessageWriter<PlaySound>,
) {
    let fx = &mut *fx;
    let sfx = sfx.as_deref();
    if let (Some(name), None, Some(s)) = (fx.aid.sound.clone(), fx.aid.sound_id, sfx) {
        let id = s.forms.sounds.iter().find(|(_, snd)| snd.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&name))).map(|(id, _)| id.0);
        if id.is_none() {
            warn!("test aid: weather fx: no SOUN {name}");
        }
        fx.aid.sound_id = Some(id);
    }
    let dt = time.delta_secs();
    let outdoors = sky.interior.is_none();
    let current = sky.weather.clone();
    let prev = sky.prev.clone();
    let mut shown: Vec<(&Weather, Role)> = vec![(&current, if prev.is_some() { Role::Incoming(sky.fade) } else { Role::Steady })];
    if let Some(p) = &prev {
        shown.push((p, Role::Outgoing(sky.fade)));
    }
    let name = |w: &Weather| w.edid.clone().unwrap_or_else(|| format!("{:08X}", w.form_id));

    // What should sound and fall now.
    let mut want_loops: Vec<(u32, WeatherSoundKind, String)> = Vec::new();
    let mut want_precip: Option<(String, String)> = None;
    let mut lightning: Option<(&Weather, Vec<u32>)> = None;
    if outdoors {
        for (i, &(w, role)) in shown.iter().enumerate() {
            let (model, sounds) = effective(w, i == 0, &fx.aid);
            let raining = fade_share_on(w.precipitation_fade, role);
            for s in &sounds {
                let on = match s.kind {
                    WeatherSoundKind::Default | WeatherSoundKind::Wind => !matches!(role, Role::Outgoing(_)),
                    WeatherSoundKind::Precipitation => raining,
                    WeatherSoundKind::Thunder => false,
                };
                if on && !want_loops.iter().any(|l| l.0 == s.sound) {
                    want_loops.push((s.sound, s.kind, name(w)));
                }
            }
            if let (Some(m), true, None) = (model, raining, &want_precip) {
                want_precip = Some((m, name(w)));
            }
            if lightning.is_none() && w.lightning() && fade_share_on(w.lightning_fade, role) {
                lightning = Some((w, sounds.iter().filter(|s| s.kind == WeatherSoundKind::Thunder).map(|s| s.sound).collect()));
            }
        }
    }

    // Sounds.
    let stop: Vec<u32> = fx.loops.iter().copied().filter(|id| !want_loops.iter().any(|l| l.0 == *id)).collect();
    for id in stop {
        loops.write(LoopSound { sound: FormId(id), on: false });
        info!("weather fx: {} stops", sound_name(sfx, id));
        fx.loops.retain(|l| *l != id);
    }
    for (id, kind, w) in &want_loops {
        if !fx.loops.contains(id) {
            loops.write(LoopSound { sound: FormId(*id), on: true });
            info!("weather fx: {} ({kind:?}) loops for {w}", sound_name(sfx, *id));
            fx.loops.push(*id);
        }
    }

    // Precipitation: (re)placed when the model changes or its holder went with the world.
    if let Some((_, e)) = &fx.precip {
        if *e != Entity::PLACEHOLDER && holders.get(*e).is_err() {
            fx.precip = None;
        }
    }
    let have = fx.precip.as_ref().map(|p| p.0.clone());
    if have != want_precip.as_ref().map(|p| p.0.clone()) {
        if let Some((m, e)) = fx.precip.take() {
            commands.entity(e).try_despawn();
            info!("weather fx: precipitation {m} off");
        }
        if let (Some((m, w)), Ok(r)) = (&want_precip, root.single()) {
            match models.model(m) {
                Some(pm) => {
                    let holder = commands.spawn((Transform::IDENTITY, Visibility::Inherited, PrecipHolder, ChildOf(r))).id();
                    crate::shot_fx::spawn_attached(&mut commands, holder, &pm, Transform::IDENTITY, m);
                    if pm.particles {
                        commands.entity(holder).insert(crate::particles::ModelEmitter::new(m.clone()));
                    }
                    info!("weather fx: precipitation {m} on for {w}{}", if pm.particles { "" } else { " (no particle systems)" });
                    fx.precip = Some((m.clone(), holder));
                }
                None => {
                    warn!("weather fx: precipitation model {m} of {w} not loaded");
                    // Do not try every frame: remember it as drawn, with no holder.
                    fx.precip = Some((m.clone(), Entity::PLACEHOLDER));
                }
            }
        }
    }
    if let (Some((_, e)), Ok(c)) = (&fx.precip, cam.single()) {
        if let Ok(mut t) = holders.get_mut(*e) {
            t.translation = bevy_to_game(c.translation);
        }
    }

    // Lightning and thunder.
    match &lightning {
        Some((w, thunder)) => {
            let left = match fx.next_flash {
                Some(left) => left - dt,
                None => {
                    let wait = lightning_wait(w.lightning_frequency, fx.rng.next_f32());
                    info!("weather fx: lightning in {} (frequency {}), first flash in {wait:.1} s", name(w), w.lightning_frequency);
                    wait
                }
            };
            if left <= 0.0 {
                let colour = w.lightning_color.map(srgb_to_linear);
                fx.flash = Some((0.0, colour));
                fx.flashes += 1;
                let next = lightning_wait(w.lightning_frequency, fx.rng.next_f32());
                info!("weather fx: lightning flash in {} (colour {:?}), next in {next:.1} s", name(w), w.lightning_color);
                if !thunder.is_empty() {
                    let pick = thunder[((fx.rng.next_f32() * thunder.len() as f32) as usize).min(thunder.len() - 1)];
                    let delay = THUNDER_DELAY.0 + (THUNDER_DELAY.1 - THUNDER_DELAY.0) * fx.rng.next_f32();
                    fx.thunder.push((delay, pick));
                }
                fx.next_flash = Some(next);
            } else {
                fx.next_flash = Some(left);
            }
        }
        None => {
            if fx.next_flash.take().is_some() {
                info!("weather fx: lightning over");
            }
        }
    }
    let mut due = Vec::new();
    fx.thunder.retain_mut(|(left, id)| {
        *left -= dt;
        let done = *left <= 0.0;
        if done {
            due.push((*id, -*left));
        }
        !done
    });
    for (id, _) in due {
        plays.write(PlaySound { sound: FormId(id), at: None });
        info!("weather fx: thunder {} after the flash", sound_name(sfx, id));
    }
    let light = match &mut fx.flash {
        Some((t, colour)) if outdoors => {
            let s = flash_strength(*t) * FLASH_PEAK;
            *t += dt;
            colour.map(|c| c * s)
        }
        _ => [0.0; 3],
    };
    if fx.flash.is_some_and(|(t, _)| t >= FLASH_SECONDS) || !outdoors {
        fx.flash = None;
    }
    sky.set_flash(light);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fades_share_the_change() {
        // Honest Hearts' rain: precipitation fades 52 / 128.
        let f = (52, 128);
        assert!(fade_share_on(f, Role::Steady));
        assert!(!fade_share_on(f, Role::Incoming(0.1)));
        assert!(fade_share_on(f, Role::Incoming(0.25)));
        assert!(fade_share_on(f, Role::Outgoing(0.4)));
        assert!(!fade_share_on(f, Role::Outgoing(0.6)));
        // 0 / 255: on through the whole change both ways.
        assert!(fade_share_on((0, 255), Role::Incoming(0.0)));
        assert!(fade_share_on((0, 255), Role::Outgoing(0.99)));
    }

    #[test]
    fn lightning_waits_shorten_with_frequency() {
        assert!((lightning_wait(255, 0.5) - LIGHTNING_MEAN_MIN).abs() < 1e-4);
        let w138 = lightning_wait(138, 0.5);
        assert!(w138 > 20.0 && w138 < 23.0, "{w138}");
        assert!(lightning_wait(15, 0.5) > w138);
        assert!((lightning_wait(138, 0.0) * 3.0 - lightning_wait(138, 1.0)).abs() < 1e-3);
    }

    #[test]
    fn a_flash_strikes_twice_and_ends() {
        assert_eq!(flash_strength(0.0), 1.0);
        assert!(flash_strength(0.1) < 0.2);
        assert!((flash_strength(0.22) - 0.6).abs() < 1e-5);
        assert_eq!(flash_strength(FLASH_SECONDS), 0.0);
        assert_eq!(flash_strength(-0.1), 0.0);
    }
}
