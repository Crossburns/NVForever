//! The machine's memory budgets and the asset caches' keep limits that follow them (#1170).
//!
//! At the start the game reads the machine's RAM (`GlobalMemoryStatusEx`) and the GPU's
//! memory budget, and logs both (`memory budget: RAM … MB, GPU budget … MB (…)`, or that the
//! GPU budget could not be read). The GPU budget is `VK_EXT_memory_budget`'s, read through
//! wgpu's Vulkan device (`Device::as_hal`): wgpu-hal 30 enables that extension whenever the
//! adapter offers it (except on Mesa's RADV) [documented: wgpu-hal 30.0.1
//! `vulkan/adapter.rs`, "Optional `VK_EXT_memory_budget`"], so no wgpu patch is needed. Of
//! the device-local heaps ([`device_local`]) it sums `heapBudget`, "an estimate of how much
//! memory the process can allocate from that heap before allocations may fail or cause
//! performance degradation", and `heapUsage`, "the estimated heap usage of the process"
//! [documented: Vulkan `VkPhysicalDeviceMemoryBudgetPropertiesEXT`]. Budget and use are the
//! same memory, measured by the driver: what the process holds of it, blocks the allocator
//! keeps empty included (#898). An integrated GPU's memory is the machine's RAM: it has no
//! GPU budget here, and the RAM check covers it.
//!
//! **Keep limits** ([`nvf_assets::KeepLimits`], read by the release sweep each 2 s) scale
//! down from five minutes and 2048 unused entries per cache on a machine with less than
//! [`FULL_RAM_MB`] of RAM or [`FULL_VRAM_MB`] of GPU budget, by the smaller share ([`limits_for`]).
//! **Pressed** ([`pressed`], checked every 2 s, the GPU budget read again each time so other
//! programs' use counts): when the process uses over 85 % of the GPU budget, or the machine
//! has under 10 % of its RAM free, the sweep keeps [`PRESSED`] (unused entries go after
//! 30 s, at most 128 per cache, the longest unused first) until the GPU use is under 75 % and
//! over 20 % of the RAM is free. Each change is logged.
//!
//! Test aids, since there is no small machine to test on: `NVF_PLAY_TEST_VRAM_MB=<MB>` and
//! `NVF_PLAY_TEST_RAM_MB=<MB>` take the GPU budget and the RAM as at most that (the RAM's
//! free part shrinks by what was taken off).

use bevy::prelude::*;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use nvf_assets::KeepLimits;

/// RAM at which the caches keep their full limits [design: the stress tour's process
/// stays under about 5 GB (owner 2026-10-22: about 4.7 GB used); 16 GB leaves the rest of
/// the machine room].
pub const FULL_RAM_MB: u64 = 16 * 1024;
/// GPU budget at which the caches keep their full limits [design: an 8 GB card; the tour's
/// GPU allocations peak around 3-4 GB].
pub const FULL_VRAM_MB: u64 = 8 * 1024;
/// A share of the GPU budget used: over `.0` presses the caches, under `.1` lets go [design].
pub const GPU_PRESSURE: (f64, f64) = (0.85, 0.75);
/// A share of the RAM in use by the whole machine: over `.0` (under 10 % free) presses the
/// caches, under `.1` lets go [design: what the machine has free is what paging follows;
/// the game's own commit (about 5 GB in a soak) on an 8 GB machine leaves about a quarter
/// free, which is not pressed].
pub const RAM_PRESSURE: (f64, f64) = (0.90, 0.80);
/// A share of the machine's commit limit (RAM plus page file) in use: over `.0` presses the
/// caches, under `.1` lets go [design: an allocation fails when commit runs out, whatever
/// RAM is free; the process's VRAM counts against it too (docs/handoff/memory-runs.md); on
/// the owner's 45 GB limit 0.85 is under 6.8 GB free, where a long session with Remix hung,
/// observed 2026-11-09/10].
pub const COMMIT_PRESSURE: (f64, f64) = (0.85, 0.75);
/// The keep limits while pressed: the least the budgets scale to [design: 10 s and 32
/// entries were tried first; on a tour slice capped at a 2 GB card they pressed at 77 s
/// and never let go (the allocator keeps emptied blocks, #898, so dropping assets does not
/// lower the process's use), and re-preparing what was dropped took the 1 % low from 35 to
/// 95 ms; streaming-hitches.md §5].
pub const PRESSED: KeepLimits = MIN_LIMITS;
/// The least the scaled limits go to [design].
const MIN_LIMITS: KeepLimits = KeepLimits { secs: 30.0, max: 128 };

