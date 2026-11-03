//! Writing a plugin of edits (the in-game editor; local design notes
//! `local/editor-design.md` §4, not yet in docs/): placed references at new positions
//! and rotations (each an override of the winning record, with its new DATA) and new
//! references the plugin adds ([`NewRef`]).
//!
//! - **Masters**: every plugin loaded before the new one, in load order (the new one
//!   last, or where it already loads). A FormID in the new file is then its load-order
//!   FormID, except the new plugin's own index. Then masters nothing in the file refers
//!   to are dropped ("clean masters", design §4.2) and the FormIDs renumbered to the
//!   shorter list; when a copied subrecord does not decode exactly, so that its FormIDs
//!   are unknown, every master stays.
//! - **Groups**: a reference goes in the same chain of groups its winning record sits
//!   in (worldspace children, exterior block and sub-block, cell children, persistent or
//!   temporary children), copied from that plugin with FormID labels remapped, and the
//!   worldspace and cell records that own those groups are copied before them
//!   [documented: esm.md group types]. So no block or sub-block number is worked out here.
//! - **Copied records** keep every subrecord; FormIDs in them are rewritten from the
//!   source plugin's master list to the new one's by decoding with the schema and encoding
//!   back ([`crate::encode::encode_type_mapped`]). A subrecord the schema does not decode
//!   exactly is kept as it was; when its plugin's FormIDs need rewriting, that is a
//!   warning in [`Written::warnings`].
//! - **New references** get the plugin's own index and object IDs from its next object
//!   ID on (0x800 for a new plugin [hypothesis: the first ID an editor gives a new
//!   plugin's records], the header's value for one already loaded). Each holds NAME (its
//!   base) and DATA, in its cell's temporary children group, which is found as above from
//!   the chain of groups the cell's winning record sits in.
//! - **Header**: HEDR version 1.34 (the highest of the shipped masters [observed:
//!   plugin.rs]), the record count as the masters have it (records + groups, the header
//!   record not counted [observed: plugin.rs]), the next object ID after the new
//!   references, CNAM author, SNAM description, MAST/DATA
//!   (DATA 0, as every shipped master [observed]), and ONAM: the overridden master records
//!   that sit in temporary children groups, the set every shipped add-on lists [observed:
//!   esm.md].

use crate::decode::{decode_sub, DecodeCtx, SubStatus};
use crate::encode::{encode_sub, encode_type_mapped};
use crate::loadorder::LoadOrder;
use crate::raw::{group_type, record_flags, visit_records, write_nodes, write_subrecords, Group, GroupHeader, Node, Record, RecordHeader, Subrecord};
use crate::schema::Schema;
use bytes::Bytes;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

/// A placed reference (load-order FormID) at a new place: game units, radians.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Placement {
    pub form_id: u32,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    /// Written Initially Disabled (record flag 0x800 [documented: GECK wiki, Initially
    /// Disabled; `fnv_sim::refs::FLAG_INITIALLY_DISABLED`]): how editor mode deletes a
    /// reference of another plugin. Off leaves the flag as the copied record has it.
    pub disabled: bool,
}

/// A reference the plugin adds: its base and cell (load-order FormIDs), place (game
/// units) and rotation (radians).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NewRef {
    pub base: u32,
    pub cell: u32,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

/// The object ID a new plugin's first record gets [hypothesis, module notes].
pub const FIRST_OBJECT_ID: u32 = 0x800;

/// Record flag of a reference that starts disabled.
pub const INITIALLY_DISABLED: u32 = 0x0000_0800;

/// The plugin to write.
#[derive(Debug, Clone, Default)]
pub struct NewPlugin {
    /// File name, e.g. `MyEdits.esp`.
    pub name: String,
    pub author: Option<String>,
    pub description: Option<String>,
}

/// A written plugin.
#[derive(Debug, Clone)]
pub struct Written {
    pub bytes: Vec<u8>,
    pub masters: Vec<String>,
    /// References written (moved or deleted ones).
    pub references: usize,
    /// The FormIDs the new references got, in the file's own numbering (top byte the
    /// plugin's index among its masters), in [`write_plugin`]'s order.
    pub new_ids: Vec<u32>,
    /// The same references' load-order FormIDs, once the plugin loads where it already
    /// is, or after the others.
    pub new_load_ids: Vec<u32>,
    pub warnings: Vec<String>,
}

/// Group types whose label is a FormID [documented: esm.md group types].
fn label_is_form(t: i32) -> bool {
    matches!(t, group_type::WORLD_CHILDREN | group_type::CELL_CHILDREN | group_type::TOPIC_CHILDREN | group_type::CELL_PERSISTENT_CHILDREN | group_type::CELL_TEMPORARY_CHILDREN | group_type::CELL_VISIBLE_DISTANT_CHILDREN)
}

