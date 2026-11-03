//! kNVSE's animation functions, first slice (N5 of docs/handoff/nvse-next-batch.md;
//! docs/design/knvse-anim.md) [documented: their pages; observed: the peer's knvse-anim
//! set, kNVSE 37]. The game keeps what scripts asked (replacement paths by actor and
//! weapon, sequences a script played) and answers the getters from the clip the engine
//! says each actor's body plays (`AnimState::clip`). The engine does not yet play a
//! replacement or a script's sequence (slice 2 of the design).

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::HashMap;

/// What scripts asked of actors' and weapons' animations this session.
#[derive(Debug, Default)]
pub struct AnimPaths {
    /// SetActorAnimationPath's replacements: (actor, first person) -> paths.
    pub actor: HashMap<(FormId, bool), Vec<String>>,
    /// SetWeaponAnimationPath's: (weapon, first person) -> paths.
    pub weapon: HashMap<(FormId, bool), Vec<String>>,
    /// Sequences a script played on an actor (PlayAnimationPath, ActivateAnim), in the
    /// script's spelling [observed: "sequences loaded by a script keep the script's
    /// spelling"].
    pub played: HashMap<FormId, Vec<String>>,
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

/// Two sequence paths name the same file: without regard to case [decision: NVForever's
/// paths do not keep the original's spelling, which the game compares exactly
/// (observed: "mtidle.kf" in lower case did not match the game's "Characters\_Male\
/// Locomotion\mtidle.kf")].
pub(crate) fn same(a: &str, b: &str) -> bool {
    a.replace('/', "\\").eq_ignore_ascii_case(&b.replace('/', "\\"))
}

/// Whether a path under meshes is a file of the game (loose or in an archive).
pub(crate) fn file_exists(path: &str) -> bool {
    crate::mod_files::exists(&format!("meshes\\{}", path.replace('/', "\\")))
}

impl Game<'_> {
    /// The sequences an actor plays as far as this side knows: its body's clip, then
    /// what scripts played.
    pub(crate) fn playing_sequences(&self, a: FormId) -> Vec<String> {
        let mut v: Vec<String> = self.senses.anim.get(&a).and_then(|s| s.clip.clone()).into_iter().collect();
        v.extend(self.anim_paths.played.get(&a).cloned().unwrap_or_default());
        v
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let list = |g: &mut Game, items: Vec<XVal>| {
        let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
        XVal::Array(g.ext.new_array(ArrayKind::List, entries))
    };
    let a = subject.unwrap_or(FormId::PLAYER_REF);
    let v = match name {
        // `firstPerson enable path …`: 1 when the file is there (or when disabling)
        // [observed: a missing file gives 0, disabling 1].
        "setactoranimationpath" | "setweaponanimationpath" => {
            let weapon = name == "setweaponanimationpath";
            let at = usize::from(weapon);
            let owner = if weapon { form(args, 0).unwrap_or(FormId(0)) } else { a };
            let first = num(args, at, 0.0) != 0.0;
            let enable = num(args, at + 1, 1.0) != 0.0;
            let path = text(args, at + 2);
            let map = if weapon { &mut g.anim_paths.weapon } else { &mut g.anim_paths.actor };
            let paths = map.entry((owner, first)).or_default();
            if !enable {
                paths.retain(|p| !same(p, &path));
                return Some(Ok(XVal::Num(1.0)));
            }
            if !file_exists(&path) {
                return Some(Ok(zero));
            }
            if !paths.iter().any(|p| same(p, &path)) {
                paths.push(path);
            }
            XVal::Num(1.0)
        }
        "playanimationpath" | "activateanim" => {
            let path = text(args, 0);
            if file_exists(&path) {
                let p = g.anim_paths.played.entry(a).or_default();
                if !p.iter().any(|x| same(x, &path)) {
                    p.push(path.clone());
                    // Its current time counts from here (`crate::nvse_knvse`).
                    g.knvse_started(a, &path);
                }
            }
            zero
        }
        "deactivateanim" => {
            let path = text(args, 0);
            if let Some(p) = g.anim_paths.played.get_mut(&a) {
                p.retain(|x| !same(x, &path));
            }
            zero
        }
        "isanimsequenceplaying" => {
            let path = text(args, 0);
            XVal::Num(f64::from(u8::from(g.playing_sequences(a).iter().any(|p| same(p, &path)))))
        }
        // Eight slots, the body's clip first [observed: an array of 8, [0] filled].
        "getplayinganimsequencepaths" => {
            let mut v: Vec<XVal> = g.senses.anim.get(&a).and_then(|s| s.clip.clone()).into_iter().map(XVal::Str).collect();
            v.resize(8, XVal::Str(String::new()));
            list(g, v)
        }
        "getactiveanims" => {
            let v = g.playing_sequences(a).into_iter().map(XVal::Str).collect();
            list(g, v)
        }
        // [observed: Idle reads 1 on a standing actor]
        "isanimplayingalt" => {
            let want = text(args, 0);
            XVal::Num(f64::from(u8::from(g.senses.anim.get(&a).and_then(|s| s.group).is_some_and(|gr| gr.eq_ignore_ascii_case(&want)))))
        }
        "getanimationsbyanimgroup" => {
            let want = text(args, 0);
            let st = g.senses.anim.get(&a);
            let v = st.filter(|s| s.group.is_some_and(|gr| gr.eq_ignore_ascii_case(&want))).and_then(|s| s.clip.clone()).into_iter().map(XVal::Str).collect();
            list(g, v)
        }
        _ => return None,
    };
    Some(Ok(v))
}
