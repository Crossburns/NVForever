//! Extender functions High and Dry calls that were not built (batch hnd2, 2026-11-09;
//! docs/spec-notes/ext-hnd1.md §hnd2). What they keep is NVForever's own state; where the
//! original's answer is not known the reading is marked [hypothesis].

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

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

/// What these functions keep for the session.
#[derive(Debug, Default)]
pub struct Hnd2State {
    /// Stewie's Tweaks' smooth iron sights values set by SetSmoothIronsightsValue, by mode.
    pub ironsights: std::collections::HashMap<i64, f64>,
    /// JohnnyGuitar's SetViewmodelClipDistance (the engine may read it).
    pub viewmodel_clip: Option<f64>,
    /// JohnnyGuitar's ToggleNthPipboyLight: button -> on.
    pub pipboy_lights: std::collections::HashMap<i64, bool>,
}

pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // `type rOut gOut bOut`: GetSystemColor's colour in parts [hypothesis: the parts of
        // the 0xRRGGBB GetSystemColor reads, ext-hnd1.md hnd4].
        "getsystemcoloralt" => {
            let c = match crate::nvse_hnd1::call(g, _ctx, "getsystemcolor", subject, &args[..args.len().min(1)]) {
                Some(Ok(XVal::Num(c))) => c as u32,
                _ => 0,
            };
            crate::nvse_jip3::outputs(g, zero, &[f64::from(c >> 16 & 0xFF), f64::from(c >> 8 & 0xFF), f64::from(c & 0xFF)])
        }
        // `mode [tolerp]` / `mode value`: the value set, else 0 [hypothesis: Stewie's
        // tweak's settings start from its INI, which NVForever does not apply].
        "getsmoothironsightsvalue" => XVal::Num(g.gaps.hnd2.ironsights.get(&(num(args, 0, 0.0) as i64)).copied().unwrap_or(0.0)),
        "setsmoothironsightsvalue" => {
            g.gaps.hnd2.ironsights.insert(num(args, 0, 0.0) as i64, num(args, 1, 0.0));
            zero
        }
        "setviewmodelclipdistance" => {
            g.gaps.hnd2.viewmodel_clip = Some(num(args, 0, 0.0));
            zero
        }
        "togglenthpipboylight" => {
            g.gaps.hnd2.pipboy_lights.insert(num(args, 0, 0.0) as i64, num(args, 1, 0.0) != 0.0);
            zero
        }
        // Re-reading a reference's animation data: NVForever reads it when it plays
        // [hypothesis: nothing is cached here to refresh].
        "refreshanimdata" => zero,
        // `xOut yOut`: the camera's movement this frame; none is told to scripts here
        // [hypothesis: 0 and 0 with the camera still].
        "getcameramovement" => crate::nvse_jip3::outputs(g, zero, &[0.0, 0.0]),
        // `child min max "setting" value`: SetMCMFloat on each of the children min..max
        // [hypothesis: by the parameter names].
        "setmcmfloatmass" => {
            let (lo, hi) = (num(args, 1, 0.0) as i64, num(args, 2, 0.0) as i64);
            for k in lo..=hi.min(lo + 256) {
                let a = [args.first().cloned().unwrap_or(XVal::Num(0.0)), XVal::Num(k as f64), XVal::Str(text(args, 3)), args.get(4).cloned().unwrap_or(XVal::Num(0.0))];
                let _ = crate::ui_functions::call(g, "setmcmfloat", &a);
            }
            zero
        }
        // `"path" append "text"`: the text written to the file under the game's folder
        // (NVForever's layer, never the game's own folder), appended or in place; 1 when
        // written [hypothesis: the path is the game folder's, as JIP's other file
        // functions].
        "writestringtofile" => {
            let path = text(args, 0);
            let append = num(args, 1, 0.0) != 0.0;
            let mut body = if append { g.mod_files.read_root_text(&path).unwrap_or_default() } else { String::new() };
            body.push_str(&text(args, 2));
            XVal::Num(f64::from(u8::from(!path.is_empty() && g.mod_files.write_root_text(&path, body))))
        }
        _ => return None,
    };
    Some(Ok(v))
}
