use std::path::Path;

#[cfg(windows)]
pub fn open_in_explorer(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{S_OK, HWND};
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let pidl = ILCreateFromPathW(PCWSTR(wide_path.as_ptr()));
        if pidl.is_null() {
            CoUninitialize();
            return Err("Failed to create item ID list".to_string());
        }

        let result = SHOpenFolderAndSelectItems(pidl, None, 0);

        ILFree(Some(pidl));
        CoUninitialize();

        result.map_err(|e| format!("Failed to open Explorer: {:?}", e))
    }
}

#[cfg(not(windows))]
pub fn open_in_explorer(_path: &Path) -> Result<(), String> {
    Err("Not supported on this platform".to_string())
}

pub fn copy_path_to_clipboard(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Foundation::{HWND, HANDLE, HGLOBAL, BOOL};
        use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
        use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

        unsafe {
            if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
                return Err("Failed to open clipboard".to_string());
            }

            let path_str = path.to_string_lossy();
            let wide: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
            let size = wide.len() * 2;

            let h_mem = GlobalAlloc(GMEM_MOVEABLE, size).unwrap();
            let ptr = GlobalLock(h_mem);
            if ptr.is_null() {
                CloseClipboard().ok();
                return Err("Failed to lock memory".to_string());
            }

            std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, size);
            GlobalUnlock(h_mem).ok();

            EmptyClipboard().ok();
            SetClipboardData(13, HANDLE(h_mem.0)).ok();
            CloseClipboard().ok();

            Ok(())
        }
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Not supported on this platform".to_string())
    }
}

#[cfg(windows)]
pub fn delete_to_recycle_bin(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::UI::Shell::{SHFileOperationW, SHFILEOPSTRUCTW, FOF_ALLOWUNDO, FOF_NO_UI, FO_DELETE};

    unsafe {
        let mut wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .chain(std::iter::once(0))
            .collect();

        let mut file_op = SHFILEOPSTRUCTW {
            hwnd: HWND(std::ptr::null_mut()),
            wFunc: FO_DELETE,
            pFrom: PCWSTR(wide_path.as_mut_ptr()),
            pTo: PCWSTR::null(),
            fFlags: (FOF_ALLOWUNDO.0 | FOF_NO_UI.0) as u16,
            fAnyOperationsAborted: Default::default(),
            hNameMappings: std::ptr::null_mut(),
            lpszProgressTitle: PCWSTR::null(),
        };

        let result = SHFileOperationW(&mut file_op);

        if result != 0 {
            Err(format!("Failed to delete to recycle bin: {}", result))
        } else if file_op.fAnyOperationsAborted.as_bool() {
            Err("Operation was aborted".to_string())
        } else {
            Ok(())
        }
    }
}

#[cfg(windows)]
pub fn delete_permanent(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{SHFileOperationW, SHFILEOPSTRUCTW, FOF_NO_UI, FO_DELETE};

    unsafe {
        let mut wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .chain(std::iter::once(0))
            .collect();

        let mut file_op = SHFILEOPSTRUCTW {
            hwnd: HWND(std::ptr::null_mut()),
            wFunc: FO_DELETE,
            pFrom: PCWSTR(wide_path.as_mut_ptr()),
            pTo: PCWSTR::null(),
            fFlags: FOF_NO_UI.0 as u16,
            fAnyOperationsAborted: Default::default(),
            hNameMappings: std::ptr::null_mut(),
            lpszProgressTitle: PCWSTR::null(),
        };

        let result = SHFileOperationW(&mut file_op);

        if result != 0 {
            Err(format!("Failed to delete permanently: {}", result))
        } else if file_op.fAnyOperationsAborted.as_bool() {
            Err("Operation was aborted".to_string())
        } else {
            Ok(())
        }
    }
}

#[cfg(not(windows))]
pub fn delete_to_recycle_bin(_path: &Path) -> Result<(), String> {
    Err("Not supported on this platform".to_string())
}

#[cfg(not(windows))]
pub fn delete_permanent(_path: &Path) -> Result<(), String> {
    Err("Not supported on this platform".to_string())
}

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