struct Writer<'l, 'a> {
    lo: &'l LoadOrder<'a>,
    schema: &'l Schema,
    /// The new plugin's place in the load order, when it is loaded.
    own: Option<usize>,
    /// Plugins before it: its masters.
    n_masters: usize,
    /// The file index each of those masters gets when unused ones are dropped (`None`:
    /// dropped); `None` keeps them all at their load-order index.
    keep: Option<Vec<Option<u32>>>,
    /// The file index of the plugin's own records: the number of masters it keeps.
    own_index: u32,
    /// Decode every copied record, even one whose FormIDs need no rewriting, so that
    /// `used` sees all of them.
    decode_all: bool,
    /// Masters (load-order index) a written FormID belongs to.
    used: RefCell<BTreeSet<usize>>,
    /// A copied subrecord was kept as it was: its FormIDs were not seen.
    opaque: bool,
    warnings: Vec<String>,
}

impl Writer<'_, '_> {
    /// A load-order FormID as the new file writes it.
    fn new_local(&self, global: u32) -> Result<u32, String> {
        let t = (global >> 24) as usize;
        let low = global & 0x00FF_FFFF;
        if Some(t) == self.own {
            Ok((self.own_index << 24) | low)
        } else if t < self.n_masters {
            self.used.borrow_mut().insert(t);
            match &self.keep {
                None => Ok(global),
                Some(k) => k[t].map(|i| (i << 24) | low).ok_or_else(|| format!("{global:08X} belongs to {}, a dropped master", self.lo.plugins[t].name)),
            }
        } else {
            Err(format!("{global:08X} belongs to {}, which loads after the new plugin", self.lo.plugins.get(t).map_or("?", |p| p.name.as_str())))
        }
    }

    /// A FormID local to plugin `pi` as the new file writes it.
    fn remap(&self, pi: usize, local: u32) -> Result<u32, String> {
        if local == 0 {
            return Ok(0);
        }
        self.new_local(self.lo.to_global(pi, local).ok_or_else(|| format!("{local:08X} cannot be mapped"))?)
    }

    /// Whether plugin `pi`'s FormIDs are written unchanged.
    fn identity(&self, pi: usize) -> bool {
        if self.decode_all || self.keep.as_ref().is_some_and(|k| k.iter().enumerate().any(|(i, &n)| n != Some(i as u32))) {
            return false;
        }
        let m = self.lo.master_map(pi);
        m.iter().enumerate().all(|(i, &g)| g as usize == i) && (m.len() == pi && Some(pi) != self.own || Some(pi) == self.own && m.len() == self.n_masters)
    }

    /// Plugin `pi`'s record `r`, copied with its FormIDs remapped.
    fn copy(&mut self, pi: usize, r: &Record) -> Result<(RecordHeader, Vec<Subrecord>), String> {
        let subs = r.subrecords().map_err(|e| e.to_string())?;
        let identity = self.identity(pi);
        let mut out = Vec::with_capacity(subs.len());
        for s in subs {
            if identity {
                out.push(s);
                continue;
            }
            let mut ctx = DecodeCtx { schema: self.schema, on_formid: None };
            let d = decode_sub(&r.header.sig, &s, &mut ctx);
            let exact = d.status == SubStatus::Ok && d.value.as_ref().is_some_and(|v| encode_sub(&r.header.sig, &s.sig, v, self.schema).ok().as_deref() == Some(&s.data[..]));
            match (exact, d.value) {
                (true, Some(v)) => {
                    let def = self.schema.lookup(&r.header.sig, &s.sig).expect("decoded, so defined");
                    let mut errs = Vec::new();
                    let mut data = Vec::with_capacity(s.data.len());
                    encode_type_mapped(&def.ty, &v, self.schema, &mut data, &mut |id| {
                        self.remap(pi, id).unwrap_or_else(|e| {
                            errs.push(e);
                            id
                        })
                    })?;
                    for e in errs {
                        self.warnings.push(format!("{} {:08X} {}: {e}", r.sig_str(), r.header.form_id, s.sig_str()));
                    }
                    out.push(Subrecord { data, ..s });
                }
                _ => {
                    self.opaque = true;
                    self.warnings.push(format!(
                        "{} {:08X} {}: not decoded exactly, kept as it was; FormIDs in it are not remapped",
                        r.sig_str(),
                        r.header.form_id,
                        s.sig_str()
                    ));
                    out.push(s);
                }
            }
        }
        let header = RecordHeader { flags: r.header.flags & !record_flags::COMPRESSED, form_id: self.remap(pi, r.header.form_id)?, ..r.header.clone() };
        Ok((header, out))
    }

