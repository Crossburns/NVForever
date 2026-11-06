//! Environment radiation in play (docs/spec-notes/radiation.md): the water the player is
//! in told to the game, the Geiger counter's clicks, the HUD's radiation meter, and the
//! log. The rule (markers, water, Rad Resistance, the rads taken) is
//! `fnv_game::radiation`; the sickness stages are `fnv_game::needs`.
//!
//! - **Water** ([`tell_water`]): the player is in water when the character swims, or wades
//!   with the feet under the surface the physics knows at that point; the water type is
//!   the cell's (its XCWT, else the worldspace's) [hypothesis: placed water volumes are
//!   taken as the cell's type].
//! - **Geiger counter** ([`geiger`]): while the level is above 0, one of the counter's
//!   one-second bursts every second: `UIPipBoyRadiationA` below [`BAND_B`] rads a
//!   second, `…B` below [`BAND_C`], `…C` from there [observed: the three SOUN records
//!   name folders of three one-second takes each, `a`, `b`, `c`; hypothesis: that they
//!   are rising strengths, the bands and the rhythm].
//! - **Meter** ([`draw`]): `RadiationMeter` of `menus\main\hud_main_menu.xml` while rads
//!   are taken: the level after Rad Resistance, GetRadiationLevel's, whole
//!   (`radiation_text_value`, font 8, right-justified at x 237) and "RAD/SEC"
//!   (`radiation_text`, font 7, from x 245), in the HUD's alternative colour
//!   (`&hudalt;`), and the pointer (`radiation_pointer`) under the bracket's notches
//!   at x = 15 + 0.3 × rads [observed: private job hud_radiation2, r_in_000-r_in_990;
//!   documented: the menu file; observed: the value's source, issue #894, radiation.md
//!   §6; hypothesis: its rounding]. The meter stands at the top right, the mirror of the
//!   corner messages' bracket at the top left [owner, 2026-10-23].
//! - **Explosion zones**: `crate::combat` leaves them when an explosion with radiation
//!   goes off (`Game::add_radiation_zone`); here they are logged as they fade away.
//! - Logged: `radiation: level <l> rads/s (<source>), taking <r>/s (Rad Resistance
//!   <rr> %), rads <n>` when the source or the rounded level changes,
//!   `radiation: the zone of <explosion> has faded`, the water's damage hits,
//!   `radiation: out of radiation, rads <n>`, `geiger: <sound> (level <l>)` when the
//!   band changes, and at exit `radiation summary: …`.

use crate::hud::HudView;
use crate::tiles::vanilla::DrawnByTiles;
use crate::interface::{at, bc_supported, face, plain_font, set_text, texture, u, Face, Set};
use crate::phys::cells::Space;
use crate::phys::PhysState;
use crate::reticle::crime_colour;
use crate::script::{with_world, ScriptState};
use crate::sfx::{PlaySound, Sfx};
use crate::sim::SimState;
use crate::{PlaySet, PlayState, PlayWorld};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_game::radiation::{Exposure, WaterContact};
use fnv_sim::FormId;
use fnv_world::cell_of;
use nvf_assets::GameData;

/// The level (rads a second) from which the counter plays its `B` and `C` bursts.
pub const BAND_B: f64 = 5.0;
pub const BAND_C: f64 = 10.0;
/// Seconds between bursts (each take is about a second long [observed: 88,2xx bytes of
/// 16-bit 44.1 kHz mono]).
const BURST: f32 = 1.0;

const SOUNDS: [&str; 3] = ["UIPipBoyRadiationA", "UIPipBoyRadiationB", "UIPipBoyRadiationC"];

/// The counter's band at a level: 0 silent, else 1 to 3 (A to C).
pub fn band(level: f64) -> usize {
    if level <= 0.0 {
        0
    } else if level < BAND_B {
        1
    } else if level < BAND_C {
        2
    } else {
        3
    }
}

// ---- the menu file's numbers [documented: hud_main_menu.xml `RadiationMeter`] ----------

