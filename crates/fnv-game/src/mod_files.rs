//! Mods' own files: the INI files under `Data\Config` that LU, ShowOff and MCM read and
//! write, and the file queries of JIP and LU (FileExists, GetFilesInFolder)
//! [documented: their pages; observed: the peer's ini-files and plugin-odds sets in the
//! original game].
//!
//! NVForever never writes into the game folder (install-layout.md): a file a mod writes
//! goes under NVForever's own layer over Data (`fnv_install::EngineHome::mods`), at the
//! same path, and readers look there first, then in the game's Data (owner-review.md,
//! J9: "writers write only under NVForever's own engine-home folder, mirroring the
//! relative path; readers prefer that copy"). Without a write root (a headless run, a test
//! run in the player's home) the writes stay in memory for the session.

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The host's answer for a path in the archives too: 0 none, 1 a loose file, 2 inside an
/// archive.
pub type Lookup = Arc<dyn Fn(&str) -> u8 + Send + Sync>;

/// The host's reader of a Data path, archives included.
pub type Reader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/// Where the files are: `read` in order (NVForever's layer first, then the game's Data),
/// `write` the layer, and `lookup` the host's answer for a path in the archives too
/// (0 none, 1 a loose file, 2 inside an archive).
#[derive(Clone, Default)]
pub struct Roots {
    pub read: Vec<PathBuf>,
    /// A folder standing for `Data\Config` before the read roots (`fnvtool --config`).
    pub config: Option<PathBuf>,
    pub write: Option<PathBuf>,
    pub lookup: Option<Lookup>,
    pub reader: Option<Reader>,
}

static ROOTS: Mutex<Option<Roots>> = Mutex::new(None);

/// Set the roots for the games made after this (the engine, `fnvtool --config`).
pub fn set_roots(r: Roots) {
    if let Ok(mut g) = ROOTS.lock() {
        *g = Some(r);
    }
}

/// Change the roots in place (the engine sets the write root, then the read roots once
/// the game's Data is open).
pub fn update_roots(f: impl FnOnce(&mut Roots)) {
    if let Ok(mut g) = ROOTS.lock() {
        f(g.get_or_insert_with(Roots::default));
    }
}

/// A Data file's bytes through the host (archives included).
pub fn read_data(path: &str) -> Option<Vec<u8>> {
    roots().reader.as_ref().and_then(|f| f(path))
}

/// Whether a NIF names a block exactly so: its header's string table holds every name
/// as a length and the characters [documented: nif.xml, the header's Strings list from
/// version 20.1].
pub fn nif_has_name(bytes: &[u8], name: &str) -> bool {
    let n = name.as_bytes();
    if n.is_empty() {
        return false;
    }
    let mut pat = (n.len() as u32).to_le_bytes().to_vec();
    pat.extend_from_slice(n);
    bytes.windows(pat.len()).any(|w| w == pat.as_slice())
}

/// The game's Data folder, when the host gave one (the last read root).
pub fn data_dir() -> Option<PathBuf> {
    roots().read.last().cloned()
}

/// Whether a Data path is a file of the game: in a read root or in an archive.
pub fn exists(path: &str) -> bool {
    let r = roots();
    r.read.iter().any(|root| on_disk(root, path).is_some_and(|p| p.is_file())) || r.lookup.as_ref().is_some_and(|f| f(path) > 0)
}

