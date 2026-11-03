//! The engine's commit for a capture's header, read at run time (docs/design/frame-telemetry.md
//! §2). Embedding it at build time made every commit, even a docs-only one, recompile the
//! nvforever crate (#1312); the source checkout's place is a compile-time constant
//! (`CARGO_MANIFEST_DIR`) that does not change from commit to commit.

use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

pub struct GitPaths {
    pub head: PathBuf,
    pub branch: Option<PathBuf>,
    pub packed_refs: Vec<PathBuf>,
}

impl GitPaths {
    /// The files a change of the checked-out commit rewrites: `HEAD` (a switch), the
    /// branch's loose ref (a commit), else the `packed-refs` files that hold it.
    pub fn ref_files(&self) -> Vec<PathBuf> {
        let mut files = vec![self.head.clone()];
        match self.branch.as_deref() {
            Some(branch) if branch.is_file() => files.push(branch.to_path_buf()),
            _ => files.extend(self.packed_refs.iter().cloned()),
        }
        files.retain(|f| f.is_file());
        files
    }
}

fn git(base: &Path, arguments: &[&str]) -> Option<String> {
    let mut command = std::process::Command::new("git");
    command.current_dir(base).args(arguments);
    #[cfg(windows)]
    {
        // No console window flashes up beside the game's.
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}

/// The commit the running engine was built from, or why it is not known (the header's
/// `unavailable` reason): the checkout at `source` (the crate's own folder) must be the
/// engine's repository (`<toplevel>/engine/nvforever` is `source`), so a source tree
/// unpacked inside another repository never reports that one's commit; and its commit
/// must not have changed since `built` (the executable's time), so a checkout moved on
/// after the build is not mistaken for the build's. Packing refs after the build counts
/// as a change: the field is then unavailable, never wrong.
pub fn engine_revision(source: &Path, built: Option<SystemTime>) -> Result<String, &'static str> {
    if !source.is_dir() {
        return Err("source_checkout_unavailable");
    }
    let toplevel = git(source, &["rev-parse", "--show-toplevel"]).ok_or("git_unavailable")?;
    let same = |a: &Path, b: &Path| matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y);
    if !same(&Path::new(&toplevel).join("engine").join("nvforever"), source) {
        return Err("checkout_not_engine_repository");
    }
    let revision = git(source, &["rev-parse", "HEAD"]).ok_or("git_unavailable")?;
    if !matches!(revision.len(), 40 | 64) || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("git_unavailable");
    }
    let git_dir = git(source, &["rev-parse", "--git-dir"]).ok_or("git_unavailable")?;
    let common_dir = git(source, &["rev-parse", "--git-common-dir"]).ok_or("git_unavailable")?;
    let symbolic = git(source, &["rev-parse", "--symbolic-full-name", "HEAD"]).ok_or("git_unavailable")?;
    let paths = resolve(source, &git_dir, &common_dir, &symbolic).ok_or("git_unavailable")?;
    let built = built.ok_or("build_time_unavailable")?;
    if moved_since(&paths.ref_files(), built) {
        return Err("checkout_changed_since_build");
    }
    Ok(revision)
}

/// Whether any of `files` was written after `built` (a file whose time is unreadable
/// counts as written).
fn moved_since(files: &[PathBuf], built: SystemTime) -> bool {
    files.iter().any(|f| std::fs::metadata(f).and_then(|m| m.modified()).map_or(true, |t| t > built))
}

/// Git's relative directory answers are relative to the command's working directory.
pub fn resolve(base: &Path, git_dir: &str, common_dir: &str, symbolic: &str) -> Option<GitPaths> {
    let absolute = |value: &str| {
        if value.is_empty() {
            return None;
        }
        let path = Path::new(value);
        Some(if path.is_absolute() { path.to_path_buf() } else { base.join(path) })
    };
    let git = absolute(git_dir)?;
    let common = absolute(common_dir)?;
    let branch = if symbolic == "HEAD" {
        None
    } else {
        if !symbolic.starts_with("refs/") || !Path::new(symbolic).components().all(|part| matches!(part, Component::Normal(_))) {
            return None;
        }
        Some(common.join(symbolic))
    };
    let mut packed_refs = vec![git.join("packed-refs")];
    if common != git {
        packed_refs.push(common.join("packed-refs"));
    }
    Some(GitPaths {
        head: git.join("HEAD"),
        branch,
        packed_refs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!(
                "nvf-git-paths-{}-{timestamp}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_commit_or_switch_after_the_build_is_seen_by_the_ref_files() {
        let temp = TempDir::new();
        let common = temp.0.join("common");
        std::fs::create_dir_all(common.join("refs/heads")).unwrap();
        std::fs::write(common.join("HEAD"), "ref: refs/heads/topic").unwrap();
        std::fs::write(common.join("packed-refs"), "synthetic").unwrap();
        let paths = resolve(&temp.0, common.to_str().unwrap(), common.to_str().unwrap(), "refs/heads/topic").unwrap();
        // A packed-only branch: HEAD and packed-refs.
        assert_eq!(paths.ref_files(), vec![common.join("HEAD"), common.join("packed-refs")]);
        let before = SystemTime::now() - std::time::Duration::from_secs(3600);
        let after = SystemTime::now() + std::time::Duration::from_secs(3600);
        assert!(moved_since(&paths.ref_files(), before), "written after a build an hour ago");
        assert!(!moved_since(&paths.ref_files(), after));
        // Its loose ref, once written, stands for the branch.
        std::fs::write(common.join("refs/heads/topic"), "synthetic").unwrap();
        assert_eq!(paths.ref_files(), vec![common.join("HEAD"), common.join("refs/heads/topic")]);
        assert!(moved_since(&[temp.0.join("missing")], after), "an unreadable time counts as moved");
    }

    #[test]
    fn no_checkout_no_revision() {
        let temp = TempDir::new();
        assert_eq!(engine_revision(&temp.0.join("missing"), Some(SystemTime::now())), Err("source_checkout_unavailable"));
    }

    #[test]
    fn regular_checkout_and_detached_head() {
        let base = std::env::temp_dir();
        let paths = resolve(&base, ".git", ".git", "refs/heads/main").unwrap();
        assert_eq!(paths.head, base.join(".git/HEAD"));
        assert_eq!(paths.branch, Some(base.join(".git/refs/heads/main")));
        assert_eq!(paths.packed_refs, vec![base.join(".git/packed-refs")]);
        assert!(resolve(&base, ".git", ".git", "HEAD").unwrap().branch.is_none());
    }

    #[test]
    fn worktree_uses_common_branch_and_both_packed_refs_candidates() {
        let base = std::env::temp_dir();
        let git = base.join("clone/.git/worktrees/performance");
        let common = base.join("clone/.git");
        let paths = resolve(&base, git.to_str().unwrap(), common.to_str().unwrap(), "refs/heads/codex/performance").unwrap();
        assert_eq!(paths.head, git.join("HEAD"));
        assert_eq!(paths.branch, Some(common.join("refs/heads/codex/performance")));
        assert_eq!(paths.packed_refs, vec![git.join("packed-refs"), common.join("packed-refs")]);
    }

    #[test]
    fn rejects_missing_directories_and_escaping_refs() {
        let base = std::env::temp_dir();
        for (git, common, symbolic) in [
            ("", ".git", "HEAD"),
            (".git", "", "HEAD"),
            (".git", ".git", "refs/../HEAD"),
            (".git", ".git", "/HEAD"),
        ] {
            assert!(resolve(&base, git, common, symbolic).is_none());
        }
    }
}