/// What was read at the start (MB, test aids applied), and whether the caches are pressed.
#[derive(Resource, Clone, Debug, Default)]
pub struct MemoryBudget {
    /// The GPU budget, `None` when not read (not Vulkan, no extension, an integrated GPU).
    pub vram_mb: Option<u64>,
    /// The test aids' caps (MB), applied again at each check.
    aid_vram: Option<u64>,
    aid_ram: Option<u64>,
    /// The limits the budgets give, when not pressed.
    pub base: KeepLimits,
    pub pressed: bool,
}

pub struct MemoryBudgetPlugin;

impl Plugin for MemoryBudgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MemoryBudget>().add_systems(Startup, read_budgets).add_systems(Update, watch_use);
    }
}

/// The keep limits for `ram_mb` of RAM and `vram_mb` of GPU budget: full at
/// [`FULL_RAM_MB`] / [`FULL_VRAM_MB`] and over, else by the smaller share, no lower than
/// 30 s and 128 entries. An unknown budget does not count.
pub fn limits_for(ram_mb: Option<u64>, vram_mb: Option<u64>) -> KeepLimits {
    let share = |have: Option<u64>, full: u64| have.map_or(1.0, |h| (h as f64 / full as f64).min(1.0));
    let s = share(ram_mb, FULL_RAM_MB).min(share(vram_mb, FULL_VRAM_MB));
    let full = KeepLimits::default();
    KeepLimits { secs: (full.secs * s).round().max(MIN_LIMITS.secs), max: ((full.max as f64 * s).round() as usize).max(MIN_LIMITS.max) }
}

/// Whether the caches are pressed now, from each budget's share used with its
/// (press, let go) thresholds: over a press threshold presses them, under every let-go
/// threshold lets go, between keeps `was`.
pub fn pressed(was: bool, shares: &[(f64, (f64, f64))]) -> bool {
    if shares.iter().any(|&(s, (on, _))| s > on) {
        true
    } else if shares.iter().all(|&(s, (_, off))| s < off) {
        false
    } else {
        was
    }
}

/// One memory heap from `VK_EXT_memory_budget`: device-local or not, its budget and this
/// process's use, bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heap {
    pub device_local: bool,
    pub budget: u64,
    pub usage: u64,
}

/// The GPU's own memory, (budget, this process's use) in bytes: the device-local heaps'
/// sums. `None` for an integrated GPU (its "device-local" heap is a carve-out of the RAM,
/// and what spills goes to the RAM, which [`RAM_PRESSURE`] watches) and without a
/// device-local budget.
pub fn device_local(heaps: &[Heap], integrated: bool) -> Option<(u64, u64)> {
    if integrated {
        return None;
    }
    let local = heaps.iter().filter(|h| h.device_local);
    let (budget, usage) = local.fold((0, 0), |(b, u), h| (b + h.budget, u + h.usage));
    (budget > 0).then_some((budget, usage))
}

/// A test aid's MB (`NVF_PLAY_TEST_VRAM_MB` / `_RAM_MB`), when set and a number.
fn aid_mb(name: &str) -> Option<u64> {
    let v = std::env::var(name).ok()?;
    match v.trim().parse::<u64>() {
        Ok(mb) if mb > 0 => {
            info!("test aid {name}: {mb} MB");
            Some(mb)
        }
        _ => {
            warn!("{name}={v}: expected megabytes, a whole number over 0");
            None
        }
    }
}

/// The lower of what was read and what a test aid allows.
fn capped(read: Option<u64>, aid: Option<u64>) -> Option<u64> {
    match (read, aid) {
        (Some(r), Some(a)) => Some(r.min(a)),
        (r, a) => r.or(a),
    }
}

