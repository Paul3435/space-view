//! The egui application: start screen, scan progress, treemap + list browser,
//! and the delete flow.

use crate::diag;
use disktree::category::Category;
use disktree::format;
use disktree::ops::{self, DeleteMode, DeleteOutcome, Drive};
use disktree::safety::{self, Protection, SystemPaths};
use disktree::scan::{self, Progress, ScanError};
use disktree::tree::{Metric, NodeId, NodeKind, ScannedDir, Tree, NO_NODE};
use disktree::treemap::{self, LayoutOptions, Tile, TileKind};
use egui::{
    Align, Align2, Color32, Event, FontId, Id, Key, Layout, Modifiers, PointerButton, Pos2, Rect,
    RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(17, 19, 25);
const PANEL: Color32 = Color32::from_rgb(25, 28, 36);
const CARD: Color32 = Color32::from_rgb(33, 37, 47);
const CARD_HOVER: Color32 = Color32::from_rgb(42, 47, 60);
const TEXT_DIM: Color32 = Color32::from_rgb(150, 156, 170);
const ACCENT: Color32 = Color32::from_rgb(98, 160, 255);
const DANGER: Color32 = Color32::from_rgb(170, 48, 48);
const WARN: Color32 = Color32::from_rgb(240, 180, 70);
const SELECT: Color32 = Color32::from_rgb(255, 214, 90);
const ROW_H: f32 = 22.0;

enum Drives {
    Loading(Receiver<Vec<Drive>>),
    Ready(Vec<Drive>),
}

struct ScanJob {
    root: PathBuf,
    progress: Arc<Progress>,
    rx: Receiver<Result<Tree, ScanError>>,
    started: Instant,
    restore: Option<PathBuf>,
}

enum TaskKind {
    Delete {
        node: NodeId,
        name: String,
        size: u64,
        mode: DeleteMode,
    },
    Refresh {
        node: NodeId,
    },
}

enum TaskResult {
    Delete(DeleteOutcome),
    Refresh(Result<ScannedDir, String>),
}

struct Task {
    kind: TaskKind,
    rx: Receiver<TaskResult>,
    label: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SortKey {
    Size,
    Name,
    Files,
}

struct ConfirmDelete {
    node: NodeId,
    path: PathBuf,
    name: String,
    allocated: u64,
    logical: u64,
    files: u64,
    is_dir: bool,
    mode: DeleteMode,
    caution: Option<String>,
    typed: String,
    /// Permanent deletes need two confirmations: step 0, then step 1.
    step: u8,
    /// The final button only works after a short pause, so a double-click on
    /// "Continue…" cannot also hit "Delete permanently" in the same spot.
    armed_at: Instant,
}

enum Dialog {
    Confirm(ConfirmDelete),
    Message { title: String, body: String },
    Unreadable,
    Shortcuts,
}

struct Toast {
    text: String,
    until: Instant,
    error: bool,
}

enum Action {
    Select(NodeId),
    Open(NodeId),
    Up,
    GoTo(NodeId),
    Reveal(NodeId),
    CopyPath(NodeId),
    Delete(NodeId, DeleteMode),
    RefreshFolder(NodeId),
    Rescan,
    NewScan,
    Sort(SortKey),
    ShowUnreadable,
    ShowShortcuts,
}

#[derive(Clone, Copy, PartialEq)]
struct LayoutKey {
    node: NodeId,
    w: u32,
    h: u32,
    metric: Metric,
    generation: u64,
}

pub struct DiskTreeApp {
    renderer: &'static str,
    sys: SystemPaths,
    drives: Drives,
    path_input: String,
    start_error: Option<String>,
    scan: Option<ScanJob>,
    tree: Option<Tree>,
    generation: u64,
    current: NodeId,
    selected: Option<NodeId>,
    context_node: Option<NodeId>,
    metric: Metric,
    sort: SortKey,
    sort_desc: bool,
    list: Vec<NodeId>,
    list_key: Option<(NodeId, Metric, SortKey, bool, u64)>,
    list_scroll_to: Option<usize>,
    tiles: Vec<Tile>,
    tiles_key: Option<LayoutKey>,
    dialog: Option<Dialog>,
    task: Option<Task>,
    toast: Option<Toast>,
    owner_hwnd: isize,
}

impl DiskTreeApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        renderer: &'static str,
        path: Option<PathBuf>,
    ) -> Self {
        setup_style(&cc.egui_ctx);
        load_system_fonts_in_background(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("disktree-worker-drives".into())
            .spawn(move || {
                let drives = ops::fixed_drives();
                let _ = tx.send(drives);
                ctx.request_repaint();
            });
        let drives = if spawned.is_ok() {
            Drives::Loading(rx)
        } else {
            Drives::Ready(Vec::new())
        };
        let mut app = DiskTreeApp {
            renderer,
            sys: SystemPaths::from_env(),
            drives,
            path_input: String::new(),
            start_error: None,
            scan: None,
            tree: None,
            generation: 0,
            current: Tree::ROOT,
            selected: None,
            context_node: None,
            metric: Metric::Allocated,
            sort: SortKey::Size,
            sort_desc: true,
            list: Vec::new(),
            list_key: None,
            list_scroll_to: None,
            tiles: Vec::new(),
            tiles_key: None,
            dialog: None,
            task: None,
            toast: None,
            owner_hwnd: 0,
        };
        if let Some(p) = path {
            app.path_input = p.display().to_string();
            app.start_scan(&cc.egui_ctx, p, None);
        }
        app
    }

    fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            until: Instant::now() + Duration::from_secs(6),
            error,
        });
    }

    // ----- scanning -------------------------------------------------------

    fn start_scan(&mut self, ctx: &egui::Context, root: PathBuf, restore: Option<PathBuf>) {
        if self.task.is_some() {
            self.toast("Wait for the current delete to finish first.", true);
            return;
        }
        let root = scan::normalize_root(&root);
        diag::log(&format!("scan started: {}", root.display()));
        let progress = Arc::new(Progress::default());
        let (tx, rx) = mpsc::channel();
        let (p, r, c) = (progress.clone(), root.clone(), ctx.clone());
        let spawned = std::thread::Builder::new()
            .name("disktree-worker-scan".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    scan::scan(&r, &p, scan::default_threads())
                }))
                .unwrap_or_else(|panic| Err(ScanError::Internal(scan::panic_message(&panic))));
                let _ = tx.send(result);
                c.request_repaint();
            });
        match spawned {
            Ok(_) => {
                self.start_error = None;
                self.dialog = None;
                self.scan = Some(ScanJob {
                    root,
                    progress,
                    rx,
                    started: Instant::now(),
                    restore,
                });
            }
            Err(e) => self.start_error = Some(format!("Could not start the scan: {e}")),
        }
    }

    fn poll_scan(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.scan else { return };
        let result = match job.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err(ScanError::Internal("the scan thread stopped".into()))
            }
        };
        let job = self.scan.take().expect("job present");
        match result {
            Ok(tree) => {
                diag::log(&format!(
                    "scan finished: {} nodes, {} files, {} unreadable, {:.2}s",
                    tree.len(),
                    tree.stats.files,
                    tree.stats.unreadable,
                    tree.stats.elapsed_secs
                ));
                self.set_tree(ctx, tree, job.restore);
            }
            Err(ScanError::Cancelled) => diag::log("scan cancelled"),
            Err(e) => {
                diag::log(&format!("scan failed: {e}"));
                if self.tree.is_some() {
                    self.toast(e.to_string(), true);
                } else {
                    self.start_error = Some(e.to_string());
                }
            }
        }
    }

    fn set_tree(&mut self, ctx: &egui::Context, tree: Tree, restore: Option<PathBuf>) {
        self.current = restore.and_then(|p| tree.find(&p)).unwrap_or(Tree::ROOT);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "disktree — {}",
            tree.root_path.display()
        )));
        if let Some(old) = self.tree.replace(tree) {
            // Freeing millions of nodes takes a moment; don't stall the UI.
            let _ = std::thread::Builder::new()
                .name("disktree-worker-free".into())
                .spawn(move || drop(old));
        }
        self.selected = None;
        self.generation += 1;
        self.list_scroll_to = Some(0);
    }

    // ----- background tasks (delete / refresh) ----------------------------

    fn spawn_task(
        &mut self,
        ctx: &egui::Context,
        kind: TaskKind,
        label: String,
        job: impl FnOnce() -> TaskResult + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        let c = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("disktree-worker-task".into())
            .spawn(move || {
                let r = job();
                let _ = tx.send(r);
                c.request_repaint();
            });
        match spawned {
            Ok(_) => self.task = Some(Task { kind, rx, label }),
            Err(e) => self.toast(format!("Could not start: {e}"), true),
        }
    }

    fn start_delete(&mut self, ctx: &egui::Context, c: ConfirmDelete) {
        let size = if self.metric == Metric::Allocated {
            c.allocated
        } else {
            c.logical
        };
        let verb = match c.mode {
            DeleteMode::RecycleBin => "Moving to Recycle Bin",
            DeleteMode::Permanent => "Deleting",
        };
        let label = format!("{verb}: {}", c.name);
        diag::log(&format!(
            "delete requested ({:?}): {}",
            c.mode,
            c.path.display()
        ));
        let (path, mode, owner) = (c.path.clone(), c.mode, self.owner_hwnd);
        self.spawn_task(
            ctx,
            TaskKind::Delete {
                node: c.node,
                name: c.name,
                size,
                mode,
            },
            label,
            move || TaskResult::Delete(ops::delete(&path, mode, owner)),
        );
    }

    fn start_refresh(&mut self, ctx: &egui::Context, node: NodeId) {
        let Some(tree) = &self.tree else { return };
        if tree.node(node).kind != NodeKind::Dir {
            return;
        }
        let path = tree.path_of(node);
        let label = format!("Refreshing {}", path.display());
        self.spawn_task(ctx, TaskKind::Refresh { node }, label, move || {
            let p = Progress::default();
            TaskResult::Refresh(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    scan::scan_subtree(&path, &p, scan::default_threads())
                }))
                .map_err(|panic| scan::panic_message(&panic))
                .and_then(|r| r.map_err(|e| e.to_string())),
            )
        });
    }

    fn poll_task(&mut self, ctx: &egui::Context) {
        let Some(task) = &self.task else { return };
        let result = match task.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => match task.kind {
                TaskKind::Delete { .. } => TaskResult::Delete(DeleteOutcome::Failed(
                    "The delete worker stopped unexpectedly.".into(),
                )),
                TaskKind::Refresh { .. } => {
                    TaskResult::Refresh(Err("The refresh worker stopped unexpectedly.".into()))
                }
            },
        };
        let task = self.task.take().expect("task present");
        match (task.kind, result) {
            (
                TaskKind::Delete {
                    node,
                    name,
                    size,
                    mode,
                },
                TaskResult::Delete(outcome),
            ) => {
                diag::log(&format!("delete finished: {outcome:?}"));
                match outcome {
                    DeleteOutcome::Deleted | DeleteOutcome::AlreadyGone => {
                        if let Some(tree) = &mut self.tree {
                            tree.remove(node);
                            if let Some(sel) = self.selected {
                                if tree.is_ancestor_or_self(node, sel) {
                                    self.selected = None;
                                }
                            }
                            if tree.is_ancestor_or_self(node, self.current) {
                                self.current = tree.node(node).parent;
                            }
                        }
                        self.generation += 1;
                        let msg = match (outcome, mode) {
                            (DeleteOutcome::AlreadyGone, _) => {
                                format!("{name} was already gone; removed it from the map.")
                            }
                            (_, DeleteMode::RecycleBin) => format!(
                                "Moved {name} to the Recycle Bin ({}).",
                                format::bytes(size)
                            ),
                            (_, DeleteMode::Permanent) => format!(
                                "Permanently deleted {name} ({} freed).",
                                format::bytes(size)
                            ),
                        };
                        self.toast(msg, false);
                    }
                    DeleteOutcome::Cancelled => {
                        self.toast(
                            "Cancelled. Anything already removed is reflected after a refresh.",
                            false,
                        );
                        self.start_refresh(ctx, node);
                    }
                    DeleteOutcome::Partial(why) => {
                        self.dialog = Some(Dialog::Message { title: "Partly deleted".into(), body: format!("{name}: {why}\n\nThe folder is being re-read so the sizes are accurate.") });
                        self.start_refresh(ctx, node);
                    }
                    DeleteOutcome::Failed(why) => {
                        self.dialog = Some(Dialog::Message {
                            title: "Could not delete".into(),
                            body: format!("{name}\n\n{why}"),
                        });
                    }
                }
            }
            (TaskKind::Refresh { node }, TaskResult::Refresh(res)) => match res {
                Ok(fresh) => {
                    if let Some(tree) = &mut self.tree {
                        tree.graft(node, fresh);
                        if let Some(sel) = self.selected {
                            if sel != node && tree.is_ancestor_or_self(node, sel) {
                                self.selected = None;
                            }
                        }
                        if self.current != node && tree.is_ancestor_or_self(node, self.current) {
                            self.current = node;
                        }
                    }
                    self.generation += 1;
                }
                Err(e) => {
                    // The folder itself may have been deleted meanwhile.
                    let gone = self
                        .tree
                        .as_ref()
                        .map(|t| !t.path_of(node).exists())
                        .unwrap_or(false);
                    if gone {
                        if let Some(tree) = &mut self.tree {
                            tree.remove(node);
                            if tree.is_ancestor_or_self(node, self.current) {
                                self.current = tree.node(node).parent;
                            }
                        }
                        self.selected = None;
                        self.generation += 1;
                    } else {
                        self.toast(format!("Refresh failed: {e}"), true);
                    }
                }
            },
            _ => {}
        }
    }

    // ----- actions --------------------------------------------------------

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        let Some(tree) = &self.tree else {
            if let Action::NewScan = action {
                self.scan = None;
            }
            return;
        };
        match action {
            Action::Select(n) => self.selected = Some(n),
            Action::Open(n) => {
                let node = tree.node(n);
                if node.is_dir() {
                    self.current = n;
                    self.selected = None;
                    self.list_scroll_to = Some(0);
                } else if node.parent != NO_NODE && node.parent != self.current {
                    self.current = node.parent;
                    self.selected = Some(n);
                }
            }
            Action::Up => {
                let parent = tree.node(self.current).parent;
                if parent != NO_NODE {
                    self.selected = Some(self.current);
                    self.current = parent;
                    self.list_scroll_to = self.selected.and_then(|s| self.list_position(s));
                }
            }
            Action::GoTo(n) => {
                if n != self.current {
                    self.selected = None;
                    self.current = n;
                    self.list_scroll_to = Some(0);
                }
            }
            Action::Reveal(n) => ops::reveal_in_explorer(&tree.path_of(n)),
            Action::CopyPath(n) => {
                let p = tree.path_of(n).display().to_string();
                ctx.copy_text(p.clone());
                self.toast(format!("Copied {p}"), false);
            }
            Action::Delete(n, mode) => self.request_delete(n, mode),
            Action::RefreshFolder(n) => {
                if self.task.is_some() {
                    self.toast("Wait for the current operation to finish first.", true);
                } else {
                    self.start_refresh(ctx, n);
                }
            }
            Action::Rescan => {
                let root = tree.root_path.clone();
                let restore = Some(tree.path_of(self.current));
                self.start_scan(ctx, root, restore);
            }
            Action::NewScan => {
                if self.task.is_some() {
                    self.toast("Wait for the current operation to finish first.", true);
                    return;
                }
                if let Some(old) = self.tree.take() {
                    let _ = std::thread::Builder::new()
                        .name("disktree-worker-free".into())
                        .spawn(move || drop(old));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Title("disktree".into()));
                self.selected = None;
                self.current = Tree::ROOT;
                self.generation += 1;
            }
            Action::Sort(k) => {
                if self.sort == k {
                    self.sort_desc = !self.sort_desc;
                } else {
                    self.sort = k;
                    self.sort_desc = k != SortKey::Name;
                }
            }
            Action::ShowUnreadable => self.dialog = Some(Dialog::Unreadable),
            Action::ShowShortcuts => self.dialog = Some(Dialog::Shortcuts),
        }
    }

    fn request_delete(&mut self, node: NodeId, mode: DeleteMode) {
        let Some(tree) = &self.tree else { return };
        if self.task.is_some() {
            self.toast("Wait for the current operation to finish first.", true);
            return;
        }
        if node == Tree::ROOT {
            self.dialog = Some(Dialog::Message {
                title: "disktree won't delete this".into(),
                body: "This is the folder you scanned. Go up a level (or scan its parent) to delete it.".into(),
            });
            return;
        }
        let path = tree.path_of(node);
        let n = tree.node(node);
        let caution = match safety::classify(&path.to_string_lossy(), n.kind, &self.sys) {
            Protection::Blocked(reason) => {
                self.dialog = Some(Dialog::Message {
                    title: "disktree won't delete this".into(),
                    body: format!("{}\n\n{reason}", path.display()),
                });
                return;
            }
            Protection::Caution(reason) => Some(reason),
            Protection::Normal => None,
        };
        self.dialog = Some(Dialog::Confirm(ConfirmDelete {
            node,
            name: n.name.to_string(),
            allocated: n.allocated,
            logical: n.logical,
            files: n.files,
            is_dir: n.is_dir(),
            path,
            mode,
            caution,
            typed: String::new(),
            step: 0,
            armed_at: Instant::now(),
        }));
    }

    fn list_position(&self, node: NodeId) -> Option<usize> {
        self.list.iter().position(|&n| n == node)
    }

    fn refresh_list(&mut self) {
        let Some(tree) = &self.tree else { return };
        let key = (
            self.current,
            self.metric,
            self.sort,
            self.sort_desc,
            self.generation,
        );
        if self.list_key == Some(key) {
            return;
        }
        let mut v: Vec<NodeId> = tree.children(self.current).collect();
        let metric = self.metric;
        match self.sort {
            SortKey::Size => v.sort_by(|&a, &b| {
                tree.node(b)
                    .size(metric)
                    .cmp(&tree.node(a).size(metric))
                    .then(a.cmp(&b))
            }),
            SortKey::Files => {
                v.sort_by(|&a, &b| tree.node(b).files.cmp(&tree.node(a).files).then(a.cmp(&b)))
            }
            SortKey::Name => {
                v.sort_by_cached_key(|&a| tree.node(a).name.to_lowercase());
                v.reverse();
            }
        }
        if !self.sort_desc {
            v.reverse();
        }
        self.list = v;
        self.list_key = Some(key);
    }

    // ----- keyboard -------------------------------------------------------

    fn handle_keys(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.dialog.is_some() || ctx.egui_wants_keyboard_input() || self.tree.is_none() {
            return;
        }
        let mut delete: Option<bool> = None;
        let (
            mut up,
            mut enter,
            mut home,
            mut f5,
            mut copy,
            mut reveal,
            mut prev,
            mut next,
            mut esc,
            mut help,
        ) = (
            false, false, false, false, false, false, false, false, false, false,
        );
        ctx.input_mut(|i| {
            // Take the modifiers from the Delete key event itself, so the mode
            // is decided by the key press, not by Shift's state later on.
            // On Windows egui-winit reports Shift+Delete as `Event::Cut` (the
            // legacy cut shortcut) and swallows the key event; Ctrl+X is also
            // `Cut` but has Ctrl held.
            let shift_only = i.modifiers.shift && !i.modifiers.command;
            i.events.retain(|e| match e {
                Event::Key {
                    key: Key::Delete,
                    pressed: true,
                    modifiers,
                    ..
                } if delete.is_none() => {
                    delete = Some(modifiers.shift);
                    false
                }
                Event::Cut if delete.is_none() && shift_only => {
                    delete = Some(true);
                    false
                }
                _ => true,
            });
            copy = i.events.iter().any(|e| matches!(e, Event::Copy))
                || i.consume_key(Modifiers::COMMAND, Key::C);
            reveal = i.consume_key(Modifiers::COMMAND, Key::E);
            up = i.consume_key(Modifiers::ALT, Key::ArrowUp)
                || i.consume_key(Modifiers::NONE, Key::Backspace)
                || i.pointer.button_pressed(PointerButton::Extra1);
            enter = i.consume_key(Modifiers::NONE, Key::Enter);
            home = i.consume_key(Modifiers::NONE, Key::Home);
            f5 = i.consume_key(Modifiers::NONE, Key::F5);
            prev = i.consume_key(Modifiers::NONE, Key::ArrowUp);
            next = i.consume_key(Modifiers::NONE, Key::ArrowDown);
            esc = i.consume_key(Modifiers::NONE, Key::Escape);
            help = i.consume_key(Modifiers::NONE, Key::F1);
        });
        if let Some(permanent) = delete {
            if let Some(sel) = self.selected {
                actions.push(Action::Delete(
                    sel,
                    if permanent {
                        DeleteMode::Permanent
                    } else {
                        DeleteMode::RecycleBin
                    },
                ));
            }
        }
        let target = self.selected.unwrap_or(self.current);
        if copy {
            actions.push(Action::CopyPath(target));
        }
        if reveal {
            actions.push(Action::Reveal(target));
        }
        if up {
            actions.push(Action::Up);
        }
        if enter {
            if let Some(sel) = self.selected {
                actions.push(Action::Open(sel));
            }
        }
        if home {
            actions.push(Action::GoTo(Tree::ROOT));
        }
        if f5 {
            actions.push(Action::Rescan);
        }
        if help {
            actions.push(Action::ShowShortcuts);
        }
        if esc {
            self.selected = None;
        }
        if prev || next {
            self.refresh_list();
            if !self.list.is_empty() {
                let idx = self.selected.and_then(|s| self.list_position(s));
                let new = match (idx, next) {
                    (None, _) => 0,
                    (Some(i), true) => (i + 1).min(self.list.len() - 1),
                    (Some(i), false) => i.saturating_sub(1),
                };
                self.selected = Some(self.list[new]);
                self.list_scroll_to = Some(new);
            }
        }
    }
}

