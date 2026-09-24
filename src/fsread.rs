//! Reading one directory's entries, with sizes, without following links.
//!
//! On Windows this uses `GetFileInformationByHandleEx(FileFullDirectoryInfo)`,
//! which returns each entry's logical size, *allocated* size, attributes and
//! reparse tag in bulk (dozens of entries per system call) without opening the
//! files. Paths are passed in `\\?\` form so names longer than `MAX_PATH` and
//! names with trailing dots/spaces work regardless of the `LongPathsEnabled`
//! policy.

use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntry {
    pub name: String,
    pub is_dir: bool,
    /// Symlink, junction or mount point: never followed.
    pub is_link: bool,
    pub allocated: u64,
    pub logical: u64,
}

pub fn read_dir(path: &Path) -> io::Result<Vec<RawEntry>> {
    #[cfg(windows)]
    {
        match win::read_dir(path) {
            Err(e) if win::should_fall_back(&e) => portable::read_dir(path),
            other => other,
        }
    }
    #[cfg(not(windows))]
    {
        portable::read_dir(path)
    }
}

/// Reparse tags that make a directory entry an alias for another location
/// (junctions, symlinks, mount points) have the "name surrogate" bit set.
/// Other reparse points (OneDrive/cloud placeholders, dedup, WOF-compressed
/// files) are real data and must still be counted and descended into.
pub fn is_name_surrogate(tag: u32) -> bool {
    tag & 0x2000_0000 != 0
}

pub const IO_REPARSE_TAG_WOF: u32 = 0x8000_0017;

