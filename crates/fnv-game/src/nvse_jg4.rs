//! JohnnyGuitar NVSE, batch 4 (#1007): idle markers' animations and traits, regions'
//! weather lists, race flags and faction members [documented: each page in the wiki
//! export; docs/spec-notes/ext-jg4.md]. Setters change the record for the session, read
//! by the getters (and, for idle markers, by the engine's idles).

use crate::data::{RegionWeather, RegionWeathers};
use crate::game::{Game, RunCtx};
use crate::package::IdleCollection;
use fnv_script::vm::{ArrayKind, XKey, XVal};
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

/// A whole number at `k`, else none.
fn index(args: &[XVal], k: usize) -> Option<i64> {
    match args.get(k) {
        Some(XVal::Num(x)) if x.fract() == 0.0 => Some(*x as i64),
        _ => None,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

/// An idle marker's collection as it stands now: a script's changes for the session
/// (`nvse_jg4`), else the record's (IDLM). The engine's idles read this.
pub fn idle_marker<'a>(g: &'a Game, marker: FormId) -> Option<&'a IdleCollection> {
    g.other.idle_markers.get(&marker).or_else(|| g.data.idle_markers.get(&marker))
}

/// A region's weather block as it stands now: a script's changes for the session, else
/// the record's (REGN RDAT type 3 + RDWT).
pub fn region_weathers<'a>(g: &'a Game, region: FormId) -> Option<&'a RegionWeathers> {
    g.other.region_weathers.get(&region).or_else(|| g.data.regions.get(&region)?.weathers.as_ref())
}

/// The idle marker's collection to change, copied from the record on the first change.
fn marker_mut<'a>(g: &'a mut Game, marker: FormId) -> Option<&'a mut IdleCollection> {
    if !g.other.idle_markers.contains_key(&marker) {
        let c = g.data.idle_markers.get(&marker)?.clone();
        g.other.idle_markers.insert(marker, c);
    }
    g.other.idle_markers.get_mut(&marker)
}

/// The region's weather block to change, copied from the record on the first change; a
/// region of the record's without one gets an empty block [hypothesis: AddRegionWeather
/// on such a region adds the weather].
fn weathers_mut<'a>(g: &'a mut Game, region: FormId) -> Option<&'a mut RegionWeathers> {
    if !g.other.region_weathers.contains_key(&region) {
        let w = g.data.regions.get(&region)?.weathers.clone().unwrap_or_default();
        g.other.region_weathers.insert(region, w);
    }
    g.other.region_weathers.get_mut(&region)
}