fn roots() -> Roots {
    ROOTS.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// The session's view of mods' files.
#[derive(Default)]
pub struct ModFiles {
    /// Files written this session, by lower-case Data path (the disk copy, if any, is
    /// written too).
    written: HashMap<String, String>,
    /// ShowOff's cached INI files, by lower-case Data path: key -> value, read once.
    cache: HashMap<String, HashMap<String, String>>,
}

/// The folder of NVForever's layer standing for the game's own folder.
const GAME_ROOT: &str = "GameRoot";

/// A value as an array file writes it: numbers as `%g` does, forms as their hex ID.
fn cell_text(v: &XVal) -> String {
    match v {
        XVal::Num(x) if x.fract() == 0.0 && x.abs() < 1e15 => format!("{}", *x as i64),
        XVal::Num(x) => format!("{x}"),
        XVal::Ref(r) => format!("{r:08X}"),
        XVal::Str(s) => s.clone(),
        XVal::Array(_) => String::new(),
    }
}

/// `base` joined with a Data-relative path whose parts are split at `\` or `/`: the
/// same path on Windows, and a real subfolder (not a name with a backslash in it) on
/// other systems. `None` for a path that could leave `base`: one that starts at a root
/// (`\x`), names a drive or stream (`C:\x`, any `:`), or has a `.` or `..` part. Every
/// part must be a plain name (`Component::Normal`), so a script's path stays under the
/// folder it is meant for (CLAUDE.md charter; PR #1291 review).
fn on_disk(base: &Path, rel: &str) -> Option<PathBuf> {
    if rel.starts_with(['\\', '/']) || rel.contains(':') {
        return None;
    }
    let mut p = base.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|c| !c.is_empty()) {
        let mut c = Path::new(part).components();
        match (c.next(), c.next()) {
            (Some(Component::Normal(_)), None) => p.push(part),
            _ => return None,
        }
    }
    Some(p)
}

/// Whether NVForever may write `path`: never inside the game's install or the original
/// game's user folder (`fnv_install::guard_write`), nor under the game folder the Data
/// read root stands in (a headless run that did not register the install).
fn may_write(path: &Path) -> bool {
    if fnv_install::guard_write(path, None).is_err() {
        return false;
    }
    let game = roots().read.last().and_then(|d| d.parent().map(Path::to_path_buf));
    !game.is_some_and(|g| path.starts_with(g))
}

/// Write `text` at `path` (its folders made), when [`may_write`] allows; whether it did.
fn write_disk(path: &Path, text: &str) -> bool {
    if !may_write(path) {
        return false;
    }
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(path, text.as_bytes()).is_ok()
}

/// A Data-relative path with `/` turned to `\` and lower case, for the maps.
fn norm(p: &str) -> String {
    p.replace('/', "\\").trim_start_matches('\\').to_ascii_lowercase()
}

impl ModFiles {
    /// [`ModFiles::read`] for the other files of the crate: a Data file's text, this
    /// session's write first.
    pub(crate) fn read_text(&self, rel: &str) -> Option<String> {
        self.read(rel)
    }

    /// A file's text: this session's write, else NVForever's layer, else the game's Data.
    fn read(&self, rel: &str) -> Option<String> {
        if let Some(t) = self.written.get(&norm(rel)) {
            return Some(t.clone());
        }
        let rel = rel.replace('/', "\\");
        let r = roots();
        let config = r.config.as_ref().and_then(|c| {
            let lower = rel.to_ascii_lowercase();
            lower.strip_prefix("config\\").and_then(|_| on_disk(c, &rel["config\\".len()..]))
        });
        on_disk(Path::new(""), &rel)?;
        config.into_iter().chain(r.read.iter().filter_map(|root| on_disk(root, &rel))).find_map(|p| std::fs::read(p).ok()).map(|b| b.iter().map(|&c| c as char).collect())
    }

    /// A file under the game's own folder (JIP's array files): this session's write,
    /// else NVForever's copy (its layer's `GameRoot` folder), else the game folder's.
    fn read_root(&self, rel: &str) -> Option<String> {
        on_disk(Path::new(""), rel)?;
        let key = format!("<root>\\{}", norm(rel));
        if let Some(t) = self.written.get(&key) {
            return Some(t.clone());
        }
        let rel = rel.replace('/', "\\");
        let r = roots();
        let mine = r.write.and_then(|w| on_disk(&w.join(GAME_ROOT), &rel));
        let game = r.read.last().and_then(|d| d.parent()).and_then(|d| on_disk(d, &rel));
        mine.into_iter().chain(game).find_map(|p| std::fs::read(p).ok()).map(|b| b.iter().map(|&c| c as char).collect())
    }

    /// Write a file of the game's own folder: into NVForever's layer, never the game's
    /// folder (CLAUDE.md charter; docs/design/install-layout.md). False, and nothing
    /// kept, for a path [`on_disk`] refuses or a disk path [`may_write`] refuses.
    fn write_root(&mut self, rel: &str, text: String) -> bool {
        on_disk(Path::new(""), rel).is_some() && self.write_under(format!("<root>\\{}", norm(rel)), rel, Some(GAME_ROOT), text)
    }

