//! JIP's NIF block functions (N6 of docs/handoff/nvse-next-batch.md;
//! docs/design/nif-blocks.md) [documented: their pages; observed: the peer's nif-blocks
//! set in the original game]. The nodes are the engine's; the game keeps a mirror of the
//! node tree of every reference a script asked about, filled by the engine each frame
//! (`SceneMirror::set_tree`), and queues the scripts' changes for the engine
//! (`SceneMirror::take_edits`). A change shows in the mirror at once [observed: a set
//! reads back in the same frame]. No memory layout of the original is used: nodes are
//! known by name, as on the pages (extender-compat.md §5.8).

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::{HashMap, HashSet};

/// One node as the scripts see it.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneNode {
    pub name: String,
    pub parent: Option<usize>,
    /// Local translation (game units), rotation (degrees, X Y Z extrinsic) and scale.
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: f32,
    /// The NiAVObject flags; bit 0 hides it.
    pub flags: u32,
}

/// A change a script made, for the engine to apply to the drawn nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneEdit {
    Translation { r: FormId, node: String, v: [f32; 3] },
    Rotation { r: FormId, node: String, v: [f32; 3] },
    Scale { r: FormId, node: String, v: f32 },
    Flags { r: FormId, node: String, flags: u32 },
    Rename { r: FormId, node: String, to: String },
    Remove { r: FormId, node: String },
    /// A new empty node under `target`, or (`as_parent`) between `target` and its parent.
    Insert { r: FormId, target: String, name: String, as_parent: bool },
    /// A model's nodes under `target` (`model` under meshes).
    Attach { r: FormId, target: String, model: String },
}

/// The mirror and the queued changes.
#[derive(Debug, Default)]
pub struct SceneMirror {
    trees: HashMap<FormId, Vec<SceneNode>>,
    watched: HashSet<FormId>,
    edits: Vec<SceneEdit>,
}

impl SceneMirror {
    /// The references whose trees the engine should tell.
    pub fn watched(&self) -> impl Iterator<Item = FormId> + '_ {
        self.watched.iter().copied()
    }

    /// The engine tells a watched reference's tree (parents before children).
    pub fn set_tree(&mut self, r: FormId, nodes: Vec<SceneNode>) {
        self.trees.insert(r, nodes);
    }

    /// The reference's 3D is gone.
    pub fn forget(&mut self, r: FormId) {
        self.trees.remove(&r);
    }

    /// The changes made since the last call, for the engine.
    pub fn take_edits(&mut self) -> Vec<SceneEdit> {
        std::mem::take(&mut self.edits)
    }

    fn tree(&mut self, r: FormId) -> Option<&mut Vec<SceneNode>> {
        self.watched.insert(r);
        self.trees.get_mut(&r)
    }
}

impl Game<'_> {
    /// A model's nodes attached at once under a node of a reference's drawn tree (its
    /// root when `target` is `None`), for the engine (JIPPPLN's AttachFormModel); false
    /// when the tree or the node is not known.
    pub(crate) fn attach_model_now(&mut self, r: FormId, target: Option<&str>, model: &str) -> bool {
        let Some(t) = self.scene.tree(r) else { return false };
        let node = match target {
            Some(n) => find(t, n).map(|i| t[i].name.clone()),
            None => t.iter().find(|n| n.parent.is_none()).map(|n| n.name.clone()),
        };
        let Some(node) = node else { return false };
        self.scene.edits.push(SceneEdit::Attach { r, target: node, model: model.to_string() });
        true
    }

    /// A node of a reference's drawn tree taken away, for the engine (JIPPPLN's
    /// ClearRuntimeNodes); the mirror forgets it at once.
    pub(crate) fn remove_node_now(&mut self, r: FormId, node: &str) {
        crate::nif_blocks::call(self, "removenifblock", Some(r), &[XVal::Str(node.to_string())]);
    }
}

/// The node of that name: exactly, else without regard to case [observed:
/// "bip01 head|…" still inserted; ModelHasBlock is exact].
fn find(t: &[SceneNode], name: &str) -> Option<usize> {
    t.iter().position(|n| n.name == name).or_else(|| t.iter().position(|n| n.name.eq_ignore_ascii_case(name)))
}

