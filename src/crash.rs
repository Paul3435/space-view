//! Crash reporting for a console-less GUI exe: every panic is written to
//! `disktree-crash.log` next to the exe (or in the temp directory if that is
//! not writable) and, unless it happened on a background worker whose error
//! the UI reports itself, shown in a message box.

use crate::diag;
use disktree::ops;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn install() {
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>").to_owned();
        let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_owned()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic payload".to_owned()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_owned());
        diag::log(&format!(
            "PANIC on thread '{thread_name}' at {location}: {message}"
        ));

        let report = format!(
            "disktree {} crashed\n\nThread:   {thread_name}\nMessage:  {message}\nLocation: {location}\n\
             Time:     unix {}\nOS:       {} {}\n\nBacktrace:\n{}\n\nRecent log:\n{}\n",
            env!("CARGO_PKG_VERSION"),
            SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::backtrace::Backtrace::force_capture(),
            diag::recent(),
        );
        let written = write_report(&report);

        // Scan/delete workers catch their own panics and the UI shows the error.
        let background =
            thread_name.starts_with("disktree-scan") || thread_name.starts_with("disktree-worker");
        if !background {
            let where_ = written
                .map(|p| format!("A crash report was saved to:\n{}", p.display()))
                .unwrap_or_else(|| "The crash report could not be saved.".to_owned());
            ops::message_box(
                "disktree has crashed",
                &format!("disktree hit an unexpected error and has to close.\n\n{message}\n({location})\n\n{where_}"),
            );
        }
    }));
}

/// Writes `report` next to the exe, falling back to the temp directory.
pub fn write_report(report: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        candidates.push(exe.with_file_name("disktree-crash.log"));
    }
    candidates.push(diag::log_dir().join("disktree-crash.log"));
    candidates.push(std::env::temp_dir().join("disktree-crash.log"));
    for path in candidates {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&path, report).is_ok() {
            return Some(path);
        }
    }
    None
}

/// No renderer could start: explain, save a report, and show a message box.
pub fn startup_failed(errors: &[(String, String)]) {
    let details: String = errors
        .iter()
        .map(|(r, e)| format!("  {r}: {e}\n"))
        .collect();
    let report = format!(
        "disktree {} could not open its window.\n\n{details}\nRecent log:\n{}\n",
        env!("CARGO_PKG_VERSION"),
        diag::recent()
    );
    let path = write_report(&report);
    let log_hint = path
        .or_else(diag::path)
        .map(|p| format!("\n\nDetails were saved to:\n{}", p.display()))
        .unwrap_or_default();
    ops::message_box(
        "disktree could not start",
        &format!(
            "disktree could not initialise graphics.\n\n{details}\n\
             Updating your graphics driver usually fixes this. You can also force a renderer:\n  \
             disktree.exe --renderer glow\n  disktree.exe --renderer wgpu{log_hint}"
        ),
    );
}