    /// The winning record of load-order FormID `global`, copied.
    fn copy_winner(&mut self, global: u32) -> Result<Record, String> {
        let d = *self.lo.index.get(&global).and_then(|v| v.last()).ok_or_else(|| format!("{global:08X} is not in the load order"))?;
        let r = self.lo.records[d.plugin as usize][d.ordinal as usize];
        let (h, mut subs) = self.copy(d.plugin as usize, r)?;
        // A worldspace's OFST holds its cells' offsets [documented: xEdit wbWorldOffsetData,
        // schemas/fnv/world.json]; offsets into the file it came from would point at the
        // wrong places in this one [hypothesis], so it is left out.
        if &h.sig == b"WRLD" {
            subs.retain(|s| &s.sig != b"OFST");
        }
        Ok(record(h, &subs))
    }
}

impl Writer<'_, '_> {
    /// The chain of groups `groups` (plugin `pi`'s) as the new file writes it: FormID
    /// labels remapped, and the worldspace and cell records that own groups copied.
    fn chain(&mut self, pi: usize, groups: &[GroupHeader]) -> Result<Vec<(GroupHeader, Option<Record>)>, String> {
        let mut chain = Vec::with_capacity(groups.len());
        for g in groups {
            let mut h = g.clone();
            let owner = if label_is_form(g.group_type) {
                let global = self.lo.to_global(pi, g.label_u32()).ok_or("group label cannot be mapped")?;
                h.label = self.new_local(global)?.to_le_bytes();
                match g.group_type {
                    group_type::WORLD_CHILDREN | group_type::CELL_CHILDREN => Some(self.copy_winner(global)?),
                    _ => None,
                }
            } else {
                None
            };
            chain.push((h, owner));
        }
        Ok(chain)
    }

    /// The chain of groups a new reference in cell `cell` (load-order FormID) goes under:
    /// the cell's own, then its children and temporary children groups.
    fn new_ref_chain(&mut self, cell: u32) -> Result<Vec<(GroupHeader, Option<Record>)>, String> {
        let d = *self.lo.index.get(&cell).and_then(|v| v.last()).ok_or_else(|| format!("cell {cell:08X} is not in the load order"))?;
        let pi = d.plugin as usize;
        let local = self.lo.records[pi][d.ordinal as usize].header.form_id;
        let mut above: Option<Vec<GroupHeader>> = None;
        visit_records(&self.lo.plugins[pi].nodes[1..], &mut |r, stack| {
            if above.is_none() && r.header.form_id == local && &r.header.sig == b"CELL" {
                above = Some(stack.iter().map(|g| (*g).clone()).collect());
            }
        });
        let above = above.ok_or_else(|| format!("{cell:08X} is not a CELL"))?;
        let mut chain = self.chain(pi, &above)?;
        let label = self.new_local(cell)?.to_le_bytes();
        let group = |group_type| GroupHeader { size: 0, label, group_type, stamp: 0, unknown1: 0, version: 0, unknown2: 0 };
        chain.push((group(group_type::CELL_CHILDREN), Some(self.copy_winner(cell)?)));
        chain.push((group(group_type::CELL_TEMPORARY_CHILDREN), None));
        Ok(chain)
    }
}

fn record(mut header: RecordHeader, subs: &[Subrecord]) -> Record {
    let raw = write_subrecords(subs);
    header.data_size = raw.len() as u32;
    Record { header, raw: Bytes::from(raw), offset: 0 }
}

fn has_record(nodes: &[Node], form_id: u32) -> bool {
    nodes.iter().any(|n| matches!(n, Node::Record(r) if r.header.form_id == form_id))
}

/// Put `leaf` under the chain of groups `chain` (each with the record that owns it, for a
/// worldspace's or a cell's children), making the groups that are missing.
fn insert(nodes: &mut Vec<Node>, chain: &[(GroupHeader, Option<Record>)], leaf: Record) {
    let Some(((gh, owner), rest)) = chain.split_first() else {
        match nodes.iter_mut().find(|n| matches!(n, Node::Record(r) if r.header.form_id == leaf.header.form_id)) {
            Some(n) => *n = Node::Record(leaf),
            None => nodes.push(Node::Record(leaf)),
        }
        return;
    };
    let at = nodes.iter().position(|n| matches!(n, Node::Group(g) if g.header.group_type == gh.group_type && g.header.label == gh.label));
    let at = match at {
        Some(i) => i,
        None => {
            if let Some(o) = owner {
                if !has_record(nodes, o.header.form_id) {
                    nodes.push(Node::Record(o.clone()));
                }
            }
            nodes.push(Node::Group(Group { header: gh.clone(), children: Vec::new(), offset: 0 }));
            nodes.len() - 1
        }
    };
    let Node::Group(g) = &mut nodes[at] else { unreachable!("found or made a group") };
    insert(&mut g.children, rest, leaf);
}