impl eframe::App for DiskTreeApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.owner_hwnd == 0 {
            self.owner_hwnd = window_handle(frame);
        }
        if let Drives::Loading(rx) = &self.drives {
            if let Ok(d) = rx.try_recv() {
                diag::log(&format!("found {} fixed drives", d.len()));
                self.drives = Drives::Ready(d);
            }
        }
        self.poll_scan(&ctx);
        self.poll_task(&ctx);

        // Dropping a folder onto the window scans it.
        let dropped: Option<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .find(|p| p.is_dir())
        });
        if let Some(p) = dropped {
            if self.scan.is_none() {
                self.path_input = p.display().to_string();
                self.start_scan(&ctx, p, None);
            }
        }

        let mut actions = Vec::new();
        if self.scan.is_some() {
            self.scanning_screen(ui);
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if self.tree.is_none() {
            self.start_screen(ui);
        } else {
            self.handle_keys(&ctx, &mut actions);
            self.browser(ui, &mut actions);
        }
        for a in actions {
            self.apply(&ctx, a);
        }
        self.dialogs(&ctx);
        self.paint_toast(&ctx);
        if self.task.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

fn window_handle(frame: &eframe::Frame) -> isize {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(h) = frame.window_handle() {
            if let RawWindowHandle::Win32(w) = h.as_raw() {
                return w.hwnd.get();
            }
        }
    }
    let _ = frame;
    0
}