fn list(g: &mut Game, items: impl Iterator<Item = XVal>) -> XVal {
    let entries = items.enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let of_sig = |g: &Game, f: Option<FormId>, sig: &[u8; 4]| f.filter(|&f| g.data.sig(f) == Some(*sig));
    let v = match name {
        // `marker`: its idles (IDLA) as an array [documented: GetIdleMarkerAnimations].
        "getidlemarkeranimations" => {
            let idles: Vec<FormId> = form(args, 0).and_then(|m| idle_marker(g, m)).map(|c| c.idles.clone()).unwrap_or_default();
            list(g, idles.into_iter().map(|i| XVal::Ref(i.0)))
        }
        // `marker trait`: 1 the flags (1 run in sequence, 4 do once), 2 the idle timer,
        // 3 the number of animations [documented: GetIdleMarkerTraitNumeric].
        "getidlemarkertraitnumeric" => {
            let Some(c) = form(args, 0).and_then(|m| idle_marker(g, m)) else { return Some(Ok(zero)) };
            XVal::Num(match index(args, 1) {
                Some(1) => f64::from(u8::from(c.run_in_sequence) | u8::from(c.do_once) << 2),
                Some(2) => f64::from(c.timer),
                Some(3) => c.idles.len() as f64,
                _ => 0.0,
            })
        }
        // `marker trait value`: 1 the flags, 2 the timer; the count is not set here
        // [documented: SetIdleMarkerTraitNumeric and its note].
        "setidlemarkertraitnumeric" => {
            let (t, v) = (index(args, 1), num(args, 2, 0.0));
            if let Some(c) = form(args, 0).and_then(|m| marker_mut(g, m)) {
                match t {
                    Some(1) => {
                        let bits = v as i64;
                        (c.run_in_sequence, c.do_once) = (bits & 1 != 0, bits & 4 != 0);
                    }
                    // A timer that is not a finite, non-negative number after the
                    // cast is refused: an endless one would stop the marker's idles for
                    // the session (#1333 review) [hypothesis: the game keeps a float].
                    Some(2) if (v as f32).is_finite() && v >= 0.0 => c.timer = v as f32,
                    _ => {}
                }
            }
            zero
        }
        // `marker index idle`: the idle at the index replaced [documented:
        // SetIdleMarkerAnimation]; an index past the list or a form not an idle changes
        // nothing [hypothesis].
        "setidlemarkeranimation" => {
            let idle = of_sig(g, form(args, 2), b"IDLE");
            let at = index(args, 1).and_then(|i| usize::try_from(i).ok());
            if let (Some(m), Some(idle), Some(at)) = (form(args, 0), idle, at) {
                if idle_marker(g, m).is_some_and(|c| at < c.idles.len()) {
                    if let Some(c) = marker_mut(g, m) {
                        c.idles[at] = idle;
                    }
                }
            }
            zero
        }
        // `marker array`: the idles become the array's, in its order; the count follows
        // [documented: SetIdleMarkerAnimations and the note on the count]. Entries that
        // are not idles are left out [hypothesis].
        "setidlemarkeranimations" => {
            let id = match args.get(1) {
                Some(XVal::Array(id)) => Some(*id),
                Some(XVal::Num(x)) if *x >= 1.0 => Some(*x as u32),
                _ => None,
            };
            let rows: Option<Vec<XVal>> = id.and_then(|id| g.ext.array(id)).map(|a| a.entries.iter().map(|(_, v)| v.clone()).collect());
            if let (Some(m), Some(rows)) = (form(args, 0), rows) {
                let idles: Vec<FormId> = rows.iter().filter_map(|v| of_sig(g, form(std::slice::from_ref(v), 0), b"IDLE")).collect();
                if let Some(c) = marker_mut(g, m) {
                    c.idles = idles;
                }
            }
            zero
        }
        // `region`: the weathers of its weather block, in record order [documented:
        // GetRegionWeathers].
        "getregionweathers" => {
            let ws: Vec<FormId> = form(args, 0).and_then(|r| region_weathers(g, r)).map(|w| w.list.iter().map(|e| e.weather).collect()).unwrap_or_default();
            list(g, ws.into_iter().map(|w| XVal::Ref(w.0)))
        }
        // `region index`: the entry's weather, chance or global [documented:
        // GetNthRegionWeatherType, …Chance, …Global].
        "getnthregionweathertype" | "getnthregionweatherchance" | "getnthregionweatherglobal" => {
            let e = form(args, 0).and_then(|r| region_weathers(g, r)).and_then(|w| w.list.get(usize::try_from(index(args, 1)?).ok()?)).copied();
            match name {
                "getnthregionweatherchance" => XVal::Num(e.map_or(0.0, |e| f64::from(e.chance))),
                "getnthregionweathertype" => XVal::Ref(e.map_or(0, |e| e.weather.0)),
                _ => XVal::Ref(e.and_then(|e| e.global).map_or(0, |g| g.0)),
            }
        }
        // `region weather` [documented: IsWeatherInRegion].
        "isweatherinregion" => {
            let w = form(args, 1);
            yes(w.is_some() && form(args, 0).and_then(|r| region_weathers(g, r)).is_some_and(|ws| ws.list.iter().any(|e| Some(e.weather) == w)))
        }
        // `region weather chance [global]` [documented: AddRegionWeather]: a weather
        // already listed is not listed twice [hypothesis].
        "addregionweather" => {
            let weather = of_sig(g, form(args, 1), b"WTHR");
            let global = of_sig(g, form(args, 3), b"GLOB");
            let chance = num(args, 2, 0.0).clamp(0.0, 100.0) as i32;
            if let (Some(r), Some(weather)) = (of_sig(g, form(args, 0), b"REGN"), weather) {
                if let Some(ws) = weathers_mut(g, r) {
                    if !ws.list.iter().any(|e| e.weather == weather) {
                        ws.list.push(RegionWeather { weather, chance, global });
                    }
                }
            }
            zero
        }
        // `region weather` [documented: RemoveRegionWeather]; `region` [documented:
        // ClearRegionWeathers].
        "removeregionweather" | "clearregionweathers" => {
            let w = form(args, 1);
            if let Some(r) = form(args, 0).filter(|&r| region_weathers(g, r).is_some()) {
                if let Some(ws) = weathers_mut(g, r) {
                    ws.list.retain(|e| name == "removeregionweather" && Some(e.weather) != w);
                }
            }
            zero
        }
        // `region [flag]`: the weather block's override flag (RDAT flags bit 0)
        // [documented: Get/SetRegionWeatherOverride; the bit:
        // `fnv_world::sky::REGION_WEATHER_OVERRIDE`].
        "getregionweatheroverride" => yes(form(args, 0).and_then(|r| region_weathers(g, r)).is_some_and(|w| w.flags & fnv_world::sky::REGION_WEATHER_OVERRIDE != 0)),
        "setregionweatheroverride" => {
            let on = num(args, 1, 0.0) != 0.0;
            if let Some(ws) = form(args, 0).filter(|&r| region_weathers(g, r).is_some()).and_then(|r| weathers_mut(g, r)) {
                ws.flags = if on { ws.flags | fnv_world::sky::REGION_WEATHER_OVERRIDE } else { ws.flags & !fnv_world::sky::REGION_WEATHER_OVERRIDE };
            }
            zero
        }
        // `region [priority]`: RDAT's priority byte [documented: Get/
        // SetRegionWeatherPriority]; the setter keeps 0-255 [hypothesis: a byte].
        "getregionweatherpriority" => XVal::Num(form(args, 0).and_then(|r| region_weathers(g, r)).map_or(0.0, |w| f64::from(w.priority))),
        "setregionweatherpriority" => {
            let p = num(args, 1, 0.0).clamp(0.0, 255.0) as u8;
            if let Some(ws) = form(args, 0).filter(|&r| region_weathers(g, r).is_some()).and_then(|r| weathers_mut(g, r)) {
                ws.priority = p;
            }
            zero
        }
        // `race flag on`: 0 playable, 2 child (RACE DATA flags), read by GetRaceFlag
        // [documented: SetRaceFlag's table]; another flag changes nothing [hypothesis].
        "setraceflag" => {
            let (Some(race), Some(bit @ (0 | 2))) = (of_sig(g, form(args, 0), b"RACE"), index(args, 1)) else { return Some(Ok(zero)) };
            let flags = match crate::nvse::record_value(g, race, "DATA", "flags") {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            let flags = if num(args, 2, 0.0) != 0.0 { flags | 1 << bit } else { flags & !(1 << bit) };
            g.field_overrides.insert((race, "DATA/flags".into()), XVal::Num(flags as f64));
            zero
        }
        // `faction [rank]`: the base actors whose own faction list (SNAM) holds the
        // faction, at that rank when one is given, in FormID order [documented:
        // GetFactionMembers; hypothesis: the record's list, not its template's, and the
        // order].
        "getfactionmembers" => {
            let rank = index(args, 1);
            let mut members: Vec<FormId> = match form(args, 0) {
                Some(f) => g.data.actors.iter().filter(|(_, a)| a.factions.iter().any(|&(af, r)| af == f && rank.is_none_or(|k| i64::from(r) == k))).map(|(&id, _)| id).collect(),
                None => Vec::new(),
            };
            members.sort();
            list(g, members.into_iter().map(|m| XVal::Ref(m.0)))
        }
        _ => return None,
    };
    let _ = (ctx, subject);
    Some(Ok(v))
}
