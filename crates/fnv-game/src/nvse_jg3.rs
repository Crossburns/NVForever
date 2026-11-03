//! JohnnyGuitar NVSE, batch 3 (#1007): effect shaders' traits and textures, media sets'
//! traits, casinos' chips and winnings, terminals' menu items and quests' script delay
//! [documented: each page in the wiki export; docs/spec-notes/ext-jg3.md]. Setters
//! change the record for the session, read by the getters.

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
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

/// A whole number at `k`, else none.
fn index(args: &[XVal], k: usize) -> Option<i64> {
    match args.get(k) {
        Some(XVal::Num(x)) if x.fract() == 0.0 => Some(*x as i64),
        _ => None,
    }
}

/// An effect shader trait 0-46 [documented: GetEffectShaderTraitNumeric's table]: the
/// EFSH DATA field, and whether it is one of the blend or Z test enums. The colours (4 fill, 14 edge) are left out: how the page turns three
/// channels into one number is not documented [decision: 0, ext-jg3.md].
fn efsh_trait(id: i64) -> Option<(&'static str, bool)> {
    const NAMES: [&str; 47] = [
        "flags",
        "membrane_source_blend",
        "membrane_blend_op",
        "membrane_z_test",
        "",
        "fill_alpha_fade_in_time",
        "fill_full_alpha_time",
        "fill_alpha_fade_out_time",
        "fill_persistent_alpha_ratio",
        "fill_alpha_pulse_amplitude",
        "fill_alpha_pulse_frequency",
        "fill_texture_speed_u",
        "fill_texture_speed_v",
        "edge_fall_off",
        "",
        "edge_alpha_fade_in_time",
        "edge_full_alpha_time",
        "edge_alpha_fade_out_time",
        "edge_persistent_alpha_ratio",
        "edge_alpha_pulse_amplitude",
        "edge_alpha_pulse_frequency",
        "fill_full_alpha_ratio",
        "edge_full_alpha_ratio",
        "membrane_dest_blend",
        "particle_source_blend",
        "particle_blend_op",
        "particle_z_test",
        "particle_dest_blend",
        "particle_birth_ramp_up_time",
        "full_particle_birth_time",
        "particle_birth_ramp_down_time",
        "full_particle_birth_ratio",
        "persistent_particle_birth_ratio",
        "particle_lifetime",
        "particle_lifetime_range",
        "initial_speed_along_normal",
        "acceleration_along_normal",
        "initial_velocity_x",
        "initial_velocity_y",
        "initial_velocity_z",
        "acceleration_x",
        "acceleration_y",
        "acceleration_z",
        "scale_key_1",
        "scale_key_2",
        "scale_key_1_time",
        "scale_key_2_time",
    ];
    let name = *NAMES.get(usize::try_from(id).ok()?)?;
    (!name.is_empty()).then_some((name, matches!(id, 1..=3 | 23..=27)))
}

/// An effect shader texture 0-2: fill (ICON), particle shader (ICO2), holes (NAM7)
/// [documented: GetEffectShaderTexturePath's table; the subrecords: xEdit EFSH].
fn efsh_texture(id: i64) -> Option<&'static str> {
    ["ICON", "ICO2", "NAM7"].get(usize::try_from(id).ok()?).copied()
}

/// A media set's numeric trait 0-17 [documented: GetMediaSetTraitNumeric's table, each
/// with its subrecord].
fn mset_numeric(id: i64) -> Option<&'static str> {
    ["NAM1", "NAM8", "NAM9", "NAM0", "ANAM", "BNAM", "CNAM", "JNAM", "KNAM", "LNAM", "MNAM", "NNAM", "ONAM", "PNAM", "DNAM", "ENAM", "FNAM", "GNAM"]
        .get(usize::try_from(id).ok()?)
        .copied()
}