/// egui's bundled fonts cover Latin, Greek and Cyrillic only. File names can
/// be anything, so add Windows' own fonts as fallbacks (read off-thread).
fn load_system_fonts_in_background(ctx: &egui::Context) {
    if !cfg!(windows) {
        return;
    }
    let ctx = ctx.clone();
    let _ = std::thread::Builder::new()
        .name("disktree-worker-fonts".into())
        .spawn(move || {
            let dir = std::env::var_os("SystemRoot")
                .or_else(|| std::env::var_os("windir"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
                .join("Fonts");
            let mut fonts = egui::FontDefinitions::default();
            let mut added = Vec::new();
            for file in [
                "segoeui.ttf",
                "seguisym.ttf",
                "msyh.ttc",
                "YuGothR.ttc",
                "meiryo.ttc",
                "malgun.ttf",
                "Nirmala.ttc",
                "seguiemj.ttf",
            ] {
                if let Ok(bytes) = std::fs::read(dir.join(file)) {
                    fonts
                        .font_data
                        .insert(file.to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
                    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                        fonts
                            .families
                            .entry(family)
                            .or_default()
                            .push(file.to_owned());
                    }
                    added.push(file);
                }
            }
            diag::log(&format!("fallback fonts: {added:?}"));
            if !added.is_empty() {
                ctx.set_fonts(fonts);
                ctx.request_repaint();
            }
        });
}

fn setup_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(12, 14, 19);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(58, 64, 80));
    visuals.faint_bg_color = CARD;
    visuals.selection.bg_fill = Color32::from_rgb(45, 90, 160);
    visuals.hyperlink_color = ACCENT;
    // The palette is dark; don't follow a light OS theme.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 4.0);
    });
}

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

