use crate::file_ops::{copy_path_to_clipboard, delete_permanent, delete_to_recycle_bin, format_size, open_in_explorer};
use crate::scanner::{get_extension_category, list_drives, scan_directory, FileNode, ScanProgress};
use crate::treemap::{layout_treemap, TreemapRect};
use egui::{Color32, Pos2, Rect, Sense, Vec2};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct DiskTreeApp {
    root: Option<FileNode>,
    current_path: Vec<usize>,
    treemap_rects: Vec<TreemapRect>,
    scan_progress: Option<ScanProgress>,
    scanning: bool,
    selected_drive: Option<String>,
    available_drives: Vec<String>,
    hovered_rect: Option<usize>,
    selected_node: Option<Vec<usize>>,
    show_delete_confirm: bool,
    delete_path: Option<PathBuf>,
    delete_size: u64,
    shift_pressed: bool,
    error_message: Option<String>,
}

impl Default for DiskTreeApp {
    fn default() -> Self {
        Self {
            root: None,
            current_path: vec![],
            treemap_rects: vec![],
            scan_progress: None,
            scanning: false,
            selected_drive: None,
            available_drives: list_drives(),
            hovered_rect: None,
            selected_node: None,
            show_delete_confirm: false,
            delete_path: None,
            delete_size: 0,
            shift_pressed: false,
            error_message: None,
        }
    }
}

impl DiskTreeApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn start_scan(&mut self, path: PathBuf) {
        let progress = ScanProgress::default();
        self.scan_progress = Some(progress.clone());
        self.scanning = true;
        self.root = None;
        self.current_path.clear();
        self.error_message = None;

        let root_arc = Arc::new(Mutex::new(None));
        let root_clone = root_arc.clone();

        thread::spawn(move || {
            let result = scan_directory(path, progress);
            *root_clone.lock().unwrap() = result;
        });
    }

    fn current_node(&self) -> Option<&FileNode> {
        let mut node = self.root.as_ref()?;
        for &idx in &self.current_path {
            node = node.children.get(idx)?;
        }
        Some(node)
    }

    fn zoom_in(&mut self, child_idx: usize) {
        if let Some(node) = self.current_node() {
            if child_idx < node.children.len() && node.children[child_idx].is_dir {
                self.current_path.push(child_idx);
                self.update_treemap();
            }
        }
    }

    fn zoom_out(&mut self) {
        if !self.current_path.is_empty() {
            self.current_path.pop();
            self.update_treemap();
        }
    }

    fn update_treemap(&mut self) {
        if let Some(node) = self.current_node() {
            self.treemap_rects = layout_treemap(node, 0.0, 0.0, 1.0, 1.0, 0.001);
        }
    }

    fn get_color_for_category(category: &str) -> Color32 {
        match category {
            "Image" => Color32::from_rgb(76, 175, 80),
            "Video" => Color32::from_rgb(244, 67, 54),
            "Audio" => Color32::from_rgb(156, 39, 176),
            "Document" => Color32::from_rgb(33, 150, 243),
            "Archive" => Color32::from_rgb(255, 152, 0),
            "Executable" => Color32::from_rgb(255, 87, 34),
            "Code" => Color32::from_rgb(63, 81, 181),
            _ => Color32::from_rgb(158, 158, 158),
        }
    }

    fn perform_delete(&mut self) {
        if let Some(path) = &self.delete_path {
            let result = if self.shift_pressed {
                delete_permanent(path)
            } else {
                delete_to_recycle_bin(path)
            };

            match result {
                Ok(_) => {
                    if let Some(node_path) = self.selected_node.clone() {
                        self.update_sizes_after_delete(&node_path);
                    }
                    self.selected_node = None;
                }
                Err(e) => {
                    self.error_message = Some(format!("Delete failed: {}", e));
                }
            }
        }

        self.show_delete_confirm = false;
        self.delete_path = None;
    }

    fn update_sizes_after_delete(&mut self, node_path: &[usize]) {
        let deleted_size = if let Some(root) = &self.root {
            Self::get_node_size_at_path(root, node_path)
        } else {
            0
        };

        if let Some(root) = &mut self.root {
            Self::remove_node_at_path_static(root, node_path);
            let current_path = self.current_path.clone();
            Self::propagate_size_change_static(root, &current_path, deleted_size as i64 * -1);
            self.update_treemap();
        }
    }

    fn get_node_size_at_path(node: &FileNode, path: &[usize]) -> u64 {
        if path.is_empty() {
            return node.size;
        }
        node.children.get(path[0])
            .and_then(|child| Some(Self::get_node_size_at_path(child, &path[1..])))
            .unwrap_or(0)
    }

    fn remove_node_at_path_static(node: &mut FileNode, path: &[usize]) {
        if path.len() == 1 {
            if path[0] < node.children.len() {
                node.children.remove(path[0]);
            }
        } else if let Some(child) = node.children.get_mut(path[0]) {
            Self::remove_node_at_path_static(child, &path[1..]);
        }
    }

    fn propagate_size_change_static(node: &mut FileNode, path: &[usize], delta: i64) {
        node.size = (node.size as i64 + delta).max(0) as u64;
        if !path.is_empty() {
            if let Some(child) = node.children.get_mut(path[0]) {
                Self::propagate_size_change_static(child, &path[1..], delta);
            }
        }
    }

    fn get_node_at_path<'a>(&'a self, node: &'a FileNode, path: &[usize]) -> Option<&'a FileNode> {
        if path.is_empty() {
            return Some(node);
        }
        let child = node.children.get(path[0])?;
        self.get_node_at_path(child, &path[1..])
    }
}

