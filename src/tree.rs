//! Flat, index-addressed file tree.
//!
//! A scan of a large drive yields millions of entries, so nodes live in one
//! `Vec` and refer to each other by `u32` index. Every directory's children are
//! stored contiguously (`first_child..first_child + child_count`), sorted by
//! on-disk size, largest first. Sizes and file counts are rolled up into every
//! directory when the tree is built, and kept consistent by [`Tree::remove`]
//! and [`Tree::graft`] so a delete never requires a full rescan.

use crate::category::Category;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub type NodeId = u32;
pub const NO_NODE: NodeId = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Dir,
    /// A symlink, junction or mount point. Never followed, counted as zero bytes.
    Link,
}

/// Which size a view is measured in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Metric {
    /// Bytes actually allocated on disk (what deleting frees).
    Allocated,
    /// Logical file length (what `dir` / file properties "Size" shows).
    Logical,
}

pub const FLAG_UNREADABLE: u8 = 1;
pub const FLAG_REMOVED: u8 = 2;

#[derive(Clone, Debug)]
pub struct Node {
    pub name: Box<str>,
    pub parent: NodeId,
    pub first_child: NodeId,
    pub child_count: u32,
    pub kind: NodeKind,
    /// File type; for directories, the type taking the most space inside.
    pub category: Category,
    pub flags: u8,
    pub allocated: u64,
    pub logical: u64,
    /// Number of files in this subtree (1 for a file, 0 for a link).
    pub files: u64,
}

impl Node {
    pub fn size(&self, metric: Metric) -> u64 {
        match metric {
            Metric::Allocated => self.allocated,
            Metric::Logical => self.logical,
        }
    }

    pub fn is_dir(&self) -> bool {
        self.kind == NodeKind::Dir
    }

    pub fn is_removed(&self) -> bool {
        self.flags & FLAG_REMOVED != 0
    }

    pub fn is_unreadable(&self) -> bool {
        self.flags & FLAG_UNREADABLE != 0
    }
}

/// A file (or link) as produced by the scanner.
#[derive(Clone, Debug)]
pub struct ScannedFile {
    pub name: Box<str>,
    pub allocated: u64,
    pub logical: u64,
    pub is_link: bool,
}

/// A directory subtree as produced by the scanner, with totals already rolled up.
#[derive(Clone, Debug, Default)]
pub struct ScannedDir {
    pub name: Box<str>,
    pub files: Vec<ScannedFile>,
    pub dirs: Vec<ScannedDir>,
    pub unreadable: bool,
    /// A second path to a directory already scanned elsewhere (a link the
    /// file system did not mark as one). Shown as a link, counted as zero.
    pub alias: bool,
    pub allocated: u64,
    pub logical: u64,
    pub file_count: u64,
    /// Allocated bytes per [`Category`] in this subtree (for colouring folders).
    pub by_category: [u64; Category::ALL.len()],
}

impl ScannedDir {
    /// The file type that takes the most space in this subtree.
    pub fn dominant(&self) -> Category {
        let (i, &max) = self
            .by_category
            .iter()
            .enumerate()
            .max_by_key(|(_, &b)| b)
            .expect("non-empty");
        if max == 0 {
            Category::Other
        } else {
            Category::ALL[i]
        }
    }

    pub fn alias(name: Box<str>) -> Self {
        ScannedDir {
            name,
            alias: true,
            ..ScannedDir::default()
        }
    }