impl DiskTreeApp {
    fn start_screen(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut scan_target: Option<PathBuf> = None;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(24.0)).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let column = 760.0f32.min(ui.available_width());
                let margin = ((ui.available_width() - column) / 2.0).max(0.0);
                ui.horizontal_top(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(column);
                    ui.add_space(28.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("disktree").size(40.0).strong().color(Color32::WHITE));
                        ui.label(RichText::new("See what fills your disk, then remove it safely.").size(16.0).color(TEXT_DIM));
                    });
                    ui.add_space(26.0);

                    if let Some(err) = &self.start_error {
                        egui::Frame::new().fill(Color32::from_rgb(70, 30, 30)).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(err).color(Color32::from_rgb(255, 200, 200)));
                        });
                        ui.add_space(16.0);
                    }

                    section_title(ui, "Scan a drive");
                    match &self.drives {
                        Drives::Loading(_) => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Looking for drives…").color(TEXT_DIM));
                            });
                        }
                        Drives::Ready(drives) if drives.is_empty() => {
                            ui.label(RichText::new("No fixed drives found. Choose a folder below.").color(TEXT_DIM));
                        }
                        Drives::Ready(drives) => {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
                                for d in drives {
                                    if drive_card(ui, d) {
                                        scan_target = Some(d.root.clone());
                                    }
                                }
                            });
                        }
                    }

                    ui.add_space(26.0);
                    section_title(ui, "Or scan a folder");
                    ui.horizontal(|ui| {
                        let browse_w = if cfg!(windows) { 110.0 } else { 0.0 };
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut self.path_input)
                                .hint_text(if cfg!(windows) { r"C:\Users\you\Downloads" } else { "/home/you" })
                                .desired_width(ui.available_width() - 90.0 - browse_w)
                                .font(FontId::proportional(15.0)),
                        );
                        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        let go = ui.add_enabled(!self.path_input.trim().is_empty(), egui::Button::new("Scan")).clicked();
                        if (go || enter) && !self.path_input.trim().is_empty() {
                            scan_target = Some(PathBuf::from(self.path_input.trim().trim_matches('"')));
                        }
                        #[cfg(windows)]
                        if ui.button("Browse…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().set_title("Choose a folder to scan").pick_folder() {
                                self.path_input = p.display().to_string();
                                scan_target = Some(p);
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("You can also drop a folder onto this window.").color(TEXT_DIM));

                    ui.add_space(30.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Junctions, symlinks and mount points are shown but never followed, so nothing is counted twice. \
                             Some system folders can only be read when disktree runs as administrator; anything unreadable is \
                             listed after the scan.",
                        )
                        .color(TEXT_DIM)
                        .size(12.5),
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("v{} · renderer: {}", env!("CARGO_PKG_VERSION"), self.renderer)).color(TEXT_DIM).size(11.0));
                });
                });
            });
        });
        if let Some(t) = scan_target {
            self.start_scan(&ctx, t, None);
        }
    }

    fn scanning_screen(&mut self, ui: &mut Ui) {
        let Some(job) = &self.scan else { return };
        let mut cancel = ui.input(|i| i.key_pressed(Key::Escape));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(24.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(640.0);
                    ui.add_space((ui.available_height() * 0.18).max(20.0));
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(22.0));
                        ui.label(
                            RichText::new(format!("Scanning {}", job.root.display()))
                                .size(22.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                    });
                    ui.add_space(22.0);
                    let p = &job.progress;
                    let files = p.files.load(std::sync::atomic::Ordering::Relaxed);
                    let dirs = p.dirs.load(std::sync::atomic::Ordering::Relaxed);
                    let bytes = p.allocated.load(std::sync::atomic::Ordering::Relaxed);
                    let bad = p.unreadable.load(std::sync::atomic::Ordering::Relaxed);
                    let secs = job.started.elapsed().as_secs_f64();
                    egui::Grid::new("scan-stats")
                        .num_columns(2)
                        .spacing([40.0, 10.0])
                        .show(ui, |ui| {
                            stat(ui, "Files", &format::count(files));
                            stat(ui, "Folders", &format::count(dirs));
                            ui.end_row();
                            stat(ui, "Found so far", &format::bytes(bytes));
                            stat(ui, "Elapsed", &format::duration(secs));
                            ui.end_row();
                            stat(
                                ui,
                                "Speed",
                                &format!(
                                    "{} files/s",
                                    format::count((files as f64 / secs.max(0.001)) as u64)
                                ),
                            );
                            stat(ui, "Unreadable", &format::count(bad));
                            ui.end_row();
                        });
                    ui.add_space(18.0);
                    let cur = p.current();
                    ui.add(
                        egui::Label::new(RichText::new(cur).color(TEXT_DIM).size(12.0)).truncate(),
                    );
                    ui.add_space(22.0);
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Cancel").size(15.0))
                                .min_size(Vec2::new(120.0, 32.0)),
                        )
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if cancel {
            job.progress.cancel();
        }
    }

    fn browser(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
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

    fn top_bar(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let Some(tree) = &self.tree else { return };
        let busy = self.task.is_some();
        egui::Panel::top("top").frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(10, 8))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("disktree").strong().size(17.0).color(Color32::WHITE));
                ui.add_space(6.0);
                if ui.add_enabled(!busy, egui::Button::new("New scan")).on_hover_text("Back to the drive list").clicked() {
                    actions.push(Action::NewScan);
                }
                if ui.add_enabled(!busy, egui::Button::new("Rescan")).on_hover_text("Scan everything again (F5)").clicked() {
                    actions.push(Action::Rescan);
                }
                let can_up = tree.node(self.current).parent != NO_NODE;
                if ui.add_enabled(can_up, egui::Button::new("Up")).on_hover_text("Parent folder (Backspace)").clicked() {
                    actions.push(Action::Up);
                }
                ui.separator();

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("?").on_hover_text("Keyboard shortcuts (F1)").clicked() {
                        actions.push(Action::ShowShortcuts);
                    }
                    ui.selectable_value(&mut self.metric, Metric::Logical, "Logical size")
                        .on_hover_text("The file length Explorer shows as \"Size\"");
                    ui.selectable_value(&mut self.metric, Metric::Allocated, "Size on disk")
                        .on_hover_text("Space actually allocated on disk: what deleting frees (accounts for compression, sparse files and cloud placeholders)");
                    if tree.stats.unreadable > 0 {
                        let t = RichText::new(format!("{} unreadable", format::count(tree.stats.unreadable))).color(WARN);
                        if ui.add(egui::Button::new(t).frame(false)).on_hover_text("Folders that could not be read (click for details)").clicked() {
                            actions.push(Action::ShowUnreadable);
                        }
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        breadcrumbs(ui, tree, self.current, actions);
                    });
                });
            });
        });
    }

    fn status_bar(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let Some(tree) = &self.tree else { return };
        let _ = actions;
        egui::Panel::bottom("status").frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(10, 6))).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for c in Category::ALL {
                    let [r, g, b] = c.rgb();
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
                    ui.add_space(-8.0);
                    ui.label(RichText::new(c.label()).size(12.0).color(TEXT_DIM));
                }
            });
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

    fn side_panel(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let Some(tree) = &self.tree else { return };
        let metric = self.metric;
        let busy = self.task.is_some();
        egui::Panel::right("side")
            .resizable(true)
            .default_size(420.0)
            .size_range(300.0..=900.0)
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
                    egui::Label::new(
                        RichText::new(title)
                            .size(17.0)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .truncate(),
                );
                ui.label(
                    RichText::new(format!(
                        "{} · {} files · {} items",
                        format::bytes(cur_size),
                        format::count(cur.files),
                        format::count(self.list.len() as u64)
                    ))
                    .color(TEXT_DIM),
                );
                ui.add_space(8.0);

                // Selection card
                egui::Frame::new()
                    .fill(CARD)
                    .corner_radius(6.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match self.selected {
                            None => {
                                ui.label(
                                    RichText::new(
                                        "Click an item in the map or the list to select it.",
                                    )
                                    .color(TEXT_DIM),
                                );
                                ui.label(
                                    RichText::new("Double-click a folder to open it.")
                                        .color(TEXT_DIM),
                                );
                            }
                            Some(sel) => {
                                let n = tree.node(sel);
                                let path = tree.path_of(sel);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&*n.name)
                                            .strong()
                                            .size(15.0)
                                            .color(Color32::WHITE),
                                    )
                                    .truncate(),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(path.display().to_string())
                                            .size(11.5)
                                            .color(TEXT_DIM),
                                    )
                                    .wrap(),
                                );
                                let kind = match n.kind {
                                    NodeKind::Dir => {
                                        format!("Folder · {} files", format::count(n.files))
                                    }
                                    NodeKind::File => n.category.label().to_owned(),
                                    NodeKind::Link => "Link (not followed)".to_owned(),
                                };
                                ui.label(format!(
                                    "{} on disk · {} logical · {} of this folder · {kind}",
                                    format::bytes(n.allocated),
                                    format::bytes(n.logical),
                                    format::percent(n.size(metric), cur_size)
                                ));
                                ui.add_space(4.0);
                                ui.horizontal_wrapped(|ui| {
                                    if n.is_dir()
                                        && ui
                                            .button("Open")
                                            .on_hover_text("Zoom into this folder (Enter)")
                                            .clicked()
                                    {
                                        actions.push(Action::Open(sel));
                                    }
                                    if ui
                                        .button("Show in Explorer")
                                        .on_hover_text("Ctrl+E")
                                        .clicked()
                                    {
                                        actions.push(Action::Reveal(sel));
                                    }
                                    if ui.button("Copy path").on_hover_text("Ctrl+C").clicked() {
                                        actions.push(Action::CopyPath(sel));
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    let can = !busy && n.kind != NodeKind::Link;
                                    if ui
                                        .add_enabled(can, egui::Button::new("Move to Recycle Bin…"))
                                        .on_hover_text("Delete key. Asks for confirmation first.")
                                        .clicked()
                                    {
                                        actions.push(Action::Delete(sel, DeleteMode::RecycleBin));
                                    }
                                    if ui
                                        .add_enabled(
                                            can,
                                            egui::Button::new(
                                                RichText::new("Delete permanently…")
                                                    .color(Color32::from_rgb(255, 170, 170)),
                                            ),
                                        )
                                        .on_hover_text(
                                            "Shift+Delete. Bypasses the Recycle Bin; asks twice.",
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
                        if resp.double_clicked() && n.is_dir() {
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

    fn treemap(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
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
                        let fill = dir_color(tile.depth, n.is_unreadable());
                        painter.rect_filled(r, 0.0, fill);
                        painter.rect_stroke(r, 0.0, Stroke::new(1.0, BG), StrokeKind::Inside);
                        if tile.has_header {
                            let text =
                                format!("{}  {}", n.name, format::bytes(n.size(self.metric)));
                            painter.with_clip_rect(r.shrink(1.0)).text(
                                Pos2::new(r.min.x + 5.0, r.min.y + 2.0),
                                Align2::LEFT_TOP,
                                text,
                                FontId::proportional(11.5),
                                Color32::from_rgb(215, 220, 230),
                            );
                        } else if tree.children(id).next().is_none()
                            || r.width() < 24.0
                            || r.height() < 24.0
                        {
                            label_tile(&painter, r, &n.name, n.size(self.metric), fill);
                        }
                    } else {
                        let [cr, cg, cb] = n.category.rgb();
                        let base = Color32::from_rgb(cr, cg, cb);
                        let fill = shade(base, 1.0 - (tile.depth as f32 * 0.035).min(0.25));
                        painter.rect_filled(r, 0.0, fill);
                        if r.width() > 3.0 && r.height() > 3.0 {
                            painter.rect_stroke(
                                r,
                                0.0,
                                Stroke::new(1.0, shade(fill, 0.55)),
                                StrokeKind::Inside,
                            );
                        }
                        label_tile(&painter, r, &n.name, n.size(self.metric), fill);
                    }
                }
                TileKind::Rest { count, size, .. } => {
                    let fill = Color32::from_rgb(52, 56, 66);
                    painter.rect_filled(r, 0.0, fill);
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, BG), StrokeKind::Inside);
                    if r.width() > 60.0 && r.height() > 16.0 {
                        label_tile(
                            &painter,
                            r,
                            &format!("{} smaller items", format::count(count as u64)),
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
                    Stroke::new(2.5, SELECT),
                    StrokeKind::Inside,
                );
            }
        }
        let hovered = response
            .hover_pos()
            .and_then(|p| treemap::hit_test(&self.tiles, p.x - origin.x, p.y - origin.y))
            .map(|i| self.tiles[i]);
        if let Some(t) = hovered {
            painter.rect_stroke(
                to_screen(&t.rect),
                0.0,
                Stroke::new(1.5, Color32::WHITE),
                StrokeKind::Inside,
            );
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

    // ----- dialogs --------------------------------------------------------

    fn dialogs(&mut self, ctx: &egui::Context) {
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

    fn paint_toast(&mut self, ctx: &egui::Context) {
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

// ---------------------------------------------------------------------------
// Widgets and drawing helpers
// ---------------------------------------------------------------------------

fn section_title(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(15.0)
            .strong()
            .color(Color32::WHITE),
    );
    ui.add_space(4.0);
}

fn stat(ui: &mut Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).color(TEXT_DIM).size(12.0));
        ui.label(
            RichText::new(value)
                .size(20.0)
                .strong()
                .color(Color32::WHITE),
        );
    });
}