const PIECE: Vec2 = Vec2::new(367.0, 60.0);
const BRACKET_PATH: &str = "textures\\interface\\hud\\glow_messages_radiation_seperator_right.dds";
const BRACKET_AT: f32 = -135.0;
/// The file's box for the bracket, 567 x 120: the picture is not stretched to it
/// ([`BRACKET`]).
#[cfg(test)]
const BRACKET_BOX: Vec2 = Vec2::new(567.0, 120.0);
const POINTER_PATH: &str = "textures\\interface\\hud\\glow_messages_radiation_seperator_arrow.dds";
const POINTER: Vec2 = Vec2::new(46.0, 24.0);
const POINTER_Y: f32 = 25.0;
const VALUE_END: f32 = 237.0;
const WORD_X: f32 = 245.0;
// The texts' y in the file, 65, is replaced by `BASE` below.
const VALUE_FONT: &str = "textures\\fonts\\glow_futura_caps_large.fnt";
const WORD_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";

// ---- set here [hypothesis] ---------------------------------------------------------------

/// The word [observed: the original's tile, hud.md §14.2;
/// `sStatsRADAbbrev` is set by no master].
pub(crate) const WORD: &str = "RAD/SEC";

/// Whole rads a second, at least 1 while any are taken [hypothesis: existing drawing
/// rule]; "+0" without radiation [observed: hud.md §14.2]. Given the level after Rad
/// Resistance (`Exposure::resisted`, GetRadiationLevel's), not the rads taken: the
/// original's +9 in the test site's shack is its 9.013784 [observed: issue #894;
/// radiation.md]. Shared with the tree [design].
pub(crate) fn value_text(resisted: f64) -> String {
    let whole = if resisted > 0.0 { resisted.round().max(1.0) as i64 } else { 0 };
    format!("+{whole}")
}
/// The bracket picture (512 x 128): its line across at rows 12–16, its upright at columns
/// 494–499, and sixteen notches hanging from the line every 20 columns from 173 to 473, the
/// ends slanted, the long ones at 233, 293, 353 and 413 [observed: the texture]. Drawn at
/// its texels' size from x -135, as the original's: its notches every 15 pixels at 720
/// lines and the ruler from x 978 to 1239 [observed: hud_compare environment-radiation at
/// 1280 x 720; menu-xml.md §3.1.3, T12c, the tile HUD's rule]. In the piece's units: the
/// line's middle 14.5 under the top, the upright's middle at x 361.5, the end notches at x
/// 38 and 338, where the pointer's tip stands at 0 and 1000 rads ([`pointer_x`] + 23).
/// Until T12e it was stretched to the file's 567 x 120 [design]: the ruler 30 pixels too
/// long to the left, its notches 16.6 pixels apart and the pointer off them.
const BRACKET: Vec2 = Vec2::new(512.0, 128.0);
/// The bracket's line and upright in the piece, drawn so.
const LINE_MIDDLE: f32 = 14.5;
const UPRIGHT: f32 = BRACKET_AT + 496.5;
/// Upper bound [hypothesis: hold the measured linear scale at 1000; captures stop at
/// 998.4 rads, private job hud_radiation2, r_in_990]. The notches are not 200-rad steps.
pub const FULL_RADS: f64 = 1000.0;
/// The pointer picture (64 x 32, drawn 46 x 24) has its triangle's tip at row 8, its middle
/// at column 32: 6 under the picture's top (just under the notches), 23 from its left
/// [observed: the texture].
const POINTER_MIDDLE: f32 = 23.0;
/// The meter's place, the mirror of the corner messages' bracket (`corner_messages.rs`:
/// its line across 47 under the view's top, its upright 57 from the left edge [observed:
/// the atlas's picture as drawn there]) [owner, 2026-10-23]: the bracket's line 47.1 under
/// the view's top and its upright 57.3 from its right edge, as before T12e, the piece
/// placed around the unstretched bracket (its top 32.6 under the view's top, its right
/// edge 51.8 from the view's).
const LINE_FROM_TOP: f32 = 47.1;
const UPRIGHT_FROM_RIGHT: f32 = 57.3;
const TOP: f32 = LINE_FROM_TOP - LINE_MIDDLE;
const RIGHT: f32 = UPRIGHT_FROM_RIGHT - (PIECE.x - UPRIGHT);
/// How far the piece moved right and up when the bracket stopped being stretched (T12e):
/// the texts keep their places on the screen.
const MOVED: Vec2 = Vec2::new(105.1 - RIGHT, 33.5 - TOP);
/// Where the pointer picture stands [observed: private job hud_radiation2,
/// r_in_000-r_in_990, linear over 2-998 rads]. Previously 34.1 + 0.3312 × rads [hypothesis].
pub fn pointer_x(rads: f64) -> f32 {
    15.0 + 0.3 * rads.clamp(0.0, FULL_RADS) as f32
}