fn count(nodes: &[Node]) -> usize {
    nodes.iter().map(|n| match n {
        Node::Record(_) => 1,
        Node::Group(g) => 1 + count(&g.children),
    }).sum()
}

fn zstring(s: &str) -> Result<Vec<u8>, String> {
    let mut b = fnv_util::cp1252::encode(s).ok_or_else(|| format!("{s:?} has a character Windows-1252 cannot hold"))?;
    b.push(0);
    Ok(b)
}

/// Write `plugin` holding `edits`, against the load order `lo` (which may or may not
/// already hold a plugin of that name; when it does, its records are copied from it).
pub fn write_placements(lo: &LoadOrder, schema: &Schema, plugin: &NewPlugin, edits: &[Placement]) -> Result<Written, String> {
    write_plugin(lo, schema, plugin, edits, &[])
}

/// [`write_placements`], and the references `new` the plugin adds.
pub fn write_plugin(lo: &LoadOrder, schema: &Schema, plugin: &NewPlugin, edits: &[Placement], new: &[NewRef]) -> Result<Written, String> {
    // A first pass finds the masters the file refers to; the second writes it with only those.
    let first = write_with(lo, schema, plugin, edits, new, None, true)?;
    let n_masters = lo.plugins.iter().position(|p| p.name.eq_ignore_ascii_case(&plugin.name)).unwrap_or(lo.plugins.len());
    if first.opaque || first.used.len() == n_masters {
        return Ok(write_with(lo, schema, plugin, edits, new, None, false)?.written);
    }
    let mut next = 0;
    let keep = (0..n_masters)
        .map(|i| {
            first.used.contains(&i).then(|| {
                next += 1;
                next - 1
            })
        })
        .collect();
    Ok(write_with(lo, schema, plugin, edits, new, Some(keep), false)?.written)
}

/// One pass of [`write_plugin`]: what it wrote, the masters it referred to, and whether
/// a subrecord's FormIDs went unseen.
struct Pass {
    written: Written,
    used: BTreeSet<usize>,
    opaque: bool,
}