fn warning_box(ui: &mut Ui, text: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgb(80, 28, 28))
        .corner_radius(6.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(Color32::from_rgb(255, 205, 205)));
        });
}

fn drive_card(ui: &mut Ui, d: &Drive) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(232.0, 96.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 8.0, if resp.hovered() { CARD_HOVER } else { CARD });
    if resp.hovered() {
        p.rect_stroke(rect, 8.0, Stroke::new(1.0, ACCENT), StrokeKind::Inside);
    }
    let root = d.root.display().to_string();
    let title = if d.label.is_empty() {
        root.clone()
    } else {
        format!("{root}  {}", d.label)
    };
    p.with_clip_rect(rect.shrink(8.0)).text(
        rect.min + Vec2::new(14.0, 12.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(17.0),
        Color32::WHITE,
    );
    if d.total > 0 {
        let used = d.total.saturating_sub(d.free);
        let frac = used as f32 / d.total as f32;
        let bar = Rect::from_min_size(
            rect.min + Vec2::new(14.0, 44.0),
            Vec2::new(rect.width() - 28.0, 8.0),
        );
        p.rect_filled(bar, 4.0, Color32::from_rgb(55, 60, 74));
        let fill = Rect::from_min_size(bar.min, Vec2::new(bar.width() * frac, bar.height()));
        p.rect_filled(
            fill,
            4.0,
            if frac > 0.9 {
                Color32::from_rgb(229, 83, 83)
            } else {
                ACCENT
            },
        );
        let text = format!(
            "{} free of {}{}",
            format::bytes(d.free),
            format::bytes(d.total),
            if d.filesystem.is_empty() {
                String::new()
            } else {
                format!(" · {}", d.filesystem)
            }
        );
        p.text(
            rect.min + Vec2::new(14.0, 62.0),
            Align2::LEFT_TOP,
            text,
            FontId::proportional(12.5),
            TEXT_DIM,
        );
    } else {
        p.text(
            rect.min + Vec2::new(14.0, 50.0),
            Align2::LEFT_TOP,
            "Click to scan",
            FontId::proportional(12.5),
            TEXT_DIM,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn breadcrumbs(ui: &mut Ui, tree: &Tree, current: NodeId, actions: &mut Vec<Action>) {
    let mut chain = tree.ancestors(current);
    chain.reverse();
    let skip = chain.len().saturating_sub(6);
    if skip > 0 {
        ui.label(RichText::new("…").color(TEXT_DIM));
    }
    for (i, &n) in chain.iter().enumerate().skip(skip) {
        if i > skip || skip > 0 {
            ui.label(RichText::new("›").color(TEXT_DIM));
        }
        let name = if n == Tree::ROOT {
            tree.root_path.display().to_string()
        } else {
            tree.node(n).name.to_string()
        };
        let is_current = n == current;
        let text = if is_current {
            RichText::new(name).strong().color(Color32::WHITE)
        } else {
            RichText::new(name).color(ACCENT)
        };
        if ui.add(egui::Button::new(text).frame(false)).clicked() && !is_current {
            actions.push(Action::GoTo(n));
        }
    }
}

fn list_row(
    ui: &mut Ui,
    n: &disktree::tree::Node,
    metric: Metric,
    parent_size: u64,
    selected: bool,
) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 3.0, Color32::from_rgb(45, 70, 115));
    } else if resp.hovered() {
        p.rect_filled(rect, 3.0, CARD);
    }
    let size = n.size(metric);
    let frac = if parent_size > 0 {
        size as f32 / parent_size as f32
    } else {
        0.0
    };
    // Share-of-folder bar behind the name.
    let bar_w = (rect.width() - 190.0).max(0.0) * frac.clamp(0.0, 1.0);
    if bar_w > 0.5 {
        p.rect_filled(
            Rect::from_min_size(
                rect.min + Vec2::new(0.0, ROW_H - 3.0),
                Vec2::new(bar_w, 2.0),
            ),
            0.0,
            Color32::from_rgb(70, 110, 170),
        );
    }
    let icon = Rect::from_min_size(rect.min + Vec2::new(4.0, 6.0), Vec2::new(11.0, 10.0));
    match n.kind {
        NodeKind::Dir => {
            p.rect_filled(
                Rect::from_min_size(icon.min, Vec2::new(5.0, 3.0)),
                1.0,
                Color32::from_rgb(220, 180, 90),
            );
            p.rect_filled(
                Rect::from_min_max(icon.min + Vec2::new(0.0, 2.0), icon.max),
                1.5,
                Color32::from_rgb(220, 180, 90),
            );
        }
        NodeKind::File => {
            let [r, g, b] = n.category.rgb();
            p.rect_filled(icon.shrink(1.0), 2.0, Color32::from_rgb(r, g, b));
        }
        NodeKind::Link => {
            p.rect_stroke(
                icon.shrink(1.0),
                2.0,
                Stroke::new(1.0, TEXT_DIM),
                StrokeKind::Inside,
            );
        }
    }
    let name_rect = Rect::from_min_max(
        rect.min + Vec2::new(22.0, 0.0),
        Pos2::new(rect.max.x - 178.0, rect.max.y),
    );
    let mut name = n.name.to_string();
    if n.kind == NodeKind::Link {
        name.push_str("  (link, not followed)");
    } else if n.is_unreadable() {
        name.push_str("  (access denied)");
    }
    let color = if n.kind == NodeKind::Link || n.is_unreadable() {
        TEXT_DIM
    } else {
        Color32::from_rgb(225, 228, 235)
    };
    p.with_clip_rect(name_rect).text(
        Pos2::new(name_rect.min.x, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.5),
        color,
    );
    p.text(
        Pos2::new(rect.max.x - 92.0, rect.center().y),
        Align2::RIGHT_CENTER,
        format::bytes(size),
        FontId::proportional(13.0),
        Color32::WHITE,
    );
    p.text(
        Pos2::new(rect.max.x - 50.0, rect.center().y),
        Align2::RIGHT_CENTER,
        format::percent(size, parent_size),
        FontId::proportional(12.0),
        TEXT_DIM,
    );
    let files = if n.is_dir() {
        format::count(n.files)
    } else {
        String::new()
    };
    p.text(
        Pos2::new(rect.max.x - 2.0, rect.center().y),
        Align2::RIGHT_CENTER,
        files,
        FontId::proportional(12.0),
        TEXT_DIM,
    );
    resp
}

