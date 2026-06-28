// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
struct SingleInstanceGuard(isize);

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        #[link(name = "kernel32")]
        extern "system" {
            fn CloseHandle(handle: isize) -> i32;
        }
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn acquire_single_instance() -> Option<SingleInstanceGuard> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(
            mutex_attributes: *mut std::ffi::c_void,
            initial_owner: i32,
            name: *const u16,
        ) -> isize;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: isize) -> i32;
    }

    const ERROR_ALREADY_EXISTS: u32 = 183;
    let name: Vec<u16> = OsStr::new("Local\\software-manager.wangjiboxi.instance")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { CreateMutexW(null_mut(), 1, name.as_ptr()) };
    if handle == 0 {
        return None;
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    }
    Some(SingleInstanceGuard(handle))
}

#[cfg(windows)]
fn set_dpi_awareness() {
    #[link(name = "user32")]
    extern "system" {
        fn SetProcessDpiAwarenessContext(dpi_context: isize) -> i32;
        fn SetProcessDPIAware() -> i32;
    }

    const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = SetProcessDPIAware();
    }
}

#[cfg(windows)]
fn use_bundled_webview2() {
    let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())) else {
        return;
    };
    let bundled = exe_dir.join("WebView2");
    if bundled.join("msedgewebview2.exe").exists() {
        std::env::set_var(
            "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
            bundled.to_string_lossy().as_ref(),
        );
    }
}

fn main() {
    #[cfg(windows)]
    let _single_instance = match acquire_single_instance() {
        Some(guard) => guard,
        None => return,
    };
    #[cfg(windows)]
    set_dpi_awareness();
    #[cfg(windows)]
    use_bundled_webview2();
    software_manager_lib::run()
}