/// The texts stand lower and further right than the file's x and y [owner, 2026-10-23:
/// "lower and to the right against the bracket but not interfering with the notches"]:
/// The word ends just inside the upright (18.8 before its middle, its glow from about 4
/// before; the letters' glow reaches about 10 past their width),
/// the value before it with the file's gap (245 - 237); their line is 88 under the
/// piece's top as placed before T12e, so the value's capitals (about 36 high) start under
/// the pointer's lowest place (y 49), clear of the notches (to y 28). Their places on the
/// screen are unchanged by T12e ([`MOVED`]).
const WORD_END: f32 = 396.0 - MOVED.x;
const GAP: f32 = WORD_X - VALUE_END;
const BASE: f32 = 88.0 + MOVED.y;

/// What the last level line named: the marker, the zone's explosion, the water, and
/// the level in whole rads.
type Shown = (Option<FormId>, Option<FormId>, Option<FormId>, i64);

/// What the log and the exit summary keep.
#[derive(Resource, Default, Debug)]
pub struct RadLog {
    shown: Option<Shown>,
    /// The explosion zones seen: serial and explosion.
    zones: Vec<(u32, FormId)>,
    pub zones_left: u32,
    water_hits: u32,
    band: usize,
    pub max_level: f64,
    pub seconds: f64,
    pub rads_start: Option<f64>,
    pub rads: f64,
    pub bursts: u32,
}

pub struct RadiationPlugin;

impl Plugin for RadiationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RadLog>()
            .add_systems(Update, (tell_water, watch, geiger, draw).chain().in_set(PlaySet::Ui).after(crate::hud::HudSet))
            .add_systems(Last, summary);
    }
}

/// Tell the game what water the player is in.
fn tell_water(phys: Option<Res<PhysState>>, d: Option<Res<PlayWorld>>, mut script: Option<ResMut<ScriptState>>, mut last: Local<Option<WaterContact>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let (Some(phys), Some(d)) = (phys, d) else { return };
    let contact = phys.player.as_ref().and_then(|c| {
        let surface = phys.world.water_surface_at(c.feet[0], c.feet[1])?;
        if !c.swimming && c.feet[2] >= surface {
            return None;
        }
        let ws = d.index.worldspaces.get(&phys.worldspace);
        let water = match phys.space {
            Space::Interior(id) => d.index.cells.get(&id).and_then(|cell| cell.water_type),
            Space::Exterior => {
                let cell = ws.and_then(|w| w.grid.get(&cell_of(c.feet[0], c.feet[1])));
                cell.and_then(|id| d.index.cells.get(id)).and_then(|cell| cell.water_type)
            }
        }
        .or(ws.and_then(|w| w.water))?;
        Some(WaterContact { water: FormId(water), swimming: c.swimming })
    });
    if *last != contact {
        match contact {
            Some(w) => info!(
                "radiation: the player is {} in water {}{}",
                if w.swimming { "swimming" } else { "wading" },
                rt.game.data.name(w.water),
                rt.game.data.water_damage.get(&w.water).map(|d| format!(" (it causes {d:.0} damage a second)")).unwrap_or_default()
            ),
            None if last.is_some() => info!("radiation: the player is out of the water"),
            None => {}
        }
        *last = contact;
    }
    rt.game.set_player_water(contact);
}