const MB: u64 = 1024 * 1024;

/// The machine's RAM in MB: (total, free now).
#[cfg(windows)]
fn read_ram() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..unsafe { std::mem::zeroed() } };
    // SAFETY: `m` is a MEMORYSTATUSEX with its length set, as the call asks.
    (unsafe { GlobalMemoryStatusEx(&mut m) } != 0).then_some((m.ullTotalPhys / MB, m.ullAvailPhys / MB))
}

#[cfg(not(windows))]
fn read_ram() -> Option<(u64, u64)> {
    None
}

/// The machine's commit limit and what of it is free, MB.
#[cfg(windows)]
fn read_commit() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..unsafe { std::mem::zeroed() } };
    // SAFETY: `m` is a MEMORYSTATUSEX with its length set, as the call asks.
    (unsafe { GlobalMemoryStatusEx(&mut m) } != 0 && m.ullTotalPageFile > 0).then_some((m.ullTotalPageFile / MB, m.ullAvailPageFile / MB))
}

#[cfg(not(windows))]
fn read_commit() -> Option<(u64, u64)> {
    None
}

/// The RAM's share in use, a test aid's cap taken as RAM the machine does not have.
fn ram_share(ram: (u64, u64), aid: Option<u64>) -> f64 {
    let (total, free) = ram;
    let cap = aid.map_or(total, |a| a.min(total));
    let free = free.saturating_sub(total - cap);
    1.0 - free as f64 / cap.max(1) as f64
}

/// The GPU's heaps from `VK_EXT_memory_budget` and whether it is integrated, or why not.
fn read_heaps(device: &RenderDevice) -> Result<Vec<Heap>, String> {
    use ash::vk;
    let wgpu_device = device.wgpu_device();
    // SAFETY: only reads through the hal device, which stays alive in the guard.
    let Some(hal) = (unsafe { wgpu_device.as_hal::<wgpu::hal::api::Vulkan>() }) else {
        return Err("not the Vulkan backend".into());
    };
    if !hal.enabled_device_extensions().contains(&ash::ext::memory_budget::NAME) {
        return Err("the device has no VK_EXT_memory_budget".into());
    }
    let instance = hal.shared_instance().raw_instance();
    let mut budget = vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
    let mut props = vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
    // SAFETY: the physical device belongs to this instance; the budget struct is chained
    // as the extension (enabled, checked above) asks. Vulkan 1.1 core, which wgpu requires.
    unsafe { instance.get_physical_device_memory_properties2(hal.raw_physical_device(), &mut props) };
    let n = props.memory_properties.memory_heap_count as usize;
    let heaps = props.memory_properties.memory_heaps[..n].iter().enumerate();
    Ok(heaps.map(|(i, h)| Heap { device_local: h.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL), budget: budget.heap_budget[i], usage: budget.heap_usage[i] }).collect())
}

/// The GPU budget and this process's use of it, MB, or why not.
fn read_vram(device: Option<&RenderDevice>, integrated: bool) -> Result<(u64, u64), String> {
    let heaps = read_heaps(device.ok_or("no render device")?)?;
    if integrated {
        return Err("an integrated GPU: its memory is the RAM".into());
    }
    device_local(&heaps, false).map(|(b, u)| (b / MB, u / MB)).ok_or_else(|| "no device-local heap budget".into())
}

fn integrated(adapter: Option<&RenderAdapterInfo>) -> bool {
    adapter.is_some_and(|a| a.device_type == wgpu::DeviceType::IntegratedGpu)
}