impl eframe::App for DiskTreeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.shift_pressed = ctx.input(|i| i.modifiers.shift);

        if self.scanning {
            if let Some(progress) = &self.scan_progress {
                let files = progress.files_scanned.load(Ordering::Relaxed);
                let dirs = progress.dirs_scanned.load(Ordering::Relaxed);
                let bytes = progress.bytes_scanned.load(Ordering::Relaxed);

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(100.0);
                        ui.heading("Scanning...");
                        ui.add_space(20.0);
                        ui.label(format!("Files: {}", files));
                        ui.label(format!("Directories: {}", dirs));
                        ui.label(format!("Total Size: {}", format_size(bytes)));
                    });
                });

                ctx.request_repaint();
            }
        }

        if self.root.is_none() && !self.scanning {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    ui.heading("Disk Tree - Windows Disk Usage Analyzer");
                    ui.add_space(30.0);

                    ui.label("Select a drive to scan:");
                    ui.add_space(10.0);

                    let mut selected_drive = None;
                    for drive in &self.available_drives {
                        if ui.button(drive).clicked() {
                            selected_drive = Some(drive.clone());
                        }
                    }
                    if let Some(drive) = selected_drive {
                        self.start_scan(PathBuf::from(drive));
                    }

                    ui.add_space(20.0);
                    ui.label("Or enter a custom path:");
                    let mut custom_path = String::new();
                    if ui.text_edit_singleline(&mut custom_path).lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        self.start_scan(PathBuf::from(custom_path));
                    }
                });
            });
            return;
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Disk Tree");

                ui.separator();

                if ui.button("⟲ Rescan").clicked() {
                    if let Some(node) = self.current_node() {
                        self.start_scan(node.path.clone());
                    }
                }

                ui.separator();

                if !self.current_path.is_empty() && ui.button("← Back").clicked() {
                    self.zoom_out();
                }

                ui.separator();

                if let Some(node) = self.current_node() {
                    ui.label(format!("{} ({})", node.path.display(), format_size(node.size)));
                }
            });
        });

        let mut clear_error = false;
        if let Some(msg) = &self.error_message {
            let msg_clone = msg.clone();
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(&msg_clone);
                    if ui.button("OK").clicked() {
                        clear_error = true;
                    }
                });
        }
        if clear_error {
            self.error_message = None;
        }

        if self.show_delete_confirm {
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    if let Some(path) = &self.delete_path {
                        ui.label(format!("Delete: {}", path.display()));
                        ui.label(format!("Size: {}", format_size(self.delete_size)));
                        ui.add_space(10.0);
                        if self.shift_pressed {
                            ui.colored_label(Color32::RED, "⚠ PERMANENT DELETE (Shift held)");
                        } else {
                            ui.label("Will be moved to Recycle Bin");
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button("Delete").clicked() {
                                self.perform_delete();
                            }
                            if ui.button("Cancel").clicked() {
                                self.show_delete_confirm = false;
                                self.delete_path = None;
                            }
                        });
                    }
                });
        }

        let mut clicked_item = None;
        let mut double_clicked_item = None;
        let mut open_explorer_path = None;
        let mut copy_clipboard_path = None;
        let mut delete_request = None;

        egui::SidePanel::right("side_panel")
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading("Largest Items");
                ui.separator();

                if let Some(node) = self.current_node() {
                    let mut sorted_children: Vec<(usize, &FileNode)> =
                        node.children.iter().enumerate().collect();
                    sorted_children.sort_by(|a, b| b.1.size.cmp(&a.1.size));

                    let selected_last_idx = self.selected_node.as_ref().and_then(|p| p.last().copied());

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (idx, child) in sorted_children.iter().take(100) {
                            let is_selected = selected_last_idx == Some(*idx);
                            
                            let response = ui.selectable_label(is_selected, "");
                            
                            ui.horizontal(|ui| {
                                ui.label(if child.is_dir { "📁" } else { "📄" });
                                ui.label(&child.name);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(format_size(child.size));
                                });
                            });

                            if response.clicked() {
                                clicked_item = Some(*idx);
                            }

                            if response.double_clicked() && child.is_dir {
                                double_clicked_item = Some(*idx);
                            }

                            if response.secondary_clicked() {
                                clicked_item = Some(*idx);
                            }

                            ui.separator();
                        }
                    });

                    ui.separator();

                    if let Some(node_path) = &self.selected_node {
                        if let Some(selected) = self.get_node_at_path(
                            self.root.as_ref().unwrap(),
                            node_path,
                        ) {
                            let selected_path = selected.path.clone();
                            let selected_size = selected.size;
                            
                            ui.heading("Actions");
                            
                            if ui.button("📂 Open in Explorer").clicked() {
                                open_explorer_path = Some(selected_path.clone());
                            }

                            if ui.button("📋 Copy Path").clicked() {
                                copy_clipboard_path = Some(selected_path.clone());
                            }

                            if ui.button("🗑 Delete").clicked() {
                                delete_request = Some((selected_path, selected_size));
                            }
                        }
                    }
                }
            });

        if let Some(idx) = clicked_item {
            let mut path = self.current_path.clone();
            path.push(idx);
            self.selected_node = Some(path);
        }

        if let Some(idx) = double_clicked_item {
            self.zoom_in(idx);
        }

        if let Some(path) = open_explorer_path {
            if let Err(e) = open_in_explorer(&path) {
                self.error_message = Some(e);
            }
        }

        if let Some(path) = copy_clipboard_path {
            if let Err(e) = copy_path_to_clipboard(&path) {
                self.error_message = Some(e);
            }
        }

        if let Some((path, size)) = delete_request {
            self.delete_path = Some(path);
            self.delete_size = size;
            self.show_delete_confirm = true;
        }

        let mut treemap_clicked = None;
        let mut treemap_double_clicked = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(node) = self.current_node() {
                let available_size = ui.available_size();
                let node_size = node.size;
                let children_data: Vec<_> = node.children.iter()
                    .map(|c| (c.path.clone(), c.name.clone(), c.size, c.is_dir, c.extension.clone()))
                    .collect();

                let (mut response, painter) = ui.allocate_painter(available_size, Sense::click());

                let mut hovered_idx = None;
                let hover_pos = response.hover_pos();

                for (i, rect) in self.treemap_rects.iter().enumerate() {
                    let screen_rect = Rect::from_min_size(
                        Pos2::new(
                            response.rect.min.x + rect.x * available_size.x,
                            response.rect.min.y + rect.y * available_size.y,
                        ),
                        Vec2::new(rect.w * available_size.x, rect.h * available_size.y),
                    );

                    let (_, name, _, is_dir, extension) = &children_data[rect.node_index];
                    let category = if *is_dir {
                        "Other"
                    } else {
                        get_extension_category(extension.as_ref())
                    };
                    let color = Self::get_color_for_category(category);

                    let is_hovered = hover_pos.map_or(false, |pos| screen_rect.contains(pos));

                    if is_hovered {
                        hovered_idx = Some(i);
                        painter.rect_filled(screen_rect, 0.0, color.linear_multiply(1.2));
                    } else {
                        painter.rect_filled(screen_rect, 0.0, color);
                    }

                    painter.rect_stroke(screen_rect, 0.0, (1.0, Color32::BLACK));

                    if screen_rect.width() > 50.0 && screen_rect.height() > 20.0 {
                        painter.text(
                            screen_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            name,
                            egui::FontId::proportional(12.0),
                            Color32::WHITE,
                        );
                    }
                }

                self.hovered_rect = hovered_idx;

                if response.clicked() {
                    if let Some(hover_idx) = hovered_idx {
                        if let Some(rect) = self.treemap_rects.get(hover_idx) {
                            treemap_clicked = Some(rect.node_index);
                        }
                    }
                }

                if response.double_clicked() {
                    if let Some(hover_idx) = hovered_idx {
                        if let Some(rect) = self.treemap_rects.get(hover_idx) {
                            treemap_double_clicked = Some(rect.node_index);
                        }
                    }
                }

                if let Some(hover_idx) = hovered_idx {
                    if let Some(rect) = self.treemap_rects.get(hover_idx) {
                        let (path, _, size, _, _) = &children_data[rect.node_index];
                        let percentage = (*size as f64 / node_size as f64) * 100.0;

                        response = response.on_hover_text(format!(
                            "{}\n{}\n{:.2}% of parent",
                            path.display(),
                            format_size(*size),
                            percentage
                        ));
                    }
                }
            }
        });

        if let Some(idx) = treemap_clicked {
            let mut path = self.current_path.clone();
            path.push(idx);
            self.selected_node = Some(path);
        }

        if let Some(idx) = treemap_double_clicked {
            self.zoom_in(idx);
        }

        if self.scanning {
            ctx.request_repaint();
        }
    }
}
