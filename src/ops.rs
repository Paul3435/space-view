//! Platform operations: deleting, revealing in Explorer, drives, message boxes.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteMode {
    RecycleBin,
    Permanent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeleteOutcome {
    /// The item no longer exists on disk.
    Deleted,
    /// It was already gone before we tried.
    AlreadyGone,
    /// The user cancelled in a shell dialog; nothing (or only part) was removed.
    Cancelled,
    /// The operation ran but the item still exists (e.g. some files in use).
    Partial(String),
    Failed(String),
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Deletes `path` (a file or a whole folder). Blocks; call from a worker
/// thread. `owner` is the raw HWND to parent shell dialogs to (0 for none).
///
/// Recycle Bin deletion uses the shell's `IFileOperation` with
/// `FOFX_RECYCLEONDELETE | FOF_ALLOWUNDO | FOF_WANTNUKEWARNING`: if an item
/// cannot be recycled (too large, or on a drive without a Recycle Bin) the
/// shell asks before destroying it instead of silently deleting it.
pub fn delete(path: &Path, mode: DeleteMode, owner: isize) -> DeleteOutcome {
    if !exists(path) {
        return DeleteOutcome::AlreadyGone;
    }
    let result = platform_delete(path, mode, owner);
    let still_there = exists(path);
    match (result, still_there) {
        (_, false) => DeleteOutcome::Deleted,
        (Err(DeleteError::Cancelled), true) => DeleteOutcome::Cancelled,
        (Err(DeleteError::Other(e)), true) => DeleteOutcome::Failed(e),
        (Ok(()), true) => DeleteOutcome::Partial(
            "Some items could not be deleted (they may be in use or protected).".to_owned(),
        ),
    }
}

#[derive(Debug)]
#[cfg_attr(not(windows), allow(dead_code))]
enum DeleteError {
    Cancelled,
    Other(String),
}

#[cfg(not(windows))]
fn platform_delete(path: &Path, mode: DeleteMode, _owner: isize) -> Result<(), DeleteError> {
    match mode {
        DeleteMode::RecycleBin => Err(DeleteError::Other(
            "Moving to the Recycle Bin is only supported on Windows.".to_owned(),
        )),
        DeleteMode::Permanent => {
            let meta =
                std::fs::symlink_metadata(path).map_err(|e| DeleteError::Other(e.to_string()))?;
            let r = if meta.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            };
            r.map_err(|e| DeleteError::Other(e.to_string()))
        }
    }
}

#[cfg(windows)]
fn platform_delete(path: &Path, mode: DeleteMode, owner: isize) -> Result<(), DeleteError> {
    let display = path.to_string_lossy().replace('/', "\\");
    let long = display.encode_utf16().count() >= 259;
    if long {
        return match mode {
            DeleteMode::RecycleBin => Err(DeleteError::Other(
                "This path is longer than the Recycle Bin supports (260 characters). \
                 Use Shift+Delete to delete it permanently instead."
                    .to_owned(),
            )),
            // std uses \\?\ paths internally and never follows junctions.
            DeleteMode::Permanent => {
                let meta = std::fs::symlink_metadata(path)
                    .map_err(|e| DeleteError::Other(e.to_string()))?;
                let r = if meta.is_dir() {
                    std::fs::remove_dir_all(path)
                } else {
                    std::fs::remove_file(path)
                };
                r.map_err(|e| DeleteError::Other(e.to_string()))
            }
        };
    }
    match win::ifileoperation_delete(&display, mode, owner) {
        Err(win::ComError::Unavailable) => win::shfileoperation_delete(&display, mode, owner),
        Err(win::ComError::Cancelled) => Err(DeleteError::Cancelled),
        Err(win::ComError::Failed(e)) => Err(DeleteError::Other(e)),
        Ok(()) => Ok(()),
    }
}

/// Opens Explorer with `path` selected. Non-blocking.
pub fn reveal_in_explorer(path: &Path) {
    let path = path.to_path_buf();
    std::thread::Builder::new()
        .name("disktree-explorer".into())
        .spawn(move || reveal_blocking(&path))
        .ok();
}

#[cfg(windows)]
fn reveal_blocking(path: &Path) {
    let display = path.to_string_lossy().replace('/', "\\");
    if win::open_folder_and_select(&display).is_err() {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{display}\""))
            .spawn();
    }
}

