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
        _ => return None,
    };
    Some(Ok(v))
}
