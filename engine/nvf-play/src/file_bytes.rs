//! Whole sound files read for streaming (radio, music, ambient loops), kept while they
//! play and for a while after, then let go.
//!
//! The radio, the music and the ambient loops each kept every file they ever read for
//! the whole session: a radio station's songs and lines (a few MB each), every music
//! track, every region's loops, so the RAM a long session used only grew [observed
//! 2026-11-06; design]. A file is in use while a playing holds its bytes (the stream's
//! decoder keeps an `Arc` of them); one nothing holds goes after the same keep limits as
//! the asset caches (nvf_assets `release.rs`, [`KeepLimits`]). Asking again reads it
//! again. A file that could not be read (`None`) is remembered: it holds nothing.

use bevy::prelude::*;
use nvf_assets::release::Unused;
use nvf_assets::KeepLimits;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct FileBytes {
    files: HashMap<String, Option<Arc<[u8]>>>,
    unused: Unused<String>,
    swept_at: f64,
    /// Files let go of so far.
    pub released: u64,
}

impl FileBytes {
    /// The file at `path`, read with `read` the first time it is asked for (or again
    /// after it was let go of).
    pub fn get_or_read(&mut self, path: &str, read: impl FnOnce() -> Option<Arc<[u8]>>) -> Option<Arc<[u8]>> {
        if let Some(f) = self.files.get(path) {
            return f.clone();
        }
        let f = read();
        self.files.insert(path.to_string(), f.clone());
        f
    }

    /// Remember `path` as a file that cannot be played.
    pub fn mark_broken(&mut self, path: String) {
        self.files.insert(path, None);
    }

    /// (files kept, those nothing holds, their bytes).
    pub fn share(&self) -> (usize, usize, u64) {
        let kept = self.files.values().flatten();
        let idle = kept.clone().filter(|b| Arc::strong_count(b) == 1).count();
        (kept.clone().count(), idle, kept.map(|b| b.len() as u64).sum())
    }

    /// Every [`nvf_assets::release::SWEEP_SECS`] of real time `now`: let go of the files
    /// nothing has held for `limits`.
    pub fn sweep(&mut self, now: f64, limits: KeepLimits) {
        if now - self.swept_at < nvf_assets::release::SWEEP_SECS as f64 {
            return;
        }
        self.swept_at = now;
        let entries: Vec<(String, bool)> = self.files.iter().filter_map(|(p, f)| Some((p.clone(), Arc::strong_count(f.as_ref()?) > 1))).collect();
        let drop = self.unused.sweep(now, entries, limits.secs, limits.max);
        for p in &drop {
            self.files.remove(p);
        }
        self.released += drop.len() as u64;
    }
}

/// The keep limits in force, the asset caches' defaults when the game set none.
pub fn limits(l: Option<Res<KeepLimits>>) -> KeepLimits {
    l.map_or_else(KeepLimits::default, |l| *l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_nothing_holds_go_after_the_keep_time() {
        let mut f = FileBytes::default();
        let limits = KeepLimits { secs: 10.0, max: 100 };
        let held = f.get_or_read("a.ogg", || Some(Arc::from(&[1u8, 2, 3][..]))).unwrap();
        f.get_or_read("b.ogg", || Some(Arc::from(&[4u8][..])));
        f.get_or_read("missing.ogg", || None);
        assert_eq!(f.share(), (2, 1, 4));
        f.sweep(5.0, limits);
        f.sweep(20.0, limits);
        // b went unused at 5 s and is gone 15 s later; a is still played; the missing
        // file stays remembered.
        assert_eq!(f.released, 1);
        assert_eq!(f.share(), (1, 0, 3));
        let mut read_again = false;
        assert!(f.get_or_read("missing.ogg", || {
            read_again = true;
            None
        })
        .is_none());
        assert!(!read_again);
        drop(held);
        f.sweep(30.0, limits);
        f.sweep(45.0, limits);
        assert_eq!(f.share(), (0, 0, 0));
        // Asked again, it is read again.
        assert!(f.get_or_read("a.ogg", || Some(Arc::from(&[9u8][..]))).is_some());
    }
}
