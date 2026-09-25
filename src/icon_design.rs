// The app icon design: coloured treemap blocks on a rounded square.
// Shared by the window icon (src/icon.rs) and the exe icon generated in
// build.rs, via include!.

/// Blocks on a 256×256 grid: (x0, y0, x1, y1, rgb).
pub const BLOCKS: [(u32, u32, u32, u32, [u8; 3]); 5] = [
    (24, 24, 140, 232, [92, 58, 58]),
    (148, 24, 232, 120, [58, 78, 108]),
    (148, 128, 232, 184, [52, 86, 64]),
    (148, 192, 188, 232, [108, 92, 48]),
    (196, 192, 232, 232, [86, 62, 100]),
];
pub const BACKGROUND: [u8; 3] = [22, 24, 31];

pub fn rgba(size: u32) -> Vec<u8> {
    let mut px = vec![0u8; (size * size * 4) as usize];
    let radius = size as f32 * 0.18;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // Rounded-square mask.
            let cx = fx.clamp(radius, size as f32 - radius);
            let cy = fy.clamp(radius, size as f32 - radius);
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            let alpha = (radius - d + 0.5).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            let gx = fx * 256.0 / size as f32;
            let gy = fy * 256.0 / size as f32;
            let color = BLOCKS
                .iter()
                .find(|(x0, y0, x1, y1, _)| {
                    gx >= *x0 as f32 && gx < *x1 as f32 && gy >= *y0 as f32 && gy < *y1 as f32
                })
                .map(|b| b.4)
                .unwrap_or(BACKGROUND);
            let i = ((y * size + x) * 4) as usize;
            px[i..i + 3].copy_from_slice(&color);
            px[i + 3] = (alpha * 255.0) as u8;
        }
    }
    px
}
