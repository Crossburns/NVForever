//! Survey of actor parts' unlit shapes that fade by their material alpha with no
//! NiAlphaProperty: in the files under `meshes\creatures\` and `meshes\characters\`, every
//! drawn shape whose shader is a BSShaderNoLightingProperty with the Dynamic_Alpha flag
//! (bit 19), with no NiAlphaProperty on it or its ancestors, and whether an
//! NiAlphaController sits on its NiMaterialProperty (the target an actor's `.kf` clips
//! drive). Prints the file, the shape, the stored material alpha and the controller's
//! flags, then counts (docs/spec-notes/actors.md, issue #1542).
//!
//! cargo run --release -p fnv-nif --example actor_alpha_survey -- <schema dir> <archive.bsa>...

use fnv_nif::scene::{scene_objects, Decoded};
use fnv_nif::{NifFile, Schema};
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let (mut with_controller, mut without) = (0usize, 0usize);
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> = bsa
            .entries()
            .map(|e| (e.path(), e.record.clone()))
            .filter(|(p, _)| p.ends_with(".nif") && (p.starts_with("meshes\\creatures\\") || p.starts_with("meshes\\characters\\")))
            .collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&file, &data, &schema);
            let objects = scene_objects(&file, &dec);
            let by_block: HashMap<usize, _> = objects.iter().map(|o| (o.block, o)).collect();
            for o in objects.iter().filter(|o| !o.is_node) {
                let mut chain = vec![o.block];
                let mut cur = o.parent;
                while let Some(p) = cur {
                    if chain.len() > 512 {
                        break;
                    }
                    chain.push(p);
                    cur = by_block.get(&p).and_then(|x| x.parent);
                }
                if chain.iter().any(|&b| dec.blocks[b].as_ref().ok().and_then(|s| s.int("flags")).unwrap_or(0) & 1 != 0) {
                    continue;
                }
                let (mut has_alpha_prop, mut dynamic_unlit, mut material) = (false, false, None);
                for &b in chain.iter().rev() {
                    for &r in dec.blocks[b].as_ref().ok().and_then(|s| s.refs("properties")).unwrap_or_default() {
                        let Ok(r) = usize::try_from(r) else { continue };
                        let Some(s) = dec.blocks.get(r).and_then(|x| x.as_ref().ok()) else { continue };
                        match file.blocks[r].type_name.as_str() {
                            "NiAlphaProperty" => has_alpha_prop = true,
                            "NiMaterialProperty" => material = Some(r),
                            "BSShaderNoLightingProperty" => dynamic_unlit = s.int("shader_flags").unwrap_or(0) & (1 << 19) != 0,
                            _ => {}
                        }
                    }
                }
                if has_alpha_prop || !dynamic_unlit {
                    continue;
                }
                let stored = material.and_then(|m| dec.blocks[m].as_ref().ok()).and_then(|s| s.float("alpha"));
                // The material's controller chain: an NiAlphaController on it?
                let mut alpha_flags = None;
                let mut c = material.and_then(|m| dec.blocks[m].as_ref().ok()).and_then(|s| s.block_ref("controller"));
                let mut n = 0;
                while let Some(cr) = c.filter(|&r| r >= 0) {
                    n += 1;
                    if n > 64 {
                        break;
                    }
                    let Some(cs) = dec.get(cr) else { break };
                    if file.blocks[cr as usize].type_name == "NiAlphaController" {
                        alpha_flags = cs.int("flags");
                    }
                    c = cs.block_ref("next_controller");
                }
                if alpha_flags.is_some() {
                    with_controller += 1;
                } else {
                    without += 1;
                }
                println!("{path} :: {:?} alpha {:?} alpha controller flags {:?}", o.name.as_deref().unwrap_or(""), stored, alpha_flags);
            }
        }
    }
    println!("unlit Dynamic_Alpha shapes without NiAlphaProperty: {with_controller} with an NiAlphaController on the material, {without} without");
}