    /// Write a Data file into NVForever's layer; false when refused, as `write_root`.
    pub(crate) fn write(&mut self, rel: &str, text: String) -> bool {
        on_disk(Path::new(""), rel).is_some() && self.write_under(norm(rel), rel, None, text)
    }

    fn write_under(&mut self, key: String, rel: &str, sub: Option<&str>, text: String) -> bool {
        if let Some(w) = roots().write {
            let base = sub.map_or_else(|| w.clone(), |s| w.join(s));
            let Some(p) = on_disk(&base, rel) else { return false };
            if !write_disk(&p, &text) {
                return false;
            }
        }
        self.written.insert(key, text);
        true
    }
}

/// The INI file a function names, under `Data\Config`, ".ini" added when it has no
/// extension [observed: "NVFExtProbe\probe" and "…\probe.ini" both read]; none given:
/// the calling mod's own, `<plugin name>.ini` [documented: GetINISection, "the name of
/// the mod the function is called from is used"].
fn ini_path(g: &Game, file: Option<&str>) -> String {
    let f = match file.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => f.replace('/', "\\"),
        None => {
            let p = &g.other.plugin;
            p.rsplit_once('.').map_or(p.as_str(), |(n, _)| n).to_string()
        }
    };
    let f = if f.to_ascii_lowercase().ends_with(".ini") { f } else { format!("{f}.ini") };
    format!("Config\\{f}")
}

/// `Section:Key` with ":", "/" or "\" between [observed: all three work], lower case.
fn section_key(s: &str) -> Option<(String, String)> {
    let i = s.find([':', '/', '\\'])?;
    Some((s[..i].trim().to_ascii_lowercase(), s[i + 1..].trim().to_ascii_lowercase()))
}

/// The value of a key in INI text (section and key without regard to case).
pub(crate) fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut cur = String::new();
    for line in text.lines() {
        let l = line.trim();
        if let Some(s) = l.strip_prefix('[') {
            cur = s.trim_end_matches(']').trim().to_ascii_lowercase();
        } else if cur == section {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    let v = v.split(';').next().unwrap_or("").trim();
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Every key=value of a section, in order.
fn ini_section(text: &str, section: &str) -> Vec<(String, String)> {
    let mut cur = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if let Some(s) = l.strip_prefix('[') {
            cur = s.trim_end_matches(']').trim().to_ascii_lowercase();
        } else if cur == section && !l.starts_with(';') {
            if let Some((k, v)) = l.split_once('=') {
                out.push((k.trim().to_string(), v.split(';').next().unwrap_or("").trim().to_string()));
            }
        }
    }
    out
}

fn ini_sections(text: &str) -> Vec<String> {
    text.lines().filter_map(|l| l.trim().strip_prefix('[').map(|s| s.trim_end_matches(']').trim().to_string())).collect()
}

/// Set a key in INI text: rewritten in place, else added at the end of its section,
/// else a new section at the end; lines end in CRLF [observed: "fSet=2.25" appended at
/// the end of [General], "fNum=6" rewritten in place, a new file "[New]\r\nfVal=1.5\r\n"].
fn ini_set(text: &str, section: &str, key: &str, value: &str, section_name: &str, key_name: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut cur = String::new();
    let mut last_in_section: Option<usize> = None;
    for (i, line) in lines.iter_mut().enumerate() {
        let l = line.trim().to_string();
        if let Some(s) = l.strip_prefix('[') {
            cur = s.trim_end_matches(']').trim().to_ascii_lowercase();
            if cur == section {
                last_in_section = Some(i);
            }
            continue;
        }
        if cur == section {
            if let Some((k, _)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    *line = format!("{}={value}", k.trim());
                    return lines.join("\r\n") + "\r\n";
                }
            }
            if !l.is_empty() {
                last_in_section = Some(i);
            }
        }
    }
    match last_in_section {
        Some(i) => lines.insert(i + 1, format!("{key_name}={value}")),
        None => {
            lines.push(format!("[{section_name}]"));
            lines.push(format!("{key_name}={value}"));
        }
    }
    lines.join("\r\n") + "\r\n"
}

