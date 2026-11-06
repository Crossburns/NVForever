//! Hardcore's need words, the radiation word and the limb word on the HUD:
//! `H20 FOD SLP RAD LMB` [observed: hud.md §14.2] on the "HP" label's line at the
//! bottom left [design: docs/spec-notes/hud.md §13].
//!
//! - The words are the game's (`HardcoreMode` in `hud_main_menu.xml`: texts `Dehydration`,
//!   `Hunger`, `Sleep` and `LMBs`, font 7, the HUD's colour, set in code); their strings
//!   are the settings `sStatsH20Abbrev`, `sStatsFODAbbrev`, `sStatsSLPAbbrev` and
//!   `sStatsLMBAbbrev`, which no master sets, so the observed words [observed: hud.md
//!   §14.2] stand in when no plugin does [design]. The file's fifth text, `Rads`
//!   (`sStatsRADAbbrev`, set by no
//!   master either), is drawn as `RAD` between SLP and LMB [owner, 2026-10-23: RAD with the
//!   other HUD indicators, "same rules"].
//! - Each word has one of four looks ([`Look`]): faded, the HUD's amber, red, flashing red.
//!   A need's word and the radiation word follow the stage the game holds for them
//!   (`fnv_game::needs`; radiation in every mode) and are hidden with no stage held; the
//!   limb word follows how many limbs are crippled and is hidden with none.
//! - The need words show in Hardcore only; the radiation and limb words in every mode
//!   [observed: private job hud_hardcore2, hc9-hc10], keeping their places in every mode.
//! - [`need_look`] and [`limb_look`] are pure, tested without the game.

use crate::combat::PlayerLimbs;
use crate::hud::{HudSet, HudView, FONT, HP_PANEL, MARGIN_X, MARGIN_Y};
use crate::interface::{amber, bc_supported, face, plain_font, set_text, u, Face, Set};
use crate::reticle::crime_colour;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::tiles::vanilla::WordsRoot;
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_game::aid::{DEHYDRATION, HUNGER, RAD_LEVEL, SLEEP_DEPRIVATION};
use fnv_sim::settings::Setting;
use nvf_assets::GameData;

/// Faded (owner's dim look), now measured [observed: private job hud_hardcore2, hc2,
/// hc6; hud_radiation2, r_in_200, r_in_300]. Previously 0.3 [design].
const FADED: f32 = 124.0 / 255.0;
/// Fixed piece and word places [observed: private job hud_hardcore2, hc0-hc11]. Previously
/// font spaces after HP and compacted outside Hardcore [hypothesis].
const WORD_X: [f32; 5] = [0.0, 55.0, 109.0, 162.0, 218.0];
/// The words' piece, `HardcoreMode`'s rect [documented: hud_main_menu.xml, HardcoreMode].
const PIECE: Vec2 = Vec2::new(386.0, 20.0);

/// The piece's left and bottom in interface units: the original's places of
/// `HardcoreMode` from `HitPoints` ([`fnv_game::ui_hud::HARDCORE_MODE_AT`], 150 to the
/// right, the same top [observed: hud.md §14]) taken from the hand-built HP piece, so the
/// words stay in line with "HP" whatever its margins (`crate::hud::MARGIN_X`, `MARGIN_Y`).
fn piece_place() -> (f32, f32) {
    use fnv_game::ui_hud::{HARDCORE_MODE_AT as HC, HIT_POINTS_AT as HP};
    let (dx, dy) = ((HC.0 - HP.0) as f32, (HC.1 - HP.1) as f32);
    (MARGIN_X + dx, MARGIN_Y + HP_PANEL.y - dy - PIECE.y)
}
/// The words are drawn at the font's own size, as tall as "HP", whatever their `zoom` 75
/// [observed: hud_compare needs_limbs at 1280 x 720, the original's words 17 pixels tall
/// (y 603-619) as its "HP", their right ends at x 145, 186, 226, 266, 308; the tiles'
/// widths read 45, 44, 43, 46, 43, "H20" as wide as at zoom 100 ("HP" 30), private job
/// hud_hardcore]. T12b drew them scaled by 0.75 (13 pixels tall), which also left their
/// starts right of the original's [design, corrected T12e]. Their first line stands
/// font 7's line under the piece's top (`tiles::draw::text_base`), as the tile-drawn
/// words [hypothesis: the capitals' top 1 pixel under the piece's, y 603 on the shot; the
/// font's tallest glyph's top at 0 stood them 4 pixels lower; the words are font 7,
/// hud.md §13].
const WORD_BASE: f32 = crate::tiles::draw::text_base(7);
/// Word alpha bounds [observed: private job hud_hardcore2, hc5]. Period unresolved;
/// retain the former 0.6 s cosine [hypothesis], independently of the limb sawtooth.
fn flash(elapsed: f32) -> Color {
    let phase = (elapsed / 0.6 * std::f32::consts::TAU).cos() * 0.5 + 0.5;
    let low = 128.0 / 255.0;
    crime_colour().with_alpha(low + (1.0 - low) * phase)
}