    /// Builds a directory from its direct contents, computing the rolled-up totals.
    pub fn new(
        name: Box<str>,
        files: Vec<ScannedFile>,
        dirs: Vec<ScannedDir>,
        unreadable: bool,
    ) -> Self {
        let mut allocated = 0u64;
        let mut logical = 0u64;
        let mut file_count = 0u64;
        let mut by_category = [0u64; Category::ALL.len()];
        for f in &files {
            allocated = allocated.saturating_add(f.allocated);
            logical = logical.saturating_add(f.logical);
            if !f.is_link {
                file_count += 1;
                let c = Category::from_name(&f.name) as usize;
                by_category[c] = by_category[c].saturating_add(f.allocated);
            }
        }
        for d in &dirs {
            allocated = allocated.saturating_add(d.allocated);
            logical = logical.saturating_add(d.logical);
            file_count += d.file_count;
            for (acc, b) in by_category.iter_mut().zip(d.by_category) {
                *acc = acc.saturating_add(b);
            }
        }
        ScannedDir {
            name,
            files,
            dirs,
            unreadable,
            alias: false,
            allocated,
            logical,
            file_count,
            by_category,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScanStats {
    pub files: u64,
    pub dirs: u64,
    pub links: u64,
    pub unreadable: u64,
    pub elapsed_secs: f64,
    /// A sample of paths that could not be read, with the reason.
    pub error_samples: Vec<(String, String)>,
}

pub struct Tree {
    pub root_path: PathBuf,
    pub nodes: Vec<Node>,
    pub stats: ScanStats,
}

impl Tree {
    pub const ROOT: NodeId = 0;

    pub fn from_scan(root_path: PathBuf, root: ScannedDir, stats: ScanStats) -> Tree {
        let display_name: Box<str> = root_path.to_string_lossy().into();
        let mut tree = Tree {
            root_path,
            nodes: Vec::new(),
            stats,
        };
        tree.nodes.push(dir_node(display_name, NO_NODE, &root));
        tree.attach_children(Tree::ROOT, root);
        tree
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Live (not removed) children of `id`, in stored order (largest on-disk size first).
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let n = self.node(id);
        let range = if n.first_child == NO_NODE {
            0..0
        } else {
            n.first_child..n.first_child + n.child_count
        };
        range.filter(move |&c| !self.node(c).is_removed())
    }

    /// Children sorted by `metric`, largest first.
    pub fn children_by_size(&self, id: NodeId, metric: Metric) -> Vec<NodeId> {
        let mut v: Vec<NodeId> = self.children(id).collect();
        if metric != Metric::Allocated {
            v.sort_by(|&a, &b| {
                self.node(b)
                    .size(metric)
                    .cmp(&self.node(a).size(metric))
                    .then_with(|| self.node(a).name.cmp(&self.node(b).name))
            });
        }
        v
    }

    /// `id` and its ancestors up to the root, starting with `id`.
    pub fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = id;
        while cur != NO_NODE {
            out.push(cur);
            cur = self.node(cur).parent;
        }
        out
    }

    pub fn is_ancestor_or_self(&self, ancestor: NodeId, id: NodeId) -> bool {
        let mut cur = id;
        while cur != NO_NODE {
            if cur == ancestor {
                return true;
            }
            cur = self.node(cur).parent;
        }
        false
    }

    pub fn path_of(&self, id: NodeId) -> PathBuf {
        let chain = self.ancestors(id);
        let mut path = self.root_path.clone();
        for &n in chain.iter().rev().skip(1) {
            path.push(&*self.node(n).name);
        }
        path
    }

    /// Finds the node for `path`, if it is inside this tree.
    pub fn find(&self, path: &Path) -> Option<NodeId> {
        let rel = path.strip_prefix(&self.root_path).ok()?;
        let mut cur = Tree::ROOT;
        for comp in rel.components() {
            let name = comp.as_os_str().to_string_lossy();
            cur = self
                .children(cur)
                .find(|&c| eq_name(&self.node(c).name, &name))?;
        }
        Some(cur)
    }

    /// Removes `id` from the tree (after it was deleted on disk) and subtracts
    /// its sizes and file count from every ancestor.
    pub fn remove(&mut self, id: NodeId) {
        if id == Tree::ROOT || self.node(id).is_removed() {
            return;
        }
        let (alloc, logical, files) = {
            let n = self.node(id);
            (n.allocated, n.logical, n.files)
        };
        self.nodes[id as usize].flags |= FLAG_REMOVED;
        let mut cur = self.node(id).parent;
        while cur != NO_NODE {
            let n = &mut self.nodes[cur as usize];
            n.allocated = n.allocated.saturating_sub(alloc);
            n.logical = n.logical.saturating_sub(logical);
            n.files = n.files.saturating_sub(files);
            cur = n.parent;
        }
    }

    /// Replaces the contents of directory `id` with a fresh scan of it and
    /// adjusts every ancestor by the difference. Old descendants become
    /// unreachable; their slots are reclaimed on the next full scan.
    pub fn graft(&mut self, id: NodeId, fresh: ScannedDir) {
        let old = self.node(id).clone();
        {
            let n = &mut self.nodes[id as usize];
            n.allocated = fresh.allocated;
            n.logical = fresh.logical;
            n.files = fresh.file_count;
            n.first_child = NO_NODE;
            n.child_count = 0;
            n.category = fresh.dominant();
            n.flags = if fresh.unreadable { FLAG_UNREADABLE } else { 0 };
        }
        let mut cur = old.parent;
        while cur != NO_NODE {
            let n = &mut self.nodes[cur as usize];
            n.allocated = n
                .allocated
                .saturating_sub(old.allocated)
                .saturating_add(fresh.allocated);
            n.logical = n
                .logical
                .saturating_sub(old.logical)
                .saturating_add(fresh.logical);
            n.files = n
                .files
                .saturating_sub(old.files)
                .saturating_add(fresh.file_count);
            cur = n.parent;
        }
        self.attach_children(id, fresh);
    }

    /// Breadth-first flattening so every directory's children are contiguous.
    fn attach_children(&mut self, id: NodeId, dir: ScannedDir) {
        let mut queue: VecDeque<(NodeId, ScannedDir)> = VecDeque::new();
        queue.push_back((id, dir));
        while let Some((parent, dir)) = queue.pop_front() {
            let ScannedDir { files, dirs, .. } = dir;
            let mut items: Vec<Item> = Vec::with_capacity(files.len() + dirs.len());
            items.extend(files.into_iter().map(Item::File));
            items.extend(dirs.into_iter().map(Item::Dir));
            items.sort_by(|a, b| {
                b.allocated()
                    .cmp(&a.allocated())
                    .then_with(|| a.name().cmp(b.name()))
            });

            let first = self.nodes.len() as NodeId;
            {
                let p = &mut self.nodes[parent as usize];
                p.first_child = if items.is_empty() { NO_NODE } else { first };
                p.child_count = items.len() as u32;
            }
            for item in items {
                match item {
                    Item::File(f) => {
                        let category = if f.is_link {
                            Category::Other
                        } else {
                            Category::from_name(&f.name)
                        };
                        self.nodes.push(Node {
                            name: f.name,
                            parent,
                            first_child: NO_NODE,
                            child_count: 0,
                            kind: if f.is_link {
                                NodeKind::Link
                            } else {
                                NodeKind::File
                            },
                            category,
                            flags: 0,
                            allocated: f.allocated,
                            logical: f.logical,
                            files: if f.is_link { 0 } else { 1 },
                        });
                    }
                    Item::Dir(d) if d.alias => {
                        self.nodes.push(Node {
                            name: d.name,
                            parent,
                            first_child: NO_NODE,
                            child_count: 0,
                            kind: NodeKind::Link,
                            category: Category::Other,
                            flags: 0,
                            allocated: 0,
                            logical: 0,
                            files: 0,
                        });
                    }
                    Item::Dir(mut d) => {
                        let cid = self.nodes.len() as NodeId;
                        let name = std::mem::take(&mut d.name);
                        self.nodes.push(dir_node(name, parent, &d));
                        queue.push_back((cid, d));
                    }
                }
            }
        }
    }
}

enum Item {
    File(ScannedFile),
    Dir(ScannedDir),
}

impl Item {
    fn allocated(&self) -> u64 {
        match self {
            Item::File(f) => f.allocated,
            Item::Dir(d) => d.allocated,
        }
    }
    fn name(&self) -> &str {
        match self {
            Item::File(f) => &f.name,
            Item::Dir(d) => &d.name,
        }
    }
}

fn dir_node(name: Box<str>, parent: NodeId, d: &ScannedDir) -> Node {
    Node {
        name,
        parent,
        first_child: NO_NODE,
        child_count: 0,
        kind: NodeKind::Dir,
        category: d.dominant(),
        flags: if d.unreadable { FLAG_UNREADABLE } else { 0 },
        allocated: d.allocated,
        logical: d.logical,
        files: d.file_count,
    }
}

fn eq_name(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, allocated: u64, logical: u64) -> ScannedFile {
        ScannedFile {
            name: name.into(),
            allocated,
            logical,
            is_link: false,
        }
    }

    fn link(name: &str) -> ScannedFile {
        ScannedFile {
            name: name.into(),
            allocated: 0,
            logical: 0,
            is_link: true,
        }
    }

    fn dir(name: &str, files: Vec<ScannedFile>, dirs: Vec<ScannedDir>) -> ScannedDir {
        ScannedDir::new(name.into(), files, dirs, false)
    }

    /// root
    /// ├── big/            (4096 + 8192)
    /// │   ├── a.bin 4096
    /// │   └── deep/
    /// │       └── b.iso 8192
    /// ├── small.txt 100 (alloc 4096)
    /// └── junction  (link)
    fn sample() -> Tree {
        let deep = dir("deep", vec![file("b.iso", 8192, 8000)], vec![]);
        let big = dir("big", vec![file("a.bin", 4096, 4000)], vec![deep]);
        let root = dir(
            "",
            vec![file("small.txt", 4096, 100), link("junction")],
            vec![big],
        );
        Tree::from_scan(PathBuf::from("/scan"), root, ScanStats::default())
    }

    fn assert_consistent(tree: &Tree, id: NodeId) {
        let n = tree.node(id);
        if n.is_dir() {
            let mut a = 0;
            let mut l = 0;
            let mut f = 0;
            for c in tree.children(id) {
                assert_consistent(tree, c);
                a += tree.node(c).allocated;
                l += tree.node(c).logical;
                f += tree.node(c).files;
            }
            assert_eq!(n.allocated, a, "allocated of {}", n.name);
            assert_eq!(n.logical, l, "logical of {}", n.name);
            assert_eq!(n.files, f, "files of {}", n.name);
        }
    }

    #[test]
    fn scanned_dir_rolls_up_totals() {
        let d = dir(
            "x",
            vec![file("a", 10, 5), link("l")],
            vec![dir("y", vec![file("b", 20, 15)], vec![])],
        );
        assert_eq!(d.allocated, 30);
        assert_eq!(d.logical, 20);
        assert_eq!(d.file_count, 2, "links are not counted as files");
    }

    #[test]
    fn aggregates_sizes_into_every_directory() {
        let t = sample();
        let root = t.node(Tree::ROOT);
        assert_eq!(root.allocated, 4096 + 8192 + 4096);
        assert_eq!(root.logical, 4000 + 8000 + 100);
        assert_eq!(root.files, 3);
        assert_consistent(&t, Tree::ROOT);
    }

    #[test]
    fn children_are_sorted_largest_first_and_contiguous() {
        let t = sample();
        let kids: Vec<_> = t.children(Tree::ROOT).collect();
        let names: Vec<_> = kids.iter().map(|&k| &*t.node(k).name).collect();
        assert_eq!(names, vec!["big", "small.txt", "junction"]);
        for w in kids.windows(2) {
            assert_eq!(w[1], w[0] + 1);
        }
        let by_logical = t.children_by_size(Tree::ROOT, Metric::Logical);
        assert_eq!(&*t.node(by_logical[0]).name, "big");
        assert_eq!(&*t.node(by_logical[1]).name, "small.txt");
    }

    #[test]
    fn links_are_zero_sized_and_marked() {
        let t = sample();
        let j = t.find(Path::new("/scan/junction")).unwrap();
        assert_eq!(t.node(j).kind, NodeKind::Link);
        assert_eq!(t.node(j).allocated, 0);
        assert_eq!(t.node(j).files, 0);
    }

    #[test]
    fn path_of_and_find_round_trip() {
        let t = sample();
        let b = t.find(Path::new("/scan/big/deep/b.iso")).unwrap();
        assert_eq!(t.path_of(b), PathBuf::from("/scan/big/deep/b.iso"));
        assert_eq!(t.path_of(Tree::ROOT), PathBuf::from("/scan"));
        assert!(t.find(Path::new("/scan/nope")).is_none());
        assert!(t.find(Path::new("/elsewhere")).is_none());
        let big = t.find(Path::new("/scan/big")).unwrap();
        assert!(t.is_ancestor_or_self(big, b));
        assert!(!t.is_ancestor_or_self(b, big));
    }

    #[test]
    fn remove_updates_all_ancestors_without_rescan() {
        let mut t = sample();
        let b = t.find(Path::new("/scan/big/deep/b.iso")).unwrap();
        t.remove(b);
        let big = t.find(Path::new("/scan/big")).unwrap();
        let deep = t.find(Path::new("/scan/big/deep")).unwrap();
        assert_eq!(t.node(deep).allocated, 0);
        assert_eq!(t.node(big).allocated, 4096);
        assert_eq!(t.node(Tree::ROOT).allocated, 8192);
        assert_eq!(t.node(Tree::ROOT).files, 2);
        assert!(t.find(Path::new("/scan/big/deep/b.iso")).is_none());
        assert_consistent(&t, Tree::ROOT);

        // Removing twice or removing the root is a no-op.
        t.remove(b);
        t.remove(Tree::ROOT);
        assert_eq!(t.node(Tree::ROOT).allocated, 8192);
    }

    #[test]
    fn remove_directory_subtracts_whole_subtree() {
        let mut t = sample();
        let big = t.find(Path::new("/scan/big")).unwrap();
        t.remove(big);
        assert_eq!(t.node(Tree::ROOT).allocated, 4096);
        assert_eq!(t.node(Tree::ROOT).logical, 100);
        assert_eq!(t.node(Tree::ROOT).files, 1);
        assert_consistent(&t, Tree::ROOT);
    }

    #[test]
    fn graft_replaces_subtree_and_adjusts_ancestors() {
        let mut t = sample();
        let big = t.find(Path::new("/scan/big")).unwrap();
        // Pretend a partial delete left only a.bin plus a new file behind.
        let fresh = dir(
            "big",
            vec![file("a.bin", 4096, 4000), file("new.log", 1024, 1000)],
            vec![],
        );
        t.graft(big, fresh);
        assert_eq!(t.node(big).allocated, 5120);
        assert_eq!(t.node(Tree::ROOT).allocated, 5120 + 4096);
        assert_eq!(t.node(Tree::ROOT).files, 3);
        assert!(t.find(Path::new("/scan/big/deep")).is_none());
        assert!(t.find(Path::new("/scan/big/new.log")).is_some());
        assert_consistent(&t, Tree::ROOT);
    }

    #[test]
    fn directories_know_their_dominant_file_type() {
        let t = sample();
        let big = t.find(Path::new("/scan/big")).unwrap();
        assert_eq!(
            t.node(big).category,
            Category::DiskImage,
            "b.iso outweighs a.bin"
        );
        assert_eq!(t.node(Tree::ROOT).category, Category::DiskImage);
        let empty = ScannedDir::new("e".into(), vec![], vec![], false);
        assert_eq!(empty.dominant(), Category::Other);
    }

    #[test]
    fn alias_directories_become_zero_sized_links() {
        let real = dir("real", vec![file("x", 100, 100)], vec![]);
        let root = ScannedDir::new(
            "".into(),
            vec![],
            vec![real, ScannedDir::alias("again".into())],
            false,
        );
        let t = Tree::from_scan(PathBuf::from("/r"), root, ScanStats::default());
        let again = t.find(Path::new("/r/again")).unwrap();
        assert_eq!(t.node(again).kind, NodeKind::Link);
        assert_eq!(t.node(Tree::ROOT).allocated, 100);
        assert_consistent(&t, Tree::ROOT);
    }

    #[test]
    fn empty_and_unreadable_directories() {
        let root = ScannedDir::new(
            "".into(),
            vec![],
            vec![ScannedDir::new("locked".into(), vec![], vec![], true)],
            false,
        );
        let t = Tree::from_scan(PathBuf::from("/r"), root, ScanStats::default());
        let locked = t.find(Path::new("/r/locked")).unwrap();
        assert!(t.node(locked).is_unreadable());
        assert_eq!(t.children(locked).count(), 0);
        assert_eq!(t.node(Tree::ROOT).allocated, 0);
    }
}