/// The exposure and the player's rads after the frame's tick, logged on change.
fn watch(time: Res<Time>, state: Res<State<PlayState>>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, mut log: ResMut<RadLog>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let e = rt.game.radiation();
    let rads = with_world(rt, &mut sim, |rt| fnv_game::functions::actor_value(&mut rt.game, FormId::PLAYER_REF, fnv_game::aid::RAD_LEVEL).unwrap_or(0.0));
    log.rads_start.get_or_insert(rads);
    log.rads = rads;
    if *state.get() != PlayState::Playing {
        return;
    }
    // Explosion zones that faded since the last frame.
    let now: Vec<(u32, FormId)> = rt.game.radiation_zones().iter().map(|z| (z.serial, z.explosion)).collect();
    for &(serial, explosion) in log.zones.iter().filter(|(s, _)| !now.iter().any(|(n, _)| n == s)) {
        info!("radiation: the zone of {} (#{serial}) has faded", rt.game.data.name(explosion));
    }
    log.zones_left += now.iter().filter(|(s, _)| !log.zones.iter().any(|(o, _)| o == s)).count() as u32;
    log.zones = now;
    if e.water_hits > log.water_hits {
        if let Some(w) = e.water {
            info!("radiation: water {} takes {:.0} health ({} hit(s) in all)", rt.game.data.name(w.water), e.water_damage, e.water_hits);
        }
        log.water_hits = e.water_hits;
    }
    // Logged by whole rads: a fading zone, and a marker's level as the player moves
    // (it falls off with distance), would make a line every few frames by tenths.
    let step = e.level.round() as i64;
    let key = (e.level > 0.0).then(|| (e.marker, e.zone, e.water.filter(|_| e.from_water > 0.0).map(|w| w.water), step));
    if key != log.shown {
        match key {
            Some(_) => info!("radiation: level {:.1} rads/s ({}), taking {:.2}/s (Rad Resistance {:.0} %), rads {rads:.1}", e.level, source(&e, rt.game.data), e.rate, e.resist),
            None => info!("radiation: out of radiation, rads {rads:.1}"),
        }
        log.shown = key;
    }
    if e.level > 0.0 {
        log.max_level = log.max_level.max(e.level);
        log.seconds += time.delta_secs_f64();
    }
}

fn source(e: &Exposure, data: &fnv_game::GameData) -> String {
    let mut parts = Vec::new();
    if let Some(m) = e.marker {
        parts.push(format!("marker {m} {:.1}", e.markers));
    }
    if let Some(z) = e.zone {
        parts.push(format!("zone of {} {:.1}", data.name(z), e.markers));
    }
    if let Some(w) = e.water.filter(|_| e.from_water > 0.0) {
        parts.push(format!("{} {} {:.2}", if w.swimming { "swimming in" } else { "wading in" }, data.name(w.water), e.from_water));
    }
    parts.join(", ")
}

/// The Geiger counter: a burst a second while there is radiation.
#[allow(clippy::too_many_arguments)]
fn geiger(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    script: Option<Res<ScriptState>>,
    sfx: Option<Res<Sfx>>,
    mut sounds: MessageWriter<PlaySound>,
    mut log: ResMut<RadLog>,
    mut since: Local<f32>,
    mut forms: Local<Option<[Option<FormId>; 3]>>,
) {
    let level = script.as_deref().and_then(|s| s.rt.as_ref()).map_or(0.0, |rt| rt.game.radiation().level);
    let b = if *state.get() == PlayState::Playing && !script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.in_menu()) { band(level) } else { 0 };
    if b != log.band {
        if b > 0 {
            info!("geiger: {} (level {level:.1})", SOUNDS[b - 1]);
        } else {
            info!("geiger: quiet");
        }
        log.band = b;
        // A new band clicks at once.
        *since = BURST;
    }
    if b == 0 {
        return;
    }
    *since += time.delta_secs();
    if *since < BURST {
        return;
    }
    *since = 0.0;
    let Some(sfx) = sfx.as_deref() else { return };
    let forms = forms.get_or_insert_with(|| {
        SOUNDS.map(|name| sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(name))).map(|(f, _)| *f))
    });
    match forms[b - 1] {
        Some(sound) => {
            sounds.write(PlaySound { sound, at: None });
            log.bursts += 1;
        }
        None => warn_once!("geiger: sound {} not found", SOUNDS[b - 1]),
    }
}

