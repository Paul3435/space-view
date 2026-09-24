mod app;
mod file_ops;
mod scanner;
mod treemap;

use app::DiskTreeApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Disk Tree - Windows Disk Usage Analyzer"),
        ..Default::default()
    };

    eframe::run_native(
        "Disk Tree",
        options,
        Box::new(|cc| Box::new(DiskTreeApp::new(cc))),
    )
}
