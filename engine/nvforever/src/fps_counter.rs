//! An on-screen frame rate counter in the top-right corner: frames a second, the mean
//! frame time and the slowest frame of the last half second, then the machine's CPU and
//! GPU use (`usage`, issue #1704). A settings-page switch
//! (`settings.rs`, kept in `NVForever.ini`) and `--fps`; off by default. Not in the
//! original [engineering choice, issue #1087].
//!
//! The times are wall-clock (`Time<Real>`), so a paused game or a slowed clock still
//! reads the frame rate the player sees. The text changes twice a second so it can be
//! read; a number that changed every frame would only flicker.

use std::sync::Arc;

use bevy::prelude::*;

/// How often the readout changes, in seconds.
const WINDOW: f32 = 0.5;

/// Above every menu, the HUD (`nvf_play::interface::TOP_MENU_Z` is 107) and the loading
/// screen (200, `nvf_play::loading`), so a load's frame rate can be read too.
const Z: i32 = 201;

/// The text until the first reading of a showing.
const WAITING: &str = "-- FPS";

/// Whether the counter is shown.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct FpsCounter {
    pub show: bool,
}

#[derive(Component)]
struct FpsText;

/// This window's frame times in seconds, and whether a reading was logged yet.
#[derive(Default)]
struct Window {
    times: Vec<f32>,
    logged: bool,
    /// The CPU/GPU sampler, started the first time the counter shows.
    usage: Option<Arc<usage::Shared>>,
    usage_logged: bool,
}

pub struct FpsCounterPlugin(pub bool);

impl Plugin for FpsCounterPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FpsCounter { show: self.0 }).add_systems(Startup, spawn).add_systems(Update, (show, count).chain());
    }
}

