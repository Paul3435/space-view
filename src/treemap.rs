//! Squarified treemap layout (Bruls, Huizing & van Wijk, 2000), nested.

use crate::tree::{Metric, NodeId, Tree};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect {
            x,
            y,
            w: w.max(0.0),
            h: h.max(0.0),
        }
    }
    pub fn area(&self) -> f32 {
        self.w * self.h
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
    pub fn shrink(&self, left: f32, top: f32, right: f32, bottom: f32) -> Rect {
        Rect::new(
            self.x + left,
            self.y + top,
            self.w - left - right,
            self.h - top - bottom,
        )
    }
}

/// Lays out `sizes` (sorted largest first, all > 0) inside `bounds` so that
/// each rectangle's area is proportional to its size and aspect ratios stay
/// close to 1. Returns one rectangle per size, in the same order.
pub fn squarify(sizes: &[f64], bounds: Rect) -> Vec<Rect> {
    let mut out = Vec::with_capacity(sizes.len());
    let total: f64 = sizes.iter().sum();
    if sizes.is_empty() || total <= 0.0 || bounds.w <= 0.0 || bounds.h <= 0.0 {
        out.resize(sizes.len(), Rect::new(bounds.x, bounds.y, 0.0, 0.0));
        return out;
    }
    let scale = (bounds.w as f64 * bounds.h as f64) / total;
    let areas: Vec<f64> = sizes.iter().map(|s| s * scale).collect();

    let (mut x, mut y, mut w, mut h) = (
        bounds.x as f64,
        bounds.y as f64,
        bounds.w as f64,
        bounds.h as f64,
    );
    let mut start = 0;
    while start < areas.len() {
        let side = w.min(h);
        // Grow the row while the worst aspect ratio improves.
        let mut end = start + 1;
        let mut row_sum = areas[start];
        let mut best = worst_ratio(row_sum, areas[start], areas[start], side);
        while end < areas.len() {
            let next_sum = row_sum + areas[end];
            let ratio = worst_ratio(next_sum, areas[start], areas[end], side);
            if ratio > best {
                break;
            }
            best = ratio;
            row_sum = next_sum;
            end += 1;
        }
        let last_row = end == areas.len();
        if w >= h {
            // Column along the left edge, full height.
            let col_w = if last_row { w } else { (row_sum / h).min(w) };
            let mut cy = y;
            for (i, &a) in areas[start..end].iter().enumerate() {
                let ih = if i + 1 == end - start {
                    y + h - cy
                } else {
                    a / col_w
                };
                out.push(Rect::new(x as f32, cy as f32, col_w as f32, ih as f32));
                cy += ih;
            }
            x += col_w;
            w -= col_w;
        } else {
            // Row along the top edge, full width.
            let row_h = if last_row { h } else { (row_sum / w).min(h) };
            let mut cx = x;
            for (i, &a) in areas[start..end].iter().enumerate() {
                let iw = if i + 1 == end - start {
                    x + w - cx
                } else {
                    a / row_h
                };
                out.push(Rect::new(cx as f32, y as f32, iw as f32, row_h as f32));
                cx += iw;
            }
            y += row_h;
            h -= row_h;
        }
        start = end;
    }
    out
}

