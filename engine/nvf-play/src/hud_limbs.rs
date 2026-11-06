//! The crippled-limb picture at the top right of the HUD: the Vault Boy for a few
//! seconds after one of the player's limbs is crippled (docs/spec-notes/hud.md §12).
//!
//! - Pieces and pictures are the game's (`crippled_limb_indicator` in
//!   `hud_main_menu.xml`: head, face, torso, arms and legs from `Interface\Stats\`); the
//!   pieces stand as on the Pip-Boy's CND page (`crate::pipboy::stats`), smaller.
//! - A whole limb is drawn solid, a crippled one by its `_broken` (dashed) picture; the
//!   limb just crippled flashes red, those crippled before stay in the HUD's colour.
//! - The face follows the player's health.
//! - [`Indicator`] keeps what shows and for how long (pure, tested without the game).

use crate::combat::PlayerLimbs;
use crate::hud::{HudSet, HudView};
use crate::interface::{amber, at, bc_supported, texture, u};
use crate::pipboy::stats::{FACE, HEAD, LIMB_PIECES, TORSO};
use crate::reticle::crime_colour;
use crate::tiles::vanilla::LimbRoot;
use crate::{PlaySet, PlayState};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use nvf_assets::GameData;

/// How long the picture shows after a limb is crippled [design: owner 2026-10-17,
/// "show for about 5 seconds"].
pub const SHOW_SECONDS: f32 = 5.0;
/// The size of the pieces against the Pip-Boy's unzoomed ones [observed: 0.735 by the
/// width and 0.744 by the height on the owner's 1920 × 1080 screenshot of the original,
/// 2026-10-17].
const ZOOM: f32 = 0.74;
/// The pieces' outer edge from the view's right edge, and the head's top from the view's
/// top, in interface units [observed: the same screenshot, the ink 113 pixels from the
/// right edge and 192 from the top at 1080 lines].
const RIGHT: f32 = 100.5;
const TOP: f32 = 170.7;
/// The pieces' ink, from the head's top left at zoom 1: the right arm's fingers reach
/// 101 left of it, the left arm's 176 right, the feet 282 down [observed: the pictures].
const INK_LEFT: f32 = 101.0;
const INK_RIGHT: f32 = 176.0;
const INK_HEIGHT: f32 = 282.0;
/// Whole pictures fade for 0.85 s at each end of the owner's five seconds
/// [observed: private job hud_xp_limbs, l1_leg, l2_head; one run]. Previously instant [design].
const FADE_SECONDS: f32 = 0.85;
/// Broken pictures flash in alpha 0-255 [observed: private job hud_xp_limbs, l1_leg,
/// l2_head]; rising sawtooth and period fitted to those samples [hypothesis].
/// Previously a brightness cosine, low 0.35, period 0.6 s [design].
const FLASH_SECONDS: f32 = 0.54;
/// The broken pictures' alpha read negative after about 4.3 s [observed: private job
/// hud_xp_limbs, l1_leg, l2_head; one run]; what the original draws then is not known.
const FLASH_STOP: f32 = 4.3;

/// The pieces, as the limb conditions 25–30 (head, torso, left arm, right arm, left leg,
/// right leg) [documented: the actor value codes]; the brain (31) has no piece.
const PIECES: [&str; 6] = ["head", "torso", "left_arm", "right_arm", "left_leg", "right_leg"];

/// How a piece is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Whole,
    Broken,
    Flashing,
}

/// What the picture shows and for how long.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Indicator {
    /// The limbs crippled as last read.
    crippled: [bool; 6],
    /// The limbs crippled by the latest crippling: these flash.
    fresh: [bool; 6],
    /// Seconds left to show.
    left: f32,
}

impl Indicator {
    /// Take the limbs as they are now; `dt` counts down while `ticking` (the HUD is on
    /// the screen). Returns whether a limb was newly crippled. A newly crippled limb
    /// shows the picture for `SHOW_SECONDS` from now and flashes; those crippled earlier
    /// stop flashing [design: owner 2026-10-17, "the limb that was just crippled will
    /// flash"]. A limb healed meanwhile is drawn whole and does not show the picture.
    pub fn update(&mut self, now: [bool; 6], dt: f32, ticking: bool) -> bool {
        let fresh: [bool; 6] = std::array::from_fn(|i| now[i] && !self.crippled[i]);
        let new = fresh.contains(&true);
        if new {
            self.fresh = fresh;
            self.left = SHOW_SECONDS;
        } else {
            if ticking {
                self.left = (self.left - dt).max(0.0);
            }
            for (f, n) in self.fresh.iter_mut().zip(now) {
                *f &= n;
            }
        }
        self.crippled = now;
        new
    }

