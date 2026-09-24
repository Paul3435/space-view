use crate::scanner::FileNode;

#[derive(Clone, Debug)]
pub struct TreemapRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub node_index: usize,
}

pub fn layout_treemap(
    node: &FileNode,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    min_size: f32,
) -> Vec<TreemapRect> {
    let mut rects = Vec::new();
    
    if w < min_size || h < min_size {
        return rects;
    }

    if !node.is_dir || node.children.is_empty() {
        return rects;
    }

    let mut children_with_indices: Vec<(usize, &FileNode)> = node
        .children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.size > 0)
        .collect();

    children_with_indices.sort_by(|a, b| b.1.size.cmp(&a.1.size));

    squarify(&children_with_indices, x, y, w, h, node.size, &mut rects);

    rects
}

fn squarify(
    children: &[(usize, &FileNode)],
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    total_size: u64,
    rects: &mut Vec<TreemapRect>,
) {
    if children.is_empty() || total_size == 0 {
        return;
    }

    squarify_recursive(children, x, y, w, h, total_size, rects);
}

fn squarify_recursive(
    items: &[(usize, &FileNode)],
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    total: u64,
    rects: &mut Vec<TreemapRect>,
) {
    if items.is_empty() {
        return;
    }

    if items.len() == 1 {
        let (idx, item) = items[0];
        rects.push(TreemapRect {
            x,
            y,
            w,
            h,
            node_index: idx,
        });
        return;
    }

    let vertical = h > w;
    let short_side = if vertical { w } else { h };

    let mut row_end = 0;
    let mut row_size = 0u64;
    let mut best_ratio = f32::INFINITY;

    for i in 0..items.len() {
        let new_row_size = row_size + items[i].1.size;
        let new_ratio = worst_ratio(&items[0..=i], short_side, new_row_size, total);

        if new_ratio < best_ratio {
            best_ratio = new_ratio;
            row_end = i + 1;
            row_size = new_row_size;
        } else {
            break;
        }
    }

    let row = &items[0..row_end];
    let rest = &items[row_end..];

    let row_ratio = row_size as f32 / total as f32;
    let row_length = if vertical { h * row_ratio } else { w * row_ratio };

    let (next_x, next_y, next_w, next_h) = if vertical {
        layout_row(row, x, y, w, row_length, row_size, false, rects);
        (x, y + row_length, w, h - row_length)
    } else {
        layout_row(row, x, y, row_length, h, row_size, true, rects);
        (x + row_length, y, w - row_length, h)
    };

    if !rest.is_empty() && next_w > 0.0 && next_h > 0.0 {
        squarify_recursive(rest, next_x, next_y, next_w, next_h, total, rects);
    }
}

fn layout_row(
    row: &[(usize, &FileNode)],
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    row_size: u64,
    horizontal: bool,
    rects: &mut Vec<TreemapRect>,
) {
    let mut offset = 0.0;

    for &(idx, item) in row {
        let ratio = item.size as f32 / row_size as f32;
        let length = if horizontal { w * ratio } else { h * ratio };

        let (rect_x, rect_y, rect_w, rect_h) = if horizontal {
            (x + offset, y, length, h)
        } else {
            (x, y + offset, w, length)
        };

        rects.push(TreemapRect {
            x: rect_x,
            y: rect_y,
            w: rect_w,
            h: rect_h,
            node_index: idx,
        });

        offset += length;
    }
}

fn worst_ratio(row: &[(usize, &FileNode)], length: f32, row_size: u64, total: u64) -> f32 {
    if row.is_empty() || row_size == 0 || total == 0 {
        return f32::INFINITY;
    }

    let row_area = (row_size as f32 / total as f32) * length * length;
    let row_width = row_area / length;

    let mut worst = 0.0f32;

    for &(_, item) in row {
        let item_area = (item.size as f32 / row_size as f32) * row_area;
        let item_height = item_area / row_width;
        let ratio = (row_width / item_height).max(item_height / row_width);
        worst = worst.max(ratio);
    }

    worst
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_node(size: u64, children: Vec<FileNode>) -> FileNode {
        let total_size = if children.is_empty() {
            size
        } else {
            children.iter().map(|c| c.size).sum()
        };

        FileNode {
            path: PathBuf::from("test"),
            name: "test".to_string(),
            size: total_size,
            is_dir: !children.is_empty(),
            children,
            extension: None,
        }
    }

    #[test]
    fn test_simple_layout() {
        let child1 = create_test_node(100, vec![]);
        let child2 = create_test_node(200, vec![]);
        let parent = create_test_node(0, vec![child1, child2]);

        let rects = layout_treemap(&parent, 0.0, 0.0, 100.0, 100.0, 1.0);

        assert_eq!(rects.len(), 2);
        
        let total_area: f32 = rects.iter().map(|r| r.w * r.h).sum();
        assert!((total_area - 10000.0).abs() < 1.0);
    }

    #[test]
    fn test_empty_children() {
        let parent = create_test_node(100, vec![]);
        let rects = layout_treemap(&parent, 0.0, 0.0, 100.0, 100.0, 1.0);
        assert_eq!(rects.len(), 0);
    }

    #[test]
    fn test_proportional_areas() {
        let child1 = create_test_node(100, vec![]);
        let child2 = create_test_node(300, vec![]);
        let parent = create_test_node(0, vec![child1, child2]);

        let rects = layout_treemap(&parent, 0.0, 0.0, 100.0, 100.0, 1.0);

        assert_eq!(rects.len(), 2);

        let rect_for_child1 = rects.iter().find(|r| r.node_index == 0).unwrap();
        let rect_for_child2 = rects.iter().find(|r| r.node_index == 1).unwrap();

        let area1 = rect_for_child1.w * rect_for_child1.h;
        let area2 = rect_for_child2.w * rect_for_child2.h;

        assert!(area2 > area1, "Larger item should have larger area");
        
        let total = area1 + area2;
        assert!((total - 10000.0).abs() < 1.0, "Total area should equal parent area");
    }
}