/// Worst aspect ratio in a row with total area `sum`, largest item `max`,
/// smallest item `min`, laid along a side of length `side`.
fn worst_ratio(sum: f64, max: f64, min: f64, side: f64) -> f64 {
    if sum <= 0.0 || min <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let s2 = sum * sum;
    let w2 = side * side;
    (w2 * max / s2).max(s2 / (w2 * min))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TileKind {
    /// A file, link, or a directory drawn as a frame around its children.
    Node(NodeId),
    /// Children of a directory too small to draw individually, aggregated.
    /// `largest` is the biggest of them (used to pick a colour).
    Rest {
        parent: NodeId,
        count: u32,
        size: u64,
        largest: NodeId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub rect: Rect,
    pub kind: TileKind,
    pub depth: u16,
    /// For directory tiles: whether the header strip holds the name.
    pub has_header: bool,
    /// For directory tiles: whether its children are laid out inside it.
    pub nested: bool,
    /// For tiles with a header: how many single-child folders below it are
    /// merged into that header ("Steam › steamapps › common"), following
    /// [`only_dir_child`]. Those folders get no header of their own.
    pub chain: u8,
}

/// The only non-empty child of `id`, if there is exactly one and it is a folder.
pub fn only_dir_child(tree: &Tree, id: NodeId, metric: Metric) -> Option<NodeId> {
    let mut it = tree.children(id).filter(|&c| tree.node(c).size(metric) > 0);
    let first = it.next()?;
    if it.next().is_some() {
        return None;
    }
    tree.node(first).is_dir().then_some(first)
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutOptions {
    /// Stop after this many tiles (keeps painting fast on huge folders).
    pub max_tiles: usize,
    /// Items whose area would be below this (px²) are merged into a "Rest" tile.
    pub min_area: f32,
    /// Directories narrower/shorter than this are not subdivided.
    pub min_dir_side: f32,
    pub header: f32,
    /// Folders smaller than this get no header strip (it would crowd out colour).
    pub header_min_w: f32,
    pub header_min_h: f32,
    pub padding: f32,
    pub max_depth: u16,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            max_tiles: 25_000,
            min_area: 12.0,
            min_dir_side: 24.0,
            header: 16.0,
            header_min_w: 90.0,
            header_min_h: 48.0,
            padding: 2.0,
            max_depth: 12,
        }
    }
}

/// Nested layout of the subtree under `root` (which itself is not drawn).
/// Tiles are emitted breadth-first, so a later tile is always on top of an
/// earlier one: hit-testing should pick the last tile containing the point.
pub fn layout(
    tree: &Tree,
    root: NodeId,
    bounds: Rect,
    metric: Metric,
    opts: &LayoutOptions,
) -> Vec<Tile> {
    let mut tiles: Vec<Tile> = Vec::new();
    // (folder, area for its children, depth, its tile, tile whose header shows its name)
    let mut queue: VecDeque<(NodeId, Rect, u16, usize, usize)> = VecDeque::new();
    queue.push_back((root, bounds, 0, usize::MAX, usize::MAX));

    while let Some((dir, area, depth, _, owner)) = queue.pop_front() {
        if tiles.len() >= opts.max_tiles {
            // Out of budget: these folders will not get their children drawn.
            for (_, _, _, t, _) in queue.drain(..) {
                tiles[t].nested = false;
            }
            break;
        }
        let children = tree.children_by_size(dir, metric);
        let total: u64 = children.iter().map(|&c| tree.node(c).size(metric)).sum();
        if total == 0 || area.w < 1.0 || area.h < 1.0 {
            continue;
        }
        let px_per_byte = area.area() as f64 / total as f64;

        let mut sizes = Vec::new();
        let mut shown = Vec::new();
        let mut rest_size = 0u64;
        let mut rest_count = 0u32;
        let mut rest_largest = crate::tree::NO_NODE;
        for &c in &children {
            let s = tree.node(c).size(metric);
            if s == 0 {
                continue;
            }
            if (s as f64 * px_per_byte) as f32 >= opts.min_area && shown.len() < opts.max_tiles {
                sizes.push(s as f64);
                shown.push(c);
            } else {
                if rest_count == 0 {
                    rest_largest = c;
                }
                rest_size += s;
                rest_count += 1;
            }
        }
        if rest_count > 0 {
            sizes.push(rest_size as f64);
        }
        let rects = squarify(&sizes, area);
        // A folder whose only content is one folder shares its header.
        let chained = owner != usize::MAX
            && shown.len() == 1
            && rest_count == 0
            && tree.node(shown[0]).is_dir();
        let pad = if depth >= 2 { 1.0 } else { opts.padding };

        for (i, r) in rects.iter().enumerate() {
            if tiles.len() >= opts.max_tiles {
                break;
            }
            if i == shown.len() {
                tiles.push(Tile {
                    rect: *r,
                    kind: TileKind::Rest {
                        parent: dir,
                        count: rest_count,
                        size: rest_size,
                        largest: rest_largest,
                    },
                    depth,
                    has_header: false,
                    nested: false,
                    chain: 0,
                });
                continue;
            }
            let id = shown[i];
            let node = tree.node(id);
            let can_nest = node.is_dir()
                && depth + 1 < opts.max_depth
                && r.w >= opts.min_dir_side
                && r.h >= opts.min_dir_side;
            let has_header =
                can_nest && !chained && r.h >= opts.header_min_h && r.w >= opts.header_min_w;
            let tile_index = tiles.len();
            tiles.push(Tile {
                rect: *r,
                kind: TileKind::Node(id),
                depth,
                has_header,
                nested: false,
                chain: 0,
            });
            if can_nest && tree.children(id).next().is_some() {
                let (inner, next_owner) = if chained {
                    tiles[owner].chain = tiles[owner].chain.saturating_add(1);
                    (*r, owner)
                } else if has_header {
                    (r.shrink(pad, opts.header, pad, pad), tile_index)
                } else {
                    (r.shrink(pad, pad, pad, pad), usize::MAX)
                };
                if inner.w >= 2.0 && inner.h >= 2.0 {
                    tiles[tile_index].nested = true;
                    queue.push_back((id, inner, depth + 1, tile_index, next_owner));
                }
            }
        }
    }
    tiles
}

/// Index of the topmost (deepest) tile containing the point.
pub fn hit_test(tiles: &[Tile], x: f32, y: f32) -> Option<usize> {
    tiles.iter().rposition(|t| t.rect.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{ScanStats, ScannedDir, ScannedFile};
    use std::path::PathBuf;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    fn overlaps(a: &Rect, b: &Rect) -> bool {
        let eps = 1e-3;
        a.x + eps < b.x + b.w
            && b.x + eps < a.x + a.w
            && a.y + eps < b.y + b.h
            && b.y + eps < a.y + a.h
    }

    fn check_partition(sizes: &[f64], bounds: Rect) -> Vec<Rect> {
        let rects = squarify(sizes, bounds);
        assert_eq!(rects.len(), sizes.len());
        let total: f64 = sizes.iter().sum();
        let mut area_sum = 0.0;
        for (r, s) in rects.iter().zip(sizes) {
            let expected = (*s / total) as f32 * bounds.area();
            assert!(
                approx(r.area(), expected, expected * 1e-3 + 1e-2),
                "area {} vs {}",
                r.area(),
                expected
            );
            assert!(r.x >= bounds.x - 1e-3 && r.y >= bounds.y - 1e-3);
            assert!(r.x + r.w <= bounds.x + bounds.w + 1e-2);
            assert!(r.y + r.h <= bounds.y + bounds.h + 1e-2);
            area_sum += r.area();
        }
        assert!(approx(area_sum, bounds.area(), bounds.area() * 1e-4));
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                assert!(
                    !overlaps(&rects[i], &rects[j]),
                    "{:?} overlaps {:?}",
                    rects[i],
                    rects[j]
                );
            }
        }
        rects
    }

    #[test]
    fn areas_are_exactly_proportional() {
        let rects = check_partition(&[300.0, 100.0], Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(approx(rects[0].area() / rects[1].area(), 3.0, 1e-3));
    }

    #[test]
    fn classic_paper_example() {
        // The example from the squarified treemap paper: 6x4 with 6,6,4,3,2,2,1.
        let rects = check_partition(
            &[6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0],
            Rect::new(0.0, 0.0, 6.0, 4.0),
        );
        // First row is the two 6s stacked in a 3-wide column.
        assert!(approx(rects[0].w, 3.0, 1e-4) && approx(rects[0].h, 2.0, 1e-4));
        assert!(approx(rects[1].w, 3.0, 1e-4) && approx(rects[1].y, 2.0, 1e-4));
    }

    #[test]
    fn aspect_ratios_stay_reasonable() {
        let sizes: Vec<f64> = (1..=60).rev().map(|i| (i * i) as f64).collect();
        let rects = check_partition(&sizes, Rect::new(10.0, 20.0, 800.0, 500.0));
        let worst = rects
            .iter()
            .map(|r| (r.w / r.h).max(r.h / r.w))
            .fold(0.0f32, f32::max);
        assert!(worst < 12.0, "worst aspect ratio {worst}");
    }

    #[test]
    fn degenerate_inputs() {
        assert!(squarify(&[], Rect::new(0.0, 0.0, 10.0, 10.0)).is_empty());
        let r = squarify(&[5.0], Rect::new(1.0, 2.0, 10.0, 20.0));
        assert_eq!(r, vec![Rect::new(1.0, 2.0, 10.0, 20.0)]);
        let z = squarify(&[1.0, 2.0], Rect::new(0.0, 0.0, 0.0, 10.0));
        assert!(z.iter().all(|r| r.area() == 0.0));
        check_partition(&[1.0; 50], Rect::new(0.0, 0.0, 1000.0, 3.0));
        check_partition(&[1e12, 1.0, 1.0], Rect::new(0.0, 0.0, 300.0, 200.0));
    }

    fn f(name: &str, size: u64) -> ScannedFile {
        ScannedFile {
            name: name.into(),
            allocated: size,
            logical: size / 2,
            is_link: false,
        }
    }

    fn sample_tree() -> Tree {
        let mut many = Vec::new();
        for i in 0..5000 {
            many.push(f(&format!("tiny{i}"), 1));
        }
        let small = ScannedDir::new("small".into(), many, vec![], false);
        let inner = ScannedDir::new(
            "inner".into(),
            vec![f("x.mkv", 400_000), f("y.mkv", 200_000)],
            vec![],
            false,
        );
        let big = ScannedDir::new("big".into(), vec![f("z.iso", 300_000)], vec![inner], false);
        let root = ScannedDir::new(
            "".into(),
            vec![f("a.zip", 100_000)],
            vec![big, small],
            false,
        );
        Tree::from_scan(PathBuf::from("/r"), root, ScanStats::default())
    }

    #[test]
    fn nested_layout_places_children_inside_parents() {
        let t = sample_tree();
        let bounds = Rect::new(0.0, 0.0, 1000.0, 700.0);
        let tiles = layout(
            &t,
            Tree::ROOT,
            bounds,
            Metric::Allocated,
            &LayoutOptions::default(),
        );
        let find = |name: &str| {
            let id = t.find(&PathBuf::from("/r").join(name)).unwrap();
            tiles
                .iter()
                .find(|tile| tile.kind == TileKind::Node(id))
                .copied()
                .unwrap()
        };
        let big = find("big");
        let inner = find("big/inner");
        let x = find("big/inner/x.mkv");
        let inside = |a: &Rect, b: &Rect| {
            a.x >= b.x
                && a.y >= b.y
                && a.x + a.w <= b.x + b.w + 1e-2
                && a.y + a.h <= b.y + b.h + 1e-2
        };
        assert!(inside(&inner.rect, &big.rect));
        assert!(inside(&x.rect, &inner.rect));
        assert_eq!(big.depth, 0);
        assert_eq!(x.depth, 2);
        assert!(big.nested && inner.nested && !x.nested);
        // Hit testing returns the deepest tile.
        let hit = hit_test(&tiles, x.rect.x + x.rect.w / 2.0, x.rect.y + x.rect.h / 2.0).unwrap();
        assert_eq!(tiles[hit].kind, x.kind);
        // Top-level tiles cover the whole bounds in proportion to size.
        let top: f32 = tiles
            .iter()
            .filter(|t| t.depth == 0)
            .map(|t| t.rect.area())
            .sum();
        assert!(approx(top, bounds.area(), 1.0));
    }

    #[test]
    fn single_folder_chains_share_one_header() {
        // root/Steam/steamapps/common/{Game A, Game B}
        let a = ScannedDir::new("Game A".into(), vec![f("a.pak", 600_000)], vec![], false);
        let b = ScannedDir::new("Game B".into(), vec![f("b.pak", 400_000)], vec![], false);
        let common = ScannedDir::new("common".into(), vec![], vec![a, b], false);
        let apps = ScannedDir::new("steamapps".into(), vec![], vec![common], false);
        let steam = ScannedDir::new("Steam".into(), vec![], vec![apps], false);
        let other = ScannedDir::new("Other".into(), vec![f("o.bin", 500_000)], vec![], false);
        let root = ScannedDir::new("".into(), vec![], vec![steam, other], false);
        let t = Tree::from_scan(PathBuf::from("/r"), root, ScanStats::default());
        let tiles = layout(
            &t,
            Tree::ROOT,
            Rect::new(0.0, 0.0, 900.0, 600.0),
            Metric::Allocated,
            &LayoutOptions::default(),
        );
        let tile = |p: &str| {
            let id = t.find(&PathBuf::from("/r").join(p)).unwrap();
            *tiles.iter().find(|x| x.kind == TileKind::Node(id)).unwrap()
        };
        let steam = tile("Steam");
        assert!(steam.has_header);
        assert_eq!(
            steam.chain, 2,
            "steamapps and common are merged into Steam's header"
        );
        assert!(!tile("Steam/steamapps").has_header);
        assert!(!tile("Steam/steamapps/common").has_header);
        assert!(tile("Steam/steamapps/common/Game A").has_header);
        let id = t.find(&PathBuf::from("/r/Steam")).unwrap();
        let first = only_dir_child(&t, id, Metric::Allocated).unwrap();
        assert_eq!(&*t.node(first).name, "steamapps");
        assert!(only_dir_child(
            &t,
            t.find(&PathBuf::from("/r/Steam/steamapps/common")).unwrap(),
            Metric::Allocated
        )
        .is_none());
    }

    #[test]
    fn tiny_items_are_aggregated_into_a_rest_tile() {
        let t = sample_tree();
        let tiles = layout(
            &t,
            Tree::ROOT,
            Rect::new(0.0, 0.0, 1000.0, 700.0),
            Metric::Allocated,
            &LayoutOptions::default(),
        );
        let small = t.find(&PathBuf::from("/r/small")).unwrap();
        // "small" is 5000 bytes of ~1 MB (a ~59px square); each 1-byte file is far below min_area.
        let rest = tiles
            .iter()
            .find(|tile| matches!(tile.kind, TileKind::Rest { parent, .. } if parent == small))
            .expect("rest tile for the tiny files");
        let TileKind::Rest {
            parent,
            count,
            size,
            largest,
        } = rest.kind
        else {
            unreachable!()
        };
        assert_eq!((parent, count, size), (small, 5000, 5000));
        assert_eq!(t.node(largest).parent, small);
        assert!(
            tiles.len() < 100,
            "tiny files must not produce thousands of tiles"
        );
    }

    #[test]
    fn respects_tile_budget_and_metric() {
        let t = sample_tree();
        let opts = LayoutOptions {
            max_tiles: 3,
            ..LayoutOptions::default()
        };
        let tiles = layout(
            &t,
            Tree::ROOT,
            Rect::new(0.0, 0.0, 1000.0, 700.0),
            Metric::Logical,
            &opts,
        );
        assert!(tiles.len() <= 3);
        let empty = layout(
            &t,
            Tree::ROOT,
            Rect::new(0.0, 0.0, 0.0, 0.0),
            Metric::Allocated,
            &LayoutOptions::default(),
        );
        assert!(empty.is_empty());
    }
}
