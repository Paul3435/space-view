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

    /// RGB colour for this category.
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

    pub fn from_name(name: &str) -> Category {
        let ext = match name.rfind('.') {
            Some(i) if i > 0 && i + 1 < name.len() => &name[i + 1..],
            _ => return Category::Other,
        };
        if ext.len() > 10 {
            return Category::Other;
        }
        let ext = ext.to_ascii_lowercase();
        match ext.as_str() {
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
            "rs" | "c" | "cc" | "cpp" | "h" | "hpp" | "cs" | "py" | "js" | "mjs" | "tsx" | "jsx" | "go" | "java" | "kt" | "swift" | "rb" | "php" | "lua" | "sh"
            | "ps1" | "bat" | "cmd" | "html" | "css" | "scss" | "json" | "xml" | "yaml"
            | "yml" | "toml" | "sql" => Category::Code,
            "db" | "sqlite" | "sqlite3" | "mdb" | "accdb" | "dat" | "bin" | "pak" | "cache"
            | "log" | "etl" | "evtx" | "tmp" | "temp" | "bak" | "dmp" | "mdmp" | "blob"
            | "idx" | "pack" | "vpk" | "ldb" | "edb" | "chk" => Category::Data,
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
}
