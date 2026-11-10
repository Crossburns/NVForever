//! Test aid: `NVF_GPU_REPORT=<seconds>` logs, once that long after start, what wgpu's
//! memory allocator holds, summed by resource label (wgpu's allocator report
//! [documented: wgpu 30 Device::generate_allocator_report]), so the game's own GPU memory
//! can be told apart by kind (meshes, textures, render targets, staging).

use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;

pub struct GpuReportPlugin;

impl Plugin for GpuReportPlugin {
    fn build(&self, app: &mut App) {
        if let Some(secs) = std::env::var("NVF_GPU_REPORT").ok().and_then(|v| v.parse::<f32>().ok()) {
            app.add_systems(Last, move |time: Res<Time<Real>>, device: Option<Res<RenderDevice>>, images: Res<Assets<Image>>, meshes: Res<Assets<Mesh>>, mut done: Local<bool>| {
                if *done || time.elapsed_secs() < secs {
                    return;
                }
                *done = true;
                // What the main world still holds of images and meshes, by asset usage.
                let mut img: std::collections::BTreeMap<String, (usize, u64, u64)> = std::collections::BTreeMap::new();
                for (_, i) in images.iter() {
                    let e = img.entry(format!("{:?} {}", i.asset_usage, i.texture_descriptor.label.unwrap_or("-"))).or_default();
                    e.0 += 1;
                    e.1 += i.data.as_ref().map_or(0, |d| d.len() as u64);
                    e.2 += u64::from(i.texture_descriptor.size.width) * u64::from(i.texture_descriptor.size.height);
                }
                for (k, (n, bytes, px)) in &img {
                    info!("gpu report: main-world images {k}: {n}, data {:.1} MiB, {:.1} Mpx", *bytes as f64 / 1048576.0, *px as f64 / 1e6);
                }
                let mut msh: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
                for (_, m) in meshes.iter() {
                    let e = msh.entry(format!("{:?}", m.asset_usage)).or_default();
                    e.0 += 1;
                    // A mesh extracted to the render world has no vertices to count here
                    // (reading them panics).
                    if m.asset_usage.contains(bevy::asset::RenderAssetUsages::MAIN_WORLD) {
                        e.1 += m.count_vertices();
                    }
                }
                for (k, (n, v)) in &msh {
                    info!("gpu report: main-world meshes {k}: {n}, {v} vertices");
                }
                let Some(device) = device else { return };
                let Some(r) = device.wgpu_device().generate_allocator_report() else {
                    info!("gpu report: the backend gives no allocator report");
                    return;
                };
                // Labels with a number in them (one per mesh slab, texture, view) summed by
                // the label without its digits.
                let mut by: std::collections::HashMap<String, (u64, u32)> = std::collections::HashMap::new();
                for a in &r.allocations {
                    let key: String = a.name.chars().filter(|c| !c.is_ascii_digit()).take(80).collect();
                    let e = by.entry(key).or_default();
                    e.0 += a.size;
                    e.1 += 1;
                }
                let mut rows: Vec<_> = by.into_iter().collect();
                rows.sort_by_key(|r| std::cmp::Reverse(r.1 .0));
                info!("gpu report: {} allocations {:.0} MiB in {} blocks {:.0} MiB", r.allocations.len(), r.total_allocated_bytes as f64 / 1048576.0, r.blocks.len(), r.total_reserved_bytes as f64 / 1048576.0);
                for (name, (bytes, n)) in rows.iter().take(30) {
                    info!("gpu report: {:>8.1} MiB {:>6} x {}", *bytes as f64 / 1048576.0, n, if name.is_empty() { "(no label)" } else { name });
                }
            });
        }
    }
}