/// Read the budgets, log them and set the keep limits.
fn read_budgets(mut commands: Commands, device: Option<Res<RenderDevice>>, adapter: Option<Res<RenderAdapterInfo>>, mut budget: ResMut<MemoryBudget>) {
    let ram = read_ram();
    let aid_ram = aid_mb("NVF_PLAY_TEST_RAM_MB");
    let ram_mb = capped(ram.map(|r| r.0), aid_ram);
    let vram = read_vram(device.as_deref(), integrated(adapter.as_deref()));
    let aid_vram = aid_mb("NVF_PLAY_TEST_VRAM_MB");
    let vram_mb = match &vram {
        Ok((b, _)) => capped(Some(*b), aid_vram),
        Err(_) => aid_vram,
    };
    let source = match &vram {
        Ok((_, used)) => format!("VK_EXT_memory_budget, device-local heaps; this process uses {used} MB"),
        Err(e) => format!("not read: {e}"),
    };
    let base = limits_for(ram_mb, vram_mb);
    let ram_text = match (ram, ram_mb) {
        (Some((t, free)), Some(mb)) if mb < t => format!("RAM {mb} MB (test aid; the machine has {t} MB, {free} free)"),
        (Some((t, free)), _) => format!("RAM {t} MB ({free} free)"),
        (None, Some(mb)) => format!("RAM {mb} MB (test aid; the machine's not read)"),
        (None, None) => "RAM not read".to_string(),
    };
    let vram_text = match (&vram, vram_mb) {
        (Ok((b, _)), Some(mb)) if mb < *b => format!("GPU budget {mb} MB (test aid; the device's {b} MB, {source})"),
        (_, Some(mb)) => format!("GPU budget {mb} MB ({source})"),
        (_, None) => format!("GPU budget {source}"),
    };
    info!("memory budget: {ram_text}, {vram_text}; caches keep unused models and textures {:.0} s, at most {} per cache", base.secs, base.max);
    *budget = MemoryBudget { vram_mb, aid_vram, aid_ram, base, pressed: false };
    commands.insert_resource(base);
}