#[cfg(not(windows))]
fn reveal_blocking(path: &Path) {
    let dir = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}

#[derive(Clone, Debug)]
pub struct Drive {
    pub root: PathBuf,
    pub label: String,
    pub filesystem: String,
    pub total: u64,
    pub free: u64,
}

/// Fixed (local, non-removable) drives. Can block briefly on sleeping disks,
/// so call it off the UI thread.
#[cfg(windows)]
pub fn fixed_drives() -> Vec<Drive> {
    win::fixed_drives()
}

#[cfg(not(windows))]
pub fn fixed_drives() -> Vec<Drive> {
    let mut v = vec![Drive {
        root: PathBuf::from("/"),
        label: "File system root".to_owned(),
        filesystem: String::new(),
        total: 0,
        free: 0,
    }];
    if let Some(home) = std::env::var_os("HOME") {
        v.push(Drive {
            root: PathBuf::from(home),
            label: "Home".to_owned(),
            filesystem: String::new(),
            total: 0,
            free: 0,
        });
    }
    v
}

/// Shows a modal message box (works before/without any window).
pub fn message_box(title: &str, text: &str) {
    #[cfg(windows)]
    win::message_box(title, text);
    #[cfg(not(windows))]
    eprintln!("{title}: {text}");
}

/// Stop Windows from showing "There is no disk in the drive" style popups.
pub fn quiet_critical_errors() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Diagnostics::Debug::{
            SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX,
        };
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX);
    }
}

/// Lets `println!` reach the console when launched from a terminal
/// (the release exe uses the GUI subsystem and has no console of its own).
pub fn attach_parent_console() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(windows)]
mod win {
    use super::{DeleteError, DeleteMode, Drive};
    use std::path::PathBuf;
    use windows::core::{BOOL, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        FileOperation, IFileOperation, ILFree, IShellItem, SHCreateItemFromParsingName,
        SHFileOperationW, SHOpenFolderAndSelectItems, SHParseDisplayName, FOFX_RECYCLEONDELETE,
        FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_WANTNUKEWARNING, FO_DELETE,
        SHFILEOPSTRUCTW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    };

    const DRIVE_FIXED: u32 = 3;
    // COPYENGINE_E_USER_CANCELLED
    const COPYENGINE_E_USER_CANCELLED: i32 = 0x8027_0000u32 as i32;

