//! Extender functions High and Dry calls most that were not built (batch hnd1, 2026-11-09;
//! docs/spec-notes/ext-hnd1.md). Each follows what the original game answered with High
//! and Dry's plugins (probe set quests2, `fnv_work/nvf_hnd/probes/quests2.py`) [observed];
//! what the probe did not settle is marked [hypothesis].

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k) {
        Some(XVal::Ref(f)) if *f != 0 => Some(FormId(*f)),
        Some(XVal::Num(x)) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

/// A list array of the values, or 0 when there are none: JIP's list functions gave no
/// array for an empty result (GetObjectiveTargets, GetQuestObjectives with flags 1-4)
/// [observed: quests2].
fn list_or_zero(g: &mut Game, items: Vec<XVal>) -> XVal {
    if items.is_empty() {
        return XVal::Num(0.0);
    }
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let v = match name {
        // `flag`: 0 the quests under way (not completed, an objective shown), any other the
        // completed ones [observed: quests2, GetQuests 0 gave VMQ01 (running) and VCG02 (not
        // running, not completed); 1, 2, 3, 4 and 8 gave VCG01 (completed)].
        "getquests" => {
            let flag = num(args, 0, 0.0);
            let mut out = Vec::new();
            for (q, s) in &g.quests {
                let take = if flag == 0.0 { !s.completed && s.objectives.values().any(|o| o.displayed) } else { s.completed };
                if take {
                    out.push(XVal::Ref(q.0));
                }
            }
            list_or_zero(g, out)
        }
        // `quest flag`: flag 0, the indices of its shown objectives [observed: quests2,
        // VMQ01 gave 10 30 25: the order there is the original's, here ascending
        // (hypothesis: the quest record's order)]; any other flag gave no array
        // [observed], so none.
        "getquestobjectives" => {
            let Some(q) = form(args, 0) else { return Some(Ok(XVal::Num(0.0))) };
            let flag = num(args, 1, 0.0);
            let shown: Vec<XVal> = if flag == 0.0 {
                g.quest_state(q).objectives.iter().filter(|(_, o)| o.displayed).map(|(i, _)| XVal::Num(f64::from(*i))).collect()
            } else {
                Vec::new()
            };
            list_or_zero(g, shown)
        }
        // `quest index`: the objective's target references; VMQ01's objective 10 has
        // none and gave no array [observed: quests2]. NVForever keeps targets on the
        // quest's records (QSTA) [hypothesis: those are what the original lists].
        "getobjectivetargets" => {
            let (Some(q), index) = (form(args, 0), num(args, 1, 0.0) as i32) else { return Some(Ok(XVal::Num(0.0))) };
            let targets: Vec<XVal> = g
                .data
                .quests
                .get(&q)
                .and_then(|d| d.objectives.get(&index))
                .map(|o| o.targets.iter().filter_map(|t| t.target).map(|t| XVal::Ref(t.0)).collect())
                .unwrap_or_default();
            list_or_zero(g, targets)
        }
        // On an actor: the leveled list its placed record names as its base; a placed
        // actor gave none [observed: quests2, GSBigHorner].
        "getactorleveledlist" => {
            let Some(r) = subject else { return Some(Ok(XVal::Ref(0))) };
            let lvl = g.world.placed.get(r).map(|p| p.base).filter(|b| g.data.leveled_actors.contains_key(b));
            XVal::Ref(lvl.map_or(0, |b| b.0))
        }
        // `sequence`: the animation group playing in that sequence slot, -1 for none;
        // slot 0 gave 0 (Idle) on a standing actor and the player, slots 1-4 -1
        // [observed: quests2]. Slot 0 is the group the body plays here; the others
        // (weapon, special idle …) are not kept [hypothesis].
        "getsequenceanimgroup" => {
            let Some(r) = subject else { return Some(Ok(XVal::Num(-1.0))) };
            if num(args, 0, 0.0) != 0.0 {
                XVal::Num(-1.0)
            } else {
                let code = g.anims.playing(r).map(f64::from).or_else(|| {
                    let group = g.anim_state(r).and_then(|s| s.group)?;
                    g.data.table.enums.get("anim_group")?.iter().find(|(_, n)| n.eq_ignore_ascii_case(group)).map(|(c, _)| f64::from(*c))
                });
                XVal::Num(code.unwrap_or(0.0))
            }
        }
        // `actorValue base`: the base value an actor with that base form has, by the rules
        // GetBaseActorValue follows (the player's base reads the player's) [observed:
        // hnd2, player Health 75, Strength 6; DocMitchell 50, 4].
        "getbaseactorvaluealt" => {
            let code = match args.first() {
                Some(XVal::Num(x)) => *x as u16,
                _ => return Some(Ok(XVal::Num(0.0))),
            };
            let Some(base) = form(args, 1) else { return Some(Ok(XVal::Num(0.0))) };
            let r = if base == FormId::PLAYER_BASE {
                Some(FormId::PLAYER_REF)
            } else {
                g.world.placed.refs.iter().find(|(_, p)| p.base == base).map(|(id, _)| *id)
            };
            match r {
                Some(r) => XVal::Num(g.base_actor_value(r, code).unwrap_or(0.0)),
                None => XVal::Num(0.0),
            }
        }
        // `firstPerson`: the camera's field of view, as Anh's GetCameraFOV [observed:
        // hnd2, 0 gave 80 and 1 gave 75 with High and Dry's INI].
        "getplayercamfov" => return crate::nvse_small::call(g, _ctx, "getcamerafov", subject, args),
        // The actor's equipped weapon has a scope (WEAP DNAM Has Scope) [observed: hnd2,
        // the player without one 0; hypothesis: a scope weapon mod counts too, not read].
        "isequippedweaponscoped" => {
            let w = crate::nvse::call(g, _ctx, "getequippedobject", subject, &[XVal::Num(5.0)]);
            let w = match w {
                Some(Ok(XVal::Ref(w))) if w != 0 => w,
                _ => return Some(Ok(XVal::Num(0.0))),
            };
            return crate::nvse::call(g, _ctx, "getweaponhasscope", None, &[XVal::Ref(w)]);
        }
        // `actorBase`: its factions, the records' list with the player's changes for the
        // player's base [observed: hnd3, the player's base 22, DocMitchell's 3 in the save;
        // hypothesis: the run-time changes are what make the player's 22].
        "getfactions" => {
            let Some(base) = form(args, 0) else { return Some(Ok(XVal::Num(0.0))) };
            let mut out: Vec<FormId> = g.data.actors.get(&base).map(|a| a.factions.iter().map(|(f, _)| *f).collect()).unwrap_or_default();
            if base == FormId::PLAYER_BASE {
                if let Some(e) = g.extra.get(&FormId::PLAYER_REF) {
                    for (f, rank) in &e.factions {
                        out.retain(|x| x != f);
                        if *rank >= 0 {
                            out.push(*f);
                        }
                    }
                }
            }
            let items = out.into_iter().map(|f| XVal::Ref(f.0)).collect();
            list_or_zero(g, items)
        }
        // yUI's item traits (`trait [form]`): what ySISetTrait gave, else "" [hypothesis:
        // ySI's own sorting tags are not read].
        "ysigettrait" => {
            let key = (text(args, 0).to_ascii_lowercase(), form(args, 1).map_or(0, |f| f.0));
            XVal::Str(g.gaps.hnd.ysi_traits.get(&key).cloned().unwrap_or_default())
        }
        "ysisettrait" => {
            let key = (text(args, 0).to_ascii_lowercase(), form(args, 2).map_or(0, |f| f.0));
            g.gaps.hnd.ysi_traits.insert(key, text(args, 1));
            XVal::Num(1.0)
        }
        _ => match string_maps(g, name, args) {
            Some(v) => v,
            None => return None,
        },
    };
    Some(Ok(v))
}

/// ShowOff's aux string maps: by name, keys in the order first set. Names and keys ignore
/// case [observed: hnd2, `GetFloat "NVFPROBE" "K1"` read what `"nvfProbe" "k1"` set].
/// Kept for the session [hypothesis: whether the original saves them is open].
#[derive(Debug, Default)]
pub struct HndState {
    pub string_maps: std::collections::BTreeMap<String, Vec<(String, XVal)>>,
    /// Where AuxStringMapArrayGetNext goes on from, per map.
    pub cursors: std::collections::HashMap<String, usize>,
    /// yUI's traits set by ySISetTrait: (trait, form) -> value.
    pub ysi_traits: std::collections::HashMap<(String, u32), String>,
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The element type code, as JIP's aux variables give them [hypothesis for ShowOff]:
/// 0 none, 1 number, 2 form, 4 string.
fn type_code(v: Option<&XVal>) -> f64 {
    match v {
        Some(XVal::Num(_)) => 1.0,
        Some(XVal::Ref(_)) => 2.0,
        Some(XVal::Str(_)) => 4.0,
        _ => 0.0,
    }
}

fn pair(g: &mut Game, k: &str, v: &XVal) -> XVal {
    let entries = vec![(XKey::Str("key".into()), XVal::Str(k.to_string())), (XKey::Str("value".into()), v.clone())];
    XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
}

/// ShowOff's AuxStringMapArray… family, `None` for any other name [observed: hnd2 for get,
/// set, missing keys, types, SetFromArray and Destroy; the rest from the names and the
/// parameter types the original lists, hypothesis].
pub(crate) fn string_maps(g: &mut Game, name: &str, args: &[XVal]) -> Option<XVal> {
    let op = name.strip_prefix("auxstringmaparray").or_else(|| name.strip_prefix("auxstringmap"))?;
    let map = text(args, 0).to_ascii_lowercase();
    let key = text(args, 1);
    let lkey = key.to_ascii_lowercase();
    let get = |g: &Game| g.gaps.hnd.string_maps.get(&map).and_then(|m| m.iter().find(|(k, _)| k.to_ascii_lowercase() == lkey)).map(|(_, v)| v.clone());
    let set = |g: &mut Game, v: XVal| {
        let m = g.gaps.hnd.string_maps.entry(map.clone()).or_default();
        match m.iter_mut().find(|(k, _)| k.to_ascii_lowercase() == lkey) {
            Some(e) => e.1 = v,
            None => m.push((key.clone(), v)),
        }
    };
    Some(match op {
        // A missing key, or one holding another type, reads 0 / "" / none [observed: hnd2].
        "getfloat" | "getflt" => match get(g) {
            Some(XVal::Num(x)) => XVal::Num(x),
            _ => XVal::Num(0.0),
        },
        "getstring" | "getstr" => match get(g) {
            Some(XVal::Str(s)) => XVal::Str(s),
            _ => XVal::Str(String::new()),
        },
        "getref" => match get(g) {
            Some(XVal::Ref(r)) => XVal::Ref(r),
            _ => XVal::Ref(0),
        },
        "getvaluetype" => XVal::Num(type_code(get(g).as_ref())),
        // Setters return 0 [observed: hnd2, SetFloat].
        "setfloat" | "setflt" => {
            set(g, XVal::Num(match args.get(2) {
                Some(XVal::Num(x)) => *x,
                _ => 0.0,
            }));
            XVal::Num(0.0)
        }
        "setstring" | "setstr" => {
            set(g, XVal::Str(text(args, 2)));
            XVal::Num(0.0)
        }
        "setref" => {
            set(g, XVal::Ref(form(args, 2).map_or(0, |f| f.0)));
            XVal::Num(0.0)
        }
        "erasekey" => {
            let had = g.gaps.hnd.string_maps.get_mut(&map).map(|m| {
                let n = m.len();
                m.retain(|(k, _)| k.to_ascii_lowercase() != lkey);
                n != m.len()
            });
            XVal::Num(f64::from(u8::from(had.unwrap_or(false))))
        }
        // Destroy returned 0 for a map it emptied and for none; reads after it give 0
        // [observed: hnd2].
        "destroy" => {
            g.gaps.hnd.string_maps.remove(&map);
            g.gaps.hnd.cursors.remove(&map);
            XVal::Num(0.0)
        }
        "getsize" | "size" => XVal::Num(g.gaps.hnd.string_maps.get(&map).map_or(0, |m| m.len()) as f64),
        "getkeys" | "keys" => {
            let keys: Vec<XVal> = g.gaps.hnd.string_maps.get(&map).map(|m| m.iter().map(|(k, _)| XVal::Str(k.clone())).collect()).unwrap_or_default();
            let entries = keys.into_iter().enumerate().map(|(i, k)| (XKey::Num(i as f64), k)).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        "getasarray" | "getasarr" => {
            let entries: Vec<(XKey, XVal)> = g.gaps.hnd.string_maps.get(&map).map(|m| m.iter().map(|(k, v)| (XKey::Str(k.clone()), v.clone())).collect()).unwrap_or_default();
            XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
        }
        // The map's values from a string map array (keys and values copied) [observed:
        // hnd2, `ar_Map "a"::1 "b"::"x"` read back 1 and "x"].
        "setfromarray" | "setfromarr" => {
            let XVal::Array(id) = args.get(1).cloned().unwrap_or(XVal::Num(0.0)) else { return Some(XVal::Num(0.0)) };
            let items: Vec<(String, XVal)> = g.ext.array(id).map(|a| a.entries.iter().filter_map(|(k, v)| match k {
                XKey::Str(s) => Some((s.clone(), v.clone())),
                _ => None,
            }).collect()).unwrap_or_default();
            let m = g.gaps.hnd.string_maps.entry(map.clone()).or_default();
            for (k, v) in items {
                let lk = k.to_ascii_lowercase();
                match m.iter_mut().find(|(x, _)| x.to_ascii_lowercase() == lk) {
                    Some(e) => e.1 = v,
                    None => m.push((k, v)),
                }
            }
            XVal::Num(0.0)
        }
        "getfirst" | "first" | "getnext" | "next" => {
            let first = op == "getfirst" || op == "first";
            let at = if first { 0 } else { g.gaps.hnd.cursors.get(&map).copied().unwrap_or(0) };
            let item = g.gaps.hnd.string_maps.get(&map).and_then(|m| m.get(at)).cloned();
            g.gaps.hnd.cursors.insert(map.clone(), at + 1);
            match item {
                Some((k, v)) => pair(g, &k, &v),
                None => XVal::Num(0.0),
            }
        }
        // Drops the entries whose form is gone; nothing here goes away [hypothesis].
        "validatevalues" | "validatevals" => XVal::Num(0.0),
        "getall" => {
            let names: Vec<XVal> = g.gaps.hnd.string_maps.keys().map(|k| XVal::Str(k.clone())).collect();
            let entries = names.into_iter().enumerate().map(|(i, k)| (XKey::Num(i as f64), k)).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        _ => return None,
    })
}
