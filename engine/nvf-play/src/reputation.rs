//! Reputation in play (docs/spec-notes/reputation.md; owner's playtest list 5, item 13):
//! the rules are the game's (`fnv_game::functions::reputation`); this module shows what
//! came of them.
//!
//! - **The box**: when a reputation's title changes (its fame or infamy reaches another
//!   level, from a script's AddReputation or a crime, `crate::crime`) the game opens a
//!   message box for it: the reputation's name as the title, the new title and its
//!   description, a Vault Boy face at the left and "Ok" [observed: the owner's two
//!   screenshots in C:\fnv-private\Faction Reputation, "Goodsprings / Liked" and "Powder
//!   Gangers / Vilified"]. It waits in the game's queue of boxes (`Game::show_box` with
//!   the REPU's FormID), so it stops the game as a message box does; `crate::script_hud`
//!   draws it with [`panel`].
//! - **Sound**: `UIRepGood` when fame reached a new level, else `UIRepBad` [observed: the
//!   two SOUN records in FalloutNV.esm; hypothesis: when each plays].
//! - **Log**: `reputation:` lines for each change of title.
//! - **Not in a fight**: while the player is in combat the box, sound and log line wait,
//!   and come once the fight has been over for a short while (a title that changed back
//!   meanwhile shows nothing) [owner requests 2026-10-19 and 2026-10-30, #1050]: the
//!   shared gate `crate::after_combat` (in combat is the wait menu's test, `crate::wait`).
//! - A save keeps the titles last shown, so a load shows no box for what it restores.
//! - **Disguises**: the titles are those of the player's real reputation
//!   (`fnv_game::functions::reputation::real`), so faction armour, whose scripts set
//!   every reputation to 0 while it is worn and back after, shows no box when put on or
//!   taken off; fame or infamy earned while it is worn shows by the real title.

use crate::message_menu::PanelView;
use crate::save::SaveHook;
use crate::script::ScriptState;
use crate::sfx::PlaySound;
use crate::sim::SimState;
use crate::PlaySet;
use bevy::prelude::*;
use fnv_game::functions as gf;
use fnv_sim::FormId;
use std::collections::BTreeMap;

/// The titles of the fame and infamy levels (0–3 each), by fame then infamy
/// [documented: GECK wiki GetReputationThreshold, the names; Fallout wiki "Reputation
/// (Fallout: New Vegas)", the table of ranges (owner's copy in C:\fnv-private\fallout
/// fandom); observed: FalloutNV.esm's `sRepTitlePosTwoNegThreeDesc` is the Soft-Hearted
/// Devil's description, so fame 2 and infamy 3 is that title, as the table has it].
pub const TITLES: [[&str; 4]; 4] = [
    ["Neutral", "Shunned", "Hated", "Vilified"],
    ["Accepted", "Mixed", "Sneering Punk", "Merciful Thug"],
    ["Liked", "Smiling Troublemaker", "Unpredictable", "Soft-Hearted Devil"],
    ["Idolized", "Good-Natured Rascal", "Dark Hero", "Wild Child"],
];

/// The titles' descriptions, as [`TITLES`] [documented: the same Fallout wiki table; the
/// game's own come from settings `sRepTitlePos…Neg…Desc` where a plugin has them,
/// [`description`]].
const DESCRIPTIONS: [[&str; 4]; 4] = [
    [
        "People don't know enough about you to form an opinion.",
        "You've left a poor impression on the community and may be shunned as a result.",
        "Now that folks know you're bad, most people outright hate you.",
        "For your overwhelmingly monstrous behavior, you have become vilified by the community.",
    ],
    [
        "Folks have come to accept you for your helpful nature.",
        "A little bit good mixed with a little bit bad, people haven't figured you out yet.",
        "Even though you've done some good for the community, people still think you're a punk.",
        "Despite your reputation as a thug, you are known to occasionally show a charitable side.",
    ],
    [
        "Enough news of your good works has been passed around that people like you.",
        "People know you're good at heart even though you're occasionally a troublemaker.",
        "No one's sure what to make of your unpredictable nature, but you've left a strong impression.",
        "Most people say you're the devil himself, but most admit you've also done a world of good.",
    ],
    [
        "Renowned for your extensive support and goodwill, you are idolized by the community.",
        "Your reputation as a good-natured friend of the community manages to outshine your dark side.",
        "Folks still think you're some kind of hero, but you sure can be nasty sometimes.",
        "Your wild, seemingly capricious behavior leaves people scratching their heads in confusion and avoiding close contact.",
    ],
];

