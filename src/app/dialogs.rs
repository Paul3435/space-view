//! Modal dialogs (delete confirmation, messages, help) and toasts.

use super::*;

impl DiskTreeApp {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut close = false;
        let mut confirmed: Option<()> = None;
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        let unreadable: Vec<(String, String)> = self
            .tree
            .as_ref()
            .map(|t| t.stats.error_samples.clone())
            .unwrap_or_default();
        let unreadable_total = self.tree.as_ref().map(|t| t.stats.unreadable).unwrap_or(0);
        let metric = self.metric;

        let modal = egui::Modal::new(Id::new("dialog")).show(ctx, |ui| {
            ui.set_width(520.0);
            match dialog {
                Dialog::Message { title, body } => {
                    ui.heading(title.as_str());
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(body.as_str()).wrap());
                    ui.add_space(10.0);
                    if ui.button("OK").clicked() {
                        close = true;
                    }
                }
                Dialog::Shortcuts => {
                    ui.heading("Keyboard shortcuts");
                    ui.add_space(6.0);
                    egui::Grid::new("keys").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                        for (k, v) in SHORTCUTS {
                            ui.label(RichText::new(*k).monospace().color(Color32::WHITE));
                            ui.label(*v);
                            ui.end_row();
                        }
                    });
                    ui.add_space(10.0);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
                Dialog::Unreadable => {
                    ui.heading(format!("{} folders could not be read", format::count(unreadable_total)));
                    ui.label(RichText::new("They are counted as 0 bytes. Running disktree as administrator can read most of them.").color(TEXT_DIM));
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for (p, e) in &unreadable {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(e).color(WARN).size(12.0));
                                ui.label(RichText::new(p).size(12.0));
                            });
                        }
                        if unreadable_total as usize > unreadable.len() {
                            ui.label(RichText::new(format!("…and {} more", unreadable_total as usize - unreadable.len())).color(TEXT_DIM));
                        }
                    });
                    ui.add_space(10.0);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
                Dialog::Confirm(c) => {
                    let permanent = c.mode == DeleteMode::Permanent;
                    let title = match (permanent, c.step) {
                        (false, _) => "Move to the Recycle Bin?",
                        (true, 0) => "Delete permanently?",
                        (true, _) => "Are you absolutely sure?",
                    };
                    ui.heading(RichText::new(title).color(if permanent { Color32::from_rgb(255, 150, 150) } else { Color32::WHITE }));
                    ui.add_space(8.0);
                    ui.label(RichText::new(&c.name).strong().size(16.0).color(Color32::WHITE));
                    ui.add(egui::Label::new(RichText::new(c.path.display().to_string()).monospace().size(12.0)).wrap());
                    ui.add_space(4.0);
                    let size = if metric == Metric::Allocated { c.allocated } else { c.logical };
                    ui.label(RichText::new(format::bytes(size)).size(22.0).strong().color(Color32::WHITE));
                    ui.label(if c.is_dir {
                        format!("{} on disk · {} logical · {} files", format::bytes(c.allocated), format::bytes(c.logical), format::count(c.files))
                    } else {
                        format!("{} on disk · {} logical", format::bytes(c.allocated), format::bytes(c.logical))
                    });
                    ui.add_space(8.0);

                    if permanent {
                        warning_box(ui, if c.step == 0 {
                            "This bypasses the Recycle Bin. The files cannot be restored."
                        } else {
                            "Last chance: this cannot be undone."
                        });
                    } else {
                        ui.label(RichText::new("You can restore it from the Recycle Bin. If Windows can't recycle it (for example it's too big), Windows will ask before deleting it permanently.").color(TEXT_DIM));
                    }

                    let mut typed_ok = true;
                    if let Some(reason) = &c.caution {
                        if c.step == 0 {
                            ui.add_space(6.0);
                            warning_box(ui, reason);
                            ui.label(format!("Type the name \"{}\" to confirm:", c.name));
                            let edit = ui.add(egui::TextEdit::singleline(&mut c.typed).desired_width(f32::INFINITY));
                            if edit.gained_focus() || c.typed.is_empty() {
                                edit.request_focus();
                            }
                            typed_ok = c.typed.trim() == c.name;
                        }
                    }
                    ui.add_space(12.0);
                    let final_step = permanent && c.step > 0;
                    let armed = !final_step || c.armed_at.elapsed() >= Duration::from_millis(800);
                    if !armed {
                        ui.ctx().request_repaint_after(Duration::from_millis(100));
                    }
                    ui.horizontal(|ui| {
                        let label = match (permanent, c.step) {
                            (false, _) => "Move to Recycle Bin",
                            (true, 0) => "Continue…",
                            (true, _) => "Delete permanently",
                        };
                        let enabled = typed_ok && armed;
                        let fill = match (enabled, final_step) {
                            (false, _) => Color32::from_rgb(55, 58, 66),
                            (true, true) => DANGER,
                            (true, false) => Color32::from_rgb(60, 90, 140),
                        };
                        let button = egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(fill).min_size(Vec2::new(150.0, 30.0));
                        let cancel = || egui::Button::new("Cancel").min_size(Vec2::new(90.0, 30.0));
                        // On the final step Cancel takes the spot the previous button was in.
                        if final_step && ui.add(cancel()).clicked() {
                            close = true;
                        }
                        if ui.add_enabled(enabled, button).clicked() {
                            if permanent && c.step == 0 {
                                c.step = 1;
                                c.armed_at = Instant::now();
                            } else {
                                confirmed = Some(());
                            }
                        }
                        if !final_step && ui.add(cancel()).clicked() {
                            close = true;
                        }
                    });
                }
            }
        });
        if modal.should_close() || esc {
            close = true;
        }
        if confirmed.is_some() {
            if let Some(Dialog::Confirm(c)) = self.dialog.take() {
                self.start_delete(ctx, c);
            }
        } else if close {
            self.dialog = None;
        }
    }

    pub(super) fn paint_toast(&mut self, ctx: &egui::Context) {
        let Some(t) = &self.toast else { return };
        let now = Instant::now();
        if now >= t.until {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(t.until - now);
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            Id::new("toast"),
        ));
        let galley = painter.layout(
            t.text.clone(),
            FontId::proportional(14.0),
            Color32::WHITE,
            560.0,
        );
        let size = galley.size() + Vec2::new(28.0, 18.0);
        let rect = Rect::from_center_size(Pos2::new(screen.center().x, screen.max.y - 90.0), size);
        painter.rect_filled(
            rect,
            8.0,
            if t.error {
                Color32::from_rgb(120, 40, 40)
            } else {
                Color32::from_rgb(40, 60, 90)
            },
        );
        painter.galley(rect.min + Vec2::new(14.0, 9.0), galley, Color32::WHITE);
    }
}

const SHORTCUTS: &[(&str, &str)] = &[
    ("Click", "Select an item"),
    ("Double-click / Enter", "Open a folder"),
    (
        "Backspace / Alt+Up",
        "Up one level (also mouse Back button, middle-click)",
    ),
    ("Home", "Back to the scanned root"),
    ("Up / Down", "Move the selection in the list"),
    ("Delete", "Move selection to the Recycle Bin (asks first)"),
    ("Shift+Delete", "Delete permanently (asks twice)"),
    ("Ctrl+C", "Copy the selected path"),
    ("Ctrl+E", "Show in Explorer"),
    ("F5", "Rescan"),
    ("Esc", "Clear selection / close dialog / cancel scan"),
    ("F1", "This help"),
];