    pub fn shown(&self) -> bool {
        self.left > 0.0
    }

    /// The whole pictures' envelope [observed: private job hud_xp_limbs, l1_leg,
    /// l2_head]: 0.85 s in, 3.3 s full, 0.85 s out within SHOW_SECONDS.
    pub fn strength(&self) -> f32 {
        ((SHOW_SECONDS - self.left) / FADE_SECONDS).min(self.left / FADE_SECONDS).clamp(0.0, 1.0)
    }

    pub fn look(&self, piece: usize) -> Look {
        match (self.crippled[piece], self.fresh[piece]) {
            (true, true) => Look::Flashing,
            (true, false) => Look::Broken,
            _ => Look::Whole,
        }
    }
}

/// The face by the player's health, 0 to 1: `face_00` (the smile) to `face_04` (the
/// eyes crossed out) as health falls [observed: the pictures grow worse in that order;
/// `face_02` at about 15 % health on the owner's screenshot (6 of 37 ticks lit);
/// hypothesis: the steps, and that health picks the face (owner 2026-10-17: "check the
/// face is determined by health percentage"); `face_10`, the tongue out, is not used].
pub fn face(health: f32) -> &'static str {
    if health > 0.8 {
        "face_00"
    } else if health > 0.6 {
        "face_01"
    } else if health > 0.15 {
        "face_02"
    } else if health > 0.0 {
        "face_03"
    } else {
        "face_04"
    }
}

/// Alpha sawtooth [observed: private job hud_xp_limbs, l1_leg, l2_head;
/// hypothesis: fitted period]. Need words have their own flash in `crate::hud_needs`.
pub fn flash(elapsed: f32) -> Color {
    crime_colour().with_alpha((elapsed / FLASH_SECONDS).rem_euclid(1.0))
}

/// The flashing piece `elapsed` seconds into the picture, under the whole pictures'
/// envelope (`strength`, [`Indicator::strength`]): the sawtooth while it flashes, then
/// held red at the envelope, so it fades in and out with the rest of the picture and
/// leaves no hole in it [design: maintainer's review of #902; previously hidden after
/// [`FLASH_STOP`], the negative alpha clamped].
pub fn flashing(elapsed: f32, strength: f32) -> Color {
    if elapsed < FLASH_STOP {
        let c = flash(elapsed);
        c.with_alpha(c.alpha() * strength)
    } else {
        crime_colour().with_alpha(strength)
    }
}

/// Where a piece's picture stands from the head's top left, and how much of it shows
/// (its top left part), at zoom 1.
fn place(piece: usize) -> (Vec2, Vec2) {
    match piece {
        0 => (Vec2::ZERO, HEAD),
        1 => TORSO,
        i => {
            let (at, size, _) = LIMB_PIECES[i - 2];
            (TORSO.0 + at, size)
        }
    }
}

pub struct HudLimbsPlugin;

impl Plugin for HudLimbsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Indicator>().add_systems(Update, (watch, draw).chain().after(HudSet).in_set(PlaySet::Ui));
    }
}

/// Read the player's limbs and count the picture's time down.
fn watch(time: Res<Time>, state: Res<State<PlayState>>, limbs: Option<Res<PlayerLimbs>>, view: Res<HudView>, mut ind: ResMut<Indicator>) {
    let Some(limbs) = limbs else { return };
    let now: [bool; 6] = std::array::from_fn(|i| limbs.crippled(25 + i as u16));
    let was = ind.shown();
    let ticking = view.shown && *state.get() == PlayState::Playing;
    if ind.update(now, time.delta_secs(), ticking) {
        let fresh: Vec<&str> = (0..6).filter(|&i| ind.look(i) == Look::Flashing).map(|i| PIECES[i]).collect();
        info!("hud: crippled limbs shown, flashing {} (crippled: {})", fresh.join(", "), limbs.crippled_names().join(", "));
    } else if was && !ind.shown() {
        info!("hud: crippled limbs hidden");
    }
}

/// One of the picture's pieces: a limb's whole or broken picture.
#[derive(Component)]
struct Piece {
    index: usize,
    broken: bool,
}
#[derive(Component)]
struct FacePiece;

