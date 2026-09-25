//! File-type categories used to colour the treemap.
//!
//! Known extensions map to a [`Category`]. Files with any other extension get
//! a stable colour of their own derived from the extension (as WinDirStat
//! does), so a folder full of an unfamiliar game format is still colourful
//! and one format is visibly distinct from another.

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
            Category::Other => "Other (colour per extension)",
        }
    }

    /// RGB colour for this category.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Category::Video => [235, 80, 80],
            Category::Audio => [176, 100, 236],
            Category::Image => [70, 196, 110],
            Category::Document => [70, 150, 245],
            Category::Archive => [245, 150, 50],
            Category::DiskImage => [236, 90, 170],
            Category::Executable => [240, 210, 70],
            Category::Code => [60, 205, 205],
            Category::Data => [165, 195, 70],
            Category::Other => [120, 128, 142],
        }
    }

    pub fn from_name(name: &str) -> Category {
        classify(name).0
    }
}

/// Colour of a file (or of a folder, from its dominant type): the category
/// colour, or for [`Category::Other`] with an extension key, a colour of its own.
pub fn rgb(category: Category, ext: u16) -> [u8; 3] {
    if category != Category::Other || ext == 0 {
        return category.rgb();
    }
    // Softer than the category colours so the two are easy to tell apart.
    let hue = (ext as f32 * 0.618_034).fract() * 360.0;
    hsv(hue, 0.42, 0.82)
}

fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}

/// The category of a file name, plus (for uncategorised extensions) a
/// non-zero 16-bit key identifying the extension; 0 when there is none.
pub fn classify(name: &str) -> (Category, u16) {
    let ext = match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => &name[i + 1..],
        _ => return (Category::Other, 0),
    };
    if ext.len() > 10 {
        return (Category::Other, 0);
    }
    // Lower-case into a stack buffer: this runs for every file scanned.
    let mut buf = [0u8; 10];
    let buf = &mut buf[..ext.len()];
    buf.copy_from_slice(ext.as_bytes());
    buf.make_ascii_lowercase();
    let Ok(ext) = std::str::from_utf8(buf) else {
        return (Category::Other, 0);
    };
    let category = match ext {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" | "ts"
        | "m2ts" | "vob" | "3gp" | "bk2" | "bik" | "usm" => Category::Video,
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "opus" | "wma" | "m4a" | "aiff" | "mid"
        | "wem" | "bnk" | "fsb" | "xwm" => Category::Audio,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "ico" | "tif" | "tiff"
        | "heic" | "heif" | "raw" | "cr2" | "nef" | "arw" | "dng" | "psd" | "avif" | "dds"
        | "tga" | "ktx" | "ktx2" | "exr" | "hdr" => Category::Image,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
        | "rtf" | "txt" | "md" | "epub" | "csv" | "one" | "pst" | "ost" => Category::Document,
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" | "lz4"
        | "nupkg" | "whl" | "jar" | "appx" | "msix" | "esd" | "wim" | "msu" | "zstd" => {
            Category::Archive
        }
        "iso" | "img" | "vhd" | "vhdx" | "vmdk" | "vdi" | "qcow2" | "avhdx" | "vmem" | "vmsn"
        | "dmg" => Category::DiskImage,
        "exe" | "dll" | "sys" | "msi" | "msp" | "com" | "scr" | "ocx" | "drv" | "efi" | "so"
        | "dylib" | "node" | "pyd" | "lib" | "a" | "pdb" | "mui" | "cpl" | "winmd" | "tlb"
        | "ax" | "mun" | "cat" | "mum" | "manifest" | "o" | "obj" | "pyc" | "class" => {
            Category::Executable
        }
        "rs" | "c" | "cc" | "cpp" | "h" | "hpp" | "cs" | "py" | "js" | "mjs" | "tsx" | "jsx"
        | "go" | "java" | "kt" | "swift" | "rb" | "php" | "lua" | "sh" | "ps1" | "bat" | "cmd"
        | "html" | "css" | "scss" | "json" | "xml" | "yaml" | "yml" | "toml" | "sql" => {
            Category::Code
        }
        "db" | "sqlite" | "sqlite3" | "mdb" | "accdb" | "dat" | "bin" | "pak" | "cache" | "log"
        | "etl" | "evtx" | "tmp" | "temp" | "bak" | "dmp" | "mdmp" | "blob" | "idx" | "pack"
        | "vpk" | "ldb" | "edb" | "chk" | "wad" | "client" | "bundle" | "ucas" | "utoc"
        | "uasset" | "umap" | "upk" | "gpk" | "forge" | "big" | "pck" | "arc" | "rpf" | "bsa"
        | "ba2" | "esm" | "assets" | "resource" | "ress" | "unity3d" | "sav" | "wal"
        | "journal" | "sst" | "pf" | "msgpack" | "ggpk" => Category::Data,
        _ => Category::Other,
    };
    let key = if category == Category::Other {
        ext_key(ext)
    } else {
        0
    };
    (category, key)
}

/// FNV-1a of the (lower-cased) extension, folded to 16 bits, never 0.
fn ext_key(ext: &str) -> u16 {
    let mut h: u32 = 0x811c_9dc5;
    for b in ext.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    let k = ((h >> 16) ^ h) as u16;
    if k == 0 {
        1
    } else {
        k
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
    fn game_and_windows_formats_are_categorised() {
        assert_eq!(Category::from_name("Aatrox.wad.client"), Category::Data);
        assert_eq!(
            Category::from_name("pakchunk51-Windows.ucas"),
            Category::Data
        );
        assert_eq!(Category::from_name("pak01_dir.vpk"), Category::Data);
        assert_eq!(Category::from_name("shell32.dll.mui"), Category::Executable);
        assert_eq!(Category::from_name("intro.bk2"), Category::Video);
        assert_eq!(Category::from_name("music.wem"), Category::Audio);
        assert_eq!(Category::from_name("rock_albedo.dds"), Category::Image);
    }

    #[test]
    fn unknown_extensions_get_a_stable_distinct_colour() {
        let (c1, k1) = classify("level.arm");
        let (_, k1b) = classify("OTHER.ARM");
        let (_, k2) = classify("level.tgt");
        assert_eq!(c1, Category::Other);
        assert_ne!(k1, 0);
        assert_eq!(k1, k1b, "case-insensitive and independent of the base name");
        assert_ne!(k1, k2);
        assert_ne!(rgb(Category::Other, k1), rgb(Category::Other, k2));
        assert_eq!(classify("Makefile"), (Category::Other, 0));
        assert_eq!(rgb(Category::Other, 0), Category::Other.rgb());
        assert_eq!(
            classify("a.mp4").1,
            0,
            "categorised files carry no extension key"
        );
    }

    #[test]
    fn names_without_real_extension_are_other() {
        assert_eq!(Category::from_name("Makefile"), Category::Other);
        assert_eq!(Category::from_name(".gitignore"), Category::Other);
        assert_eq!(Category::from_name("trailingdot."), Category::Other);
        assert_eq!(Category::from_name("x.unknownext"), Category::Other);
    }
}