/// A record's number: a number or a flags value (the record's enums decode as their
/// numbers, `GameData::record_fields`).
fn number(v: Option<XVal>) -> f64 {
    match v {
        Some(XVal::Num(x)) => x,
        Some(XVal::Ref(r)) => f64::from(r),
        _ => 0.0,
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let field = |g: &Game, f: FormId, sub: &str, path: &str| crate::nvse::record_value(g, f, sub, path);
    let of_sig = |g: &Game, f: Option<FormId>, sig: &[u8; 4]| f.filter(|&f| g.data.sig(f) == Some(*sig));
    let zero = XVal::Num(0.0);
    let empty = XVal::Str(String::new());
    let v = match name {
        // `shader trait [value]`: the EFSH DATA field [documented: Get/
        // SetEffectShaderTraitNumeric]; the setter takes the traits its page lists (0-42)
        // and returns 1 when done [documented: "returning 1 on success"].
        "geteffectshadertraitnumeric" | "seteffectshadertraitnumeric" => {
            let set = name.starts_with("set");
            let (Some(s), Some((path, by_name))) = (of_sig(g, form(args, 0), b"EFSH"), index(args, 1).and_then(efsh_trait)) else { return Some(Ok(zero)) };
            if set {
                if index(args, 1).is_some_and(|t| t > 42) {
                    return Some(Ok(zero));
                }
                let v = num(args, 2, 0.0);
                // A flag byte and the enums keep whole numbers [hypothesis].
                let v = if path == "flags" || by_name { v.trunc() } else { v };
                g.field_overrides.insert((s, format!("DATA/{path}")), XVal::Num(v));
                XVal::Num(1.0)
            } else {
                XVal::Num(number(field(g, s, "DATA", path)))
            }
        }
        // `shader id [path]`: 0 fill, 1 particle, 2 holes [documented: Get/
        // SetEffectShaderTexturePath].
        "geteffectshadertexturepath" | "seteffectshadertexturepath" => {
            let (Some(s), Some(sub)) = (of_sig(g, form(args, 0), b"EFSH"), index(args, 1).and_then(efsh_texture)) else {
                return Some(Ok(if name.starts_with("get") { empty } else { zero }));
            };
            if name.starts_with("set") {
                g.field_overrides.insert((s, format!("{sub}/")), XVal::Str(text(args, 2)));
                zero
            } else {
                XVal::Str(match field(g, s, sub, "") {
                    Some(XVal::Str(p)) => p,
                    _ => String::new(),
                })
            }
        }
        // `set trait [value]` [documented: Get/SetMediaSetTraitNumeric, traits 0-17].
        "getmediasettraitnumeric" | "setmediasettraitnumeric" => {
            let (Some(m), Some(sub)) = (of_sig(g, form(args, 0), b"MSET"), index(args, 1).and_then(mset_numeric)) else { return Some(Ok(zero)) };
            if name.starts_with("set") {
                let v = num(args, 2, 0.0);
                let v = if matches!(sub, "NAM1" | "PNAM") { v.trunc() } else { v };
                g.field_overrides.insert((m, format!("{sub}/")), XVal::Num(v));
                zero
            } else {
                XVal::Num(number(field(g, m, sub, "")))
            }
        }
        // `set trait [path]`: NAM2-NAM7 [documented: Get/SetMediaSetTraitString, 0-5].
        "getmediasettraitstring" | "setmediasettraitstring" => {
            let sub = index(args, 1).filter(|t| (0..=5).contains(t)).map(|t| format!("NAM{}", t + 2));
            let (Some(m), Some(sub)) = (of_sig(g, form(args, 0), b"MSET"), sub) else {
                return Some(Ok(if name.starts_with("get") { empty } else { zero }));
            };
            if name.starts_with("set") {
                g.field_overrides.insert((m, format!("{sub}/")), XVal::Str(text(args, 2)));
                zero
            } else {
                XVal::Str(match field(g, m, &sub, "") {
                    Some(XVal::Str(p)) => p,
                    _ => String::new(),
                })
            }
        }
        // `set trait [sound]`: 0 HNAM, 1 INAM [documented: Get/SetMediaSetTraitSound].
        "getmediasettraitsound" | "setmediasettraitsound" => {
            let sub = match index(args, 1) {
                Some(0) => "HNAM",
                Some(1) => "INAM",
                _ => return Some(Ok(if name.starts_with("get") { XVal::Ref(0) } else { zero })),
            };
            let Some(m) = of_sig(g, form(args, 0), b"MSET") else {
                return Some(Ok(if name.starts_with("get") { XVal::Ref(0) } else { zero }));
            };
            if name.starts_with("set") {
                // A sound form, or none [hypothesis: anything else is refused].
                let s = form(args, 2);
                if s.is_none() || of_sig(g, s, b"SOUN").is_some() {
                    g.field_overrides.insert((m, format!("{sub}/")), XVal::Ref(s.map_or(0, |s| s.0)));
                }
                zero
            } else {
                XVal::Ref(match field(g, m, sub, "") {
                    Some(XVal::Ref(r)) => r,
                    _ => 0,
                })
            }
        }
        // `casino [chip]`: the casino's chip, DATA's currency [documented: Get/
        // SetCasinoChip; the field: xEdit CSNO]; the setter takes a casino chip form.
        "getcasinochip" => XVal::Ref(form(args, 0).and_then(|c| crate::casino::chip(g, c)).map_or(0, |c| c.0)),
        "setcasinochip" => {
            if let (Some(c), Some(chip)) = (form(args, 0).filter(|c| g.data.casinos.contains_key(c)), of_sig(g, form(args, 1), b"CHIP")) {
                g.other.casino_chips.insert(c, chip);
            }
            zero
        }
        // `casino [chips]`: the chips the player has won at the casino, the net of its
        // games (`crate::casino::add_earnings`) [documented: Get/SetCasinoWinnings]; the
        // setter also sets the winnings level the casino's scripts read.
        "getcasinowinnings" => XVal::Num(form(args, 0).and_then(|c| g.player.casino_earnings.get(&c)).map_or(0.0, |&e| e as f64)),
        "setcasinowinnings" => {
            if let Some(c) = form(args, 0).filter(|c| g.data.casinos.contains_key(c)) {
                let max = g.data.casinos[&c].max_winnings;
                // The earnings set outright (`as` saturates a huge number), then the level
                // worked out as after a round, so no difference can overflow (#1328 review).
                g.player.casino_earnings.insert(c, num(args, 1, 0.0).trunc() as i64);
                crate::casino::add_earnings(g, c, 0, max);
            }
            zero
        }
        // `terminal`: the number of its menu items [documented: GetTerminalMenuItemCount].
        "getterminalmenuitemcount" => XVal::Num(form(args, 0).and_then(|t| g.data.terminals.get(&t)).map_or(0.0, |t| t.items.len() as f64)),
        // `terminal textID item [text]`: 1 the item's line (ITXT), 2 its result (RNAM)
        // [documented: Get/SetTerminalMenuItemText].
        "getterminalmenuitemtext" | "setterminalmenuitemtext" => {
            let set = name.starts_with("set");
            let (Some(t), Some(which), Some(i)) = (form(args, 0), index(args, 1).filter(|w| matches!(w, 1 | 2)), index(args, 2)) else {
                return Some(Ok(if set { zero } else { empty }));
            };
            let Some(item) = usize::try_from(i).ok().and_then(|i| crate::terminal::item(g, t, i).map(|it| (i, it))) else {
                return Some(Ok(if set { zero } else { empty }));
            };
            if set {
                let edit = g.other.terminal_items.entry((t, item.0)).or_default();
                if which == 1 {
                    edit.text = Some(text(args, 3));
                } else {
                    edit.result = Some(text(args, 3));
                }
                zero
            } else {
                XVal::Str(if which == 1 { item.1.text } else { item.1.result })
            }
        }
        // `terminal item [form]`: the note it shows (INAM), the terminal it opens (TNAM),
        // its flags (ANAM: 1 Add Note, 2 Force Redraw) [documented: Get/
        // SetTerminalMenuItemNote, …Submenu, …Flags].
        "getterminalmenuitemnote" | "getterminalmenuitemsubmenu" | "getterminalmenuitemflags" => {
            let it = form(args, 0).zip(index(args, 1).and_then(|i| usize::try_from(i).ok())).and_then(|(t, i)| crate::terminal::item(g, t, i));
            match name {
                "getterminalmenuitemnote" => XVal::Ref(it.and_then(|it| it.note).map_or(0, |n| n.0)),
                "getterminalmenuitemsubmenu" => XVal::Ref(it.and_then(|it| it.submenu).map_or(0, |n| n.0)),
                _ => XVal::Num(it.map_or(0.0, |it| f64::from(it.flags))),
            }
        }
        "setterminalmenuitemnote" | "setterminalmenuitemsubmenu" | "setterminalmenuitemflags" => {
            let Some((t, i)) = form(args, 0).zip(index(args, 1).and_then(|i| usize::try_from(i).ok())).filter(|&(t, i)| crate::terminal::item(g, t, i).is_some()) else {
                return Some(Ok(zero));
            };
            match name {
                "setterminalmenuitemnote" => {
                    // A note, or none [hypothesis: anything else is refused].
                    let n = form(args, 2);
                    if n.is_none() || of_sig(g, n, b"NOTE").is_some() {
                        g.other.terminal_items.entry((t, i)).or_default().note = Some(n);
                    }
                }
                "setterminalmenuitemsubmenu" => {
                    let s = form(args, 2);
                    if s.is_none() || of_sig(g, s, b"TERM").is_some() {
                        g.other.terminal_items.entry((t, i)).or_default().submenu = Some(s);
                    }
                }
                _ => {
                    // The flag byte keeps its two bits [documented: the page's bits].
                    g.other.terminal_items.entry((t, i)).or_default().flags = Some((num(args, 2, 0.0) as i64 & 3) as u8);
                }
            }
            zero
        }
        // `quest`: the quest script's delay, SetQuestDelay's first, else the record's,
        // 5 s when that is 0 or missing [documented: GetQuestDelay; the default:
        // `crate::quest::QuestDef::script_delay`].
        "getquestdelay" => {
            let Some(q) = form(args, 0).filter(|q| g.data.quests.contains_key(q)) else { return Some(Ok(zero)) };
            XVal::Num(g.quests.get(&q).and_then(|s| s.delay).unwrap_or_else(|| g.data.quests[&q].script_delay()))
        }
        _ => return None,
    };
    let _ = (ctx, subject);
    Some(Ok(v))
}
