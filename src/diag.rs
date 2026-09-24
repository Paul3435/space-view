//! Startup/diagnostic log. The release exe has no console, so anything that
//! goes wrong before (or instead of) a window appearing is written here:
//! `%LOCALAPPDATA%\disktree\disktree.log` (fallback: the temp directory).

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct Log {
    file: Option<File>,
    path: Option<PathBuf>,
    recent: VecDeque<String>,
    started: Instant,
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);

pub fn log_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("disktree")
}

pub fn init() {
    let dir = log_dir();
    let path = dir.join("disktree.log");
    let file = std::fs::create_dir_all(&dir)
        .ok()
        .and_then(|_| File::create(&path).ok())
        .or_else(|| File::create(std::env::temp_dir().join("disktree.log")).ok());
    if let Ok(mut g) = LOG.lock() {
        *g = Some(Log {
            file,
            path: Some(path),
            recent: VecDeque::new(),
            started: Instant::now(),
        });
    }
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    log(&format!(
        "disktree {} ({} {}) starting, unix time {secs}, exe {:?}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::current_exe().ok()
    ));
}

pub fn log(msg: &str) {
    if let Ok(mut g) = LOG.lock() {
        if let Some(l) = g.as_mut() {
            let line = format!("[{:>8.3}s] {msg}", l.started.elapsed().as_secs_f64());
            if let Some(f) = l.file.as_mut() {
                let _ = writeln!(f, "{line}");
                let _ = f.flush();
            }
            if l.recent.len() >= 200 {
                l.recent.pop_front();
            }
            l.recent.push_back(line);
        }
    }
    if cfg!(debug_assertions) {
        eprintln!("{msg}");
    }
}

pub fn recent() -> String {
    LOG.lock()
        .ok()
        .and_then(|g| g.as_ref().map(|l| l.recent.iter().cloned().collect::<Vec<_>>().join("\n")))
        .unwrap_or_default()
}

pub fn path() -> Option<PathBuf> {
    LOG.lock().ok().and_then(|g| g.as_ref().and_then(|l| l.path.clone()))
}