/// A number as LU writes it: no decimals for a whole number [observed: "fNum=6",
/// "fSet=2.25"].
fn lu_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn num_of(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k)? {
        XVal::Str(s) => Some(s.clone()),
        XVal::Num(n) => Some(format!("{n}")),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- JIP's array files, under the game's folder ----
        // `path append array [transpose]`: a list is one line of tab-separated values; a
        // list of lists is its columns side by side (transposed: its rows); appending
        // starts a new line; returns 1 [observed: gaps10, ar_List 4 5 6 writes `4\t5\t6`
        // with no newline after, appending 7 8 9 adds `\n7\t8\t9`].
        "writearraytofile" => {
            let path = text(args, 0).unwrap_or_default();
            let append = num(args, 1, 0.0) != 0.0;
            let transpose = num(args, 3, 0.0) != 0.0;
            let Some(XVal::Array(id)) = args.get(2) else { return Some(Ok(zero)) };
            let vals: Vec<XVal> = g.ext.array(*id).map(|a| a.entries.iter().map(|(_, v)| v.clone()).collect()).unwrap_or_default();
            let cols: Vec<Vec<XVal>> = vals
                .iter()
                .filter_map(|v| match v {
                    XVal::Array(i) => g.ext.array(*i).map(|a| a.entries.iter().map(|(_, v)| v.clone()).collect()),
                    _ => None,
                })
                .collect();
            let rows: Vec<Vec<XVal>> = if cols.is_empty() {
                vec![vals]
            } else if transpose {
                cols
            } else {
                let n = cols.iter().map(Vec::len).max().unwrap_or(0);
                (0..n).map(|i| cols.iter().map(|c| c.get(i).cloned().unwrap_or(XVal::Str(String::new()))).collect()).collect()
            };
            let body = rows.iter().map(|r| r.iter().map(cell_text).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n");
            let text = match g.mod_files.read_root(&path).filter(|_| append) {
                Some(old) if !old.is_empty() => format!("{old}\n{body}"),
                _ => body,
            };
            XVal::Num(if g.mod_files.write_root(&path, text) { 1.0 } else { 0.0 })
        }
        // `path [transpose]`: one line gives a list of its values; more give a list of
        // columns (transposed: of rows); numbers read as numbers; a missing file gives
        // no array [observed: gaps10, 4 5 6 reads back with element 1 = 5; two lines of
        // three give 3; missing -1].
        "readarrayfromfile" => {
            let path = text(args, 0).unwrap_or_default();
            let transpose = num(args, 1, 0.0) != 0.0;
            let Some(t) = g.mod_files.read_root(&path) else { return Some(Ok(zero)) };
            let parse = |c: &str| match c.trim().parse::<f64>() {
                Ok(x) => XVal::Num(x),
                Err(_) => XVal::Str(c.to_string()),
            };
            let rows: Vec<Vec<XVal>> = t.lines().filter(|l| !l.trim().is_empty()).map(|l| l.split('\t').map(parse).collect()).collect();
            let list = |g: &mut Game, items: Vec<XVal>| {
                let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
                XVal::Array(g.ext.new_array(ArrayKind::List, entries))
            };
            if rows.len() == 1 {
                let r = rows.into_iter().next().unwrap_or_default();
                return Some(Ok(list(g, r)));
            }
            let groups: Vec<Vec<XVal>> = if transpose {
                rows
            } else {
                let n = rows.iter().map(Vec::len).max().unwrap_or(0);
                (0..n).map(|i| rows.iter().filter_map(|r| r.get(i).cloned()).collect()).collect()
            };
            let inner: Vec<XVal> = groups.into_iter().map(|c| list(g, c)).collect();
            list(g, inner)
        }
        // ---- ppNVSE's line files ----
        // `path [start] [end]`: the file's lines as strings, from `start` (0 the first) up
        // to before `end` [documented: pReadFile, "0 5" reads the first five lines;
        // hypothesis: the end left out], the path under the game's folder as
        // ReadArrayFromFile's [documented: pWriteFile, "relative to the game's
        // installation path"]. A missing file gives an empty array [hypothesis].
        "preadfile" => {
            let path = text(args, 0).unwrap_or_default();
            let lines: Vec<String> = g.mod_files.read_root(&path).map(|t| t.lines().map(str::to_string).collect()).unwrap_or_default();
            let start = num(args, 1, 0.0).max(0.0) as usize;
            let end = match args.get(2) {
                Some(XVal::Num(e)) => e.max(0.0) as usize,
                _ => lines.len(),
            }
            .min(lines.len());
            let entries = lines.into_iter().skip(start).take(end.saturating_sub(start)).enumerate().map(|(i, l)| (XKey::Num(i as f64), XVal::Str(l))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        // `path array [append]`: one line per element [documented: pWriteFile], into
        // NVForever's own layer, never the game's folder (`write_root`); pWriteFileAlt's
        // path is under Data [documented: pWriteFileAlt, "relative to the game's DATA
        // path"; hypothesis: the calling mod's staging folder is its place in Data]. 1
        // when written, 0 without a path or an array [documented: pWriteFileAlt].
        "pwritefile" | "pwritefilealt" => {
            let path = text(args, 0).unwrap_or_default();
            let Some(XVal::Array(id)) = args.get(1) else { return Some(Ok(zero)) };
            if path.trim().is_empty() {
                return Some(Ok(zero));
            }
            let lines: Vec<String> = g.ext.array(*id).map(|a| a.entries.iter().map(|(_, v)| cell_text(v)).collect()).unwrap_or_default();
            let append = num(args, 2, 0.0) != 0.0;
            let body = lines.join("\n");
            let alt = name == "pwritefilealt";
            let old = if alt { g.mod_files.read(&path) } else { g.mod_files.read_root(&path) };
            let text = match old.filter(|_| append) {
                Some(o) if !o.is_empty() => format!("{o}\n{body}"),
                _ => body,
            };
            // A path that would leave NVForever's layer is refused: 0 [decision: PR #1291
            // review, the charter; the page gives no failure value].
            let ok = if alt { g.mod_files.write(&path, text) } else { g.mod_files.write_root(&path, text) };
            XVal::Num(if ok { 1.0 } else { 0.0 })
        }
        // ---- LU: read the file each time; write it at once ----
        "getinifloat" | "getinistring" => {
            let Some((s, k)) = text(args, 0).as_deref().and_then(section_key) else { return Some(Ok(zero)) };
            let path = ini_path(g, text(args, 1).as_deref());
            let v = g.mod_files.read(&path).and_then(|t| ini_get(&t, &s, &k));
            // A text value reads 0 as a number [observed].
            if name == "getinifloat" { XVal::Num(v.map_or(0.0, |v| num_of(&v))) } else { XVal::Str(v.unwrap_or_default()) }
        }
        "setinifloat" | "setinistring" => {
            let Some(raw) = text(args, 0) else { return Some(Ok(zero)) };
            let Some((s, k)) = section_key(&raw) else { return Some(Ok(zero)) };
            let i = raw.find([':', '/', '\\']).unwrap_or(0);
            let (sn, kn) = (raw[..i].trim().to_string(), raw[i + 1..].trim().to_string());
            let value = if name == "setinifloat" { lu_number(num(args, 1, 0.0)) } else { text(args, 1).unwrap_or_default() };
            let path = ini_path(g, text(args, 2).as_deref());
            let old = g.mod_files.read(&path).unwrap_or_default();
            let new = ini_set(&old, &s, &k, &value, &sn, &kn);
            g.mod_files.write(&path, new);
            zero
        }
        "getinisection" => {
            let sec = text(args, 0).unwrap_or_default().to_ascii_lowercase();
            let path = ini_path(g, text(args, 1).as_deref());
            let numeric = num(args, 2, 0.0) != 0.0;
            let pairs = g.mod_files.read(&path).map(|t| ini_section(&t, &sec)).unwrap_or_default();
            let entries = pairs.into_iter().map(|(k, v)| (XKey::Str(k), if numeric { XVal::Num(num_of(&v)) } else { XVal::Str(v) })).collect();
            XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
        }
        "getinisectionnames" => {
            let path = ini_path(g, text(args, 0).as_deref());
            let names = g.mod_files.read(&path).map(|t| ini_sections(&t)).unwrap_or_default();
            let entries = names.into_iter().enumerate().map(|(i, n)| (XKey::Num(i as f64), XVal::Str(n))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        // ---- ShowOff: a file is read once and kept; OrCreate changes only the cache
        // [observed: the created key was not in the file, and LU read 0] ----
        "getinifloatordefault_cached" | "getinistringordefault_cached" | "getinifloatorcreate_cached" | "getinistringorcreate_cached" | "setinivalue_cached" | "setiniinteger_cached" => {
            let Some(raw) = text(args, 0) else { return Some(Ok(zero)) };
            let Some((s, k)) = section_key(&raw) else { return Some(Ok(zero)) };
            let file_at = if name.starts_with("set") { 2 } else { 1 };
            let path = ini_path(g, text(args, file_at).as_deref());
            let key = norm(&path);
            if !g.mod_files.cache.contains_key(&key) {
                let text = g.mod_files.read(&path).unwrap_or_default();
                let mut m = HashMap::new();
                for sec in ini_sections(&text) {
                    let ls = sec.to_ascii_lowercase();
                    for (kk, vv) in ini_section(&text, &ls) {
                        m.insert(format!("{ls}:{}", kk.to_ascii_lowercase()), vv);
                    }
                }
                g.mod_files.cache.insert(key.clone(), m);
            }
            let ck = format!("{s}:{k}");
            let cached = g.mod_files.cache.get(&key).and_then(|m| m.get(&ck)).cloned();
            let float = name.contains("float") || name == "setiniinteger_cached";
            match name {
                // A missing key gives the default (0 without one); a text value gives 0
                // for the float form, not the default [observed].
                "getinifloatordefault_cached" => XVal::Num(cached.map_or(num(args, 2, 0.0), |v| num_of(&v))),
                "getinistringordefault_cached" => XVal::Str(cached.or_else(|| text(args, 2)).unwrap_or_default()),
                "getinifloatorcreate_cached" | "getinistringorcreate_cached" => {
                    let v = match cached {
                        Some(v) => v,
                        None => {
                            let v = text(args, 2).unwrap_or_default();
                            if let Some(m) = g.mod_files.cache.get_mut(&key) {
                                m.insert(ck, v.clone());
                            }
                            v
                        }
                    };
                    if float { XVal::Num(num_of(&v)) } else { XVal::Str(v) }
                }
                // The cache and the file [hypothesis: not probed].
                _ => {
                    let value = if float { lu_number(num(args, 1, 0.0).trunc()) } else { text(args, 1).unwrap_or_default() };
                    if let Some(m) = g.mod_files.cache.get_mut(&key) {
                        m.insert(ck, value.clone());
                    }
                    let i = raw.find([':', '/', '\\']).unwrap_or(0);
                    let old = g.mod_files.read(&path).unwrap_or_default();
                    let new = ini_set(&old, &s, &k, &value, raw[..i].trim(), raw[i + 1..].trim());
                    g.mod_files.write(&path, new);
                    XVal::Num(1.0)
                }
            }
        }
        "clearfilecacheshowoff" => {
            let path = norm(&text(args, 0).unwrap_or_default());
            let had = g.mod_files.cache.keys().any(|k| k.ends_with(&path) && !path.is_empty());
            g.mod_files.cache.retain(|k, _| path.is_empty() || !k.ends_with(&path));
            XVal::Num(f64::from(u8::from(had)))
        }
        // ---- MCM: "File/Section/Key" of Data\Config\File.ini; a number is written with
        // four decimals [observed: "fMcm=5.0000"] ----
        "getmodinisetting" | "setmodinisetting" => {
            let raw = text(args, 0).unwrap_or_default();
            let mut parts = raw.splitn(3, ['/', '\\']);
            let (Some(file), Some(sec), Some(key)) = (parts.next(), parts.next(), parts.next()) else { return Some(Ok(zero)) };
            let path = ini_path(g, Some(file));
            if name == "getmodinisetting" {
                return Some(Ok(match g.mod_files.read(&path).and_then(|t| ini_get(&t, &sec.to_ascii_lowercase(), key)) {
                    Some(v) => match v.trim().parse::<f64>() {
                        Ok(n) => XVal::Num(n),
                        Err(_) => XVal::Str(v),
                    },
                    None => zero,
                }));
            }
            let value = match args.get(1) {
                Some(XVal::Num(n)) => format!("{n:.4}"),
                Some(XVal::Str(s)) => s.clone(),
                _ => String::new(),
            };
            let old = g.mod_files.read(&path).unwrap_or_default();
            let new = ini_set(&old, &sec.to_ascii_lowercase(), &key.to_ascii_lowercase(), &value, sec, key);
            g.mod_files.write(&path, new);
            zero
        }
        // ---- JIP / LU files ----
        // 1 a loose file, 2 inside an archive, 0 none; with isFolder, a folder
        // [observed: a loose plugin 1, a mesh in a BSA 2, "NVSE" as a folder 1].
        "fileexists" => {
            let p = text(args, 0).unwrap_or_default().replace('/', "\\");
            let folder = num(args, 1, 0.0) != 0.0;
            let r = roots();
            let loose = r.read.iter().any(|root| {
                let full = root.join(&p);
                if folder { full.is_dir() } else { full.is_file() }
            }) || (!folder && g.mod_files.written.contains_key(&norm(&p)));
            if loose {
                XVal::Num(1.0)
            } else if !folder {
                XVal::Num(f64::from(r.lookup.as_ref().map_or(0, |f| f(&p))))
            } else {
                zero
            }
        }
        // `folder [filter]`: the file names (no path) of a folder under the game's
        // folder and NVForever's layer [observed: "NVSE\Plugins" "*.dll" gives bare
        // names]; a missing folder gives an empty array [observed: plugin-odds files.none,
        // ar_Size 0, where an unset array reads -1].
        // GetFoldersInFolder the same for folders [observed: gaps3, "NVSE" gives
        // "Plugins"; a missing folder an empty array].
        "getfilesinfolder" | "getfoldersinfolder" => {
            let folders = name == "getfoldersinfolder";
            let p = text(args, 0).unwrap_or_default().replace('/', "\\");
            let filter = text(args, 1).unwrap_or_else(|| "*".into()).to_ascii_lowercase();
            let mut names: Vec<String> = Vec::new();
            for root in roots().read {
                // A path under the game folder (NVSE\Plugins) or under Data.
                for base in [root.parent().map(|x| x.to_path_buf()), Some(root.clone())].into_iter().flatten() {
                    if let Ok(rd) = std::fs::read_dir(base.join(&p)) {
                        for e in rd.flatten() {
                            if e.path().is_dir() == folders {
                                let n = e.file_name().to_string_lossy().to_string();
                                if glob(&filter, &n.to_ascii_lowercase()) && !names.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                                    names.push(n);
                                }
                            }
                        }
                    }
                }
            }
            names.sort_by_key(|n| n.to_ascii_lowercase());
            let entries = names.into_iter().enumerate().map(|(i, n)| (XKey::Num(i as f64), XVal::Str(n))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// A `*` / `?` file-name pattern.
fn glob(pat: &str, s: &str) -> bool {
    fn go(p: &[u8], s: &[u8]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], s) || (!s.is_empty() && go(p, &s[1..])),
            (Some(b'?'), Some(_)) => go(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &s[1..]),
            _ => false,
        }
    }
    go(pat.as_bytes(), s.as_bytes())
}

#[cfg(test)]
mod path_tests {
    use super::on_disk;
    use std::path::Path;

    #[test]
    fn only_plain_names_stay_under_the_base() {
        let base = Path::new("layer");
        assert_eq!(on_disk(base, "Config\\x.ini"), Some(base.join("Config").join("x.ini")));
        assert_eq!(on_disk(base, "Config//x.ini"), Some(base.join("Config").join("x.ini")));
        for bad in ["..\\..\\x", "../x", "Config\\..\\..\\x", "C:\\x", "C:x", "\\x", "/x", ".\\x", "x:stream", "\\\\server\\share\\x"] {
            assert_eq!(on_disk(base, bad), None, "{bad}");
        }
    }
}