/// Every node under `i`, depth first in order.
fn descendants(t: &[SceneNode], i: Option<usize>) -> Vec<usize> {
    let mut out = Vec::new();
    fn walk(t: &[SceneNode], p: Option<usize>, out: &mut Vec<usize>) {
        for (k, n) in t.iter().enumerate() {
            if n.parent == p && Some(k) != p {
                out.push(k);
                walk(t, Some(k), out);
            }
        }
    }
    walk(t, i, &mut out);
    out
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

/// The axis an argument names (`X`, `Y`, `Z`), when it names one.
fn axis(v: Option<&XVal>) -> Option<usize> {
    match v? {
        XVal::Str(s) => match s.to_ascii_uppercase().as_str() {
            "X" => Some(0),
            "Y" => Some(1),
            "Z" => Some(2),
            _ => None,
        },
        _ => None,
    }
}

/// The model file of a base form (MODL), under meshes.
pub(crate) fn model_of(g: &Game, f: FormId) -> Option<String> {
    let b = crate::functions::base_of(g, f).unwrap_or(f);
    match crate::nvse::record_value(g, b, "MODL", "").or_else(|| crate::nvse::record_value(g, b, "MODL", "model")) {
        Some(XVal::Str(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let list = |g: &mut Game, items: Vec<XVal>| {
        let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
        XVal::Array(g.ext.new_array(ArrayKind::List, entries))
    };
    let r = subject.unwrap_or(FormId(0));
    let v = match name {
        // A missing block, or 3D not there yet, gives ar_Null [observed].
        "getchildblocks" => {
            let parent = text(args, 0);
            let Some(t) = g.scene.tree(r).cloned() else { return Some(Ok(XVal::Array(0))) };
            let at = if parent.is_empty() { None } else {
                match find(&t, &parent) {
                    Some(i) => Some(i),
                    None => return Some(Ok(XVal::Array(0))),
                }
            };
            let names = descendants(&t, at).into_iter().map(|i| XVal::Str(t[i].name.clone())).collect();
            list(g, names)
        }
        // Innermost first [observed: Bip01 Neck1 … Scene Root].
        "getnifblockparentnodes" => {
            let Some(t) = g.scene.tree(r).cloned() else { return Some(Ok(XVal::Array(0))) };
            let Some(mut i) = find(&t, &text(args, 0)) else { return Some(Ok(XVal::Array(0))) };
            let mut out = Vec::new();
            while let Some(p) = t[i].parent {
                out.push(XVal::Str(t[p].name.clone()));
                i = p;
            }
            list(g, out)
        }
        // `block [axis]`: an axis gives that number; otherwise the three as an array
        // [observed: JIP 57.3 gives [5.59, 0, 0] for Bip01 Head].
        "getnifblocktranslation" | "getnifblockrotation" => {
            let Some(t) = g.scene.tree(r) else { return Some(Ok(XVal::Array(0))) };
            let Some(i) = find(t, &text(args, 0)) else { return Some(Ok(XVal::Array(0))) };
            let v = if name == "getnifblocktranslation" { t[i].translation } else { t[i].rotation };
            match axis(args.get(1)) {
                Some(a) => XVal::Num(f64::from(v[a])),
                None => {
                    let items = v.iter().map(|x| XVal::Num(f64::from(*x))).collect();
                    list(g, items)
                }
            }
        }
        "setnifblocktranslation" | "setnifblockrotation" => {
            let node = text(args, 0);
            let v = [num(args, 1, 0.0) as f32, num(args, 2, 0.0) as f32, num(args, 3, 0.0) as f32];
            if let Some(t) = g.scene.tree(r) {
                if let Some(i) = find(t, &node) {
                    if name == "setnifblocktranslation" { t[i].translation = v } else { t[i].rotation = v }
                }
            }
            g.scene.edits.push(if name == "setnifblocktranslation" { SceneEdit::Translation { r, node, v } } else { SceneEdit::Rotation { r, node, v } });
            zero
        }
        // A missing block 0 [observed].
        "getnifblockscale" => {
            let Some(t) = g.scene.tree(r) else { return Some(Ok(zero)) };
            XVal::Num(find(t, &text(args, 0)).map_or(0.0, |i| f64::from(t[i].scale)))
        }
        "setnifblockscale" => {
            let node = text(args, 0);
            let v = num(args, 1, 1.0) as f32;
            if let Some(t) = g.scene.tree(r) {
                if let Some(i) = find(t, &node) {
                    t[i].scale = v;
                }
            }
            g.scene.edits.push(SceneEdit::Scale { r, node, v });
            zero
        }
        "getnifblockflag" => {
            let Some(t) = g.scene.tree(r) else { return Some(Ok(zero)) };
            let bit = num(args, 1, 0.0) as u32;
            XVal::Num(find(t, &text(args, 0)).map_or(0.0, |i| f64::from((t[i].flags >> bit.min(31)) & 1)))
        }
        "setnifblockflag" => {
            let node = text(args, 0);
            let bit = 1u32 << (num(args, 1, 0.0) as u32).min(31);
            let on = num(args, 2, 0.0) != 0.0;
            let mut flags = None;
            if let Some(t) = g.scene.tree(r) {
                if let Some(i) = find(t, &node) {
                    t[i].flags = if on { t[i].flags | bit } else { t[i].flags & !bit };
                    flags = Some(t[i].flags);
                }
            }
            if let Some(flags) = flags {
                g.scene.edits.push(SceneEdit::Flags { r, node, flags });
            }
            zero
        }
        "renamenifblock" => {
            let (node, to) = (text(args, 0), text(args, 1));
            let ok = g.scene.tree(r).and_then(|t| find(t, &node).map(|i| t[i].name = to.clone())).is_some();
            if ok {
                g.scene.edits.push(SceneEdit::Rename { r, node, to });
            }
            XVal::Num(f64::from(u8::from(ok)))
        }
        "removenifblock" => {
            let node = text(args, 0);
            let ok = match g.scene.tree(r) {
                Some(t) => match find(t, &node) {
                    Some(i) => {
                        let mut gone: HashSet<usize> = descendants(t, Some(i)).into_iter().collect();
                        gone.insert(i);
                        let keep: Vec<usize> = (0..t.len()).filter(|k| !gone.contains(k)).collect();
                        let remap: HashMap<usize, usize> = keep.iter().enumerate().map(|(n, &o)| (o, n)).collect();
                        let mut nt: Vec<SceneNode> = keep.iter().map(|&k| t[k].clone()).collect();
                        for n in &mut nt {
                            n.parent = n.parent.and_then(|p| remap.get(&p).copied());
                        }
                        *t = nt;
                        true
                    }
                    None => false,
                },
                None => false,
            };
            if ok {
                g.scene.edits.push(SceneEdit::Remove { r, node });
            }
            XVal::Num(f64::from(u8::from(ok)))
        }
        // `ref add "Target|Name"`: a new node under Target; "^Name" puts it above Target
        // [documented: InsertNode; observed: both return 1 and show in the walks].
        "insertnode" | "attachmodel" => {
            let target_ref = form(args, 0).unwrap_or(r);
            // Modes 1 and 3 insert, 0 and 2 remove [documented: InsertNode's addOrRemove
            // table]. The mirror drops nothing on a remove: mode 0 waits for a reload and
            // mode 2's instant removal is a follow-up (ext-jippp.md).
            let mode = num(args, 1, 1.0);
            let add = mode as i64 & 1 == 1;
            let spec = text(args, 2);
            let Some((target, what)) = spec.split_once('|') else { return Some(Ok(zero)) };
            // JIPPPLN's runtime node lists (`crate::nvse_jippp`) keep every well-formed call.
            crate::nvse_jippp::record(g, target_ref, name == "attachmodel", &spec, mode);
            if !add {
                return Some(Ok(zero));
            }
            let Some(t) = g.scene.tree(target_ref) else { return Some(Ok(zero)) };
            let Some(i) = find(t, target) else { return Some(Ok(zero)) };
            if name == "insertnode" {
                let (as_parent, nm) = match what.strip_prefix('^') {
                    Some(n) => (true, n.to_string()),
                    None => (false, what.to_string()),
                };
                let node = SceneNode { name: nm.clone(), parent: if as_parent { t[i].parent } else { Some(i) }, translation: [0.0; 3], rotation: [0.0; 3], scale: 1.0, flags: 0 };
                t.push(node);
                if as_parent {
                    let new = t.len() - 1;
                    t[i].parent = Some(new);
                }
                g.scene.edits.push(SceneEdit::Insert { r: target_ref, target: target.to_string(), name: nm, as_parent });
            } else {
                g.scene.edits.push(SceneEdit::Attach { r: target_ref, target: target.to_string(), model: what.to_string() });
            }
            XVal::Num(1.0)
        }
        // `form block`: the form's model has a node of that exact name [observed:
        // "projectilenode" 0 where "ProjectileNode" is 1].
        "modelhasblock" => {
            let (Some(f), want) = (form(args, 0), text(args, 1)) else { return Some(Ok(zero)) };
            let found = model_of(g, f).and_then(|m| crate::mod_files::read_data(&format!("meshes\\{m}"))).is_some_and(|bytes| crate::mod_files::nif_has_name(&bytes, &want));
            XVal::Num(f64::from(u8::from(found)))
        }
        _ => return None,
    };
    Some(Ok(v))
}