#[derive(Component)]
struct MeterRoot;
#[derive(Component)]
struct Texts;
#[derive(Component)]
struct Pointer;

struct Kit {
    bracket: Option<Handle<Image>>,
    pointer: Option<Handle<Image>>,
    value: Face,
    word: Face,
}

/// The meter while rads are taken and the HUD is up.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    view: Res<HudView>,
    script: Option<Res<ScriptState>>,
    log: Res<RadLog>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut kit: Local<Option<Kit>>,
    mut shown: Local<Option<String>>,
    mut root: Query<&mut Visibility, With<MeterRoot>>,
    texts: Query<Entity, With<Texts>>,
    mut pointer: Query<&mut Node, With<Pointer>>,
) {
    // The HUD's lengths at the UI scale (`crate::interface::hud_units`, #1631).
    let _hud = crate::interface::hud_units();
    // Shown while rads are taken; the number is the level after Rad Resistance, as
    // GetRadiationLevel's (`Exposure::resisted`, untouched by fRadiationAccumulationRate).
    let (rate, resisted) = script.as_deref().and_then(|s| s.rt.as_ref()).map_or((0.0, 0.0), |rt| (rt.game.radiation().rate, rt.game.radiation().resisted));
    let on = view.shown && rate > 0.0;
    if kit.is_none() {
        if !on {
            return;
        }
        let bc = bc_supported(formats.as_deref());
        let d = data.as_deref();
        let loaded = Kit {
            bracket: d.and_then(|d| texture(BRACKET_PATH, d, &mut images, bc)).map(|t| t.0),
            pointer: d.and_then(|d| texture(POINTER_PATH, d, &mut images, bc)).map(|t| t.0),
            value: face(VALUE_FONT, plain_font(52.0, 22.0), d, &mut images, bc),
            word: face(WORD_FONT, plain_font(36.0, 15.0), d, &mut images, bc),
        };
        spawn(&mut commands, &loaded);
        *kit = Some(loaded);
        *shown = None;
        return;
    }
    for mut v in &mut root {
        v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !on {
        return;
    }
    let left = u(pointer_x(log.rads));
    for mut n in &mut pointer {
        if n.left != left {
            n.left = left;
        }
    }
    let Some(kit) = kit.as_ref() else { return };
    let value = value_text(resisted);
    if shown.as_deref() == Some(value.as_str()) {
        return;
    }
    let Ok(e) = texts.single() else { return };
    let colour = crime_colour();
    commands.entity(e).despawn_related::<Children>().with_children(|p| {
        // Right to left from the upright: the word, then the value.
        let mut end = WORD_END;
        if let Some(l) = kit.word.font.lines(WORD, None).into_iter().next() {
            end -= l.width;
            set_text(p, &kit.word, &Set { line: l, x: end, base: BASE }, Vec2::ZERO, colour, ());
        }
        if let Some(l) = kit.value.font.lines(&value, None).into_iter().next() {
            let x = end - GAP - l.width;
            set_text(p, &kit.value, &Set { line: l, x, base: BASE }, Vec2::ZERO, colour, ());
        }
    });
    *shown = Some(value);
}

fn spawn(commands: &mut Commands, kit: &Kit) {
    let piece = Node { position_type: PositionType::Absolute, right: u(RIGHT), top: u(TOP), width: u(PIECE.x), height: u(PIECE.y), ..default() };
    commands.spawn((MeterRoot, piece, Visibility::Hidden, Pickable::IGNORE, DrawnByTiles)).with_children(|p| {
        if let Some(image) = &kit.bracket {
            p.spawn((at(BRACKET_AT, 0.0, BRACKET.x, BRACKET.y), ImageNode { color: crime_colour(), image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) }));
        }
        let place = at(pointer_x(0.0), POINTER_Y, POINTER.x, POINTER.y);
        match &kit.pointer {
            Some(image) => {
                p.spawn((Pointer, place, ImageNode { color: crime_colour(), image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) }));
            }
            None => {
                // Without the picture: a small block where the triangle would be.
                p.spawn((Pointer, place)).with_children(|b| {
                    b.spawn((at(POINTER_MIDDLE - 6.0, 6.0, 12.0, 10.0), BackgroundColor(crime_colour())));
                });
            }
        }
        p.spawn((Texts, at(0.0, 0.0, PIECE.x, PIECE.y)));
    });
}