/// The faces' pictures, by name.
struct Faces(Vec<(&'static str, Handle<Image>)>);

/// A picture's top left `size` (texels at zoom 1) at `x`, `y`, zoomed.
fn cut(image: Handle<Image>, x: f32, y: f32, size: Vec2, color: Color) -> (Node, ImageNode) {
    let rect = Rect::from_corners(Vec2::ZERO, size);
    (at(x * ZOOM, y * ZOOM, size.x * ZOOM, size.y * ZOOM), ImageNode { rect: Some(rect), color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    view: Res<HudView>,
    ind: Res<Indicator>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut faces: Local<Option<Faces>>,
    mut root: Query<&mut Visibility, (With<LimbRoot>, Without<Piece>)>,
    mut pieces: Query<(&Piece, &mut Visibility, &mut ImageNode), (Without<LimbRoot>, Without<FacePiece>)>,
    mut face_node: Query<&mut ImageNode, (With<FacePiece>, Without<Piece>)>,
) {
    // The HUD's lengths at the UI scale (`crate::interface::hud_units`, #1631).
    let _hud = crate::interface::hud_units();
    let wanted = view.shown && ind.shown();
    if faces.is_none() {
        // The first time the picture is wanted: read the game's pictures and set it up.
        // Without them it is not drawn.
        if !wanted {
            return;
        }
        let Some(data) = data.as_deref() else { return };
        let bc = bc_supported(formats.as_deref());
        let mut read = |name: &str| texture(&format!("textures\\interface\\stats\\{name}.dds"), data, &mut images, bc).map(|t| t.0);
        let loaded: Vec<_> = ["face_00", "face_01", "face_02", "face_03", "face_04"].into_iter().filter_map(|n| Some((n, read(n)?))).collect();
        let mut parts = Vec::new();
        for (i, name) in PIECES.iter().enumerate() {
            for broken in [false, true] {
                let file = if broken { format!("{name}_broken") } else { (*name).to_string() };
                match read(&file) {
                    Some(image) => parts.push((i, broken, image)),
                    None => warn!("hud: no picture {file} for the crippled limbs"),
                }
            }
        }
        let width = (INK_LEFT + INK_RIGHT) * ZOOM;
        let node = Node { position_type: PositionType::Absolute, right: u(RIGHT), top: u(TOP), width: u(width), height: u(INK_HEIGHT * ZOOM), ..default() };
        let face_image = loaded.iter().find(|(n, _)| *n == face(view.health)).or(loaded.first()).map(|(_, h)| h.clone());
        commands.spawn((LimbRoot, node, Visibility::Hidden, Pickable::IGNORE)).with_children(|p| {
            // The head's top left, in the root.
            p.spawn((at(INK_LEFT * ZOOM, 0.0, 0.0, 0.0), Pickable::IGNORE)).with_children(|h| {
                for (i, broken, image) in parts {
                    let (place, size) = place(i);
                    h.spawn((Piece { index: i, broken }, Visibility::Hidden, cut(image, place.x, place.y, size, amber(255.0))));
                }
                // [hypothesis, as the Pip-Boy's] the face centred in the head's height and
                // 16 lower.
                if let Some(image) = face_image {
                    let y = (HEAD.y - FACE.1.y) / 2.0 + FACE.0.y;
                    h.spawn((FacePiece, cut(image, FACE.0.x, y, FACE.1, amber(255.0))));
                }
            });
        });
        *faces = Some(Faces(loaded));
        return;
    }
    let show = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        v.set_if_neq(show(wanted));
    }
    if !wanted {
        return;
    }
    let flash = flashing(SHOW_SECONDS - ind.left, ind.strength());
    for (piece, mut vis, mut image) in &mut pieces {
        let look = ind.look(piece.index);
        vis.set_if_neq(show((look != Look::Whole) == piece.broken));
        let color = if look == Look::Flashing { flash } else { amber(255.0).with_alpha(ind.strength()) };
        if image.color != color {
            image.color = color;
        }
    }
    let name = face(view.health);
    if let (Some(Faces(all)), Ok(mut node)) = (faces.as_ref(), face_node.single_mut()) {
        node.color = amber(255.0).with_alpha(ind.strength());
        if let Some((_, h)) = all.iter().find(|(n, _)| *n == name) {
            if node.image != *h {
                node.image = h.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: [bool; 6] = [false; 6];
    fn only(i: usize) -> [bool; 6] {
        std::array::from_fn(|j| j == i)
    }

    #[test]
    fn a_crippling_shows_the_picture_for_five_seconds() {
        let mut ind = Indicator::default();
        assert!(!ind.update(NONE, 0.1, true));
        assert!(!ind.shown());
        assert!(ind.update(only(5), 0.1, true));
        assert!(ind.shown());
        assert_eq!(ind.look(5), Look::Flashing);
        assert_eq!(ind.look(4), Look::Whole);
        for _ in 0..49 {
            ind.update(only(5), 0.1, true);
        }
        assert!(ind.shown());
        ind.update(only(5), 0.11, true);
        assert!(!ind.shown());
    }

    #[test]
    fn time_stands_while_the_hud_is_off() {
        let mut ind = Indicator::default();
        ind.update(only(0), 0.0, true);
        ind.update(only(0), 10.0, false);
        assert!(ind.shown());
    }

    #[test]
    fn the_whole_pictures_fade_within_the_owners_five_seconds() {
        let mut ind = Indicator::default();
        ind.update(only(0), 0.0, true);
        assert_eq!(ind.strength(), 0.0);
        ind.update(only(0), 0.425, true);
        assert!((ind.strength() - 0.5).abs() < 1e-5);
        ind.update(only(0), 0.425, true);
        assert!((ind.strength() - 1.0).abs() < 1e-5);
        ind.update(only(0), 3.3, true);
        assert!((ind.strength() - 1.0).abs() < 1e-5);
        ind.update(only(0), 0.425, true);
        assert!((ind.strength() - 0.5).abs() < 1e-5);
        let held = ind.strength();
        ind.update(only(0), 10.0, false);
        assert_eq!(ind.strength(), held);
        ind.update(only(0), 0.425, true);
        assert!(ind.strength() < 1e-5);
    }

    #[test]
    fn the_flash_changes_alpha_and_keeps_the_red() {
        let red = crime_colour().to_srgba();
        for (t, alpha) in [(0.0, 0.0), (0.27, 0.5), (0.54, 0.0)] {
            let c = flash(t).to_srgba();
            assert_eq!((c.red, c.green, c.blue), (red.red, red.green, red.blue));
            assert!((c.alpha - alpha).abs() < 1e-5);
        }
    }

    #[test]
    fn the_flashing_piece_follows_the_pictures_envelope() {
        let alpha = |t: f32| {
            let ind = Indicator { left: SHOW_SECONDS - t, ..Default::default() };
            flashing(t, ind.strength()).alpha()
        };
        // Fading in: the sawtooth under the envelope (0.5 of a flash, 0.5 of the fade).
        assert!((alpha(0.27) - 0.5 * 0.27 / FADE_SECONDS).abs() < 1e-5);
        // Full: the sawtooth alone.
        assert!((alpha(2.27) - flash(2.27).alpha()).abs() < 1e-5);
        // After the flash: held at the envelope while the picture fades out, gone at 5 s.
        assert!((alpha(4.5) - 0.5 / FADE_SECONDS).abs() < 1e-5);
        assert!(alpha(SHOW_SECONDS) < 1e-5);
        assert_eq!(flashing(4.5, 1.0).to_srgba().red, crime_colour().to_srgba().red);
    }

    #[test]
    fn only_the_newest_crippling_flashes() {
        let mut ind = Indicator::default();
        ind.update(only(4), 0.1, true);
        ind.update(only(4), 3.0, true);
        let both = [false, false, false, false, true, true];
        assert!(ind.update(both, 0.1, true));
        assert_eq!(ind.look(4), Look::Broken);
        assert_eq!(ind.look(5), Look::Flashing);
        // The time starts again.
        ind.update(both, 4.5, true);
        assert!(ind.shown());
    }

    #[test]
    fn healing_shows_nothing_and_draws_the_limb_whole() {
        let mut ind = Indicator::default();
        ind.update(only(2), 0.1, true);
        assert!(!ind.update(NONE, 0.1, true));
        assert_eq!(ind.look(2), Look::Whole);
        ind.update(NONE, 5.0, true);
        assert!(!ind.update(NONE, 0.1, true));
        assert!(!ind.shown());
    }

    #[test]
    fn the_face_worsens_as_health_falls() {
        assert_eq!(face(1.0), "face_00");
        assert_eq!(face(0.7), "face_01");
        assert_eq!(face(0.16), "face_02");
        assert_eq!(face(0.1), "face_03");
        assert_eq!(face(0.0), "face_04");
    }

    #[test]
    fn the_pieces_fill_the_measured_box() {
        // The ink box from the pieces' places matches what was measured on the pictures.
        let (ra, _) = place(3);
        let (la, la_size) = place(2);
        assert_eq!(ra.x, -102.0);
        assert_eq!(la.x + la_size.x, 176.0);
    }
}
