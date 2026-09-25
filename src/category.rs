//! File-type categories used to colour the treemap.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Category {
    Video,
    Audio,
    Image,
    Document,
    Archive,
    DiskImage,
    Executable,
    Code,
    Data,
    Other,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Video,
        Category::Audio,
        Category::Image,
        Category::Document,
        Category::Archive,
        Category::DiskImage,
        Category::Executable,
        Category::Code,
        Category::Data,
        Category::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Video => "Video",
            Category::Audio => "Audio",
            Category::Image => "Images",
            Category::Document => "Documents",
            Category::Archive => "Archives",
            Category::DiskImage => "Disk images & VMs",
            Category::Executable => "Programs & libraries",
            Category::Code => "Source code",
            Category::Data => "Data, caches & logs",
            Category::Other => "Other",
        }
    }

    /// Saturated swatch. Depth-0 mosaic fills use the same triples: crushing
    /// them into one dark band made neighbouring types unreadable.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Category::Video => [229, 83, 83],
            Category::Audio => [178, 102, 224],
            Category::Image => [92, 190, 108],
            Category::Document => [77, 148, 235],
            Category::Archive => [240, 160, 60],
            Category::DiskImage => [214, 92, 160],
            Category::Executable => [232, 204, 72],
            Category::Code => [72, 196, 196],
            Category::Data => [140, 150, 110],
            Category::Other => [128, 136, 150],
        }
    }

    /// Mosaic fill. `depth` 0 is the category colour itself. Deeper tiles
    /// lift toward white by a visible step, without closing the gap between
    /// neighbouring categories.
    pub fn fill(self, depth: u16) -> [u8; 3] {
        let [r, g, b] = self.rgb();
        let t = (depth.min(3) as f32) * 0.05;
        let lift = |c: u8| (c as f32 * (1.0 - t) + 255.0 * t).round() as u8;
        [lift(r), lift(g), lift(b)]
    }

    /// Top-level folder strip and list swatch. Same hue as the fill, so the
    /// key and the map name the same colour.
    pub fn accent(self) -> [u8; 3] {
        self.rgb()
    }

    pub fn from_name(name: &str) -> Category {
        let ext = match name.rfind('.') {
            Some(i) if i > 0 && i + 1 < name.len() => &name[i + 1..],
            _ => return Category::Other,
        };
        if ext.len() > 10 {
            return Category::Other;
        }
        // Lower-case into a stack buffer: this runs for every file scanned.
        let mut buf = [0u8; 10];
        let buf = &mut buf[..ext.len()];
        buf.copy_from_slice(ext.as_bytes());
        buf.make_ascii_lowercase();
        let Ok(ext) = std::str::from_utf8(buf) else {
            return Category::Other;
        };
        match ext {
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg"
            | "ts" | "m2ts" | "vob" | "3gp" => Category::Video,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "opus" | "wma" | "m4a" | "aiff" | "mid" => {
                Category::Audio
            }
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "ico" | "tif" | "tiff"
            | "heic" | "heif" | "raw" | "cr2" | "nef" | "arw" | "dng" | "psd" | "avif" => {
                Category::Image
            }
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
            | "rtf" | "txt" | "md" | "epub" | "csv" | "one" | "pst" | "ost" => Category::Document,
            "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" | "lz4"
            | "nupkg" | "whl" | "jar" | "appx" | "msix" | "esd" | "wim" => Category::Archive,
            "iso" | "img" | "vhd" | "vhdx" | "vmdk" | "vdi" | "qcow2" | "avhdx" | "vmem"
            | "vmsn" | "dmg" => Category::DiskImage,
            "exe" | "dll" | "sys" | "msi" | "msp" | "com" | "scr" | "ocx" | "drv" | "efi"
            | "so" | "dylib" | "node" | "pyd" | "lib" | "a" | "pdb" => Category::Executable,
            "rs" | "c" | "cc" | "cpp" | "h" | "hpp" | "cs" | "py" | "js" | "mjs" | "tsx"
            | "jsx" | "go" | "java" | "kt" | "swift" | "rb" | "php" | "lua" | "sh" | "ps1"
            | "bat" | "cmd" | "html" | "css" | "scss" | "json" | "xml" | "yaml" | "yml"
            | "toml" | "sql" => Category::Code,
            "db" | "sqlite" | "sqlite3" | "mdb" | "accdb" | "dat" | "bin" | "pak" | "cache"
            | "log" | "etl" | "evtx" | "tmp" | "temp" | "bak" | "dmp" | "mdmp" | "blob" | "idx"
            | "pack" | "vpk" | "ldb" | "edb" | "chk" => Category::Data,
            _ => Category::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorises_by_extension_case_insensitively() {
        assert_eq!(Category::from_name("Holiday.MP4"), Category::Video);
        assert_eq!(Category::from_name("song.flac"), Category::Audio);
        assert_eq!(Category::from_name("a.tar.gz"), Category::Archive);
        assert_eq!(Category::from_name("Win11.iso"), Category::DiskImage);
        assert_eq!(Category::from_name("kernel32.DLL"), Category::Executable);
        assert_eq!(Category::from_name("main.rs"), Category::Code);
        assert_eq!(Category::from_name("pagefile.sys"), Category::Executable);
    }

    #[test]
    fn names_without_real_extension_are_other() {
        assert_eq!(Category::from_name("Makefile"), Category::Other);
        assert_eq!(Category::from_name(".gitignore"), Category::Other);
        assert_eq!(Category::from_name("trailingdot."), Category::Other);
        assert_eq!(Category::from_name("x.unknownext"), Category::Other);
    }

    #[test]
    fn mosaic_fills_match_the_legend_and_lift_with_depth() {
        for c in Category::ALL {
            assert_eq!(
                c.fill(0),
                c.rgb(),
                "{c:?} depth 0 must be the legend colour"
            );
            assert_eq!(c.accent(), c.rgb());
            let top = c.fill(0);
            let deep = c.fill(3);
            assert!(deep[0] >= top[0] && deep[1] >= top[1] && deep[2] >= top[2]);
            assert!(deep != top, "{c:?} lift must be visible");
        }
        // CIEDE2000 on the depth-0 fills. The rejected pass measured 6.3
        // between the closest pair; the previous palette measured 15.6.
        let fills: Vec<[u8; 3]> = Category::ALL.iter().map(|c| c.fill(0)).collect();
        let closest = closest_delta_e(&fills);
        assert!(
            closest >= 15.0,
            "closest category pair is {closest:.2}, want >= 15"
        );
        let deut = closest_below(&fills, deut_rgb, 10.0);
        let prot = closest_below(&fills, prot_rgb, 10.0);
        assert!(deut <= 10, "deuteranopia pairs under 10: {deut}");
        assert!(prot <= 6, "protanopia pairs under 10: {prot}");
        let lights: Vec<f32> = fills.iter().map(|c| lab(*c).0).collect();
        let min_l = lights.iter().cloned().fold(f32::MAX, f32::min);
        let max_l = lights.iter().cloned().fold(0.0, f32::max);
        assert!(min_l >= 50.0 && max_l >= 75.0, "L {min_l:.1}..{max_l:.1}");
    }

    fn closest_delta_e(colors: &[[u8; 3]]) -> f32 {
        let mut best = f32::MAX;
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                best = best.min(delta_e(lab(colors[i]), lab(colors[j])));
            }
        }
        best
    }

    fn closest_below(colors: &[[u8; 3]], sim: fn([u8; 3]) -> [u8; 3], limit: f32) -> usize {
        let simmed: Vec<[u8; 3]> = colors.iter().copied().map(sim).collect();
        let mut n = 0;
        for i in 0..simmed.len() {
            for j in (i + 1)..simmed.len() {
                if delta_e(lab(simmed[i]), lab(simmed[j])) < limit {
                    n += 1;
                }
            }
        }
        n
    }

    fn lin(c: f32) -> f32 {
        let c = c / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn unlin(c: f32) -> u8 {
        let c = c.clamp(0.0, 1.0);
        let v = if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    fn mat(m: [[f32; 3]; 3], rgb: [u8; 3]) -> [u8; 3] {
        let r = lin(rgb[0] as f32);
        let g = lin(rgb[1] as f32);
        let b = lin(rgb[2] as f32);
        let x = |row: [f32; 3]| row[0] * r + row[1] * g + row[2] * b;
        [unlin(x(m[0])), unlin(x(m[1])), unlin(x(m[2]))]
    }

    fn deut_rgb(rgb: [u8; 3]) -> [u8; 3] {
        mat(
            [
                [0.367322, 0.860646, -0.227968],
                [0.280085, 0.672501, 0.047413],
                [-0.011820, 0.042940, 0.968881],
            ],
            rgb,
        )
    }

    fn prot_rgb(rgb: [u8; 3]) -> [u8; 3] {
        mat(
            [
                [0.152286, 1.052583, -0.204868],
                [0.114503, 0.786281, 0.099216],
                [-0.003882, -0.048116, 1.051998],
            ],
            rgb,
        )
    }

    fn lab(rgb: [u8; 3]) -> (f32, f32, f32) {
        let (r, g, b) = (lin(rgb[0] as f32), lin(rgb[1] as f32), lin(rgb[2] as f32));
        let x = r * 0.4124564 + g * 0.3575761 + b * 0.1804375;
        let y = r * 0.2126729 + g * 0.7151522 + b * 0.0721750;
        let z = r * 0.0193339 + g * 0.1191920 + b * 0.9503041;
        let f = |t: f32| {
            let d = 6.0 / 29.0;
            if t > d * d * d {
                t.powf(1.0 / 3.0)
            } else {
                t / (3.0 * d * d) + 4.0 / 29.0
            }
        };
        let (fx, fy, fz) = (f(x / 0.95047), f(y), f(z / 1.08883));
        (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
    }

    fn delta_e(a: (f32, f32, f32), b: (f32, f32, f32)) -> f32 {
        let (l1, a1, b1) = a;
        let (l2, a2, b2) = b;
        let c1 = a1.hypot(b1);
        let c2 = a2.hypot(b2);
        let cbar = (c1 + c2) / 2.0;
        let g = 0.5 * (1.0 - (cbar.powi(7) / (cbar.powi(7) + 25.0_f32.powi(7))).sqrt());
        let a1p = (1.0 + g) * a1;
        let a2p = (1.0 + g) * a2;
        let c1p = a1p.hypot(b1);
        let c2p = a2p.hypot(b2);
        let h = |ap: f32, bp: f32| bp.atan2(ap).rem_euclid(std::f32::consts::TAU);
        let h1 = h(a1p, b1);
        let h2 = h(a2p, b2);
        let dlp = l2 - l1;
        let dcp = c2p - c1p;
        let mut dhp = h2 - h1;
        if c1p * c2p == 0.0 {
            dhp = 0.0;
        } else if dhp > std::f32::consts::PI {
            dhp -= std::f32::consts::TAU;
        } else if dhp < -std::f32::consts::PI {
            dhp += std::f32::consts::TAU;
        }
        let dhp = 2.0 * (c1p * c2p).sqrt() * (dhp / 2.0).sin();
        let lbar = (l1 + l2) / 2.0;
        let cbarp = (c1p + c2p) / 2.0;
        let hbar = if c1p * c2p == 0.0 {
            h1 + h2
        } else {
            let mut hbar = (h1 + h2) / 2.0;
            if (h1 - h2).abs() > std::f32::consts::PI {
                hbar += if hbar < std::f32::consts::PI {
                    std::f32::consts::PI
                } else {
                    -std::f32::consts::PI
                };
            }
            hbar
        };
        let t = 1.0 - 0.17 * (hbar - 30.0_f32.to_radians()).cos()
            + 0.24 * (2.0 * hbar).cos()
            + 0.32 * (3.0 * hbar + 6.0_f32.to_radians()).cos()
            - 0.20 * (4.0 * hbar - 63.0_f32.to_radians()).cos();
        let dtheta = 30.0_f32.to_radians() * (-((hbar.to_degrees() - 275.0) / 25.0).powi(2)).exp();
        let rc = 2.0 * (cbarp.powi(7) / (cbarp.powi(7) + 25.0_f32.powi(7))).sqrt();
        let sl = 1.0 + 0.015 * (lbar - 50.0).powi(2) / (20.0 + (lbar - 50.0).powi(2)).sqrt();
        let sc = 1.0 + 0.045 * cbarp;
        let sh = 1.0 + 0.015 * cbarp * t;
        let rt = -(2.0 * dtheta).sin() * rc;
        ((dlp / sl).powi(2)
            + (dcp / sc).powi(2)
            + (dhp / sh).powi(2)
            + rt * (dcp / sc) * (dhp / sh))
            .sqrt()
    }
}