fn spawn(mut commands: Commands, counter: Res<FpsCounter>) {
    commands.spawn((
        Text::new(WAITING),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(Color::srgb(1.0, 1.0, 0.6)),
        Node { position_type: PositionType::Absolute, top: Val::Px(6.0), right: Val::Px(8.0), padding: UiRect::axes(Val::Px(5.0), Val::Px(2.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        GlobalZIndex(Z),
        if counter.show { Visibility::Inherited } else { Visibility::Hidden },
        FpsText,
    ));
}

/// Shows or hides the text when the switch changes; a new showing starts from the
/// waiting text, not the last showing's reading.
fn show(counter: Res<FpsCounter>, mut text: Query<(&mut Visibility, &mut Text), With<FpsText>>) {
    if !counter.is_changed() {
        return;
    }
    if !counter.is_added() {
        info!("fps counter: {}", if counter.show { "on" } else { "off" });
    }
    for (mut v, mut t) in &mut text {
        *v = if counter.show { Visibility::Inherited } else { Visibility::Hidden };
        if counter.show {
            t.0 = WAITING.to_string();
        }
    }
}

fn count(counter: Res<FpsCounter>, time: Res<Time<Real>>, mut text: Query<&mut Text, With<FpsText>>, mut w: Local<Window>) {
    if !counter.show {
        w.times.clear();
        if let Some(u) = &w.usage {
            u.set_on(false);
        }
        return;
    }
    let u = w.usage.get_or_insert_with(usage::start);
    u.set_on(true);
    let (cpu, gpu) = u.read();
    let dt = time.delta_secs();
    if dt > 0.0 {
        w.times.push(dt);
    }
    let Some(r) = reading(&w.times, WINDOW) else { return };
    w.times.clear();
    if !w.logged {
        w.logged = true;
        info!("fps counter: first reading {:.0} fps, {:.1} ms, slowest {:.1} ms", r.fps, r.mean_ms, r.worst_ms);
    }
    if !w.usage_logged && (cpu.is_some() || gpu.is_some()) {
        w.usage_logged = true;
        info!("fps counter: first usage reading{}", usage_text(cpu, gpu));
    }
    for mut t in &mut text {
        t.0 = format!("{:.0} FPS  {:.1} ms  (slowest {:.1}){}", r.fps, r.mean_ms, r.worst_ms, usage_text(cpu, gpu));
    }
}

#[derive(Debug, PartialEq)]
struct Reading {
    fps: f32,
    mean_ms: f32,
    worst_ms: f32,
}

/// The reading of frame times (seconds) once they cover `window` seconds, else none.
fn reading(times: &[f32], window: f32) -> Option<Reading> {
    let total: f32 = times.iter().sum();
    if times.is_empty() || total < window {
        return None;
    }
    let worst = times.iter().copied().fold(0.0, f32::max);
    Some(Reading { fps: times.len() as f32 / total, mean_ms: total * 1e3 / times.len() as f32, worst_ms: worst * 1e3 })
}

/// `  CPU 34%  GPU 71%`, each part only when it was read.
fn usage_text(cpu: Option<f32>, gpu: Option<f32>) -> String {
    let mut s = String::new();
    if let Some(c) = cpu {
        s += &format!("  CPU {c:.0}%");
    }
    if let Some(g) = gpu {
        s += &format!("  GPU {g:.0}%");
    }
    s
}

/// The machine's CPU and GPU use, read from Windows' performance counters (PDH) on a
/// thread of its own once a second while the counter shows, so no frame waits for a
/// collection (the GPU counters have one instance per process and engine, and reading
/// them is not free).
///
/// - CPU: `\Processor Information(_Total)\% Processor Utility`, the figure Task Manager
///   shows; `\Processor(_Total)\% Processor Time` where that counter is missing.
/// - GPU: `\GPU Engine(*)\Utilization Percentage`; the instances of one engine are summed
///   over processes and the busiest engine is the reading, as Task Manager does.
///
/// Off Windows, or where a counter cannot be opened, that part reads none.
mod usage {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    /// What the sampler stores for "no reading".
    const NONE: u32 = u32::MAX;

    /// Shared between the game and the sampler thread; percentages are kept in tenths.
    pub struct Shared {
        on: AtomicBool,
        cpu: AtomicU32,
        gpu: AtomicU32,
    }

    impl Shared {
        pub fn set_on(&self, on: bool) {
            if self.on.swap(on, Ordering::Relaxed) && !on {
                // A new showing waits for fresh readings, not the last showing's.
                self.cpu.store(NONE, Ordering::Relaxed);
                self.gpu.store(NONE, Ordering::Relaxed);
            }
        }

        pub fn read(&self) -> (Option<f32>, Option<f32>) {
            let get = |a: &AtomicU32| match a.load(Ordering::Relaxed) {
                NONE => None,
                v => Some(v as f32 / 10.0),
            };
            (get(&self.cpu), get(&self.gpu))
        }

        #[cfg_attr(not(windows), allow(dead_code))]
        fn store(&self, cpu: Option<f32>, gpu: Option<f32>) {
            let put = |v: Option<f32>| v.map_or(NONE, |p| (p.clamp(0.0, 100.0) * 10.0).round() as u32);
            self.cpu.store(put(cpu), Ordering::Relaxed);
            self.gpu.store(put(gpu), Ordering::Relaxed);
        }
    }

    /// Starts the sampler thread (none off Windows) and returns what it fills in.
    pub fn start() -> Arc<Shared> {
        let shared = Arc::new(Shared { on: AtomicBool::new(true), cpu: AtomicU32::new(NONE), gpu: AtomicU32::new(NONE) });
        #[cfg(windows)]
        {
            let s = shared.clone();
            if let Err(e) = std::thread::Builder::new().name("fps-usage".into()).spawn(move || pdh::run(&s)) {
                bevy::log::warn!("fps counter: no CPU/GPU usage thread: {e}");
            }
        }
        shared
    }

    /// The busiest engine's use from `\GPU Engine(*)` instances (name, percent). An
    /// instance is named `pid_<n>_luid_<adapter>_phys_<n>_eng_<n>_engtype_<kind>`
    /// [observed]; everything from `luid_` on names the engine.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn busiest_engine<'a>(items: impl IntoIterator<Item = (&'a str, f64)>) -> Option<f32> {
        let mut per: HashMap<&str, f64> = HashMap::new();
        for (name, v) in items {
            let engine = name.find("luid_").map_or(name, |i| &name[i..]);
            *per.entry(engine).or_default() += v;
        }
        per.into_values().reduce(f64::max).map(|v| v.clamp(0.0, 100.0) as f32)
    }

    #[cfg(windows)]
    mod pdh {
        use std::ptr::{null, null_mut};
        use std::sync::atomic::Ordering;
        use std::time::Duration;

        use windows_sys::Win32::System::Performance::{
            PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW,
            PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
        };

        use super::{Shared, busiest_engine};

        const PERIOD: Duration = Duration::from_secs(1);

        pub fn run(shared: &Shared) {
            let Some(mut q) = Query::open() else {
                bevy::log::warn!("fps counter: no performance counters, CPU/GPU usage not shown");
                return;
            };
            bevy::log::info!("fps counter: usage counters cpu {} gpu {}", q.cpu.is_some(), q.gpu.is_some());
            let mut primed = false;
            loop {
                if !shared.on.load(Ordering::Relaxed) {
                    // Rates are differences between two collections; after a pause the
                    // first collection only starts a fresh pair.
                    primed = false;
                    std::thread::sleep(PERIOD / 4);
                    continue;
                }
                let ok = q.collect();
                if ok && primed && shared.on.load(Ordering::Relaxed) {
                    shared.store(q.cpu(), q.gpu());
                }
                primed = ok;
                std::thread::sleep(PERIOD);
            }
        }

        /// An open PDH query, used only on the sampler thread.
        struct Query {
            q: PDH_HQUERY,
            cpu: Option<PDH_HCOUNTER>,
            gpu: Option<PDH_HCOUNTER>,
        }

        impl Query {
            fn open() -> Option<Query> {
                let mut q = null_mut();
                // SAFETY: a null data source is the live machine; `q` receives the handle.
                if unsafe { PdhOpenQueryW(null(), 0, &mut q) } != 0 {
                    return None;
                }
                let add = |path: &str| {
                    let w: Vec<u16> = path.encode_utf16().chain([0]).collect();
                    let mut c = null_mut();
                    // SAFETY: `q` is open and `w` is a NUL-terminated wide string.
                    (unsafe { PdhAddEnglishCounterW(q, w.as_ptr(), 0, &mut c) } == 0).then_some(c)
                };
                let cpu = add("\\Processor Information(_Total)\\% Processor Utility").or_else(|| add("\\Processor(_Total)\\% Processor Time"));
                let gpu = add("\\GPU Engine(*)\\Utilization Percentage");
                if cpu.is_none() && gpu.is_none() {
                    // SAFETY: `q` is open and closed once.
                    unsafe { PdhCloseQuery(q) };
                    return None;
                }
                Some(Query { q, cpu, gpu })
            }

            fn collect(&mut self) -> bool {
                // SAFETY: `self.q` is open.
                unsafe { PdhCollectQueryData(self.q) == 0 }
            }

            fn cpu(&self) -> Option<f32> {
                let c = self.cpu?;
                let mut v = PDH_FMT_COUNTERVALUE::default();
                // SAFETY: `c` belongs to the open query; `v` receives the value.
                let status = unsafe { PdhGetFormattedCounterValue(c, PDH_FMT_DOUBLE, null_mut(), &mut v) };
                // SAFETY: PDH_FMT_DOUBLE fills `doubleValue`; any bit pattern is an f64.
                (status == 0 && valid(v.CStatus)).then_some(unsafe { v.Anonymous.doubleValue } as f32)
            }

            fn gpu(&self) -> Option<f32> {
                let c = self.gpu?;
                let (mut bytes, mut count) = (0u32, 0u32);
                // SAFETY: a call without a buffer only reports the size needed.
                let status = unsafe { PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut bytes, &mut count, null_mut()) };
                if status != PDH_MORE_DATA || bytes == 0 {
                    return None;
                }
                // The names follow the items in the same buffer: sized in bytes, kept as
                // items for their alignment.
                let n = (bytes as usize).div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
                let mut buf = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); n];
                // SAFETY: `buf` holds at least `bytes` bytes.
                if unsafe { PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut bytes, &mut count, buf.as_mut_ptr()) } != 0 {
                    return None;
                }
                let items: Vec<(String, f64)> = buf
                    .iter()
                    .take(count as usize)
                    .filter(|i| valid(i.FmtValue.CStatus) && !i.szName.is_null())
                    // SAFETY: PDH wrote NUL-terminated names inside `buf`, and doubles.
                    .map(|i| unsafe { (wide(i.szName), i.FmtValue.Anonymous.doubleValue) })
                    .collect();
                busiest_engine(items.iter().map(|(n, v)| (n.as_str(), *v)))
            }
        }

        impl Drop for Query {
            fn drop(&mut self) {
                // SAFETY: `self.q` is open and closed once.
                unsafe { PdhCloseQuery(self.q) };
            }
        }

        fn valid(status: u32) -> bool {
            status == PDH_CSTATUS_VALID_DATA || status == PDH_CSTATUS_NEW_DATA
        }

        /// # Safety
        /// `p` points to a NUL-terminated wide string.
        unsafe fn wide(p: *const u16) -> String {
            let mut len = 0;
            // SAFETY: the string ends with a NUL.
            while unsafe { *p.add(len) } != 0 {
                len += 1;
            }
            // SAFETY: `len` wide characters precede the NUL.
            String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reading_until_the_window_is_full() {
        assert_eq!(reading(&[], 0.5), None);
        assert_eq!(reading(&[0.1; 4], 0.5), None);
    }

    #[test]
    fn a_reading_gives_rate_mean_and_slowest() {
        let mut times = vec![0.01; 49];
        times.push(0.02);
        let r = reading(&times, 0.5).expect("0.51 s of frames");
        assert!((r.fps - 50.0 / 0.51).abs() < 0.01, "{r:?}");
        assert!((r.mean_ms - 10.2).abs() < 0.01, "{r:?}");
        assert!((r.worst_ms - 20.0).abs() < 0.01, "{r:?}");
    }

    #[test]
    fn usage_text_shows_what_was_read() {
        assert_eq!(usage_text(None, None), "");
        assert_eq!(usage_text(Some(34.4), Some(70.6)), "  CPU 34%  GPU 71%");
        assert_eq!(usage_text(None, Some(5.0)), "  GPU 5%");
    }

    #[test]
    fn gpu_use_is_the_busiest_engine_summed_over_processes() {
        let items = [
            ("pid_100_luid_0x0_0xC6A1_phys_0_eng_0_engtype_3D", 40.0),
            ("pid_200_luid_0x0_0xC6A1_phys_0_eng_0_engtype_3D", 25.0),
            ("pid_100_luid_0x0_0xC6A1_phys_0_eng_3_engtype_Copy", 10.0),
            ("pid_300_luid_0x0_0x1234_phys_0_eng_0_engtype_3D", 50.0),
        ];
        assert_eq!(usage::busiest_engine(items), Some(65.0));
        assert_eq!(usage::busiest_engine([]), None);
        assert_eq!(usage::busiest_engine([("pid_1_luid_a_eng_0", 80.0), ("pid_2_luid_a_eng_0", 70.0)]), Some(100.0));
    }
}
