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

    /// Saturated swatch for the legend and list icons. The mosaic itself is
    /// painted from [`fill`](Self::fill), a muted version of the same hue.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Category::Video => [196, 92, 92],
            Category::Audio => [168, 112, 196],
            Category::Image => [86, 168, 112],
            Category::Document => [96, 140, 196],
            Category::Archive => [196, 140, 72],
            Category::DiskImage => [186, 96, 148],
            Category::Executable => [196, 176, 78],
            Category::Code => [78, 168, 176],
            Category::Data => [148, 156, 96],
            Category::Other => [140, 146, 158],
        }
    }

    /// Mosaic fill: one muted level for every kind, lifted slightly with
    /// depth so nesting reads without borders. `depth` 0 is the outermost
    /// tile of the current view.
    pub fn fill(self, depth: u16) -> [u8; 3] {
        let [r, g, b] = self.rgb();
        let step = depth.min(4) as f32;
        // Pull hard toward the canvas, then lift a little per nesting level.
        // Saturated blocks at full chroma are what made the map look like a
        // chart instead of a surface.
        let toward = 0.34;
        let lift = 10.0 + step * 7.0;
        let mute = |c: u8| ((c as f32) * toward + 22.0 * (1.0 - toward) + lift).clamp(0.0, 255.0);
        [mute(r) as u8, mute(g) as u8, mute(b) as u8]
    }

    /// The thin colour strip on a top-level directory: the hue, readable,
    /// still quieter than a full-chroma block.
    pub fn accent(self) -> [u8; 3] {
        let [r, g, b] = self.rgb();
        let mix = |c: u8| ((c as f32) * 0.72 + 28.0).clamp(0.0, 255.0) as u8;
        [mix(r), mix(g), mix(b)]
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
    fn mosaic_fills_are_muted_and_lift_with_depth() {
        for c in Category::ALL {
            let top = c.fill(0);
            let deep = c.fill(3);
            let sat = c.rgb();
            // Quieter than the legend swatch, so the map is a surface.
            let chroma = |rgb: [u8; 3]| {
                let max = rgb[0].max(rgb[1]).max(rgb[2]) as i16;
                let min = rgb[0].min(rgb[1]).min(rgb[2]) as i16;
                max - min
            };
            assert!(chroma(top) < chroma(sat), "{c:?}");
            assert!(deep[0] >= top[0] && deep[1] >= top[1] && deep[2] >= top[2]);
        }
    }
}