/// Every 2 s: this process's use of the GPU budget (both read again: other programs change
/// the budget) and the machine's free RAM; press the caches near either.
fn watch_use(
    real: Res<Time<Real>>,
    device: Option<Res<RenderDevice>>,
    adapter: Option<Res<RenderAdapterInfo>>,
    mut budget: ResMut<MemoryBudget>,
    mut limits: ResMut<KeepLimits>,
    mut last: Local<f64>,
) {
    let now = real.elapsed_secs_f64();
    if now - *last < f64::from(nvf_assets::release::SWEEP_SECS) {
        return;
    }
    *last = now;
    let gpu = budget.vram_mb.and_then(|_| read_vram(device.as_deref(), integrated(adapter.as_deref())).ok()).map(|(b, used)| (capped(Some(b), budget.aid_vram).unwrap_or(b), used));
    let ram = read_ram();
    let mut shares = Vec::new();
    if let Some((b, used)) = gpu {
        shares.push((used as f64 / b.max(1) as f64, GPU_PRESSURE));
    }
    if let Some(r) = ram {
        shares.push((ram_share(r, budget.aid_ram), RAM_PRESSURE));
    }
    let commit = read_commit();
    if let Some((total, free)) = commit {
        shares.push((1.0 - free as f64 / total.max(1) as f64, COMMIT_PRESSURE));
    }
    let now_pressed = pressed(budget.pressed, &shares);
    if now_pressed == budget.pressed {
        return;
    }
    budget.pressed = now_pressed;
    *limits = if now_pressed { PRESSED } else { budget.base };
    let gpu_text = gpu.map_or("?".to_string(), |(b, used)| format!("{used} of {b} MB"));
    let ram_text = ram.map_or("?".to_string(), |r| format!("{:.0} % in use", ram_share(r, budget.aid_ram) * 100.0));
    let commit_text = commit.map_or("?".to_string(), |(t, f)| format!("{f} of {t} MB free"));
    info!(
        "memory budget: {} (GPU: this process uses {gpu_text}; RAM: {ram_text}; commit: {commit_text}); caches keep unused {:.0} s, at most {} per cache",
        if now_pressed { "near a budget, caches pressed" } else { "back under the budgets" },
        limits.secs,
        limits.max
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_limits_scale_with_the_smaller_budget_share() {
        let full = KeepLimits::default();
        assert_eq!(limits_for(Some(48 * 1024), Some(12 * 1024)), full, "the owner's machine");
        assert_eq!(limits_for(None, None), full, "nothing read: as before");
        assert_eq!(limits_for(Some(8 * 1024), None), KeepLimits { secs: 150.0, max: 1024 }, "8 GB of RAM: half");
        assert_eq!(limits_for(Some(32 * 1024), Some(4 * 1024)), KeepLimits { secs: 150.0, max: 1024 }, "a 4 GB card: half");
        assert_eq!(limits_for(Some(8 * 1024), Some(2 * 1024)), KeepLimits { secs: 75.0, max: 512 }, "the smaller share");
        assert_eq!(limits_for(Some(1024), Some(256)), MIN_LIMITS, "never under the least");
    }

    #[test]
    fn caches_are_pressed_near_a_budget_and_let_go_well_under() {
        assert!(!pressed(false, &[]));
        assert!(pressed(false, &[(0.5, GPU_PRESSURE), (0.9, GPU_PRESSURE)]), "one budget over 85 %");
        assert!(pressed(true, &[(0.8, GPU_PRESSURE), (0.2, RAM_PRESSURE)]), "between: stays pressed");
        assert!(!pressed(false, &[(0.8, GPU_PRESSURE)]), "between: stays let go");
        assert!(!pressed(true, &[(0.7, GPU_PRESSURE), (0.74, RAM_PRESSURE)]), "every budget under its let-go share");
        assert!(!pressed(false, &[(0.88, RAM_PRESSURE)]), "RAM 88 % in use: not yet");
        assert!(pressed(false, &[(0.92, RAM_PRESSURE)]), "under 10 % of the RAM free");
    }

    /// The GPU budget is the device-local heaps' only, and the use compared with it is
    /// those heaps' too; an integrated GPU, or none device-local, has none.
    #[test]
    fn the_gpu_budget_is_the_device_local_heaps() {
        const G: u64 = 1 << 30;
        // A discrete card: 12 GB of its own, 32 GB of the machine's seen through it.
        let discrete = [Heap { device_local: true, budget: 11 * G, usage: 3 * G }, Heap { device_local: false, budget: 30 * G, usage: 5 * G }];
        assert_eq!(device_local(&discrete, false), Some((11 * G, 3 * G)), "the host heap's use does not count against the card");
        // A card with a second, small device-local heap (the host-visible BAR window).
        let bar = [Heap { device_local: true, budget: 7 * G, usage: G }, Heap { device_local: true, budget: G / 4, usage: G / 8 }, Heap { device_local: false, budget: 16 * G, usage: G }];
        assert_eq!(device_local(&bar, false), Some((7 * G + G / 4, G + G / 8)));
        // An APU: a 512 MB carve-out marked device-local, the rest in the RAM's heap.
        let apu = [Heap { device_local: true, budget: G / 2, usage: G / 2 }, Heap { device_local: false, budget: 14 * G, usage: 2 * G }];
        assert_eq!(device_local(&apu, true), None, "an integrated GPU: the RAM check covers it");
        // No device-local budget reported.
        assert_eq!(device_local(&[Heap { device_local: false, budget: 8 * G, usage: G }], false), None);
        assert_eq!(device_local(&[], false), None, "no extension: no heaps");
    }

    #[test]
    fn the_ram_aid_takes_ram_away_from_whats_free() {
        // 48 GB, 32 GB free: 33 % in use.
        assert!((ram_share((48_000, 32_000), None) - 1.0 / 3.0).abs() < 1e-9);
        // Capped at 6 GB: the 16 GB in use leave nothing free.
        assert_eq!(ram_share((48_000, 32_000), Some(6_000)), 1.0);
        // Capped at 24 GB: 8 of them free.
        assert!((ram_share((48_000, 32_000), Some(24_000)) - (1.0 - 8.0 / 24.0)).abs() < 1e-9);
    }

    #[test]
    fn a_test_aid_only_lowers_what_was_read() {
        assert_eq!(capped(Some(12000), Some(2048)), Some(2048));
        assert_eq!(capped(Some(1000), Some(2048)), Some(1000));
        assert_eq!(capped(None, Some(2048)), Some(2048));
        assert_eq!(capped(Some(12000), None), Some(12000));
    }
}