/// The settings [documented: hud_main_menu.xml, HardcoreMode] and the words used
/// without them [observed: hud.md §14.2; H20 ends in a zero].
const WORDS: [(&str, &str); 5] = [("sStatsH20Abbrev", "H20"), ("sStatsFODAbbrev", "FOD"), ("sStatsSLPAbbrev", "SLP"), ("sStatsRADAbbrev", "RAD"), ("sStatsLMBAbbrev", "LMB")];
/// The needs behind the first three words.
const NEEDS: [u16; 3] = [DEHYDRATION, HUNGER, SLEEP_DEPRIVATION];
/// The radiation and limb words' indices in [`WORDS`]; they show in every mode.
const RAD: usize = 3;
const LMB: usize = 4;
const N: usize = WORDS.len();

/// How a word is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Hidden,
    Faded,
    Normal,
    Red,
    Flashing,
}

impl Look {
    pub fn name(self) -> &'static str {
        match self {
            Look::Hidden => "hidden",
            Look::Faded => "faded",
            Look::Normal => "normal",
            Look::Red => "red",
            Look::Flashing => "flashing",
        }
    }
}

/// A need's or the radiation's look from the stage the game holds (`stage`, 0 the lowest,
/// of `count` stages) [design: owner 2026-10-23, "none/lowest faded, then normal, red,
/// flashing for the last ones"; owner 2026-10-24: nothing shows under the first stage, and
/// radiation follows the same rule]: with the masters' five stages (Minor 200, Advanced
/// 400, Critical 600, Deadly 800, Fatal 1000) no stage is hidden, Minor faded, Advanced
/// normal, Critical red, Deadly and Fatal flash. Fewer stages (a plugin's) give up the
/// faded stage first, then the normal one.
pub fn need_look(stage: Option<usize>, count: usize) -> Look {
    let Some(i) = stage else { return Look::Hidden };
    // The stage counted from the worst: 0 the last.
    let from_top = count.saturating_sub(1).saturating_sub(i);
    match from_top {
        0 | 1 => Look::Flashing,
        2 => Look::Red,
        3 => Look::Normal,
        _ => Look::Faded,
    }
}

/// The limb word's look from how many limbs are crippled [design: owner 2026-10-23,
/// "1 normal, 2 red, 3 or more flashing red"; owner 2026-10-24: hidden with none].
pub fn limb_look(crippled: usize) -> Look {
    match crippled {
        0 => Look::Hidden,
        1 => Look::Normal,
        2 => Look::Red,
        _ => Look::Flashing,
    }
}

/// The words' looks, and what each look was read from (for the log).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct NeedWords {
    pub looks: [Look; N],
    pub why: [String; N],
    /// Hardcore mode is on (the need words show).
    pub hardcore: bool,
}

impl Default for NeedWords {
    fn default() -> Self {
        NeedWords { looks: [Look::Hidden; N], why: Default::default(), hardcore: false }
    }
}

pub struct HudNeedsPlugin;

impl Plugin for HudNeedsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NeedWords>().add_systems(Update, (watch, draw).chain().after(HudSet).in_set(PlaySet::Ui));
    }
}

