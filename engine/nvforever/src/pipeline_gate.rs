//! What a loading screen needs from the render world: how many pipelines the destination
//! is still waiting for (#1520).
//!
//! Bevy compiles pipelines in the background and skips a draw whose pipeline is not ready,
//! pass by pass [documented: bevy_render 0.20.1 `render_phase/mod.rs`, `SetItemPipeline`].
//! A screen that came down with them still compiling left the world's first seconds to the
//! clear colour: a flat view, then everything at once [observed: Goodsprings start on main
//! e2517687, about 120 pipelines waiting when the screen came down, the view flat for about
//! 2 s after it].
//!
//! [`count`] runs at the end of each render frame and reads each pipeline's state rather
//! than `PipelineCache::waiting_pipelines`, which cannot tell why one is waiting [design:
//! bevy_render 0.20.1 `pipeline_cache.rs`, `process_pipeline`]. A pipeline counts as busy
//! while it is queued or compiling, and after the two errors Bevy retries (a shader or an
//! import not loaded yet). An error Bevy does not retry is logged once with the pipeline's
//! label and is not waited for; a draw that needs it stays skipped, as before. So is one
//! still being retried [`PIPELINE_CAP_SECS`] after it was asked for, a shader that never
//! loads for instance, so that it does not hold every screen after it to the screen's own
//! cap. One merely slow to compile is still waited for, up to that cap.
//!
//! The render world answers through [`PipelineGate`], shared with the main world, so what
//! the main world reads may be a frame or two old; a loading screen's settle time
//! (`nvf_play::loading::SETTLE_SECS`) covers that, and pipelines asked for late.
//!
//! Test aid `NVF_PLAY_TEST_PIPELINE_LOG=1`: one log line for each pipeline when it is asked
//! for and when it is ready or has failed for good, with its label and the time between.

use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache, PipelineDescriptor};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::shader::ShaderCacheError;
use nvf_play::loading::PIPELINE_CAP_SECS;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Instant;

#[derive(Default)]
struct Shared {
    busy: AtomicU32,
    screen_busy: AtomicU32,
    frames: AtomicU32,
    list_next: AtomicBool,
}

/// The render world's answer. A clone lives in each world.
#[derive(Resource, Clone, Default)]
pub struct PipelineGate(Arc<Shared>);

impl PipelineGate {
    /// Pipelines still to be ready, by the last render frame's count.
    pub fn busy(&self) -> u32 {
        self.0.busy.load(Relaxed)
    }

    /// Of those, the ones a loading screen itself is drawn with ([`draws_the_screen`]).
    pub fn screen_busy(&self) -> u32 {
        self.0.screen_busy.load(Relaxed)
    }

    /// Render frames finished so far. Before the first, nothing has asked for a pipeline.
    pub fn frames(&self) -> u32 {
        self.0.frames.load(Relaxed)
    }

    /// Have the next render frame log the labels of those still busy.
    pub fn list_busy(&self) {
        self.0.list_next.store(true, Relaxed);
    }
}

pub struct PipelineGatePlugin;

impl Plugin for PipelineGatePlugin {
    fn build(&self, app: &mut App) {
        let gate = PipelineGate::default();
        app.insert_resource(gate.clone());
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.insert_resource(gate).add_systems(Render, count.in_set(RenderSystems::Cleanup));
            if std::env::var_os("NVF_PLAY_TEST_PIPELINE_LOG").is_some() {
                info!("test aid NVF_PLAY_TEST_PIPELINE_LOG: each pipeline logged");
                render.add_systems(Render, log_each.in_set(RenderSystems::Cleanup));
            }
        }
    }
}

/// A pipeline the screen waits for: compiling, or to be tried again.
fn waited_for(state: &CachedPipelineState) -> bool {
    match state {
        CachedPipelineState::Queued | CachedPipelineState::Creating(_) => true,
        CachedPipelineState::Ok(_) => false,
        CachedPipelineState::Err(e) => retried(e),
    }
}

/// The errors Bevy tries again [design: bevy_render 0.20.1 `process_pipeline`].
fn retried(e: &ShaderCacheError) -> bool {
    matches!(e, ShaderCacheError::ShaderNotLoaded(_) | ShaderCacheError::ShaderImportNotYetAvailable)
}