/// The faces at the box's left: more fame than infamy, more infamy, as much of each
/// [observed: the owner's screenshots show `glow_message_vaultboy_very_happy` for
/// "Liked" and `glow_message_vaultboy_sad` for "Vilified"; hypothesis: the rule, and the
/// neutral face for mixed titles].
const GOOD_FACE: &str = "textures\\interface\\icons\\message icons\\glow_message_vaultboy_very_happy.dds";
const BAD_FACE: &str = "textures\\interface\\icons\\message icons\\glow_message_vaultboy_sad.dds";
const MIXED_FACE: &str = "textures\\interface\\icons\\message icons\\glow_message_vaultboy_neutral.dds";

/// The setting naming the box's button, read through `fnv_game::ui_loading::game_string`
/// (the load order's GMST, else NVForever's own label; LEGAL.md) [design, #1183].
const OK_SETTING: &str = "sOk";

const NUMBERS: [&str; 4] = ["Zero", "One", "Two", "Three"];

pub fn title(fame: usize, infamy: usize) -> &'static str {
    TITLES[fame.min(3)][infamy.min(3)]
}

/// The description of a title: the game setting `sRepTitlePos<fame>Neg<infamy>Desc`
/// (numbers in words) when the load order has it [hypothesis: the pattern of the one
/// FalloutNV.esm overrides], else [`DESCRIPTIONS`].
pub fn description(fame: usize, infamy: usize, setting: impl Fn(&str) -> Option<String>) -> String {
    let (f, i) = (fame.min(3), infamy.min(3));
    setting(&format!("sRepTitlePos{}Neg{}Desc", NUMBERS[f], NUMBERS[i])).filter(|s| !s.trim().is_empty()).unwrap_or_else(|| DESCRIPTIONS[f][i].to_string())
}

pub fn face(fame: usize, infamy: usize) -> &'static str {
    match fame.cmp(&infamy) {
        std::cmp::Ordering::Greater => GOOD_FACE,
        std::cmp::Ordering::Less => BAD_FACE,
        std::cmp::Ordering::Equal => MIXED_FACE,
    }
}

/// The fame and infamy levels of the player's real reputation now.
pub fn levels(g: &fnv_game::Game, rep: FormId) -> (usize, usize) {
    let scale = g.data.reputations.get(&rep).copied().unwrap_or(0.0) as f64;
    let [infamy, fame] = gf::reputation_real(g, rep);
    (gf::reputation_level(fame, scale), gf::reputation_level(infamy, scale))
}

/// A reputation's name (REPU FULL), else its Editor ID.
pub fn name(g: &fnv_game::Game, rep: FormId) -> String {
    g.data.reputation_names.get(&rep).cloned().unwrap_or_else(|| g.data.name(rep))
}

/// The box for reputation `rep` (the front of the game's box queue).
pub fn panel(g: &fnv_game::Game, sim: &SimState, rep: FormId) -> PanelView {
    let (fame, infamy) = levels(g, rep);
    let setting = |n: &str| match sim.forms.settings.get(&n.to_ascii_lowercase()) {
        Some(fnv_sim::settings::Setting::Str(s)) => Some(s.clone()),
        _ => None,
    };
    let name = name(g, rep);
    PanelView {
        title: Some(name),
        text: format!("{}\n{}", title(fame, infamy), description(fame, infamy, setting)),
        buttons: vec![fnv_game::ui_loading::game_string(g, OK_SETTING)],
        icon: Some(face(fame, infamy).into()),
    }
}

/// Whether the box at the front is a reputation's.
pub fn is_reputation(g: &fnv_game::Game, front: FormId) -> bool {
    g.data.reputations.contains_key(&front)
}

/// The levels last shown for each reputation; `None` until the game is first seen (or
/// again after a load without them).
#[derive(Resource, Default)]
pub struct Shown(Option<BTreeMap<u32, (usize, usize)>>);

