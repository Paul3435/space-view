use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
    pub extension: Option<String>,
}

#[derive(Clone)]
pub struct ScanProgress {
    pub files_scanned: Arc<AtomicUsize>,
    pub dirs_scanned: Arc<AtomicUsize>,
    pub bytes_scanned: Arc<AtomicU64>,
    pub errors: Arc<AtomicUsize>,
    pub start_time: Arc<Mutex<Option<Instant>>>,
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self {
            files_scanned: Arc::new(AtomicUsize::new(0)),
            dirs_scanned: Arc::new(AtomicUsize::new(0)),
            bytes_scanned: Arc::new(AtomicU64::new(0)),
            errors: Arc::new(AtomicUsize::new(0)),
            start_time: Arc::new(Mutex::new(None)),
        }
    }
}

impl ScanProgress {
    pub fn start(&self) {
        *self.start_time.lock().unwrap() = Some(Instant::now());
    }

    pub fn elapsed(&self) -> Option<f64> {
        self.start_time
            .lock()
            .unwrap()
            .map(|t| t.elapsed().as_secs_f64())
    }
}

pub fn scan_directory(path: PathBuf, progress: ScanProgress) -> Option<FileNode> {
    progress.start();
    scan_recursive(&path, &progress)
}

fn scan_recursive(path: &Path, progress: &ScanProgress) -> Option<FileNode> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return None;
        }
    };

    #[cfg(windows)]
    {
        if is_reparse_point(&metadata) {
            return Some(FileNode {
                path: path.to_path_buf(),
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                size: 0,
                is_dir: false,
                children: vec![],
                extension: None,
            });
        }
    }

    if !metadata.is_dir() {
        let size = metadata.len();
        progress.files_scanned.fetch_add(1, Ordering::Relaxed);
        progress.bytes_scanned.fetch_add(size, Ordering::Relaxed);

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());

        return Some(FileNode {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            size,
            is_dir: false,
            children: vec![],
            extension,
        });
    }

    progress.dirs_scanned.fetch_add(1, Ordering::Relaxed);

    let entries: Vec<_> = match fs::read_dir(path) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return Some(FileNode {
                path: path.to_path_buf(),
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                size: 0,
                is_dir: true,
                children: vec![],
                extension: None,
            });
        }
    };

    let children: Vec<FileNode> = entries
        .par_iter()
        .filter_map(|entry| scan_recursive(&entry.path(), progress))
        .collect();

    let size: u64 = children.iter().map(|c| c.size).sum();

    Some(FileNode {
        path: path.to_path_buf(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        size,
        is_dir: true,
        children,
        extension: None,
    })
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    (metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0
}

pub fn get_extension_category(ext: Option<&String>) -> &'static str {
    match ext.map(|s| s.as_str()) {
        Some("jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "ico") => "Image",
        Some("mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm" | "m4v") => "Video",
        Some("mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" | "m4a") => "Audio",
        Some("pdf" | "doc" | "docx" | "txt" | "rtf" | "odt" | "md") => "Document",
        Some("zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz") => "Archive",
        Some("exe" | "dll" | "so" | "dylib" | "sys") => "Executable",
        Some("rs" | "c" | "cpp" | "h" | "hpp" | "py" | "js" | "ts" | "go" | "java") => "Code",
        _ => "Other",
    }
}

#[cfg(windows)]
pub fn list_drives() -> Vec<String> {
    use windows::Win32::Storage::FileSystem::{GetLogicalDrives, GetDriveTypeW};
    use windows::core::PCWSTR;
    
    let mut drives = Vec::new();
    
    unsafe {
        let bitmask = GetLogicalDrives();
        for i in 0..26 {
            if (bitmask & (1 << i)) != 0 {
                let letter = (b'A' + i) as char;
                let drive_str = format!("{}:\\", letter);
                
                let wide: Vec<u16> = drive_str.encode_utf16().chain(std::iter::once(0)).collect();
                let drive_type = GetDriveTypeW(PCWSTR(wide.as_ptr()));
                
                if drive_type == 3 {
                    drives.push(drive_str);
                }
            }
        }
    }
    
    drives
}

#[cfg(not(windows))]
pub fn list_drives() -> Vec<String> {
    vec!["/".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_scan_simple_directory() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path();

        let mut file1 = File::create(temp_path.join("file1.txt")).unwrap();
        file1.write_all(b"hello").unwrap();

        let mut file2 = File::create(temp_path.join("file2.txt")).unwrap();
        file2.write_all(b"world!").unwrap();

        let progress = ScanProgress::default();
        let result = scan_directory(temp_path.to_path_buf(), progress.clone()).unwrap();

        assert_eq!(result.is_dir, true);
        assert_eq!(result.children.len(), 2);
        assert_eq!(result.size, 11);
        assert_eq!(progress.files_scanned.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_scan_nested_directory() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path();

        fs::create_dir(temp_path.join("subdir")).unwrap();
        let mut file1 = File::create(temp_path.join("subdir").join("nested.txt")).unwrap();
        file1.write_all(b"test").unwrap();

        let progress = ScanProgress::default();
        let result = scan_directory(temp_path.to_path_buf(), progress).unwrap();

        assert_eq!(result.size, 4);
        assert_eq!(result.children.len(), 1);
        assert!(result.children[0].is_dir);
        assert_eq!(result.children[0].size, 4);
    }

    #[test]
    fn test_extension_category() {
        assert_eq!(get_extension_category(Some(&"jpg".to_string())), "Image");
        assert_eq!(get_extension_category(Some(&"mp4".to_string())), "Video");
        assert_eq!(get_extension_category(Some(&"rs".to_string())), "Code");
        assert_eq!(get_extension_category(Some(&"xyz".to_string())), "Other");
        assert_eq!(get_extension_category(None), "Other");
    }
}
