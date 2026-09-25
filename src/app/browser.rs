//! The main view: top bar, status bar, side list and the treemap.

use super::*;

impl DiskTreeApp {
    pub(super) fn browser(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        self.refresh_list();
        self.top_bar(ui, actions);
        self.status_bar(ui, actions);
        self.side_panel(ui, actions);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(6.0))
            .show(ui, |ui| {
                self.treemap(ui, actions);
            });
    }

    pub(super) fn top_bar(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let Some(tree) = &self.tree else { return };
        let busy = self.task.is_some();
        egui::Panel::top("top")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .stroke(Stroke::new(1.0, HAIRLINE)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (icon, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                    let p = ui.painter();
                    // Four muted blocks, the same idea as the window icon, small
                    // enough to sit in the bar without becoming a logo contest.
                    let gap = 1.0;
                    let half = (icon.width() - gap) / 2.0;
                    let blocks = [
                        (0.0, 0.0, Category::Code),
                        (half + gap, 0.0, Category::Archive),
                        (0.0, half + gap, Category::Video),
                        (half + gap, half + gap, Category::Audio),
                    ];
                    for (x, y, cat) in blocks {
                        let [r, g, b] = cat.fill(1);
                        p.rect_filled(
                            Rect::from_min_size(icon.min + Vec2::new(x, y), Vec2::splat(half)),
                            0.0,
                            Color32::from_rgb(r, g, b),
                        );
                    }
                    ui.label(RichText::new("disktree").strong().size(15.0).color(TEXT));
                    ui.painter().line_segment(
                        [
                            Pos2::new(ui.cursor().min.x - 2.0, ui.cursor().min.y + 2.0),
                            Pos2::new(ui.cursor().min.x - 2.0, ui.cursor().max.y - 2.0),
                        ],
                        Stroke::new(1.0, HAIRLINE),
                    );
                    breadcrumbs(ui, tree, self.current, actions);

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if ui
                            .button("?")
                            .on_hover_text("Keyboard shortcuts (F1)")
                            .clicked()
                        {
                            actions.push(Action::ShowShortcuts);
                        }
                        let on_disk = self.metric == Metric::Allocated;
                        let metric_label = if on_disk {
                            "Size on disk"
                        } else {
                            "Logical size"
                        };
                        if ui
                            .add(egui::Button::new(
                                RichText::new(metric_label).color(if on_disk {
                                    SELECT
                                } else {
                                    TEXT_DIM
                                }),
                            ))
                            .on_hover_text(
                                "Toggle allocated size (what deleting frees) and logical size",
                            )
                            .clicked()
                        {
                            self.metric = if on_disk {
                                Metric::Logical
                            } else {
                                Metric::Allocated
                            };
                        }
                        if ui
                            .add_enabled(!busy, egui::Button::new("Rescan"))
                            .on_hover_text("Scan everything again (F5)")
                            .clicked()
                        {
                            actions.push(Action::Rescan);
                        }
                        let can_up = tree.node(self.current).parent != NO_NODE;
                        if ui
                            .add_enabled(can_up, egui::Button::new("Up"))
                            .on_hover_text("Parent folder (Backspace)")
                            .clicked()
                        {
                            actions.push(Action::Up);
                        }
                        if ui
                            .add_enabled(!busy, egui::Button::new("New scan"))
                            .on_hover_text("Back to the drive list")
                            .clicked()
                        {
                            actions.push(Action::NewScan);
                        }
                        if tree.stats.unreadable > 0 {
                            let t = RichText::new(format!(
                                "{} unreadable",
                                format::count(tree.stats.unreadable)
                            ))
                            .color(WARN)
                            .size(12.0);
                            if ui
                                .add(egui::Button::new(t).frame(false))
                                .on_hover_text("Folders that could not be read")
                                .clicked()
                            {
                                actions.push(Action::ShowUnreadable);
                            }
                        }
                    });
                });
            });
    }

    pub(super) fn status_bar(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let Some(tree) = &self.tree else { return };
        let _ = actions;
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(14, 7))
                    .stroke(Stroke::new(1.0, HAIRLINE)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for c in Category::ALL {
                        let [r, g, b] = c.rgb();
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
                        ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
                        ui.add_space(-6.0);
                        ui.label(RichText::new(c.label()).size(11.5).color(TEXT_DIM));
                    }
                });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let root = tree.node(Tree::ROOT);
                let s = &tree.stats;
                let text = format!(
                    "{} on disk ({} logical) · {} files · scanned {} folders in {} · {} links not followed · {}",
                    format::bytes(root.allocated),
                    format::bytes(root.logical),
                    format::count(root.files),
                    format::count(s.dirs),
                    format::duration(s.elapsed_secs),
                    format::count(s.links),
                    self.renderer
                );
                ui.label(RichText::new(text).size(12.0).color(TEXT_DIM));
                if let Some(task) = &self.task {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(&task.label).size(12.0).color(WARN));
                        ui.spinner();
                    });
                }
            });
        });
    }

    pub(super) fn side_panel(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let Some(tree) = &self.tree else { return };
        let metric = self.metric;
        let busy = self.task.is_some();
        egui::Panel::right("side")
            .resizable(true)
            .default_size(420.0)
            .size_range(360.0..=900.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(10.0))
            .show(ui, |ui| {
                let cur = tree.node(self.current);
                let cur_size = cur.size(metric);
                let title = if self.current == Tree::ROOT {
                    tree.root_path.display().to_string()
                } else {
                    cur.name.to_string()
                };
                ui.add(
                    egui::Label::new(RichText::new(title).size(15.0).strong().color(TEXT))
                        .truncate(),
                );
                ui.label(
                    RichText::new(format!(
                        "{} · {} files · {} items",
                        format::bytes(cur_size),
                        format::count(cur.files),
                        format::count(self.list.len() as u64)
                    ))
                    .size(12.0)
                    .color(TEXT_DIM),
                );
                ui.add_space(10.0);

                // Selection card. Fixed height: selecting must not shift the
                // list under the cursor (the second click of a double-click
                // would hit another row).
                egui::Frame::new()
                    .fill(CARD)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.set_height(SELECTION_CARD_H);
                        match self.selected {
                            None => {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new("SELECTION").size(10.5).color(TEXT_DIM),
                                    )
                                    .truncate(),
                                );
                                ui.add_space(4.0);
                                ui.label(RichText::new("Nothing selected").size(15.0).color(TEXT));
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(
                                        "Click a tile or a row. Double-click a folder to open it.",
                                    )
                                    .size(12.5)
                                    .color(TEXT_DIM),
                                );
                            }
                            Some(sel) => {
                                let n = tree.node(sel);
                                let path = tree.path_of(sel);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new("SELECTION").size(10.5).color(TEXT_DIM),
                                    )
                                    .truncate(),
                                );
                                ui.add_space(2.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&*n.name).strong().size(14.0).color(TEXT),
                                    )
                                    .truncate(),
                                );
                                let size = n.size(metric);
                                let (num, unit) = split_bytes(size);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(num)
                                            .size(32.0)
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                    ui.label(RichText::new(unit).size(16.0).color(TEXT_DIM));
                                });
                                ui.label(
                                    RichText::new(format!(
                                        "{} on disk · {} logical",
                                        format::bytes(n.allocated),
                                        format::bytes(n.logical)
                                    ))
                                    .size(12.0)
                                    .color(TEXT_DIM),
                                );
                                let frac = if cur_size > 0 {
                                    (size as f32 / cur_size as f32).clamp(0.0, 1.0)
                                } else {
                                    0.0
                                };
                                let (bar, _) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 3.0),
                                    Sense::hover(),
                                );
                                ui.painter()
                                    .rect_filled(bar, 0.0, Color32::from_rgb(48, 42, 30));
                                ui.painter().rect_filled(
                                    Rect::from_min_size(
                                        bar.min,
                                        Vec2::new(bar.width() * frac, bar.height()),
                                    ),
                                    0.0,
                                    ACCENT,
                                );
                                ui.add_space(4.0);
                                let kind = match n.kind {
                                    NodeKind::Dir => format!("{} files", format::count(n.files)),
                                    NodeKind::File => n.category.label().to_owned(),
                                    NodeKind::Link => "Link, not followed".to_owned(),
                                };
                                ui.label(
                                    RichText::new(format!(
                                        "{} of this folder  ·  {kind}",
                                        format::percent(size, cur_size)
                                    ))
                                    .size(12.0)
                                    .color(TEXT_DIM),
                                );
                                let path_text = path.display().to_string();
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&path_text).size(11.5).color(TEXT_DIM),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&path_text);
                                ui.add_space(4.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    ui.spacing_mut().item_spacing.y = 4.0;
                                    if n.is_dir()
                                        && ui.button("Open").on_hover_text("Enter").clicked()
                                    {
                                        actions.push(Action::Open(sel));
                                    }
                                    if ui.button("Explorer").on_hover_text("Ctrl+E").clicked() {
                                        actions.push(Action::Reveal(sel));
                                    }
                                    if ui.button("Copy").on_hover_text("Ctrl+C").clicked() {
                                        actions.push(Action::CopyPath(sel));
                                    }
                                    let can = !busy && n.kind != NodeKind::Link;
                                    if ui
                                        .add_enabled(can, egui::Button::new("Recycle"))
                                        .on_hover_text("Move to the Recycle Bin. Asks first.")
                                        .clicked()
                                    {
                                        actions.push(Action::Delete(sel, DeleteMode::RecycleBin));
                                    }
                                    if ui
                                        .add_enabled(
                                            can,
                                            egui::Button::new(
                                                RichText::new("Delete permanently…")
                                                    .color(Color32::from_rgb(232, 160, 160)),
                                            ),
                                        )
                                        .on_hover_text(
                                            "Shift+Delete. Asks twice. Cannot be undone.",
                                        )
                                        .clicked()
                                    {
                                        actions.push(Action::Delete(sel, DeleteMode::Permanent));
                                    }
                                });
                            }
                        }
                    });
                ui.add_space(8.0);

                // Column headers
                ui.horizontal(|ui| {
                    let w = ui.available_width();
                    for (key, label, width) in [
                        (SortKey::Name, "Name", w - 190.0),
                        (SortKey::Size, "Size", 90.0),
                        (SortKey::Files, "Files", 80.0),
                    ] {
                        let active = self.sort == key;
                        let text = RichText::new(label).color(if active {
                            Color32::WHITE
                        } else {
                            TEXT_DIM
                        });
                        let resp = ui
                            .add_sized([width, 20.0], egui::Button::new(text).frame(false))
                            .on_hover_text("Sort (click again to reverse)");
                        if active {
                            sort_arrow(ui.painter(), resp.rect, self.sort_desc);
                        }
                        if resp.clicked() {
                            actions.push(Action::Sort(key));
                        }
                    }
                });
                ui.separator();

                if self.list.is_empty() {
                    ui.add_space(20.0);
                    let msg = if cur.is_unreadable() {
                        "This folder could not be read (access denied)."
                    } else {
                        "This folder is empty."
                    };
                    ui.vertical_centered(|ui| ui.label(RichText::new(msg).color(TEXT_DIM)));
                    return;
                }

                let spacing = ui.spacing().item_spacing.y;
                let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
                if let Some(idx) = self.list_scroll_to.take() {
                    let row = ROW_H + spacing;
                    let view_h = ui.available_height();
                    let prev = ui
                        .ctx()
                        .data(|d| d.get_temp::<f32>(Id::new("list-offset")))
                        .unwrap_or(0.0);
                    let top = idx as f32 * row;
                    let offset = if top < prev {
                        top
                    } else if top + row > prev + view_h {
                        top + row - view_h
                    } else {
                        prev
                    };
                    area = area.vertical_scroll_offset(offset.max(0.0));
                }
                let list = &self.list;
                let selected = self.selected;
                let output = area.show_rows(ui, ROW_H, list.len(), |ui, range| {
                    for i in range {
                        let id = list[i];
                        let n = tree.node(id);
                        let resp = list_row(ui, n, metric, cur_size, selected == Some(id));
                        if resp.clicked() {
                            actions.push(Action::Select(id));
                        }
                        if resp.double_clicked() && n.is_dir() && selected == Some(id) {
                            actions.push(Action::Open(id));
                        }
                        if resp.secondary_clicked() {
                            actions.push(Action::Select(id));
                        }
                        resp.context_menu(|ui| context_menu(ui, id, n.kind, busy, actions));
                    }
                });
                ui.ctx()
                    .data_mut(|d| d.insert_temp(Id::new("list-offset"), output.state.offset.y));
            });
    }

    pub(super) fn treemap(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let Some(tree) = &self.tree else { return };
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        let key = LayoutKey {
            node: self.current,
            w: rect.width().max(0.0) as u32,
            h: rect.height().max(0.0) as u32,
            metric: self.metric,
            generation: self.generation,
        };
        if self.tiles_key != Some(key) {
            let t0 = Instant::now();
            self.tiles = treemap::layout(
                tree,
                self.current,
                treemap::Rect::new(0.0, 0.0, key.w as f32, key.h as f32),
                self.metric,
                &LayoutOptions::default(),
            );
            self.tiles_key = Some(key);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            if ms > 50.0 {
                diag::log(&format!(
                    "treemap layout: {} tiles in {ms:.1} ms",
                    self.tiles.len()
                ));
            }
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BG);
        let origin = rect.min;
        let to_screen = |r: &treemap::Rect| {
            Rect::from_min_size(
                Pos2::new(origin.x + r.x, origin.y + r.y),
                Vec2::new(r.w, r.h),
            )
        };

        if self.tiles.is_empty() {
            let cur = tree.node(self.current);
            let msg = if cur.is_unreadable() {
                "This folder could not be read (access denied)."
            } else if tree.children(self.current).next().is_none() {
                "This folder is empty."
            } else {
                "Everything in this folder is 0 bytes."
            };
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                msg,
                FontId::proportional(16.0),
                TEXT_DIM,
            );
            return;
        }

        for tile in &self.tiles {
            let r = to_screen(&tile.rect);
            if r.width() < 0.5 || r.height() < 0.5 {
                continue;
            }
            match tile.kind {
                TileKind::Node(id) => {
                    let n = tree.node(id);
                    if n.is_dir() {
                        let frame = dir_color(tile.depth, n.is_unreadable());
                        // A folder too small to open up takes the hue of the
                        // file type that fills it. Nested folders stay the
                        // frame colour; the gap around them is the separator.
                        let fill = if n.is_unreadable() || tile.nested || n.allocated == 0 {
                            frame
                        } else {
                            let [cr, cg, cb] = n.category.fill(tile.depth);
                            Color32::from_rgb(cr, cg, cb)
                        };
                        let leaf = !tile.nested;
                        let drawn = if leaf { r.shrink(1.0) } else { r };
                        painter.rect_filled(drawn, 0.0, fill);
                        if leaf {
                            // 1px darker edge so a leaf folder does not melt
                            // into a neighbouring file of the same type.
                            painter.rect_stroke(
                                drawn,
                                0.0,
                                Stroke::new(1.0, BG),
                                StrokeKind::Inside,
                            );
                        }
                        if tile.has_header {
                            let header_h = 16.0_f32.min(r.height());
                            let header = Rect::from_min_size(r.min, Vec2::new(r.width(), header_h));
                            painter.rect_filled(header, 0.0, mix(fill, Color32::BLACK, 0.28));
                            let text =
                                format!("{}  {}", n.name, format::bytes(n.size(self.metric)));
                            painter.with_clip_rect(header.shrink(1.0)).text(
                                Pos2::new(r.min.x + 6.0, r.min.y + 3.0),
                                Align2::LEFT_TOP,
                                text,
                                FontId::proportional(11.0),
                                Color32::from_rgb(214, 218, 226),
                            );
                        } else if !tile.nested {
                            label_tile(&painter, drawn, &n.name, n.size(self.metric), fill);
                        }
                        // One strip, drawn last, so the header fill cannot cover it.
                        if tile.depth == 0 && !n.is_unreadable() {
                            let [ar, ag, ab] = n.category.accent();
                            let strip_h = 2.0_f32.min(r.height());
                            painter.rect_filled(
                                Rect::from_min_size(r.min, Vec2::new(r.width(), strip_h)),
                                0.0,
                                Color32::from_rgb(ar, ag, ab),
                            );
                        }
                    } else {
                        let [cr, cg, cb] = n.category.fill(tile.depth);
                        let fill = Color32::from_rgb(cr, cg, cb);
                        // Inset plus a canvas stroke: sibling files share no
                        // gap in the layout, so without this two videos read
                        // as one tile.
                        let drawn = r.shrink(1.0);
                        painter.rect_filled(drawn, 0.0, fill);
                        painter.rect_stroke(drawn, 0.0, Stroke::new(1.0, BG), StrokeKind::Inside);
                        label_tile(&painter, drawn, &n.name, n.size(self.metric), fill);
                    }
                }
                TileKind::Rest {
                    count,
                    size,
                    largest,
                    ..
                } => {
                    let base = if largest != NO_NODE && tree.node(largest).kind == NodeKind::File {
                        let [cr, cg, cb] = tree.node(largest).category.fill(tile.depth);
                        Color32::from_rgb(cr, cg, cb)
                    } else {
                        dir_color(tile.depth, false)
                    };
                    let fill = mix(base, BG, 0.35);
                    let drawn = r.shrink(1.0);
                    painter.rect_filled(drawn, 0.0, fill);
                    painter.rect_stroke(drawn, 0.0, Stroke::new(1.0, BG), StrokeKind::Inside);
                    if r.width() > 60.0 && r.height() > 16.0 {
                        label_tile(
                            &painter,
                            drawn,
                            &format!("{} smaller", format::count(count as u64)),
                            size,
                            fill,
                        );
                    }
                }
            }
        }

        // Selection and hover outlines.
        if let Some(sel) = self.selected {
            if let Some(t) = self.tiles.iter().find(|t| t.kind == TileKind::Node(sel)) {
                painter.rect_stroke(
                    to_screen(&t.rect),
                    0.0,
                    Stroke::new(2.0, SELECT),
                    StrokeKind::Inside,
                );
            }
        }
        let hovered = response
            .hover_pos()
            .and_then(|p| treemap::hit_test(&self.tiles, p.x - origin.x, p.y - origin.y))
            .map(|i| self.tiles[i]);
        if let Some(t) = hovered {
            let ring = to_screen(&t.rect);
            // Skip the ring when it would just restate the selection.
            let same = self
                .selected
                .is_some_and(|sel| t.kind == TileKind::Node(sel));
            if !same {
                painter.rect_stroke(
                    ring,
                    0.0,
                    Stroke::new(1.0, Color32::from_white_alpha(140)),
                    StrokeKind::Inside,
                );
            }
        }

        if response.secondary_clicked() {
            self.context_node = hovered.and_then(|t| match t.kind {
                TileKind::Node(id) => Some(id),
                TileKind::Rest { .. } => None,
            });
            if let Some(id) = self.context_node {
                actions.push(Action::Select(id));
            }
        }
        if response.double_clicked() {
            match hovered.map(|t| t.kind) {
                Some(TileKind::Node(id)) => actions.push(Action::Open(id)),
                Some(TileKind::Rest { parent, .. }) => actions.push(Action::Open(parent)),
                None => {}
            }
        } else if response.clicked() {
            match hovered.map(|t| t.kind) {
                Some(TileKind::Node(id)) => actions.push(Action::Select(id)),
                _ => self.selected = None,
            }
        }
        if response.clicked_by(PointerButton::Middle) {
            actions.push(Action::Up);
        }

        let busy = self.task.is_some();
        if let Some(id) = self.context_node {
            let kind = tree.node(id).kind;
            response.context_menu(|ui| context_menu(ui, id, kind, busy, actions));
        }

        if let Some(t) = hovered {
            let metric = self.metric;
            let current = self.current;
            response.on_hover_ui_at_pointer(|ui| {
                ui.set_max_width(480.0);
                match t.kind {
                    TileKind::Node(id) => {
                        let n = tree.node(id);
                        let parent = tree.node(n.parent);
                        ui.label(RichText::new(&*n.name).strong().color(Color32::WHITE));
                        ui.label(
                            RichText::new(tree.path_of(id).display().to_string())
                                .size(11.5)
                                .color(TEXT_DIM),
                        );
                        ui.label(format!(
                            "{} on disk · {} logical",
                            format::bytes(n.allocated),
                            format::bytes(n.logical)
                        ));
                        let parent_name = if n.parent == Tree::ROOT {
                            tree.root_path.display().to_string()
                        } else {
                            parent.name.to_string()
                        };
                        ui.label(format!(
                            "{} of {parent_name}",
                            format::percent(n.size(metric), parent.size(metric))
                        ));
                        if n.parent != current {
                            ui.label(format!(
                                "{} of the current folder",
                                format::percent(n.size(metric), tree.node(current).size(metric))
                            ));
                        }
                        match n.kind {
                            NodeKind::Dir => {
                                ui.label(format!("Folder with {} files", format::count(n.files)));
                                ui.label(
                                    RichText::new("Double-click to open · right-click for actions")
                                        .size(11.0)
                                        .color(TEXT_DIM),
                                );
                            }
                            NodeKind::File => {
                                ui.label(
                                    RichText::new(n.category.label()).size(11.5).color(TEXT_DIM),
                                );
                            }
                            NodeKind::Link => {}
                        }
                    }
                    TileKind::Rest {
                        parent,
                        count,
                        size,
                        ..
                    } => {
                        ui.label(
                            RichText::new(format!("{} smaller items", format::count(count as u64)))
                                .strong(),
                        );
                        ui.label(format!(
                            "{} in {}",
                            format::bytes(size),
                            tree.path_of(parent).display()
                        ));
                        ui.label(
                            RichText::new(
                                "Too small to draw individually; open the folder to see them.",
                            )
                            .size(11.0)
                            .color(TEXT_DIM),
                        );
                    }
                }
            });
        }
    }
}
