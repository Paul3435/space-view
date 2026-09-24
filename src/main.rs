// Release builds are GUI-subsystem apps (no console window). Errors are
// surfaced through the crash hook, the startup log and message boxes instead.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod crash;
mod diag;
mod icon;

use disktree::{format, ops, scan, tree::Tree};
use eframe::{egui_wgpu, wgpu};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const USAGE: &str = "\
disktree — a treemap for finding and removing what fills your disk

Usage:
  disktree.exe [PATH]                 open the app (and scan PATH right away)
  disktree.exe --renderer wgpu|glow   force a graphics backend
  disktree.exe --print-scan PATH      scan without a window and print a summary
  disktree.exe --version | --help

Environment: DISKTREE_RENDERER=wgpu|glow";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererChoice {
    Wgpu,
    Glow,
}

impl RendererChoice {
    fn parse(s: &str) -> Option<RendererChoice> {
        match s.trim().to_ascii_lowercase().as_str() {
            "wgpu" | "dx12" | "d3d12" => Some(RendererChoice::Wgpu),
            "glow" | "gl" | "opengl" => Some(RendererChoice::Glow),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RendererChoice::Wgpu => "wgpu",
            RendererChoice::Glow => "OpenGL",
        }
    }
}

#[derive(Default)]
struct Args {
    path: Option<PathBuf>,
    renderer: Option<RendererChoice>,
    print_scan: Option<PathBuf>,
    help: bool,
    version: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = std::env::args_os().skip(1);
    while let Some(a) = it.next() {
        let s = a.to_string_lossy().to_string();
        match s.as_str() {
            "-h" | "--help" | "/?" => args.help = true,
            "-V" | "--version" => args.version = true,
            "--renderer" => {
                let v = it.next().ok_or("--renderer needs a value")?;
                args.renderer = Some(RendererChoice::parse(&v.to_string_lossy()).ok_or("unknown renderer")?);
            }
            "--print-scan" => args.print_scan = Some(PathBuf::from(it.next().ok_or("--print-scan needs a path")?)),
            _ if s.starts_with("--renderer=") => {
                args.renderer = Some(RendererChoice::parse(&s["--renderer=".len()..]).ok_or("unknown renderer")?);
            }
            _ if s.starts_with("--") => return Err(format!("unknown option {s}")),
            _ => args.path = Some(PathBuf::from(a)),
        }
    }
    Ok(args)
}

fn main() -> ExitCode {
    crash::install();
    diag::init();
    ops::quiet_critical_errors();

    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            ops::attach_parent_console();
            eprintln!("disktree: {e}\n\n{USAGE}");
            ops::message_box("disktree", &format!("{e}\n\n{USAGE}"));
            return ExitCode::from(2);
        }
    };
    if args.help || args.version {
        ops::attach_parent_console();
        if args.version {
            println!("disktree {}", env!("CARGO_PKG_VERSION"));
        } else {
            println!("{USAGE}");
        }
        return ExitCode::SUCCESS;
    }
    if let Some(path) = &args.print_scan {
        ops::attach_parent_console();
        return print_scan(path);
    }
    run_gui(args)
}

fn print_scan(path: &std::path::Path) -> ExitCode {
    let progress = scan::Progress::default();
    match scan::scan(path, &progress, scan::default_threads()) {
        Ok(tree) => {
            let root = tree.node(Tree::ROOT);
            println!("{}", tree.root_path.display());
            println!(
                "  {} on disk, {} logical, {} files, {} folders, {} links skipped, {} unreadable, {:.2} s",
                format::bytes(root.allocated),
                format::bytes(root.logical),
                format::count(root.files),
                format::count(tree.stats.dirs),
                format::count(tree.stats.links),
                format::count(tree.stats.unreadable),
                tree.stats.elapsed_secs
            );
            for c in tree.children(Tree::ROOT).take(15) {
                let n = tree.node(c);
                println!(
                    "  {:>10}  {:>6}  {}{}",
                    format::bytes(n.allocated),
                    format::percent(n.allocated, root.allocated),
                    n.name,
                    if n.is_dir() { "\\" } else { "" }
                );
            }
            for (p, e) in tree.stats.error_samples.iter().take(10) {
                println!("  unreadable: {p} ({e})");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("disktree: {e}");
            ExitCode::FAILURE
        }
    }
}

fn renderer_order(args: &Args) -> Vec<RendererChoice> {
    let forced = args
        .renderer
        .or_else(|| std::env::var("DISKTREE_RENDERER").ok().and_then(|v| RendererChoice::parse(&v)));
    if let Some(r) = forced {
        return vec![r];
    }
    if cfg!(windows) {
        // Direct3D 12 exists on every Windows 10/11 machine and falls back to
        // the WARP software rasterizer when there is no usable GPU driver (VMs,
        // RDP, basic display adapter), where OpenGL is often only 1.1. OpenGL
        // remains as a second chance.
        vec![RendererChoice::Wgpu, RendererChoice::Glow]
    } else {
        vec![RendererChoice::Glow, RendererChoice::Wgpu]
    }
}

fn native_options(renderer: RendererChoice) -> eframe::NativeOptions {
    let backends = wgpu::Backends::from_env().unwrap_or(if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::VULKAN | wgpu::Backends::GL
    });
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = backends;
    // A 2D app: don't wake a discrete GPU on laptops.
    setup.power_preference = wgpu::PowerPreference::from_env().unwrap_or(wgpu::PowerPreference::LowPower);

    eframe::NativeOptions {
        renderer: match renderer {
            RendererChoice::Wgpu => eframe::Renderer::Wgpu,
            RendererChoice::Glow => eframe::Renderer::Glow,
        },
        viewport: egui::ViewportBuilder::default()
            .with_title("disktree")
            .with_app_id("disktree")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([760.0, 480.0])
            .with_icon(icon::app_icon())
            .with_drag_and_drop(true),
        centered: true,
        persist_window: false,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn run_gui(args: Args) -> ExitCode {
    let mut errors: Vec<(String, String)> = Vec::new();
    for renderer in renderer_order(&args) {
        diag::log(&format!("starting window with renderer {}", renderer.name()));
        let started = Arc::new(AtomicBool::new(false));
        let started_in_app = started.clone();
        let path = args.path.clone();
        let result = eframe::run_native(
            "disktree",
            native_options(renderer),
            Box::new(move |cc| {
                started_in_app.store(true, Ordering::SeqCst);
                diag::log(&format!("window created, renderer {}", renderer.name()));
                Ok(Box::new(app::DiskTreeApp::new(cc, renderer.name(), path)))
            }),
        );
        match result {
            Ok(()) => {
                diag::log("window closed normally");
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                diag::log(&format!("renderer {} failed: {e}", renderer.name()));
                errors.push((renderer.name().to_owned(), e.to_string()));
                if started.load(Ordering::SeqCst) {
                    // The app itself ran; don't reopen it with another renderer.
                    break;
                }
            }
        }
    }
    crash::startup_failed(&errors);
    ExitCode::FAILURE
}
