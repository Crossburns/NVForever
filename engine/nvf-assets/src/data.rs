//! ArchivePlugin: the game's virtual file system and the NIF schema as resources.

use bevy::prelude::*;
use fnv_nif::Schema;
use fnv_vfs::Vfs;
use std::path::Path;
use std::sync::Arc;

#[derive(Resource, Clone)]
pub struct GameData {
    pub vfs: Arc<Vfs>,
    pub schema: Arc<Schema>,
}

impl GameData {
    /// Open the default archive set of `data_dir` (read-only) and load the schemas.
    pub fn open(data_dir: &Path, schema_dir: &Path) -> Result<GameData, String> {
        GameData::open_for_plugins(data_dir, &fnv_vfs::DEFAULT_PLUGINS, schema_dir)
    }

    /// [`GameData::open`] with the archives of `plugins` (load order): mods' archives
    /// over the base game's, loose files over both.
    pub fn open_for_plugins<S: AsRef<str>>(data_dir: &Path, plugins: &[S], schema_dir: &Path) -> Result<GameData, String> {
        GameData::open_for_plugins_in(data_dir, None, plugins, schema_dir)
    }

    /// [`GameData::open_for_plugins`] with the archives beside the plugins in NVForever's
    /// mods folder `mods` too (`fnv_vfs::Vfs::archive_order_in`).
    pub fn open_for_plugins_in<S: AsRef<str>>(data_dir: &Path, mods: Option<&Path>, plugins: &[S], schema_dir: &Path) -> Result<GameData, String> {
        let vfs = Vfs::open_for_plugins_in(data_dir, mods, plugins).map_err(|e| e.to_string())?;
        let schema = Schema::load_dir(schema_dir).map_err(|e| e.to_string())?;
        Ok(GameData { vfs: Arc::new(vfs), schema: Arc::new(schema) })
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let broken = broken_files().iter().find(|(_, part)| path.to_ascii_lowercase().contains(part.as_str()));
        if let Some((Broken::Missing, _)) = broken {
            return None;
        }
        let bytes = match self.vfs.read(path)? {
            Ok(d) => d,
            Err(e) => {
                warn!("{path}: {e}");
                return None;
            }
        };
        test_broken(path, bytes)
    }
}

/// `bytes` of the file at `path` as the test aid [`broken_files`] has them: unchanged,
/// missing, garbled, or a panic. For readers that do not go through [`GameData::read`]
/// (the voice bank).
pub fn test_broken(path: &str, mut bytes: Vec<u8>) -> Option<Vec<u8>> {
    let broken = broken_files().iter().find(|(_, part)| path.to_ascii_lowercase().contains(part.as_str()));
    match broken.map(|b| b.0) {
        Some(Broken::Missing) => return None,
        Some(Broken::Corrupt) => corrupt(&mut bytes),
        Some(Broken::Panic) => panic!("test panic reading {path} (NVF_PLAY_TEST_BROKEN_FILES)"),
        None => {}
    }
    Some(bytes)
}

/// How a file named in the test aid is broken.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Broken {
    /// As if a mod left it out.
    Missing,
    /// Its header kept, the rest garbled.
    Corrupt,
    /// Reading it panics (a bug of ours met by a bad file).
    Panic,
}

/// Test aid `NVF_PLAY_TEST_BROKEN_FILES="missing:<part>;corrupt:<part>;panic:<part>"`:
/// files whose archive path contains `<part>` (any case) read as missing, garbled or
/// panicking, to check the game goes on (guard.rs). A test aid: nothing is changed on
/// disk.
fn broken_files() -> &'static [(Broken, String)] {
    static FILES: std::sync::OnceLock<Vec<(Broken, String)>> = std::sync::OnceLock::new();
    FILES.get_or_init(|| std::env::var("NVF_PLAY_TEST_BROKEN_FILES").map(|v| parse_broken(&v)).unwrap_or_default())
}

fn parse_broken(v: &str) -> Vec<(Broken, String)> {
    v.split(';')
        .filter_map(|e| {
            let (kind, part) = e.trim().split_once(':')?;
            let kind = match kind.trim() {
                "missing" => Broken::Missing,
                "corrupt" => Broken::Corrupt,
                "panic" => Broken::Panic,
                _ => return None,
            };
            let part = part.trim().replace('/', "\\").to_ascii_lowercase();
            (!part.is_empty()).then_some((kind, part))
        })
        .collect()
}

/// Garble every byte after the first 64 (a NIF or DDS header stays readable, so the
/// parsers get past it into the broken part). Fixed, so runs repeat.
fn corrupt(bytes: &mut [u8]) {
    for (i, b) in bytes.iter_mut().enumerate().skip(64) {
        *b ^= (i as u8).wrapping_mul(151).wrapping_add(0x5A);
    }
}

pub struct ArchivePlugin(pub GameData);

impl Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::nif_material::NifMaterialPlugin)
            .insert_resource(self.0.clone())
            .init_resource::<crate::texture::TextureCache>()
            .init_resource::<crate::model::ModelCache>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_file_list() {
        assert_eq!(
            parse_broken("missing: Meshes/Clutter/ ; corrupt:mtidle.kf;bogus:x;panic:"),
            vec![(Broken::Missing, "meshes\\clutter\\".to_string()), (Broken::Corrupt, "mtidle.kf".to_string())]
        );
        let mut b = vec![0u8; 70];
        corrupt(&mut b);
        assert!(b[..64].iter().all(|&x| x == 0) && b[64..].iter().any(|&x| x != 0));
    }
}