/// At exit: how much radiation the run met.
fn summary(mut exit: MessageReader<AppExit>, log: Res<RadLog>) {
    if exit.read().next().is_none() {
        return;
    }
    let start = log.rads_start.unwrap_or(0.0);
    info!(
        "radiation summary: {:.1} s in radiation, highest level {:.1} rads/s, rads {start:.1} -> {:.1} (+{:.1}), {} Geiger bursts, {} explosion zone(s), {} water hit(s)",
        log.seconds,
        log.max_level,
        log.rads,
        log.rads - start,
        log.bursts,
        log.zones_left,
        log.water_hits
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawn_value_and_tree_share_whole_rads_and_zero() {
        assert_eq!(value_text(0.0), "+0");
        assert_eq!(value_text(0.2), "+1");
        assert_eq!(value_text(8.6), "+9");
        assert_eq!(value_text(9.4), "+9");
    }

    #[test]
    fn the_counter_rises_through_its_three_bands() {
        assert_eq!(band(0.0), 0);
        assert_eq!(band(0.2), 1);
        assert_eq!(band(4.9), 1);
        assert_eq!(band(5.0), 2);
        assert_eq!(band(10.0), 3);
        assert_eq!(band(720.0), 3);
    }

    /// T12e: the bracket at its texels' size; its upright and line where the owner's place
    /// had them, the texts unmoved on the screen, the pointer's tip on the end notches at 0
    /// and 1000 rads.
    #[test]
    fn the_bracket_stands_unstretched_where_it_stood() {
        // The old stretched bracket's upright and line, from the view's right and top.
        let stretch = BRACKET_BOX.x / 512.0;
        let old_upright = 105.1 + PIECE.x - (BRACKET_AT + 496.5 * stretch);
        assert!((old_upright - UPRIGHT_FROM_RIGHT).abs() < 0.1, "{old_upright}");
        assert!((RIGHT + (PIECE.x - UPRIGHT) - UPRIGHT_FROM_RIGHT).abs() < 1e-4);
        assert!((TOP + LINE_MIDDLE - LINE_FROM_TOP).abs() < 1e-4);
        // The word's end on the screen, from the view's right edge, as before.
        assert!((RIGHT + PIECE.x - WORD_END - (105.1 + PIECE.x - 396.0)).abs() < 1e-3);
        assert!((TOP + BASE - (33.5 + 88.0)).abs() < 1e-3);
        // Notches every 20 units, the end ones under the pointer's tip at 0 and 1000 rads.
        assert_eq!((BRACKET_AT + 173.0, BRACKET_AT + 473.0), (pointer_x(0.0) + POINTER_MIDDLE, pointer_x(1000.0) + POINTER_MIDDLE));
    }

    #[test]
    fn the_pointer_matches_the_measured_rad_scale() {
        // [observed: private job hud_radiation2, r_in_000-r_in_990]; up to 0.7 units
        // below the rule because the pointer lags the actor-value read by 0-2 rads.
        for (rads, x) in [(2.25, 15.3), (958.0, 302.1), (998.4, 314.1)] {
            assert!((pointer_x(rads) - x).abs() <= 0.7);
        }
        assert_eq!(pointer_x(0.0), 15.0);
        assert_eq!(pointer_x(1000.0), 315.0);
        assert_eq!(pointer_x(-5.0), pointer_x(0.0));
        assert_eq!(pointer_x(1500.0), pointer_x(1000.0));
    }
}