    struct Com(bool);
    impl Com {
        fn init() -> Com {
            let hr =
                unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
            Com(hr.is_ok())
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub enum ComError {
        Unavailable,
        Cancelled,
        Failed(String),
    }

    fn owner_hwnd(owner: isize) -> HWND {
        HWND(owner as *mut core::ffi::c_void)
    }

    pub fn ifileoperation_delete(
        path: &str,
        mode: DeleteMode,
        owner: isize,
    ) -> Result<(), ComError> {
        let _com = Com::init();
        unsafe {
            let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)
                .map_err(|_| ComError::Unavailable)?;
            let mut flags = FOF_NOCONFIRMATION | FOF_NOCONFIRMMKDIR;
            if mode == DeleteMode::RecycleBin {
                flags = flags | FOF_ALLOWUNDO | FOFX_RECYCLEONDELETE | FOF_WANTNUKEWARNING;
            }
            op.SetOperationFlags(flags)
                .map_err(|_| ComError::Unavailable)?;
            if owner != 0 {
                let _ = op.SetOwnerWindow(owner_hwnd(owner));
            }
            let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path), None)
                .map_err(|e| ComError::Failed(format!("Cannot open {path}: {}", e.message())))?;
            op.DeleteItem(&item, None)
                .map_err(|e| ComError::Failed(e.message()))?;
            let performed = op.PerformOperations();
            let aborted = op
                .GetAnyOperationsAborted()
                .map(|b| b.as_bool())
                .unwrap_or(false);
            match performed {
                Err(e)
                    if e.code().0 == COPYENGINE_E_USER_CANCELLED
                        || e.code() == ERROR_CANCELLED.to_hresult() =>
                {
                    Err(ComError::Cancelled)
                }
                Err(e) => Err(ComError::Failed(e.message())),
                Ok(()) if aborted => Err(ComError::Cancelled),
                Ok(()) => Ok(()),
            }
        }
    }

    /// Fallback for systems where IFileOperation is unavailable.
    pub fn shfileoperation_delete(
        path: &str,
        mode: DeleteMode,
        owner: isize,
    ) -> Result<(), DeleteError> {
        // pFrom is a double-NUL-terminated list.
        let mut from: Vec<u16> = path.encode_utf16().collect();
        from.push(0);
        from.push(0);
        let mut flags = FOF_NOCONFIRMATION.0 | FOF_NOCONFIRMMKDIR.0;
        if mode == DeleteMode::RecycleBin {
            flags |= FOF_ALLOWUNDO.0 | FOF_WANTNUKEWARNING.0;
        }
        let mut op = SHFILEOPSTRUCTW {
            hwnd: owner_hwnd(owner),
            wFunc: FO_DELETE,
            pFrom: PCWSTR(from.as_ptr()),
            pTo: PCWSTR::null(),
            fFlags: flags as u16,
            fAnyOperationsAborted: BOOL(0),
            hNameMappings: std::ptr::null_mut(),
            lpszProgressTitle: PCWSTR::null(),
        };
        let rc = unsafe { SHFileOperationW(&mut op) };
        if op.fAnyOperationsAborted.as_bool() {
            return Err(DeleteError::Cancelled);
        }
        if rc != 0 {
            return Err(DeleteError::Other(format!(
                "The shell reported error 0x{rc:X}"
            )));
        }
        Ok(())
    }

    pub fn open_folder_and_select(path: &str) -> windows::core::Result<()> {
        let _com = Com::init();
        unsafe {
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            SHParseDisplayName(&HSTRING::from(path), None, &mut pidl, 0, None)?;
            let r = SHOpenFolderAndSelectItems(pidl, None, 0);
            ILFree(Some(pidl));
            r
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn fixed_drives() -> Vec<Drive> {
        let mut out = Vec::new();
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = format!("{}:\\", (b'A' + i as u8) as char);
            let w = wide(&root);
            if unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) } != DRIVE_FIXED {
                continue;
            }
            let mut label = [0u16; 261];
            let mut fs = [0u16; 261];
            let (label, filesystem) = match unsafe {
                GetVolumeInformationW(
                    PCWSTR(w.as_ptr()),
                    Some(&mut label),
                    None,
                    None,
                    None,
                    Some(&mut fs),
                )
            } {
                Ok(()) => (from_wide(&label), from_wide(&fs)),
                Err(_) => (String::new(), String::new()),
            };
            let (mut free, mut total) = (0u64, 0u64);
            let _ = unsafe {
                GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut free), Some(&mut total), None)
            };
            out.push(Drive {
                root: PathBuf::from(root),
                label,
                filesystem,
                total,
                free,
            });
        }
        out
    }

    fn from_wide(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    pub fn message_box(title: &str, text: &str) {
        unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(text),
                &HSTRING::from(title),
                MB_OK | MB_ICONERROR | MB_SETFOREGROUND | MB_TOPMOST,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_reports_already_gone() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            delete(&tmp.path().join("nope"), DeleteMode::Permanent, 0),
            DeleteOutcome::AlreadyGone
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn permanent_delete_removes_folder_and_recycle_is_refused_off_windows() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("d");
        std::fs::create_dir_all(d.join("x")).unwrap();
        std::fs::write(d.join("x/f"), b"1").unwrap();
        assert!(matches!(
            delete(&d, DeleteMode::RecycleBin, 0),
            DeleteOutcome::Failed(_)
        ));
        assert!(d.exists(), "a refused recycle must not delete anything");
        assert_eq!(delete(&d, DeleteMode::Permanent, 0), DeleteOutcome::Deleted);
        assert!(!d.exists());
    }

    #[cfg(unix)]
    #[test]
    fn permanent_delete_of_folder_does_not_follow_links() {
        let tmp = tempfile::tempdir().unwrap();
        let keep = tmp.path().join("keep");
        std::fs::create_dir(&keep).unwrap();
        std::fs::write(keep.join("precious"), b"x").unwrap();
        let d = tmp.path().join("d");
        std::fs::create_dir(&d).unwrap();
        std::os::unix::fs::symlink(&keep, d.join("link-to-keep")).unwrap();
        assert_eq!(delete(&d, DeleteMode::Permanent, 0), DeleteOutcome::Deleted);
        assert!(keep.join("precious").exists());
    }
}