pub struct ReputationPlugin;

impl Plugin for ReputationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shown>().add_systems(Update, watch.after(PlaySet::Scripts).before(PlaySet::Sync));
        crate::save::add_save_hook(app, SAVE_HOOK);
    }
}

const SAVE_HOOK: SaveHook = SaveHook {
    key: "reputation_shown",
    save: |w| {
        let s = w.get_resource::<Shown>()?.0.as_ref()?;
        serde_json::to_value(s.iter().map(|(k, v)| (format!("{k:08X}"), [v.0, v.1])).collect::<BTreeMap<_, _>>()).ok()
    },
    load: |w, v| {
        let m: BTreeMap<String, [usize; 2]> = serde_json::from_value(v.clone()).unwrap_or_default();
        let m = m.into_iter().filter_map(|(k, [f, i])| Some((u32::from_str_radix(&k, 16).ok()?, (f, i)))).collect();
        w.insert_resource(Shown(Some(m)));
    },
    // Keyed by faction FormID in hex: moved to the current load order, those of plugins
    // no longer loaded dropped.
    remap: Some(|v, r| {
        if let Some(o) = v.as_object_mut() {
            *o = std::mem::take(o).into_iter().filter_map(|(k, x)| Some((format!("{:08X}", r.map(u32::from_str_radix(&k, 16).ok()?)?), x))).collect();
        }
    }),
};

/// A title that changed since it was last shown: the box, the sound, the log line, held
/// back while the player is in combat and for a while after (`crate::after_combat`).
pub(crate) fn watch(
    mut script: Option<ResMut<ScriptState>>,
    sim: Res<SimState>,
    after: Option<Res<crate::after_combat::AfterCombat>>,
    mut shown: ResMut<Shown>,
    mut sounds: MessageWriter<PlaySound>,
    tweaks: Option<Res<crate::tweaks::TweakSettings>>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let g = &mut rt.game;
    if shown.0.is_some() && after.is_some_and(|a| !a.settled()) {
        return;
    }
    let now: BTreeMap<u32, (usize, usize)> = g.player.reputation.keys().chain(g.player.reputation_hidden.keys()).map(|&r| (r.0, levels(g, r))).collect();
    let Some(before) = shown.0.as_ref() else {
        shown.0 = Some(now);
        return;
    };
    if *before == now {
        return;
    }
    for (&r, &(fame, infamy)) in &now {
        let (f0, i0) = before.get(&r).copied().unwrap_or((0, 0));
        if (f0, i0) == (fame, infamy) {
            continue;
        }
        let rep = FormId(r);
        let [inf, fam] = gf::reputation_real(g, rep);
        let name = name(g, rep);
        info!("reputation: {name} {} -> {} (fame {fam}, infamy {inf})", title(f0, i0), title(fame, infamy));
        // The player's tweak keeps reputation changes quiet [design: docs/design/tweaks.md].
        if tweaks.as_ref().is_some_and(|t| t.no_reputation_messages) {
            continue;
        }
        g.show_box(rep);
        let sound = if fame > f0 { "UIRepGood" } else { "UIRepBad" };
        if let Some(s) = sim.forms.by_edid(sound) {
            sounds.write(PlaySound { sound: s, at: None });
        }
    }
    shown.0 = Some(now);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_faces_follow_fame_and_infamy() {
        assert_eq!(title(2, 0), "Liked");
        assert_eq!(title(0, 3), "Vilified");
        assert_eq!(title(2, 3), "Soft-Hearted Devil");
        assert_eq!(title(3, 2), "Dark Hero");
        assert_eq!(title(9, 9), "Wild Child");
        assert_eq!(face(2, 0), GOOD_FACE);
        assert_eq!(face(0, 3), BAD_FACE);
        assert_eq!(face(1, 1), MIXED_FACE);
    }

    #[test]
    fn a_plugins_description_wins() {
        let none = |_: &str| None;
        assert!(description(2, 0, none).starts_with("Enough news"));
        let one = |n: &str| (n == "sRepTitlePosTwoNegThreeDesc").then(|| "Theirs.".to_string());
        assert_eq!(description(2, 3, one), "Theirs.");
        assert!(description(3, 2, one).starts_with("Folks still think"));
    }
}