fn context_menu(ui: &mut Ui, id: NodeId, kind: NodeKind, busy: bool, actions: &mut Vec<Action>) {
    if kind == NodeKind::Dir && ui.button("Open").clicked() {
        actions.push(Action::Open(id));
        ui.close();
    }
    if ui.button("Show in Explorer").clicked() {
        actions.push(Action::Reveal(id));
        ui.close();
    }
    if ui.button("Copy path").clicked() {
        actions.push(Action::CopyPath(id));
        ui.close();
    }
    if kind == NodeKind::Dir
        && ui
            .add_enabled(!busy, egui::Button::new("Refresh this folder"))
            .clicked()
    {
        actions.push(Action::RefreshFolder(id));
        ui.close();
    }
    ui.separator();
    let can = !busy && kind != NodeKind::Link;
    if ui
        .add_enabled(can, egui::Button::new("Move to Recycle Bin…"))
        .clicked()
    {
        actions.push(Action::Delete(id, DeleteMode::RecycleBin));
        ui.close();
    }
    if ui
        .add_enabled(can, egui::Button::new("Delete permanently…"))
        .clicked()
    {
        actions.push(Action::Delete(id, DeleteMode::Permanent));
        ui.close();
    }
}

fn sort_arrow(painter: &egui::Painter, r: Rect, desc: bool) {
    let c = Pos2::new(r.max.x - 8.0, r.center().y);
    let (a, b) = (4.0, 3.0);
    let pts = if desc {
        vec![
            Pos2::new(c.x - a, c.y - b),
            Pos2::new(c.x + a, c.y - b),
            Pos2::new(c.x, c.y + b),
        ]
    } else {
        vec![
            Pos2::new(c.x - a, c.y + b),
            Pos2::new(c.x + a, c.y + b),
            Pos2::new(c.x, c.y - b),
        ]
    };
    painter.add(egui::Shape::convex_polygon(pts, ACCENT, Stroke::NONE));
}

