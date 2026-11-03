//! xNVSE's animation lists and generic form lists [documented: AddAnimation;
//! DelAnimation; DelAnimations; GetNthAnimation; GenericAddForm; GenericDeleteForm;
//! GenericGetForm; GenericReplaceForm]. Changes last for the session.

use crate::game::Game;
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

fn ids(v: Option<&serde_json::Value>) -> Vec<FormId> {
    let one = |x: &serde_json::Value| x.as_str().and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()).map(FormId);
    match v {
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(one).collect(),
        Some(x) => one(x).into_iter().collect(),
        None => Vec::new(),
    }
}

impl Game<'_> {
    /// An actor base's animations (KFFZ), AddAnimation's and DelAnimation's changes first.
    pub fn base_animations(&self, b: FormId) -> Vec<String> {
        if let Some(v) = self.other.animations.get(&b) {
            return v.clone();
        }
        match self.data.record_field(b, "KFFZ", "") {
            Some(serde_json::Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).filter(|s| !s.is_empty()).map(str::to_string).collect(),
            Some(serde_json::Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        }
    }

    /// A generic form list: 0 race hairs (HNAM), 1 race eyes (ENAM), 5 actor base
    /// packages, 6 actor base spells, 7 leveled spells (none read), 10 NPC head parts,
    /// 13 a form list [documented: GenericGetForm's list codes]; a script's change first.
    pub fn generic_list(&self, code: i64, target: FormId) -> Option<Vec<FormId>> {
        if let Some(v) = self.other.generic.get(&(code, target)) {
            return Some(v.clone());
        }
        Some(match code {
            0 => ids(self.data.record_field(target, "HNAM", "")),
            1 => ids(self.data.record_field(target, "ENAM", "")),
            // From the template's list where GetBasePackages and GetBaseSpellListSpells
            // read it (actors.md §2.2).
            5 => match self.other.package_lists.get(&target) {
                Some(l) => l.clone(),
                None => self.data.actors.get(&self.data.base_template_source(target, |d| d.template_packages)).map(|d| d.packages.clone()).unwrap_or_default(),
            },
            6 => self.data.actors.get(&self.data.base_template_source(target, |d| d.template_spells)).map(|d| d.spells.clone()).unwrap_or_default(),
            7 => Vec::new(),
            // In GetHeadParts' order [observed: nvse-fourth, the record's order reversed].
            10 => {
                let mut v: Vec<FormId> = self
                    .data
                    .record_fields
                    .get(&target)
                    .into_iter()
                    .flatten()
                    .filter(|(s, _)| s == b"PNAM")
                    .filter_map(|(_, v)| v.as_str().and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()).map(FormId))
                    .collect();
                v.reverse();
                v
            }
            13 => crate::functions::list_members(self, target),
            _ => return None,
        })
    }

    /// Store a changed generic list where its readers look (packages: the package list;
    /// a form list: the list's edits; the rest: `OtherState::generic`).
    fn set_generic_list(&mut self, code: i64, target: FormId, list: Vec<FormId>) {
        match code {
            5 => {
                self.other.package_lists.insert(target, list);
            }
            13 => {
                let _ = crate::nvse::call(self, &crate::game::RunCtx::default(), "listclear", None, &[XVal::Ref(target.0)]);
                for f in list {
                    let _ = crate::nvse::call(self, &crate::game::RunCtx::default(), "listaddform", None, &[XVal::Ref(target.0), XVal::Ref(f.0), XVal::Num(-1.0)]);
                }
            }
            _ => {
                self.other.generic.insert((code, target), list);
            }
        }
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let zero = XVal::Num(0.0);
    let v = match name {
        // `animName [baseActor]` (DelAnimations: `[baseActor]`); the calling reference's
        // base when none is given [documented].
        "addanimation" | "delanimation" | "delanimations" | "getnthanimation" => {
            let at = usize::from(name != "delanimations");
            let Some(b) = form(args, at).or(subject).map(|f| base(g, f)) else { return Some(Ok(zero)) };
            let mut list = g.base_animations(b);
            let out = match name {
                "getnthanimation" => {
                    let i = num(args, 0, -1.0) as i64;
                    return Some(Ok(XVal::Str(usize::try_from(i).ok().and_then(|i| list.get(i).cloned()).unwrap_or_default())));
                }
                "addanimation" => {
                    if let Some(XVal::Str(a)) = args.first() {
                        if !list.iter().any(|x| x.eq_ignore_ascii_case(a)) {
                            list.push(a.clone());
                        }
                    }
                    zero
                }
                "delanimation" => {
                    if let Some(XVal::Str(a)) = args.first() {
                        list.retain(|x| !x.eq_ignore_ascii_case(a));
                    }
                    zero
                }
                _ => {
                    list.clear();
                    zero
                }
            };
            g.other.animations.insert(b, list);
            out
        }
        // `code target [new] index`: -2 adds at the end (GenericAddForm); the delete and
        // replace give the form taken out [documented].
        "genericgetform" | "genericaddform" | "genericdeleteform" | "genericreplaceform" => {
            let code = num(args, 0, -1.0) as i64;
            let Some(t) = form(args, 1) else { return Some(Ok(XVal::Ref(0))) };
            let t = if matches!(code, 0 | 1 | 13) { t } else { base(g, t) };
            let Some(mut list) = g.generic_list(code, t) else { return Some(Ok(XVal::Ref(0))) };
            let idx_at = if matches!(name, "genericaddform" | "genericreplaceform") { 3 } else { 2 };
            let i = num(args, idx_at, -1.0) as i64;
            let at = usize::try_from(i).ok().filter(|&i| i < list.len());
            let out = match name {
                "genericgetform" => return Some(Ok(XVal::Ref(at.map_or(0, |i| list[i].0)))),
                "genericaddform" => {
                    let Some(f) = form(args, 2) else { return Some(Ok(zero)) };
                    // The wiki gives no result, but the game gives the index it went in at,
                    // and for -2 the index of the last entry before it, as AddPackageAt -1
                    // does [observed: nvse-eighth, Doc's 9 packages: index 0 gave 0, -2
                    // gave 8 and the count became 10].
                    let n = list.len();
                    let (at, out) = match usize::try_from(i).ok().filter(|&i| i <= n) {
                        Some(at) => (at, at as f64),
                        _ => (n, n as f64 - 1.0),
                    };
                    list.insert(at, f);
                    XVal::Num(out)
                }
                "genericdeleteform" => match at {
                    Some(i) => XVal::Ref(list.remove(i).0),
                    None => return Some(Ok(XVal::Ref(0))),
                },
                _ => match (at, form(args, 2)) {
                    (Some(i), Some(f)) => XVal::Ref(std::mem::replace(&mut list[i], f).0),
                    _ => return Some(Ok(XVal::Ref(0))),
                },
            };
            g.set_generic_list(code, t, list);
            out
        }
        _ => return None,
    };
    Some(Ok(v))
}
