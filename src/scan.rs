//! Parallel directory scan into a [`Tree`].
//!
//! Directory enumeration is I/O bound, so the scan runs on a dedicated rayon
//! pool with more threads than cores. Each directory is read once; its
//! subdirectories are scanned in parallel via work stealing. Links (junctions,
//! symlinks, mount points) are recorded but never followed, so there are no
//! loops and nothing is counted twice. Unreadable directories are counted and
//! sampled, never fatal. The whole scan can be cancelled at any time.

use crate::fsread::{self, RawEntry};
use crate::tree::{ScanStats, ScannedDir, ScannedFile, Tree};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

const MAX_ERROR_SAMPLES: usize = 200;

#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub links: AtomicU64,
    pub unreadable: AtomicU64,
    pub allocated: AtomicU64,
    pub cancel: AtomicBool,
    current: Mutex<String>,
    errors: Mutex<Vec<(String, String)>>,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// A recently visited directory, for display.
    pub fn current(&self) -> String {
        self.current.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn record_error(&self, path: &Path, err: &std::io::Error) {
        self.unreadable.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut v) = self.errors.lock() {
            if v.len() < MAX_ERROR_SAMPLES {
                v.push((path.display().to_string(), describe(err)));
            }
        }
    }

    fn take_errors(&self) -> Vec<(String, String)> {
        self.errors.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
    }
}

fn describe(err: &std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => "Access denied".to_owned(),
        std::io::ErrorKind::NotFound => "Disappeared during scan".to_owned(),
        _ => err.to_string(),
    }
}

#[derive(Debug)]
pub enum ScanError {
    Cancelled,
    NotADirectory(PathBuf),
    Unreadable(PathBuf, String),
    Internal(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Cancelled => write!(f, "Scan cancelled"),
            ScanError::NotADirectory(p) => write!(f, "{} is not a folder", p.display()),
            ScanError::Unreadable(p, e) => write!(f, "Cannot read {}: {e}", p.display()),
            ScanError::Internal(e) => write!(f, "Internal error during scan: {e}"),
        }
    }
}

/// Number of scanner threads: directory reads block on I/O, so oversubscribe.
pub fn default_threads() -> usize {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    (cores * 4).clamp(8, 64)
}

/// Makes `path` absolute and normalised (on Windows: backslashes, no `.`/`..`).
pub fn normalize_root(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(windows)]
    {
        // A bare drive letter ("C:") means "current dir on C:"; users mean the root.
        let s = abs.to_string_lossy();
        if s.len() == 2 && s.ends_with(':') {
            return PathBuf::from(format!("{s}\\"));
        }
    }
    abs
}

/// Scans `root` completely. Blocks the calling thread; call it from a worker.
pub fn scan(root: &Path, progress: &Progress, threads: usize) -> Result<Tree, ScanError> {
    let started = Instant::now();
    let root = normalize_root(root);
    let dir = scan_subtree(&root, progress, threads)?;
    let stats = ScanStats {
        files: progress.files.load(Ordering::Relaxed),
        dirs: progress.dirs.load(Ordering::Relaxed),
        links: progress.links.load(Ordering::Relaxed),
        unreadable: progress.unreadable.load(Ordering::Relaxed),
        elapsed_secs: started.elapsed().as_secs_f64(),
        error_samples: progress.take_errors(),
    };
    Ok(Tree::from_scan(root, dir, stats))
}

/// Scans one directory subtree (used for full scans and for refreshing a
/// folder after a partial delete).
pub fn scan_subtree(root: &Path, progress: &Progress, threads: usize) -> Result<ScannedDir, ScanError> {
    let meta = std::fs::metadata(root)
        .map_err(|e| ScanError::Unreadable(root.to_path_buf(), describe(&e)))?;
    if !meta.is_dir() {
        return Err(ScanError::NotADirectory(root.to_path_buf()));
    }
    // Probe the root so an unreadable root is an error rather than an empty tree.
    fsread::read_dir(root).map_err(|e| ScanError::Unreadable(root.to_path_buf(), describe(&e)))?;

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        // Deeply nested trees recurse deeply; reserve generous stacks (only
        // committed as used) so pathological nesting cannot overflow.
        .stack_size(32 * 1024 * 1024)
        .thread_name(|i| format!("disktree-scan-{i}"))
        .build()
        .map_err(|e| ScanError::Internal(e.to_string()))?;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pool.install(|| scan_dir(root, Box::from(""), progress))
    }));
    match result {
        Ok(dir) if progress.is_cancelled() => {
            drop(dir);
            Err(ScanError::Cancelled)
        }
        Ok(dir) => Ok(dir),
        Err(panic) => Err(ScanError::Internal(panic_message(&panic))),
    }
}

pub fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_owned()
    }
}

