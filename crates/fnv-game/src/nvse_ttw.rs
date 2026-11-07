//! The extender functions Tale of Two Wastelands calls that nothing else built (issue
//! #1408; docs/handoff/ttw-fo3-script-audit-2026-11-02.md §2): JIP's ScriptWait (with
//! IsScriptWaiting and StopScriptWaiting), MarkActivatorAshPile and AttachAshPileEx,
//! DonnerReedKuruParty, SetMusicState and SetTerminalUIModel, and JohnnyGuitar's
//! InitExtraMiscStat with ModExtraMiscStat. Behaviour from the GECK wiki pages (owner's
//! export), in our words in docs/spec-notes/jip-ttw-functions.md.

use crate::game::{Game, InstKey, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize) -> Option<f64> {
    match args.get(k)? {
        XVal::Num(n) => Some(*n),
        _ => None,
    }
}

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k)? {
        XVal::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// A form, or the forms of a form list [documented: MarkActivatorAshPile and
/// SetTerminalUIModel take either].
fn form_or_list(g: &Game, f: FormId) -> Vec<FormId> {
    if g.data.sig(f) == Some(*b"FLST") {
        crate::functions::list_members(g, f)
    } else {
        vec![f]
    }
}

/// The script instance IsScriptWaiting and StopScriptWaiting look at: the quest given,
/// else the calling reference's, else the running quest's [documented: "the script run
/// by the calling reference/quest"].
fn waiting_key(ctx: &RunCtx, subject: Option<FormId>, args: &[XVal]) -> Option<InstKey> {
    match form(args, 0) {
        Some(quest) => Some(InstKey::Quest { quest }),
        None => subject.map(|reference| InstKey::Ref { reference }).or(ctx.quest.map(|quest| InstKey::Quest { quest })),
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // `ScriptWait n`: the rest of the block waits n processing cycles (`Game::run_blocks`
        // keeps it, `fnv_script::vm::Vm::resume` goes on) [documented: ScriptWait]. Only a
        // script's own block waits: called from a result script, a function or anything
        // a block called, it does nothing [documented: not in effect, UDF or result
        // scripts; hypothesis: there it is ignored].
        "scriptwait" => {
            if g.jip.wait_depth == Some(g.depth) {
                g.jip.wait_request = Some(num(args, 0).unwrap_or(0.0).max(0.0) as u32);
            } else {
                tracing::info!("ScriptWait outside a script's block does not wait");
            }
            zero
        }
        "isscriptwaiting" => {
            let w = waiting_key(ctx, subject, args).is_some_and(|k| g.jip.waits.contains_key(&k));
            XVal::Num(f64::from(u8::from(w)))
        }
        // The waiting block goes on no more: it runs from its start the next time
        // [hypothesis: "stop it" read as dropping the wait, not running the rest].
        "stopscriptwaiting" => {
            if let Some(k) = waiting_key(ctx, subject, args) {
                g.jip.waits.remove(&k);
            }
            zero
        }
        // Activators marked as ash piles for the session [documented: MarkActivatorAshPile];
        // forms of another type are left out [hypothesis].
        "markactivatorashpile" => {
            if let Some(f) = form(args, 0) {
                for a in form_or_list(g, f) {
                    if g.data.sig(a) == Some(*b"ACTI") {
                        g.jip.kept.ash_pile_bases.insert(a);
                    }
                }
            }
            zero
        }
        // `actor.AttachAshPileEx activator`: as AttachAshPile, with that activator as the
        // pile [documented: AttachAshPileEx]. One not marked first does nothing
        // [hypothesis: the page says it "must first be marked"].
        "attachashpileex" => {
            let (Some(actor), Some(base)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            if g.jip.kept.ash_pile_bases.contains(&base) {
                if let Err(e) = g.attach_ash_pile_of(actor, base) {
                    tracing::info!("AttachAshPileEx on {actor}: {e}");
                }
            } else {
                tracing::info!("AttachAshPileEx on {actor}: {base} is not marked as an ash pile (MarkActivatorAshPile), no pile");
            }
            zero
        }
        // `actor.DonnerReedKuruParty 0/1`: the corpse eaten or not, as Cannibal's feeding
        // leaves it (HasBeenEaten) [documented: DonnerReedKuruParty, alias
        // TheOtherOtherWhiteMeat; eaten blocks feeding on it].
        "donnerreedkuruparty" | "theotherotherwhitemeat" => {
            if let Some(r) = subject {
                g.extra.entry(r).or_default().eaten = num(args, 0).unwrap_or(0.0) != 0.0;
            }
            zero
        }
        // 0 stop, 1 pause, 2 resume the music playing [documented: SetMusicState]; the
        // engine takes the requests (`nvf_play::music`).
        "setmusicstate" => {
            match num(args, 0).map(|n| n as i64) {
                Some(s @ 0..=2) => g.jip.music_states.push(s as u8),
                _ => tracing::info!("SetMusicState: not 0, 1 or 2, nothing done"),
            }
            zero
        }
        // The menu art of a terminal or the terminals of a list, a path under `meshes\`;
        // none: back to the default [documented: SetTerminalUIModel]. Kept for the engine's
        // terminal menu ([`Game::terminal_ui_model`]).
        "setterminaluimodel" => {
            if let Some(f) = form(args, 0) {
                let path = text(args, 1).filter(|p| !p.trim().is_empty());
                for t in form_or_list(g, f) {
                    match &path {
                        Some(p) => g.jip.kept.terminal_models.insert(t, p.clone()),
                        None => g.jip.kept.terminal_models.remove(&t),
                    };
                }
            }
            zero
        }
        // A custom misc stat at 0, required before the others use it [documented:
        // InitExtraMiscStat; GetExtraMiscStat and ModExtraMiscStat, "Starting from version
        // 4.98 InitExtraMiscStat is required"]. A stat already there keeps its value
        // [hypothesis].
        "initextramiscstat" => {
            if let Some(n) = text(args, 0) {
                g.jip2.misc_stats.entry(n.to_ascii_lowercase()).or_insert(0.0);
            }
            zero
        }
        // Adds to a stat InitExtraMiscStat made [documented: ModExtraMiscStat]; one not
        // made stays unknown [observed: GetExtraMiscStat -1 after ModExtraMiscStat alone].
        "modextramiscstat" => {
            if let (Some(n), Some(d)) = (text(args, 0), num(args, 1)) {
                if let Some(v) = g.jip2.misc_stats.get_mut(&n.to_ascii_lowercase()) {
                    *v += d.trunc();
                }
            }
            zero
        }
        _ => return None,
    };
    Some(Ok(v))
}

impl Game<'_> {
    /// The menu art SetTerminalUIModel gave a terminal (its base form), a path under
    /// `meshes\`; `None`: the default.
    pub fn terminal_ui_model(&self, terminal: FormId) -> Option<&str> {
        self.jip.kept.terminal_models.get(&terminal).map(String::as_str)
    }

    /// SetMusicState's requests since the last call, for the engine.
    pub fn take_music_states(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.jip.music_states)
    }
}