fn label_tile(painter: &egui::Painter, r: Rect, name: &str, size: u64, fill: Color32) {
    if r.width() < 44.0 || r.height() < 15.0 {
        return;
    }
    let text_color = if luminance(fill) > 0.55 {
        Color32::from_rgb(20, 20, 24)
    } else {
        Color32::WHITE
    };
    let clip = painter.with_clip_rect(r.shrink(2.0));
    if r.height() >= 32.0 {
        clip.text(
            Pos2::new(r.min.x + 5.0, r.min.y + 4.0),
            Align2::LEFT_TOP,
            name,
            FontId::proportional(12.0),
            text_color,
        );
        clip.text(
            Pos2::new(r.min.x + 5.0, r.min.y + 18.0),
            Align2::LEFT_TOP,
            format::bytes(size),
            FontId::proportional(11.0),
            text_color.gamma_multiply(0.8),
        );
    } else {
        clip.text(
            Pos2::new(r.min.x + 4.0, r.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(11.0),
            text_color,
        );
    }
}

fn dir_color(depth: u16, unreadable: bool) -> Color32 {
    if unreadable {
        return Color32::from_rgb(78, 44, 44);
    }
    let d = depth.min(8) as u8;
    Color32::from_rgb(34 + d * 7, 39 + d * 7, 50 + d * 7)
}

fn shade(c: Color32, f: f32) -> Color32 {
    Color32::from_rgb(
        (c.r() as f32 * f) as u8,
        (c.g() as f32 * f) as u8,
        (c.b() as f32 * f) as u8,
    )
}

fn luminance(c: Color32) -> f32 {
    (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0
}
