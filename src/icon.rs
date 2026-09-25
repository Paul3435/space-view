//! The window icon. The same design is embedded as the exe icon by build.rs.

include!("icon_design.rs");

pub fn app_icon() -> egui::IconData {
    let size = 64;
    egui::IconData {
        rgba: rgba(size),
        width: size,
        height: size,
    }
}