fn write_with(lo: &LoadOrder, schema: &Schema, plugin: &NewPlugin, edits: &[Placement], new: &[NewRef], keep: Option<Vec<Option<u32>>>, decode_all: bool) -> Result<Pass, String> {
    let own = lo.plugins.iter().position(|p| p.name.eq_ignore_ascii_case(&plugin.name));
    let n_masters = own.unwrap_or(lo.plugins.len());
    let own_index = keep.as_ref().map_or(n_masters, |k| k.iter().flatten().count()) as u32;
    let mut w = Writer { lo, schema, own, n_masters, keep, own_index, decode_all, used: RefCell::new(BTreeSet::new()), opaque: false, warnings: Vec::new() };
    // Each reference's winning definition, by plugin.
    let mut wanted: BTreeMap<usize, BTreeMap<u32, Placement>> = BTreeMap::new();
    for e in edits {
        let d = *lo.index.get(&e.form_id).and_then(|v| v.last()).ok_or_else(|| format!("{:08X} is not in the load order", e.form_id))?;
        let r = lo.records[d.plugin as usize][d.ordinal as usize];
        if !matches!(&r.header.sig, b"REFR" | b"ACHR" | b"ACRE" | b"PGRE" | b"PMIS") {
            return Err(format!("{:08X} is a {}, not a placed reference", e.form_id, r.sig_str()));
        }
        wanted.entry(d.plugin as usize).or_default().insert(r.header.form_id, *e);
    }
    let mut root: Vec<Node> = Vec::new();
    let mut onam = BTreeSet::new();
    let mut written = 0;
    for (pi, want) in wanted {
        // The chain of groups above each wanted record in its plugin.
        let mut found: Vec<(&Record, Vec<GroupHeader>)> = Vec::new();
        visit_records(&lo.plugins[pi].nodes[1..], &mut |r, stack| {
            if want.contains_key(&r.header.form_id) && matches!(&r.header.sig, b"REFR" | b"ACHR" | b"ACRE" | b"PGRE" | b"PMIS") {
                found.push((r, stack.iter().map(|g| (*g).clone()).collect()));
            }
        });
        for (r, groups) in found {
            let e = want[&r.header.form_id];
            let chain = w.chain(pi, &groups)?;
            let (mut header, mut subs) = w.copy(pi, r)?;
            if e.disabled {
                header.flags |= INITIALLY_DISABLED;
            }
            let mut data = Vec::with_capacity(24);
            for v in e.pos.iter().chain(&e.rot) {
                data.extend(v.to_le_bytes());
            }
            match subs.iter_mut().find(|s| &s.sig == b"DATA") {
                Some(s) => s.data = data,
                None => subs.push(Subrecord { sig: *b"DATA", data, extended: false }),
            }
            let in_temporary = chain.last().is_some_and(|(g, _)| g.group_type == group_type::CELL_TEMPORARY_CHILDREN);
            if in_temporary && header.form_id >> 24 != own_index {
                onam.insert(header.form_id);
            }
            insert(&mut root, &chain, record(header, &subs));
            written += 1;
        }
    }
    // New references, numbered on from the plugin's next object ID.
    let form_version = lo.plugins.first().map_or(15, |p| p.header_record().header.form_version);
    let mut next = own.map_or(FIRST_OBJECT_ID, |pi| lo.plugins[pi].header.next_object_id.max(FIRST_OBJECT_ID));
    let mut new_ids = Vec::with_capacity(new.len());
    let mut new_load_ids = Vec::with_capacity(new.len());
    for n in new {
        let chain = w.new_ref_chain(n.cell)?;
        if next > 0x00FF_FFFF {
            return Err("the plugin has no object IDs left".into());
        }
        let form_id = (own_index << 24) | next;
        new_load_ids.push(((n_masters as u32) << 24) | next);
        next += 1;
        let mut data = Vec::with_capacity(24);
        for v in n.pos.iter().chain(&n.rot) {
            data.extend(v.to_le_bytes());
        }
        let subs = [Subrecord { sig: *b"NAME", data: w.new_local(n.base)?.to_le_bytes().to_vec(), extended: false }, Subrecord { sig: *b"DATA", data, extended: false }];
        let header = RecordHeader { sig: *b"REFR", data_size: 0, flags: 0, form_id, vc_info: 0, form_version, vc_info2: 0 };
        insert(&mut root, &chain, record(header, &subs));
        new_ids.push(form_id);
    }
    // The top groups in the masters' order: cells before worldspaces.
    root.sort_by_key(|n| match n {
        Node::Group(g) if &g.header.label == b"CELL" => 0,
        _ => 1,
    });
    let masters: Vec<String> = lo.plugins[..n_masters].iter().enumerate().filter(|(i, _)| w.keep.as_ref().is_none_or(|k| k[*i].is_some())).map(|(_, p)| p.name.clone()).collect();
    let mut subs = vec![Subrecord {
        sig: *b"HEDR",
        data: [1.34f32.to_le_bytes(), (count(&root) as i32).to_le_bytes(), next.to_le_bytes()].concat(),
        extended: false,
    }];
    subs.push(Subrecord { sig: *b"CNAM", data: zstring(plugin.author.as_deref().unwrap_or("NVForever"))?, extended: false });
    if let Some(d) = &plugin.description {
        subs.push(Subrecord { sig: *b"SNAM", data: zstring(d)?, extended: false });
    }
    for m in &masters {
        subs.push(Subrecord { sig: *b"MAST", data: zstring(m)?, extended: false });
        subs.push(Subrecord { sig: *b"DATA", data: vec![0; 8], extended: false });
    }
    if !onam.is_empty() {
        subs.push(Subrecord { sig: *b"ONAM", data: onam.iter().flat_map(|f| f.to_le_bytes()).collect(), extended: false });
    }
    let tes4 = record(RecordHeader { sig: *b"TES4", data_size: 0, flags: 0, form_id: 0, vc_info: 0, form_version, vc_info2: 0 }, &subs);
    let mut bytes = Vec::new();
    write_nodes(&[Node::Record(tes4)], &mut bytes);
    write_nodes(&root, &mut bytes);
    let used = w.used.take();
    Ok(Pass { written: Written { bytes, masters, references: written, new_ids, new_load_ids, warnings: w.warnings }, used, opaque: w.opaque })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::Plugin;

    fn sub(sig: &[u8; 4], data: &[u8]) -> Subrecord {
        Subrecord { sig: *sig, data: data.to_vec(), extended: false }
    }

    fn rec(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
        let mut out = Vec::new();
        write_nodes(&[Node::Record(record(RecordHeader { sig: *sig, data_size: 0, flags: 0, form_id: fid, vc_info: 0, form_version: 15, vc_info2: 0 }, subs))], &mut out);
        out
    }

    fn grp(label: [u8; 4], t: i32, body: &[u8]) -> Vec<u8> {
        let mut v = b"GRUP".to_vec();
        v.extend(((24 + body.len()) as u32).to_le_bytes());
        v.extend(label);
        v.extend(t.to_le_bytes());
        v.extend([0u8; 8]);
        v.extend(body);
        v
    }

    fn header(masters: &[&str]) -> Vec<u8> {
        let mut subs = vec![sub(b"HEDR", &[1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat())];
        for m in masters {
            subs.push(sub(b"MAST", &zstring(m).unwrap()));
            subs.push(sub(b"DATA", &[0; 8]));
        }
        rec(b"TES4", 0, &subs)
    }

    fn pos_rot(p: [f32; 6]) -> Vec<u8> {
        p.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    const SCHEMA: &str = r#"{"format":"fnv.schema/1","common":{"EDID":{"type":"zstring"}},
      "records":{"REFR":{"subrecords":{"NAME":{"type":"formid"},"DATA":{"type":{"array":"f32","count":6}}}},
                 "CELL":{"subrecords":{"XCLC":{"type":{"array":"i32","count":2}}}},"WRLD":{"subrecords":{}}}}"#;

    /// A master with a worldspace, an exterior cell and two references in it (one
    /// persistent, one temporary), and an add-on (a master of its own) placing a third.
    fn plugins() -> Vec<Plugin> {
        let refr = |fid: u32, base: u32, x: f32| rec(b"REFR", fid, &[sub(b"NAME", &base.to_le_bytes()), sub(b"DATA", &pos_rot([x, 0.0, 0.0, 0.0, 0.0, 0.0]))]);
        let cell = rec(b"CELL", 0x0000_2000, &[sub(b"XCLC", &[2i32.to_le_bytes(), 3i32.to_le_bytes()].concat())]);
        let children = [grp(0x2000u32.to_le_bytes(), 8, &refr(0x0000_3000, 0x0000_0010, 1.0)), grp(0x2000u32.to_le_bytes(), 9, &refr(0x0000_3001, 0x0000_0011, 2.0))].concat();
        let block = grp([0, 0, 0, 0], 4, &grp([0, 0, 0, 0], 5, &[cell, grp(0x2000u32.to_le_bytes(), 6, &children)].concat()));
        let world = [rec(b"WRLD", 0x0000_1000, &[sub(b"EDID", b"World\0")]), grp(0x1000u32.to_le_bytes(), 1, &block)].concat();
        let master = [header(&[]), grp(*b"WRLD", 0, &world)].concat();
        // The add-on places 0x01000005 (its own) in the master's cell.
        let addon_children = grp(0x2000u32.to_le_bytes(), 9, &refr(0x0100_0005, 0x0000_0010, 5.0));
        let addon_cell = rec(b"CELL", 0x0000_2000, &[sub(b"XCLC", &[2i32.to_le_bytes(), 3i32.to_le_bytes()].concat())]);
        let addon_world = [rec(b"WRLD", 0x0000_1000, &[sub(b"EDID", b"World\0")]), grp(0x1000u32.to_le_bytes(), 1, &grp([0; 4], 4, &grp([0; 4], 5, &[addon_cell, grp(0x2000u32.to_le_bytes(), 6, &addon_children)].concat())))].concat();
        let addon = [header(&["Base.esm"]), grp(*b"WRLD", 0, &addon_world)].concat();
        let other = [header(&["Base.esm"])].concat();
        vec![Plugin::parse(&master, "Base.esm".into()).unwrap(), Plugin::parse(&other, "Other.esm".into()).unwrap(), Plugin::parse(&addon, "Addon.esm".into()).unwrap()]
    }

    fn schema() -> Schema {
        let mut s = Schema::default();
        s.merge_str(SCHEMA, "t").unwrap();
        s.validate().unwrap();
        s
    }

    #[test]
    fn moved_references_are_written_under_their_cells_groups() {
        let ps = plugins();
        let lo = LoadOrder::new(&ps).unwrap();
        let s = schema();
        let edits = [
            Placement { form_id: 0x0000_3000, pos: [10.0, 20.0, 30.0], rot: [0.0, 0.0, 1.5], disabled: false },
            Placement { form_id: 0x0000_3001, pos: [7.0, 0.0, 0.0], rot: [0.0; 3], disabled: false },
            // The add-on's own reference: load-order index 2, its file's index 1.
            Placement { form_id: 0x0200_0005, pos: [9.0, 0.0, 0.0], rot: [0.0; 3], disabled: false },
        ];
        let out = write_placements(&lo, &s, &NewPlugin { name: "Edits.esp".into(), author: Some("me".into()), description: None }, &edits).unwrap();
        // Nothing refers to Other.esm: it is dropped, and the add-on becomes the second master.
        assert_eq!(out.masters, ["Base.esm", "Addon.esm"]);
        assert_eq!(out.references, 3);
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        // Loads after the others, and its overrides win.
        let mut all = plugins();
        all.push(Plugin::parse(&out.bytes, "Edits.esp".into()).unwrap());
        let edit = &all[3];
        assert_eq!(edit.header.masters.len(), 2);
        assert_eq!(edit.header.author.as_deref(), Some("me"));
        // Records + groups: WRLD, CELL, 3 REFR; WRLD top, world children, block, sub-block,
        // cell children, persistent, temporary.
        assert_eq!(edit.header.num_records, 12);
        // ONAM: the master's temporary reference and the add-on's (in its new index).
        assert_eq!(edit.header.overridden_forms, vec![0x0000_3001, 0x0100_0005]);
        let lo2 = LoadOrder::new(&all).unwrap();
        let (p, r) = lo2.winner(0x0000_3000).unwrap();
        assert_eq!(p.name, "Edits.esp");
        let data = r.subrecords().unwrap().into_iter().find(|s| &s.sig == b"DATA").unwrap().data;
        assert_eq!(data, pos_rot([10.0, 20.0, 30.0, 0.0, 0.0, 1.5]));
        assert_eq!(lo2.winner(0x0200_0005).unwrap().0.name, "Edits.esp");
        assert_eq!(lo2.winner(0x0000_3000).unwrap().1.header.flags & INITIALLY_DISABLED, 0);
        // Deleted as editor mode deletes: Initially Disabled.
        let gone = [Placement { form_id: 0x0000_3001, pos: [0.0, 0.0, -30000.0], rot: [0.0; 3], disabled: true }];
        let out = write_placements(&lo, &s, &NewPlugin { name: "Gone.esp".into(), ..Default::default() }, &gone).unwrap();
        let p = Plugin::parse(&out.bytes, "Gone.esp".into()).unwrap();
        let mut flags = Vec::new();
        visit_records(&p.nodes[1..], &mut |r, _| {
            if &r.header.sig == b"REFR" {
                flags.push(r.header.flags);
            }
        });
        assert_eq!(flags, [INITIALLY_DISABLED]);
        // The add-on's reference names its base in the master, still 0x10 after the remap.
        let name = lo2.winner(0x0200_0005).unwrap().1.subrecords().unwrap().into_iter().find(|s| &s.sig == b"NAME").unwrap().data;
        assert_eq!(name, 0x10u32.to_le_bytes());
        // One worldspace, one cell: the three references share their groups.
        let mut cells = 0;
        visit_records(&edit.nodes[1..], &mut |r, stack| {
            if &r.header.sig == b"CELL" {
                cells += 1;
                assert_eq!(stack.iter().map(|g| g.group_type).collect::<Vec<_>>(), [0, 1, 4, 5]);
            }
        });
        assert_eq!(cells, 1);
    }

    #[test]
    fn new_references_go_in_their_cells_temporary_group() {
        let ps = plugins();
        let lo = LoadOrder::new(&ps).unwrap();
        let s = schema();
        let np = NewPlugin { name: "Adds.esp".into(), ..Default::default() };
        // Two new references in the master's cell, and the master's persistent one moved.
        let new = [NewRef { base: 0x10, cell: 0x2000, pos: [1.0, 2.0, 3.0], rot: [0.0; 3] }, NewRef { base: 0x0200_0006, cell: 0x2000, pos: [4.0, 5.0, 6.0], rot: [0.0; 3] }];
        let moved = [Placement { form_id: 0x0000_3000, pos: [0.0; 3], rot: [0.0; 3], disabled: false }];
        let out = write_plugin(&lo, &s, &np, &moved, &new).unwrap();
        // Masters Base and Addon (Other is dropped): the plugin's own index is 2 in the file,
        // 3 in the load order.
        assert_eq!(out.masters, ["Base.esm", "Addon.esm"]);
        assert_eq!(out.new_ids, [0x0200_0800, 0x0200_0801]);
        assert_eq!(out.new_load_ids, [0x0300_0800, 0x0300_0801]);
        let p = Plugin::parse(&out.bytes, "Adds.esp".into()).unwrap();
        assert_eq!(p.header.next_object_id, 0x802);
        let mut refs = Vec::new();
        visit_records(&p.nodes[1..], &mut |r, stack| {
            if &r.header.sig == b"REFR" {
                let name = r.subrecords().unwrap().into_iter().find(|s| &s.sig == b"NAME").unwrap().data;
                refs.push((r.header.form_id, stack.last().unwrap().group_type, u32::from_le_bytes(name.try_into().unwrap())));
            }
        });
        // The add-on's base is renumbered: the add-on is the new file's second master.
        assert_eq!(refs, [(0x0000_3000, 8, 0x10), (0x0200_0800, 9, 0x10), (0x0200_0801, 9, 0x0100_0006)]);
        // One cell record, one temporary group for both new references.
        let mut cells = 0;
        visit_records(&p.nodes[1..], &mut |r, _| cells += (&r.header.sig == b"CELL") as usize);
        assert_eq!(cells, 1);
        // Loaded next to the others, the new references are found at their load-order FormIDs.
        let mut all = plugins();
        all.push(p);
        let lo2 = LoadOrder::new(&all).unwrap();
        assert_eq!(lo2.winner(0x0300_0801).unwrap().0.name, "Adds.esp");
        assert!(write_plugin(&lo, &s, &np, &[], &[NewRef { cell: 0x3000, ..Default::default() }]).unwrap_err().contains("not a CELL"));
    }

    #[test]
    fn refuses_what_it_cannot_write() {
        let ps = plugins();
        let lo = LoadOrder::new(&ps).unwrap();
        let s = schema();
        let np = NewPlugin { name: "Edits.esp".into(), ..Default::default() };
        let cell = [Placement { form_id: 0x0000_2000, pos: [0.0; 3], rot: [0.0; 3], disabled: false }];
        assert!(write_placements(&lo, &s, &np, &cell).unwrap_err().contains("not a placed reference"));
        let missing = [Placement { form_id: 0x0000_9999, pos: [0.0; 3], rot: [0.0; 3], disabled: false }];
        assert!(write_placements(&lo, &s, &np, &missing).unwrap_err().contains("not in the load order"));
        // Written into the add-on's place in the order: its own reference is its own index.
        let np = NewPlugin { name: "Addon.esm".into(), ..Default::default() };
        let own = [Placement { form_id: 0x0200_0005, pos: [1.0; 3], rot: [0.0; 3], disabled: false }];
        let out = write_placements(&lo, &s, &np, &own).unwrap();
        assert_eq!(out.masters, ["Base.esm"]);
        let p = Plugin::parse(&out.bytes, "Addon.esm".into()).unwrap();
        let mut ids = Vec::new();
        visit_records(&p.nodes[1..], &mut |r, _| ids.push(r.header.form_id));
        assert!(ids.contains(&0x0100_0005));
    }

    #[test]
    fn keeps_every_master_when_a_subrecord_is_not_decoded() {
        // The schema does not know XSCL: the moved reference's FormIDs cannot all be seen.
        let mut ps = plugins();
        let refr = rec(b"REFR", 0x0000_3000, &[sub(b"NAME", &0x10u32.to_le_bytes()), sub(b"XSCL", &1.0f32.to_le_bytes()), sub(b"DATA", &pos_rot([1.0, 0.0, 0.0, 0.0, 0.0, 0.0]))]);
        let cell = rec(b"CELL", 0x0000_2000, &[sub(b"XCLC", &[2i32.to_le_bytes(), 3i32.to_le_bytes()].concat())]);
        let block = grp([0; 4], 4, &grp([0; 4], 5, &[cell, grp(0x2000u32.to_le_bytes(), 6, &grp(0x2000u32.to_le_bytes(), 8, &refr))].concat()));
        let world = [rec(b"WRLD", 0x0000_1000, &[sub(b"EDID", b"World ")]), grp(0x1000u32.to_le_bytes(), 1, &block)].concat();
        ps[0] = Plugin::parse(&[header(&[]), grp(*b"WRLD", 0, &world)].concat(), "Base.esm".into()).unwrap();
        let lo = LoadOrder::new(&ps).unwrap();
        let s = schema();
        let moved = [Placement { form_id: 0x0000_3000, pos: [0.0; 3], rot: [0.0; 3], disabled: false }];
        let out = write_placements(&lo, &s, &NewPlugin { name: "Edits.esp".into(), ..Default::default() }, &moved).unwrap();
        assert_eq!(out.masters, ["Base.esm", "Other.esm", "Addon.esm"]);
        // Its plugin's FormIDs need no rewriting, so keeping it as it was is no warning.
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    }
}