/// Read the needs' and radiation's stages and the crippled limbs; log a word whose look
/// changes.
fn watch(mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, limbs: Option<Res<PlayerLimbs>>, mut words: ResMut<NeedWords>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let hardcore = with_world(rt, &mut sim, |rt| rt.game.hardcore());
    let mut looks = [Look::Hidden; N];
    let mut why: [String; N] = Default::default();
    for (i, code) in NEEDS.into_iter().enumerate() {
        if !hardcore {
            why[i] = "Hardcore off".into();
            continue;
        }
        let g = &rt.game;
        let stages = g.data.need_stages.get(&code).map_or(&[][..], |s| s.as_slice());
        let held = g.player.need_stages.get(&code).and_then(|h| stages.iter().position(|s| s.stage == *h));
        looks[i] = need_look(held, stages.len());
        why[i] = match held {
            Some(s) => format!("stage {} of {}, {}", s + 1, stages.len(), g.data.name(stages[s].spell)),
            None => "no stage".into(),
        };
    }
    {
        let g = &rt.game;
        let stages = g.data.need_stages.get(&RAD_LEVEL).map_or(&[][..], |s| s.as_slice());
        let held = g.player.need_stages.get(&RAD_LEVEL).and_then(|h| stages.iter().position(|s| s.stage == *h));
        looks[RAD] = need_look(held, stages.len());
        why[RAD] = match held {
            Some(s) => format!("stage {} of {}, {}", s + 1, stages.len(), g.data.name(stages[s].spell)),
            None => "no stage".into(),
        };
    }
    let crippled = limbs.map_or(0, |l| (25..=30).filter(|&c| l.crippled(c)).count());
    looks[LMB] = limb_look(crippled);
    why[LMB] = format!("{crippled} limbs crippled");
    for i in 0..N {
        if looks[i] != words.looks[i] {
            info!("hud needs: {} {} ({})", WORDS[i].1, looks[i].name(), why[i]);
        }
    }
    let now = NeedWords { looks, why, hardcore };
    words.set_if_neq(now);
}

/// A word's line in its node, which stands at the word's x: right-justified, as the file
/// says [documented: hud_main_menu.xml], the x its right end [hypothesis], unscaled, its
/// line [`WORD_BASE`] under the piece's top (see [`WORD_BASE`]).
fn word_set(font: &fnv_font::Font, text: &str) -> Option<Set> {
    font.lines(text, None).into_iter().next().map(|line| Set { x: -line.width, line, base: WORD_BASE })
}

/// A word's letters (they carry its colour).
#[derive(Component, Clone, Copy)]
struct Letter(usize);
/// A word, shown or hidden.
#[derive(Component)]
struct Word(usize);

