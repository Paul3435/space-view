use std::path::{Path, PathBuf};

include!("src/icon_design.rs");

fn main() {
    println!("cargo:rerun-if-changed=assets/disktree.rc");
    println!("cargo:rerun-if-changed=assets/disktree.exe.manifest");
    println!("cargo:rerun-if-changed=src/icon_design.rs");

    // Embed icon, manifest (DPI awareness, longPathAware, asInvoker) and
    // version info into the Windows exe. Cross-compiling uses llvm-rc; see
    // scripts/build-windows.sh.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));

    let ico = out.join("disktree.ico");
    std::fs::write(&ico, ico_file(&[16, 20, 24, 32, 40, 48, 64, 128, 256])).expect("write icon");

    let template =
        std::fs::read_to_string(manifest_dir.join("assets/disktree.rc")).expect("read rc");
    let rc = template
        .replace("\"disktree.ico\"", &rc_path(&ico))
        .replace(
            "\"disktree.exe.manifest\"",
            &rc_path(&manifest_dir.join("assets/disktree.exe.manifest")),
        );
    let rc_file = out.join("disktree.rc");
    std::fs::write(&rc_file, rc).expect("write rc");

    embed_resource::compile_for(&rc_file, ["disktree"], embed_resource::NONE)
        .manifest_required()
        .expect("failed to compile Windows resources (is llvm-rc / rc.exe installed?)");
}

/// A quoted path for a .rc file; forward slashes avoid escape handling.
fn rc_path(p: &Path) -> String {
    format!("\"{}\"", p.display().to_string().replace('\\', "/"))
}

/// An .ico with one 32-bit BMP image per size (alpha channel, empty AND mask).
fn ico_file(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| bmp_image(s)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, img) in sizes.iter().zip(&images) {
        let dim = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

fn bmp_image(size: u32) -> Vec<u8> {
    let px = rgba(size);
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    // BITMAPINFOHEADER; height is doubled to cover the AND mask.
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(size * size * 4 + mask_row * size).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    // Pixels bottom-up, BGRA.
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[px[i + 2], px[i + 1], px[i], px[i + 3]]);
        }
    }
    out.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
    out
}