/// Pipelines a loading screen needs to be seen: the interface's, and the blit that puts the
/// view on the window, which the first frame blocks on [design: bevy_ui_render and
/// bevy_core_pipeline 0.20.1 labels].
fn draws_the_screen(label: &str) -> bool {
    matches!(label, "ui_pipeline" | "blit pipeline")
}

fn label(d: &PipelineDescriptor) -> &str {
    match d {
        PipelineDescriptor::RenderPipelineDescriptor(d) => d.label.as_deref(),
        PipelineDescriptor::ComputePipelineDescriptor(d) => d.label.as_deref(),
        PipelineDescriptor::MeshPipelineDescriptor(d) => d.label.as_deref(),
    }
    .unwrap_or("(no label)")
}

fn count(gate: Res<PipelineGate>, cache: Res<PipelineCache>, mut failed: Local<HashSet<usize>>, mut asked: Local<Vec<Instant>>) {
    let now = Instant::now();
    let (mut busy, mut screen_busy) = (0, 0);
    for (i, p) in cache.pipelines().enumerate() {
        if i >= asked.len() {
            asked.push(now);
        }
        if waited_for(&p.state) {
            if failed.contains(&i) {
                continue;
            }
            if matches!(p.state, CachedPipelineState::Err(_)) && now.duration_since(asked[i]).as_secs_f64() >= PIPELINE_CAP_SECS {
                failed.insert(i);
                warn!("pipeline {i} still being retried after {PIPELINE_CAP_SECS:.0} s and no longer waited for: {}", label(&p.descriptor));
                continue;
            }
            busy += 1;
            if draws_the_screen(label(&p.descriptor)) {
                screen_busy += 1;
            }
        } else if let CachedPipelineState::Err(e) = &p.state {
            if failed.insert(i) {
                warn!("pipeline {i} failed and will not be tried again: {}: {e}", label(&p.descriptor));
            }
        }
    }
    gate.0.busy.store(busy, Relaxed);
    gate.0.screen_busy.store(screen_busy, Relaxed);
    gate.0.frames.fetch_add(1, Relaxed);
    if gate.0.list_next.swap(false, Relaxed) {
        let waiting = cache.pipelines().enumerate().filter(|(i, p)| waited_for(&p.state) && !failed.contains(i));
        let mut labels: Vec<&str> = waiting.map(|(_, p)| label(&p.descriptor)).collect();
        labels.sort_unstable();
        labels.dedup();
        warn!("pipelines still waited for ({busy}): {}", labels.join(", "));
    }
}

/// `NVF_PLAY_TEST_PIPELINE_LOG` (module notes).
fn log_each(cache: Res<PipelineCache>, mut seen: Local<Vec<(Instant, bool)>>) {
    let now = Instant::now();
    for (i, p) in cache.pipelines().enumerate() {
        if i >= seen.len() {
            seen.push((now, false));
            info!("pipeline {i} asked: {}", label(&p.descriptor));
        }
        if !seen[i].1 && !waited_for(&p.state) {
            seen[i].1 = true;
            let how = if matches!(p.state, CachedPipelineState::Ok(_)) { "ready" } else { "failed" };
            info!("pipeline {i} {how} after {:.0} ms: {}", now.duration_since(seen[i].0).as_secs_f64() * 1000.0, label(&p.descriptor));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_what_bevy_will_still_finish_and_not_for_a_final_failure() {
        assert!(waited_for(&CachedPipelineState::Queued));
        assert!(waited_for(&CachedPipelineState::Err(ShaderCacheError::ShaderImportNotYetAvailable)));
        assert!(waited_for(&CachedPipelineState::Err(ShaderCacheError::ShaderNotLoaded(AssetId::default()))));
        assert!(!waited_for(&CachedPipelineState::Err(ShaderCacheError::ProcessShaderError("bad".into()))));
        assert!(!waited_for(&CachedPipelineState::Err(ShaderCacheError::CreateShaderModule("bad".into()))));
    }

    #[test]
    fn the_screen_needs_only_the_interface_and_the_blit() {
        assert!(draws_the_screen("ui_pipeline"));
        assert!(draws_the_screen("blit pipeline"));
        assert!(!draws_the_screen("pbr_opaque_mesh_pipeline"));
        assert!(!draws_the_screen("post_pipeline"));
    }
}