fn scan_dir(path: &Path, name: Box<str>, progress: &Progress) -> ScannedDir {
    if progress.is_cancelled() {
        return ScannedDir::new(name, Vec::new(), Vec::new(), false);
    }
    let n = progress.dirs.fetch_add(1, Ordering::Relaxed);
    if n.is_multiple_of(256) {
        if let Ok(mut cur) = progress.current.try_lock() {
            *cur = path.display().to_string();
        }
    }

    let entries = match fsread::read_dir(path) {
        Ok(e) => e,
        Err(err) => {
            progress.record_error(path, &err);
            return ScannedDir::new(name, Vec::new(), Vec::new(), true);
        }
    };

    let mut files = Vec::new();
    let mut subdirs: Vec<String> = Vec::new();
    let mut allocated = 0u64;
    let mut file_count = 0u64;
    for RawEntry { name, is_dir, is_link, allocated: a, logical } in entries {
        if is_link {
            progress.links.fetch_add(1, Ordering::Relaxed);
            files.push(ScannedFile { name: name.into(), allocated: 0, logical: 0, is_link: true });
        } else if is_dir {
            subdirs.push(name);
        } else {
            allocated += a;
            file_count += 1;
            files.push(ScannedFile { name: name.into(), allocated: a, logical, is_link: false });
        }
    }
    progress.files.fetch_add(file_count, Ordering::Relaxed);
    progress.allocated.fetch_add(allocated, Ordering::Relaxed);

    let dirs: Vec<ScannedDir> = if subdirs.len() == 1 {
        let n = subdirs.pop().unwrap();
        vec![scan_dir(&path.join(&n), n.into(), progress)]
    } else {
        subdirs
            .into_par_iter()
            .map(|n| scan_dir(&path.join(&n), n.into(), progress))
            .collect()
    };
    ScannedDir::new(name, files, dirs, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Metric, NodeKind};
    use std::fs;

    fn write(path: &Path, bytes: usize) {
        fs::write(path, vec![7u8; bytes]).unwrap();
    }

    #[test]
    fn scans_nested_tree_and_aggregates() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir_all(r.join("a/b/c")).unwrap();
        fs::create_dir(r.join("empty")).unwrap();
        write(&r.join("top.txt"), 5);
        write(&r.join("a/one.bin"), 3000);
        write(&r.join("a/b/c/two.bin"), 9000);

        let p = Progress::default();
        let t = scan(r, &p, 4).unwrap();
        let root = t.node(Tree::ROOT);
        assert_eq!(root.logical, 12005);
        assert_eq!(root.files, 3);
        assert!(root.allocated >= root.logical);
        assert_eq!(t.stats.files, 3);
        assert_eq!(t.stats.dirs, 5, "root, a, b, c, empty");
        assert_eq!(t.stats.unreadable, 0);

        let a = t.find(&r.join("a")).unwrap();
        assert_eq!(t.node(a).logical, 12000);
        let c = t.find(&r.join("a/b/c")).unwrap();
        assert_eq!(t.node(c).files, 1);
        // Largest first.
        let first = t.children(Tree::ROOT).next().unwrap();
        assert_eq!(&*t.node(first).name, "a");
        assert_eq!(t.children_by_size(Tree::ROOT, Metric::Logical)[0], a);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_links_or_loop() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir(r.join("real")).unwrap();
        write(&r.join("real/data.bin"), 50_000);
        // A link to the real folder and a link back to the root (a loop).
        std::os::unix::fs::symlink(r.join("real"), r.join("alias")).unwrap();
        std::os::unix::fs::symlink(r, r.join("real/loop")).unwrap();

        let p = Progress::default();
        let t = scan(r, &p, 4).unwrap();
        assert_eq!(t.node(Tree::ROOT).logical, 50_000, "counted once, under its real path");
        let alias = t.find(&r.join("alias")).unwrap();
        assert_eq!(t.node(alias).kind, NodeKind::Link);
        assert_eq!(t.stats.links, 2);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_directories_are_counted_not_fatal() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path();
        fs::create_dir(r.join("locked")).unwrap();
        write(&r.join("locked/secret"), 10);
        write(&r.join("ok.txt"), 10);
        fs::set_permissions(r.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        let readable_anyway = fs::read_dir(r.join("locked")).is_ok(); // running as root
        let p = Progress::default();
        let t = scan(r, &p, 2);
        fs::set_permissions(r.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
        let t = t.unwrap();
        if !readable_anyway {
            assert_eq!(t.stats.unreadable, 1);
            assert_eq!(t.stats.error_samples.len(), 1);
            assert_eq!(t.stats.error_samples[0].1, "Access denied");
            let locked = t.find(&r.join("locked")).unwrap();
            assert!(t.node(locked).is_unreadable());
            assert_eq!(t.node(Tree::ROOT).logical, 10);
        }
    }

    #[test]
    fn cancel_stops_the_scan() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("x")).unwrap();
        let p = Progress::default();
        p.cancel();
        assert!(matches!(scan(tmp.path(), &p, 2), Err(ScanError::Cancelled)));
    }

    #[test]
    fn rejects_missing_and_non_directory_roots() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("f"), 1);
        let p = Progress::default();
        assert!(matches!(scan(&tmp.path().join("f"), &p, 2), Err(ScanError::NotADirectory(_))));
        assert!(matches!(scan(&tmp.path().join("missing"), &p, 2), Err(ScanError::Unreadable(..))));
    }

    #[test]
    fn deep_nesting_does_not_overflow() {
        let tmp = tempfile::tempdir().unwrap();
        let mut p = tmp.path().to_path_buf();
        for _ in 0..300 {
            p.push("d");
        }
        fs::create_dir_all(&p).unwrap();
        write(&p.join("leaf"), 1);
        let t = scan(tmp.path(), &Progress::default(), 4).unwrap();
        assert_eq!(t.node(Tree::ROOT).files, 1);
        assert_eq!(t.stats.dirs, 301);
    }

    #[test]
    fn many_files_scan() {
        let tmp = tempfile::tempdir().unwrap();
        for d in 0..20 {
            let dir = tmp.path().join(format!("d{d}"));
            fs::create_dir(&dir).unwrap();
            for f in 0..100 {
                write(&dir.join(format!("f{f}")), f);
            }
        }
        let t = scan(tmp.path(), &Progress::default(), 8).unwrap();
        assert_eq!(t.node(Tree::ROOT).files, 2000);
        assert_eq!(t.node(Tree::ROOT).logical, 20 * (0..100).sum::<u64>());
    }
}