fn setting_str(sim: &SimState, name: &str) -> Option<String> {
    match sim.forms.settings.get(&name.to_ascii_lowercase()) {
        Some(Setting::Str(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

/// The same setting overrides and fallbacks for the drawing and tree [design].
pub(crate) fn texts(sim: &SimState) -> [String; N] {
    WORDS.map(|(setting, word)| setting_str(sim, setting).unwrap_or_else(|| word.to_string()))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<HudView>,
    words: Res<NeedWords>,
    sim: Res<SimState>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut spawned: Local<bool>,
    mut root: Query<&mut Visibility, (With<WordsRoot>, Without<Word>)>,
    mut word_nodes: Query<(&Word, &mut Visibility), Without<WordsRoot>>,
    mut letters: Query<(&Letter, Option<&mut ImageNode>, Option<&mut TextColor>)>,
) {
    // The HUD's lengths at the UI scale (`crate::interface::hud_units`, #1631).
    let _hud = crate::interface::hud_units();
    if !*spawned {
        // The first frame the HUD is wanted: set the words out.
        if !view.shown {
            return;
        }
        let face_: Face = face(FONT, plain_font(36.0, 15.0), data.as_deref(), &mut images, bc_supported(formats.as_deref()));
        let font = &face_.font;
        let texts = texts(&sim);
        let (left, bottom) = piece_place();
        let piece = Node { position_type: PositionType::Absolute, left: u(left), bottom: u(bottom), width: u(PIECE.x), height: u(PIECE.y), ..default() };
        commands.spawn((WordsRoot, piece, Visibility::Hidden, Pickable::IGNORE)).with_children(|p| {
            for (i, t) in texts.iter().enumerate() {
                let node = Node { position_type: PositionType::Absolute, left: u(WORD_X[i]), top: u(0.0), width: u(0.0), height: u(0.0), ..default() };
                p.spawn((Word(i), node, Visibility::Hidden)).with_children(|w| {
                    if let Some(set) = word_set(font, t) {
                        set_text(w, &face_, &set, Vec2::ZERO, amber(255.0), Letter(i));
                    }
                });
            }
        });
        *spawned = true;
        return;
    }
    let show = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        v.set_if_neq(show(view.shown));
    }
    if !view.shown {
        return;
    }
    for (w, mut vis) in &mut word_nodes {
        vis.set_if_neq(show(words.looks[w.0] != Look::Hidden));
    }
    let flash = flash(time.elapsed_secs());
    for (l, image, text) in &mut letters {
        let color = match words.looks[l.0] {
            Look::Hidden | Look::Normal => amber(255.0),
            Look::Faded => amber(255.0).with_alpha(FADED),
            Look::Red => crime_colour(),
            Look::Flashing => flash,
        };
        if let Some(mut image) = image {
            if image.color != color {
                image.color = color;
            }
        }
        if let Some(mut text) = text {
            if text.0 != color {
                text.0 = color;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_piece_stands_at_the_originals_place() {
        // The view is 960 units high: (190, 803) from its top left [observed: hud.md §14].
        let (left, bottom) = piece_place();
        assert_eq!((left, 960.0 - bottom - PIECE.y), (190.0, 803.0));
    }

    /// T12e: the words at the font's own size (the original's are as tall as "HP" with
    /// their zoom 75), right-justified at their x, the line font 7's 24 under the top.
    #[test]
    fn the_words_are_drawn_unscaled_and_end_at_their_x() {
        let font = crate::interface::plain_font(36.0, 15.0);
        let set = word_set(&font, "H20").unwrap();
        assert_eq!(set.line.width, font.width("H20"));
        assert_eq!((set.x + set.line.width, set.base), (0.0, 24.0));
        assert!(word_set(&font, "").is_none() || word_set(&font, "").unwrap().line.width == 0.0);
    }

    #[test]
    fn flashing_words_keep_the_red_and_the_measured_alpha_floor() {
        let red = crime_colour().to_srgba();
        for (t, alpha) in [(0.0, 1.0), (0.3, 128.0 / 255.0), (0.6, 1.0)] {
            let c = flash(t).to_srgba();
            assert_eq!((c.red, c.green, c.blue), (red.red, red.green, red.blue));
            assert!((c.alpha - alpha).abs() < 1e-5);
        }
    }

    #[test]
    fn the_masters_five_stages_take_the_four_looks() {
        // Under the first stage (200) nothing shows.
        assert_eq!(need_look(None, 5), Look::Hidden);
        let looks: Vec<Look> = (0..5).map(|i| need_look(Some(i), 5)).collect();
        assert_eq!(looks, [Look::Faded, Look::Normal, Look::Red, Look::Flashing, Look::Flashing]);
    }

    #[test]
    fn fewer_stages_keep_the_worst_looks() {
        assert_eq!(need_look(Some(0), 3), Look::Red);
        assert_eq!(need_look(Some(2), 3), Look::Flashing);
        assert_eq!(need_look(Some(0), 1), Look::Flashing);
        // A stage out of range reads as the worst, never a panic.
        assert_eq!(need_look(Some(9), 5), Look::Flashing);
    }

    #[test]
    fn crippled_limbs_count_up_to_flashing() {
        assert_eq!(limb_look(0), Look::Hidden);
        assert_eq!(limb_look(1), Look::Normal);
        assert_eq!(limb_look(2), Look::Red);
        assert_eq!(limb_look(3), Look::Flashing);
        assert_eq!(limb_look(6), Look::Flashing);
    }
}
