//! Extender functions High and Dry calls that were not built (batch hnd3, 2026-11-09;
//! docs/spec-notes/ext-hnd1.md §hnd3). Three kinds: ones that are a vanilla function with
//! more arguments run that function; setters keep their value for the session and the
//! matching getter reads it back; what only the original's renderer or interface shows
//! (map icons, UI projection, camera queries) answers as nothing is there. Where the
//! original's answer is not known the reading is marked [hypothesis].

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArgValue, Value, XVal};
use fnv_sim::FormId;
use std::collections::HashMap;

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        Some(XVal::Ref(r)) => f64::from(*r),
        _ => dflt,
    }
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        Some(XVal::Num(x)) => format!("{x}"),
        _ => String::new(),
    }
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k) {
        Some(XVal::Ref(r)) if *r != 0 => Some(FormId(*r)),
        Some(XVal::Num(x)) if *x > 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn nv(x: f64) -> ArgValue {
    ArgValue::Value(Value::Num(x))
}

fn fv(f: FormId) -> ArgValue {
    ArgValue::Value(Value::Ref(f.0))
}

/// Run a game function by its catalogue name, as `on.Name args` would.
fn vanilla(g: &mut Game, ctx: &RunCtx, name: &str, on: Option<FormId>, args: Vec<ArgValue>) -> Value {
    let Some(opcode) = g.data.table.by_name(name) else { return Value::ZERO };
    let c = crate::functions::Ctx {
        unit: "hnd3".into(),
        opcode,
        on,
        self_ref: ctx.self_ref,
        action_ref: ctx.action_ref,
        quest: ctx.quest,
        inst: None,
        in_condition: false,
    };
    g.call_function(&c, &args)
}

fn xv(v: Value) -> XVal {
    XVal::from_value(v)
}

/// What these functions keep for the session.
#[derive(Debug, Default)]
pub struct Hnd3State {
    /// Companion Command & Control's saved numbers, strings and forms, by slot.
    pub ccc_float: HashMap<i64, f64>,
    pub ccc_string: HashMap<i64, String>,
    pub ccc_form: HashMap<i64, FormId>,
    /// Follow state, traits and NCCS settings (by actor).
    pub ccc_follow: HashMap<u32, i64>,
    pub ccc_traits: HashMap<(u32, i64), i64>,
    /// SetRefExtraData: (reference, slot) -> value.
    pub ref_extra: HashMap<(u32, i64), XVal>,
    /// SetDetonationTimer, by reference.
    pub detonation: HashMap<u32, f64>,
    /// Scripts SetScriptDisabled turned off.
    pub scripts_disabled: std::collections::HashSet<u32>,
    /// SetActivatorPrompt: activator (0 the calling reference's base) -> prompt.
    pub activator_prompt: HashMap<u32, String>,
    /// Load screen textures set, by screen.
    pub load_screens: HashMap<u32, String>,
    /// Custom map marker icons, by marker; reputation change icons, by (faction, tier).
    pub map_icons: HashMap<u32, String>,
    pub rep_icons: HashMap<(u32, i64), String>,
    /// Lights attached by name (AttachLight / RemoveLight): node -> light form.
    pub attached_lights: HashMap<(u32, String), FormId>,
    /// Saves turned off (ToggleDisableSaves).
    pub saves_disabled: bool,
    /// Toggles kept for their getters (detection fix, purge on unload, ignore locked
    /// doors, collision, VATS targetable, equipment lock, teammate ammo).
    pub toggles: HashMap<(String, u32), f64>,
    /// Actor template flags set (base, flag) -> on.
    pub template_flags: HashMap<(u32, i64), bool>,
    /// Variables added to references by AddScriptVariable: (reference, name).
    pub added_vars: std::collections::HashSet<(u32, String)>,
}

pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let one = XVal::Num(1.0);
    let me = subject.unwrap_or(FormId::PLAYER_REF);
    let v = match name {
        // --- Vanilla functions with more arguments -------------------------------------
        // `x y z [method]`: SetAngle on each axis [documented: SetAngleEx sets all three;
        // hypothesis: the method (local or world) is not told apart].
        "setangleex" => {
            for (k, a) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                vanilla(g, ctx, "SetAngle", subject, vec![ArgValue::Axis(a), nv(num(args, k, 0.0))]);
            }
            zero
        }
        "playidleex" => match form(args, 0) {
            Some(i) => xv(vanilla(g, ctx, "PlayIdle", subject, vec![fv(i)])),
            None => zero,
        },
        // Pushes as PushActorAway from the actor's own place, by the force given
        // [hypothesis: the offsets and the no-ragdoll form move the same way here].
        "pushactorawayalt" | "pushactornoragdoll" => {
            let force = num(args, 0, 0.0);
            xv(vanilla(g, ctx, "PushActorAway", subject, vec![fv(me), nv(force)]))
        }
        "fireweaponex" => match form(args, 0) {
            Some(w) => xv(vanilla(g, ctx, "FireWeapon", subject, vec![fv(w)])),
            None => zero,
        },
        // `target [flags]` / `target node [x y z]`: moved to the target (its node not told
        // apart) [hypothesis].
        "traveltoref" | "movetonode" => match form(args, 0) {
            Some(t) => {
                let off = if name == "movetonode" { [num(args, 2, 0.0), num(args, 3, 0.0), num(args, 4, 0.0)] } else { [0.0; 3] };
                vanilla(g, ctx, "MoveTo", subject, vec![fv(t), nv(off[0]), nv(off[1]), nv(off[2])]);
                one
            }
            None => zero,
        },
        // `"x y z"` (JIP's position string): SetPos on each axis.
        "movetoposstr" => {
            let parts: Vec<f64> = text(args, 0).split(|c: char| c.is_whitespace() || c == ',').filter_map(|p| p.parse().ok()).collect();
            if parts.len() >= 3 {
                for (a, p) in ['X', 'Y', 'Z'].into_iter().zip(&parts) {
                    vanilla(g, ctx, "SetPos", subject, vec![ArgValue::Axis(a), nv(*p)]);
                }
                one
            } else {
                zero
            }
        }
        // The calling reference's place as JIP's position string.
        "reftoposstr" => {
            let p: Vec<f64> = ['X', 'Y', 'Z'].into_iter().map(|a| vanilla(g, ctx, "GetPos", subject, vec![ArgValue::Axis(a)]).num()).collect();
            XVal::Str(format!("{:.4} {:.4} {:.4}", p[0], p[1], p[2]))
        }
        // The player's reticle target's place is not traced here: the reference stays
        // [hypothesis: 0, not moved].
        "movetoreticle" => zero,
        // Stepping out of the player's way: NVForever's actors already make room when
        // the player walks into them [hypothesis: nothing more].
        "moveawayfromplayer" => zero,
        // Damage resistance and threshold as the actor values hold them [hypothesis: the
        // armour's effective values are the actor's totals].
        "getarmoreffectivedr" => xv(vanilla(g, ctx, "GetActorValue", subject, vec![ArgValue::Enum(18)])),
        "getarmoreffectivedt" => xv(vanilla(g, ctx, "GetActorValue", subject, vec![ArgValue::Enum(76)])),
        "setbasefactionrank" => match form(args, 0) {
            Some(f) => xv(vanilla(g, ctx, "SetFactionRank", subject, vec![fv(f), nv(num(args, 1, 0.0))])),
            None => zero,
        },
        "clearactivequest" => {
            g.gaps.hnd3.toggles.insert(("activequest".into(), 0), 0.0);
            zero
        }
        "dismemberlimb" => {
            // Kill-and-dismember without the gore system here: the limb's actor value
            // to 0 [hypothesis: body part ids 1-6 are the limb conditions 25-30].
            let part = num(args, 0, 0.0) as u16;
            if (1..=6).contains(&part) {
                vanilla(g, ctx, "SetActorValue", subject, vec![ArgValue::Enum(24 + part), nv(0.0)]);
            }
            zero
        }
        // --- Companion Command & Control (JIP's CCC functions) -------------------------
        "cccsetfloat" => {
            g.gaps.hnd3.ccc_float.insert(num(args, 0, 0.0) as i64, num(args, 1, 0.0));
            zero
        }
        "cccsetstring" => {
            g.gaps.hnd3.ccc_string.insert(num(args, 0, 0.0) as i64, text(args, 1));
            zero
        }
        "cccsavedform" => g.gaps.hnd3.ccc_form.get(&(num(args, 0, 0.0) as i64)).map_or(zero, |f| XVal::Ref(f.0)),
        "cccinfaction" => match form(args, 0) {
            Some(f) => xv(vanilla(g, ctx, "GetInFaction", subject, vec![fv(f)])),
            None => zero,
        },
        "cccsayto" => match (form(args, 0), form(args, 1)) {
            (Some(a), Some(t)) => xv(vanilla(g, ctx, "SayTo", subject, vec![fv(a), fv(t)])),
            _ => zero,
        },
        "cccgetdistance" => match form(args, 0) {
            Some(r) => xv(vanilla(g, ctx, "GetDistance", subject, vec![fv(r)])),
            None => zero,
        },
        "cccsetfollowstate" => {
            g.gaps.hnd3.ccc_follow.insert(me.0, num(args, 0, 0.0) as i64);
            zero
        }
        "cccsettrait" => {
            g.gaps.hnd3.ccc_traits.insert((me.0, num(args, 0, 0.0) as i64), num(args, 1, 0.0) as i64);
            zero
        }
        // Equipment, NCCS settings and load hooks: kept by the companion's own scripts
        // here [hypothesis: nothing more to do without the plugin's UI].
        "cccsetequipped" | "cccsetnccs" | "cccloadnccs" | "cccsms" | "cccrunresultscripts" | "ccconload" => zero,
        "cccgetreputation" => zero,
        "ccclocationname" => XVal::Str(String::new()),
        // --- Setters and their getters -------------------------------------------------
        "setrefextradata" => {
            g.gaps.hnd3.ref_extra.insert((me.0, num(args, 0, 0.0) as i64), args.get(1).cloned().unwrap_or(XVal::Num(0.0)));
            zero
        }
        "getrefextradata" => g.gaps.hnd3.ref_extra.get(&(me.0, num(args, 0, 0.0) as i64)).cloned().unwrap_or(zero),
        "setdetonationtimer" => {
            g.gaps.hnd3.detonation.insert(me.0, num(args, 0, 0.0));
            zero
        }
        "getdetonationtimer" => XVal::Num(g.gaps.hnd3.detonation.get(&me.0).copied().unwrap_or(0.0)),
        "setscriptdisabled" => {
            if let Some(s) = form(args, 0) {
                if num(args, 1, 0.0) != 0.0 {
                    g.gaps.hnd3.scripts_disabled.insert(s.0);
                } else {
                    g.gaps.hnd3.scripts_disabled.remove(&s.0);
                }
            }
            zero
        }
        "setactivatorprompt" => {
            let key = form(args, 1).or_else(|| subject.and_then(|r| crate::functions::base_of(g, r))).map_or(0, |f| f.0);
            g.gaps.hnd3.activator_prompt.insert(key, text(args, 0));
            zero
        }
        "getactivatorprompt" => {
            let key = form(args, 0).or_else(|| subject.and_then(|r| crate::functions::base_of(g, r))).map_or(0, |f| f.0);
            XVal::Str(g.gaps.hnd3.activator_prompt.get(&key).cloned().unwrap_or_default())
        }
        "setloadscreentexture" => {
            if let Some(s) = form(args, 0) {
                g.gaps.hnd3.load_screens.insert(s.0, text(args, 1));
            }
            zero
        }
        "setcustommapmarkericon" => {
            g.gaps.hnd3.map_icons.insert(me.0, text(args, 0));
            one
        }
        "getcustommapmarkericon" => XVal::Str(g.gaps.hnd3.map_icons.get(&me.0).cloned().unwrap_or_default()),
        "setcustomreputationchangeicon" => {
            if let Some(f) = form(args, 0) {
                g.gaps.hnd3.rep_icons.insert((f.0, num(args, 1, 0.0) as i64), text(args, 2));
                one
            } else {
                zero
            }
        }
        // `"node" light [...]` / `"node"`: the light kept by node [hypothesis: NVForever
        // draws a reference's lights from its model; an attached one is not drawn yet].
        "attachlight" => match form(args, 1) {
            Some(l) => {
                g.gaps.hnd3.attached_lights.insert((me.0, text(args, 0).to_ascii_lowercase()), l);
                one
            }
            None => zero,
        },
        "removelight" => XVal::Num(f64::from(u8::from(g.gaps.hnd3.attached_lights.remove(&(me.0, text(args, 0).to_ascii_lowercase())).is_some()))),
        "toggledisablesaves" => {
            g.gaps.hnd3.saves_disabled = num(args, 0, 0.0) != 0.0;
            zero
        }
        "setactortemplateflag" => {
            let base = form(args, 2).or_else(|| subject.and_then(|r| crate::functions::base_of(g, r))).map_or(0, |f| f.0);
            g.gaps.hnd3.template_flags.insert((base, num(args, 0, 0.0) as i64), num(args, 1, 0.0) != 0.0);
            zero
        }
        "addscriptvariable" => {
            g.gaps.hnd3.added_vars.insert((me.0, text(args, 0).to_ascii_lowercase()));
            one
        }
        "hasvariableadded" => XVal::Num(f64::from(u8::from(g.gaps.hnd3.added_vars.contains(&(me.0, text(args, 0).to_ascii_lowercase()))))),
        // On/off switches kept for the session; their getters, where there is one, read
        // them back.
        "toggledetectionfix" | "togglepurgeonunload" | "toggleignorelockeddoors" | "toggleobjectcollision" | "setvatstargetable" | "lockequipment" | "setteammateusingammo" | "disablescriptedactivate" | "enableimprovedrecipemenu" => {
            g.gaps.hnd3.toggles.insert((name.to_string(), me.0), num(args, 0, 0.0));
            zero
        }
        // --- What only the original's renderer, camera or interface has ----------------
        // `xOut yOut zOut x y z [type] [ref]`: on screen or not; the view's projection is
        // the renderer's [hypothesis: 0, off screen, outputs 0].
        "worldtoscreen" => crate::nvse_jip3::outputs(g, zero, &[0.0, 0.0, 0.0]),
        "getworldmapposmults" => crate::nvse_jip3::outputs(g, zero, &[1.0, 1.0]),
        "projectuitile" | "updatecrosshairprompt" | "deactivateallhighlightsalt" | "highlightbodypartalt" | "setcamerashake" | "update3dalt" | "refreshidle" | "resetfalltime" | "togglepipboy" => zero,
        "getcameratranslation" => crate::nvse_jip3::outputs(g, zero, &[0.0, 0.0, 0.0]),
        "get3ddistancetoninode" | "get3ddistancebetweenninodes" | "getanimsequencefrequency" | "getactoralpha" => XVal::Num(if name == "getactoralpha" { 1.0 } else { 0.0 }),
        "setanimadditiveinterpsweightmult" | "isanimadditive" => zero,
        "setnipsysmodifiervalue" | "setparticleemitterspawnrate" | "swaptextureex" | "removescopemodelpath" | "ejectcasing" | "setequippeddata" | "setbaseform" => zero,
        "playsoundfrompath" | "playsound3dfrompath" | "stopsound3dfrompath" => zero,
        "getrecipemenuselection" | "getrecipemenucategory" | "getreticlenode" | "getcontainerclosesound" | "getcontaineropensound" | "getequippedweaponpoison" | "getworldspacepersistentcell" => zero,
        "getcellnorthrotation" | "getgroundmaterial" | "gethascontact" | "getdetectionvalue" | "getactorlightamount" | "getkillxp" | "getrunspeed" | "getrespawn" | "getsleepwaitmenustate" | "isgrabbable" | "calculateshotspersec" => zero,
        "getviewmodelclipdistance" => XVal::Num(g.gaps.hnd2.viewmodel_clip.unwrap_or(0.0)),
        "getreticleposalt" | "getcenterpos" => crate::nvse_jip3::outputs(g, zero, &[0.0, 0.0, 0.0]),
        "getcurrentskycolor" => crate::nvse_jip3::outputs(g, zero, &[0.0, 0.0, 0.0]),
        "getnibound" => crate::nvse_jip3::outputs(g, zero, &[0.0, 0.0, 0.0, 0.0]),
        "getnifblocknumverticles" | "nifblockgetverticles" => zero,
        // Menus NVForever's interface does not have yet: closed at once [hypothesis].
        "showquantitymenu" | "showtextinputmenu" | "settextinputextendedprops" => zero,
        "getactiveobjectives" | "getcombattargets" | "getcombatallies" => XVal::Array(g.ext.new_array(fnv_script::vm::ArrayKind::List, Vec::new())),
        // Script and plugin plumbing.
        "fakescriptevent" | "reloadpluginconfig" | "clearjipsaveddata" | "runbatchscript" | "setmcmmodlist" | "hudbarsetvalueudf" => zero,
        "getmcmlistwidth" => zero,
        "setmcmstringmass" => {
            let (lo, hi) = (num(args, 1, 0.0) as i64, num(args, 2, 0.0) as i64);
            for k in lo..=hi.min(lo + 256) {
                let a = [args.first().cloned().unwrap_or(XVal::Num(0.0)), XVal::Num(k as f64), XVal::Str(text(args, 3)), XVal::Str(text(args, 4))];
                let _ = crate::ui_functions::call(g, "setmcmstring", &a);
            }
            zero
        }
        "datan2" => XVal::Num(num(args, 0, 0.0).atan2(num(args, 1, 1.0)).to_degrees()),
        "getdirectoryfiles" => XVal::Array(g.ext.new_array(fnv_script::vm::ArrayKind::List, Vec::new())),
        "readstringfromfile" => XVal::Str(g.mod_files.read_root_text(&text(args, 0)).unwrap_or_default()),
        "md5file" => XVal::Str(String::new()),
        "setprojectilerefweapon" => args.first().cloned().unwrap_or(zero),
        "setactorvelocity" | "setvelex" | "baseadditemhealth" | "addneweffect" | "removenththeffect" | "removentheffect" | "getequippedex" => zero,
        // 1 when the last hit on the actor got through its Damage Threshold: Simple Attack
        // Reactions plays its hit reactions in combat only then (218 dialogue result
        // scripts, `if GetArmourPenetrated == 1 || IsInCombat == 0`). A hit with no armour
        // step (a script's damage) counts as through [hypothesis]; no hit yet is 0.
        "getarmourpenetrated" => XVal::Num(match subject.and_then(|r| g.extra.get(&r)).and_then(|e| e.last_hit) {
            Some(h) => f64::from(u8::from(h.penetrated.unwrap_or(true))),
            None => 0.0,
        }),
        _ => return None,
    };
    Some(Ok(v))
}