#[cfg(windows)]
pub mod win {
    use super::{is_name_surrogate, RawEntry, IO_REPARSE_TAG_WOF};
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
        ERROR_NO_MORE_FILES, HANDLE, SetLastError, WIN32_ERROR,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FileFullDirectoryInfo, GetCompressedFileSizeW, GetFileInformationByHandleEx,
        FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ATTRIBUTE_SPARSE_FILE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FULL_DIR_INFO,
        FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        INVALID_FILE_SIZE, OPEN_EXISTING,
    };

    /// `C:\x` -> `\\?\C:\x`, `\\srv\share\x` -> `\\?\UNC\srv\share\x`.
    pub fn verbatim(path: &Path) -> Vec<u16> {
        let s = path.as_os_str();
        let wide: Vec<u16> = s.encode_wide().map(|c| if c == b'/' as u16 { b'\\' as u16 } else { c }).collect();
        let bs = b'\\' as u16;
        let mut out: Vec<u16> = Vec::with_capacity(wide.len() + 8);
        if wide.starts_with(&[bs, bs, b'?' as u16, bs]) || wide.starts_with(&[bs, bs, b'.' as u16, bs]) {
            out.extend_from_slice(&wide);
        } else if wide.starts_with(&[bs, bs]) {
            out.extend(r"\\?\UNC\".encode_utf16());
            out.extend_from_slice(&wide[2..]);
        } else if wide.len() >= 2 && wide[1] == b':' as u16 {
            out.extend(r"\\?\".encode_utf16());
            out.extend_from_slice(&wide);
        } else {
            out.extend_from_slice(&wide);
        }
        out.push(0);
        out
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub fn should_fall_back(e: &io::Error) -> bool {
        matches!(
            e.raw_os_error().map(|c| c as u32),
            Some(c) if c == ERROR_INVALID_PARAMETER.0 || c == ERROR_NOT_SUPPORTED.0 || c == ERROR_INVALID_FUNCTION.0
        )
    }

    fn to_io(e: windows::core::Error) -> io::Error {
        let code = e.code().0 as u32;
        // HRESULT_FROM_WIN32 -> Win32 error code
        if code & 0xFFFF_0000 == 0x8007_0000 {
            io::Error::from_raw_os_error((code & 0xFFFF) as i32)
        } else {
            io::Error::other(e.message())
        }
    }

    pub fn read_dir(path: &Path) -> io::Result<Vec<RawEntry>> {
        let dir_w = verbatim(path);
        let handle = unsafe {
            CreateFileW(
                PCWSTR(dir_w.as_ptr()),
                FILE_LIST_DIRECTORY.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        }
        .map_err(to_io)?;
        let handle = Handle(handle);

        // 64 KiB, 8-byte aligned.
        let mut buf = vec![0u64; 8 * 1024];
        let buf_bytes = (buf.len() * 8) as u32;
        let mut out = Vec::new();
        loop {
            let res = unsafe {
                GetFileInformationByHandleEx(
                    handle.0,
                    FileFullDirectoryInfo,
                    buf.as_mut_ptr().cast(),
                    buf_bytes,
                )
            };
            if let Err(e) = res {
                if e.code() == ERROR_NO_MORE_FILES.to_hresult() {
                    break;
                }
                return Err(to_io(e));
            }
            let base = buf.as_ptr() as *const u8;
            let mut offset = 0usize;
            loop {
                // SAFETY: the API fills `buf` with a chain of FILE_FULL_DIR_INFO
                // records, each 8-byte aligned, linked by NextEntryOffset.
                let info = unsafe { &*(base.add(offset) as *const FILE_FULL_DIR_INFO) };
                let name_len = info.FileNameLength as usize / 2;
                let name_ptr = std::ptr::addr_of!(info.FileName) as *const u16;
                let name_w = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
                if !(name_w == [b'.' as u16] || name_w == [b'.' as u16, b'.' as u16]) {
                    out.push(entry_from(path, &dir_w, name_w, info));
                }
                if info.NextEntryOffset == 0 {
                    break;
                }
                offset += info.NextEntryOffset as usize;
            }
        }
        Ok(out)
    }

    fn entry_from(_dir: &Path, dir_w: &[u16], name_w: &[u16], info: &FILE_FULL_DIR_INFO) -> RawEntry {
        let attrs = info.FileAttributes;
        let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
        let is_reparse = attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
        // For reparse points, EaSize holds the reparse tag.
        let tag = if is_reparse { info.EaSize } else { 0 };
        let is_link = is_reparse && is_name_surrogate(tag);
        let name = String::from_utf16_lossy(name_w);
        if is_link {
            return RawEntry { name, is_dir, is_link: true, allocated: 0, logical: 0 };
        }
        let logical = info.EndOfFile.max(0) as u64;
        let mut allocated = info.AllocationSize.max(0) as u64;
        let compressed = attrs & (FILE_ATTRIBUTE_COMPRESSED.0 | FILE_ATTRIBUTE_SPARSE_FILE.0) != 0
            || tag == IO_REPARSE_TAG_WOF;
        if !is_dir && compressed {
            // The directory entry does not reflect NTFS/WOF compression or
            // sparse ranges; ask for the real on-disk size of just these files.
            let mut full: Vec<u16> = Vec::with_capacity(dir_w.len() + name_w.len() + 1);
            full.extend_from_slice(&dir_w[..dir_w.len() - 1]);
            if full.last() != Some(&(b'\\' as u16)) {
                full.push(b'\\' as u16);
            }
            full.extend_from_slice(name_w);
            full.push(0);
            let mut high = 0u32;
            let low = unsafe {
                SetLastError(WIN32_ERROR(0));
                GetCompressedFileSizeW(PCWSTR(full.as_ptr()), Some(&mut high))
            };
            if !(low == INVALID_FILE_SIZE && std::io::Error::last_os_error().raw_os_error() != Some(0)) {
                allocated = ((high as u64) << 32) | low as u64;
            }
        }
        RawEntry { name, is_dir, is_link: false, allocated, logical }
    }
}

pub mod portable {
    use super::RawEntry;
    use std::fs;
    use std::io;
    use std::path::Path;

    pub fn read_dir(path: &Path) -> io::Result<Vec<RawEntry>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            // DirEntry::metadata does not traverse symlinks.
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let ft = meta.file_type();
            if ft.is_symlink() || is_windows_link(&meta) {
                out.push(RawEntry {
                    name,
                    is_dir: ft.is_dir(),
                    is_link: true,
                    allocated: 0,
                    logical: 0,
                });
                continue;
            }
            let logical = if ft.is_dir() { 0 } else { meta.len() };
            out.push(RawEntry {
                name,
                is_dir: ft.is_dir(),
                is_link: false,
                allocated: if ft.is_dir() { 0 } else { allocated(&meta) },
                logical,
            });
        }
        Ok(out)
    }

    #[cfg(windows)]
    fn is_windows_link(meta: &fs::Metadata) -> bool {
        use std::os::windows::fs::MetadataExt;
        // Without the reparse tag, treat every reparse *directory* as a link.
        meta.file_attributes() & 0x400 != 0 && meta.is_dir()
    }

    #[cfg(not(windows))]
    fn is_windows_link(_meta: &fs::Metadata) -> bool {
        false
    }

    #[cfg(unix)]
    fn allocated(meta: &fs::Metadata) -> u64 {
        use std::os::unix::fs::MetadataExt;
        meta.blocks() * 512
    }

    #[cfg(not(unix))]
    fn allocated(meta: &fs::Metadata) -> u64 {
        meta.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_surrogate_tags() {
        const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
        const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
        const IO_REPARSE_TAG_CLOUD_6: u32 = 0x9000_601A;
        const IO_REPARSE_TAG_DEDUP: u32 = 0x8000_0013;
        const IO_REPARSE_TAG_APPEXECLINK: u32 = 0x8000_001B;
        assert!(is_name_surrogate(IO_REPARSE_TAG_MOUNT_POINT));
        assert!(is_name_surrogate(IO_REPARSE_TAG_SYMLINK));
        assert!(!is_name_surrogate(IO_REPARSE_TAG_CLOUD_6), "OneDrive folders must be scanned");
        assert!(!is_name_surrogate(IO_REPARSE_TAG_DEDUP));
        assert!(!is_name_surrogate(IO_REPARSE_TAG_WOF));
        assert!(!is_name_surrogate(IO_REPARSE_TAG_APPEXECLINK));
    }

    #[cfg(unix)]
    #[test]
    fn portable_reader_reports_sizes_and_links() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("f.bin"), vec![1u8; 10_000]).unwrap();
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("sub"), tmp.path().join("link")).unwrap();
        let mut entries = read_dir(tmp.path()).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["f.bin", "link", "sub"]);
        let f = &entries[0];
        assert_eq!(f.logical, 10_000);
        assert!(f.allocated >= 10_000, "allocated is rounded up to blocks");
        assert!(entries[1].is_link && entries[1].allocated == 0);
        assert!(entries[2].is_dir && !entries[2].is_link);
    }
}
