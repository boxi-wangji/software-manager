use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{data_dir, get_install_base};

#[cfg(windows)]
fn hidden_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
fn hidden_command(program: &str) -> Command {
    Command::new(program)
}

#[derive(Debug, Clone, Serialize)]
pub struct PickedScreenColor {
    pub hex: String,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub screen_x: i32,
    pub screen_y: i32,
}

#[cfg(windows)]
#[tauri::command]
pub fn pick_screen_color_cmd(delay_ms: Option<u64>) -> Result<PickedScreenColor, String> {
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(point: *mut Point) -> i32;
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, dc: isize) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn GetPixel(dc: isize, x: i32, y: i32) -> u32;
    }

    std::thread::sleep(Duration::from_millis(delay_ms.unwrap_or(0).min(5000)));

    let mut point = Point::default();
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return Err("读取鼠标位置失败".into());
    }
    let dc = unsafe { GetDC(0) };
    if dc == 0 {
        return Err("读取屏幕失败".into());
    }
    let color = unsafe { GetPixel(dc, point.x, point.y) };
    unsafe {
        let _ = ReleaseDC(0, dc);
    }
    if color == 0xFFFF_FFFF {
        return Err("拾取颜色失败".into());
    }

    let r = (color & 0xff) as u8;
    let g = ((color >> 8) & 0xff) as u8;
    let b = ((color >> 16) & 0xff) as u8;
    Ok(PickedScreenColor {
        hex: format!("#{r:02X}{g:02X}{b:02X}"),
        r,
        g,
        b,
        screen_x: point.x,
        screen_y: point.y,
    })
}

#[cfg(not(windows))]
#[tauri::command]
pub fn pick_screen_color_cmd(_delay_ms: Option<u64>) -> Result<PickedScreenColor, String> {
    Err("当前平台暂不支持屏幕拾色".into())
}

#[cfg(windows)]
#[tauri::command]
pub fn close_target_window_cmd(window_title: String) -> Result<VisualTargetResult, String> {
    let title = if window_title.trim().is_empty() {
        "WeGame".to_string()
    } else {
        window_title.trim().to_string()
    };
    let escaped_title = title.replace('\'', "''");
    let script = r#"
$needle = '__TARGET_TITLE__'
$matched = @(Get-Process | Where-Object { $_.MainWindowTitle -and $_.MainWindowTitle -like "*$needle*" })
$closed = 0
$killed = 0
foreach ($p in $matched) {
  try {
    if ($p.CloseMainWindow()) { $closed++ }
  } catch {}
}
Start-Sleep -Milliseconds 800
foreach ($p in $matched) {
  try {
    $p.Refresh()
    if (-not $p.HasExited) {
      Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
      $killed++
    }
  } catch {}
}
Write-Output ("matched={0};closed={1};killed={2}" -f $matched.Count, $closed, $killed)
if ($matched.Count -eq 0) { exit 2 }
"#
    .replace("__TARGET_TITLE__", &escaped_title);
    let args = vec![
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-NoProfile".to_string(),
        "-Command".to_string(),
        script,
    ];
    let output = hidden_command("powershell")
        .args(&args)
        .output()
        .map_err(|e| format!("关闭目标窗口失败: {e}"))?;
    let stdout = decode_console_output(&output.stdout);
    let stderr = decode_console_output(&output.stderr);
    let success = output.status.success();
    let message = if success {
        format!("已关闭目标窗口: {}", stdout.trim())
    } else if output.status.code() == Some(2) {
        format!("未找到目标窗口: {title}")
    } else {
        format!(
            "关闭目标窗口失败: {}",
            if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            }
        )
    };
    Ok(VisualTargetResult {
        success,
        raw_screen_x: None,
        raw_screen_y: None,
        offset_x: None,
        offset_y: None,
        screen_x: None,
        screen_y: None,
        window_left: None,
        window_top: None,
        window_width: None,
        window_height: None,
        window_title: Some(title),
        detail: None,
        preview_image: None,
        window_handle: None,
        message,
    })
}

#[cfg(not(windows))]
#[tauri::command]
pub fn close_target_window_cmd(_window_title: String) -> Result<VisualTargetResult, String> {
    Err("当前平台暂不支持关闭目标窗口".into())
}

#[cfg(windows)]
#[allow(dead_code)]
mod cursor_control {
    use super::Duration;
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::OsStringExt;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct MouseInput {
        dx: i32,
        dy: i32,
        mouse_data: u32,
        dw_flags: u32,
        time: u32,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct Input {
        input_type: u32,
        _padding: u32,
        mi: MouseInput,
    }

    const _: () = assert!(std::mem::size_of::<Input>() == 40);

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct KeyboardInputData {
        w_vk: u16,
        w_scan: u16,
        dw_flags: u32,
        time: u32,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct KeyboardInput {
        input_type: u32,
        _padding: u32,
        ki: KeyboardInputData,
        _tail_padding: [u8; 8],
    }

    const _: () = assert!(std::mem::size_of::<KeyboardInput>() == 40);

    #[derive(Clone, Debug)]
    pub struct InputReport {
        pub success: bool,
        pub method: String,
        pub target_x: i32,
        pub target_y: i32,
        pub final_x: Option<i32>,
        pub final_y: Option<i32>,
    }

    #[derive(Clone, Debug)]
    pub struct WindowInputReport {
        pub success: bool,
        pub method: String,
        pub target_x: i32,
        pub target_y: i32,
        pub client_x: Option<i32>,
        pub client_y: Option<i32>,
    }

    #[link(name = "user32")]
    extern "system" {
        fn SetThreadDpiAwarenessContext(dpi_context: isize) -> isize;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn GetCursorPos(point: *mut Point) -> i32;
        fn SendInput(count: u32, inputs: *mut c_void, size: i32) -> u32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra_info: usize);
        fn EnumWindows(
            callback: Option<unsafe extern "system" fn(isize, isize) -> i32>,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn GetWindowTextLengthW(hwnd: isize) -> i32;
        fn GetWindowTextW(hwnd: isize, text: *mut u16, max: i32) -> i32;
        fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        fn SetForegroundWindow(hwnd: isize) -> i32;
        fn GetForegroundWindow() -> isize;
        fn GetWindowThreadProcessId(hwnd: isize, process_id: *mut u32) -> u32;
        fn AttachThreadInput(id_attach: u32, id_attach_to: u32, attach: i32) -> i32;
        fn BringWindowToTop(hwnd: isize) -> i32;
        fn WindowFromPoint(point: Point) -> isize;
        fn ScreenToClient(hwnd: isize, point: *mut Point) -> i32;
        fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
        fn GetClassNameW(hwnd: isize, class_name: *mut u16, max: i32) -> i32;
        fn IsWindow(hwnd: isize) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    const INPUT_MOUSE: u32 = 0;
    const INPUT_KEYBOARD: u32 = 1;
    const MOUSEEVENTF_MOVE: u32 = 0x0001;
    const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;
    const MOUSEEVENTF_VIRTUALDESK: u32 = 0x4000;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const KEYEVENTF_UNICODE: u32 = 0x0004;
    const VK_CONTROL: u16 = 0x11;
    const VK_A: u16 = 0x41;
    const SM_XVIRTUALSCREEN: i32 = 76;
    const SM_YVIRTUALSCREEN: i32 = 77;
    const SM_CXVIRTUALSCREEN: i32 = 78;
    const SM_CYVIRTUALSCREEN: i32 = 79;
    const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
    const SW_RESTORE: i32 = 9;
    const WM_MOUSEMOVE: u32 = 0x0200;
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;
    const BM_CLICK: u32 = 0x00F5;
    const MK_LBUTTON: usize = 0x0001;

    fn set_thread_dpi_awareness() {
        unsafe {
            let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    fn window_text(hwnd: isize) -> String {
        unsafe {
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                return String::new();
            }
            let mut buf = vec![0u16; (len + 1) as usize];
            let read = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            OsString::from_wide(&buf[..read.max(0) as usize])
                .to_string_lossy()
                .into_owned()
        }
    }

    fn window_class(hwnd: isize) -> String {
        unsafe {
            let mut buf = [0u16; 256];
            let read = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            OsString::from_wide(&buf[..read.max(0) as usize])
                .to_string_lossy()
                .into_owned()
        }
    }

    fn is_valid_hwnd(hwnd: isize) -> bool {
        hwnd != 0 && unsafe { IsWindow(hwnd) != 0 }
    }

    unsafe extern "system" fn enum_window_by_title(hwnd: isize, lparam: isize) -> i32 {
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let filter = {
            let slot = &mut *(lparam as *mut Option<(String, isize)>);
            slot.as_ref().map(|(f, _)| f.clone())
        };
        let Some(filter) = filter else {
            return 1;
        };
        let title = window_text(hwnd);
        if title.is_empty() {
            return 1;
        }
        if !title.to_lowercase().contains(&filter.to_lowercase()) {
            return 1;
        }
        let mut rect = Rect::default();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }
        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;
        if w < 80 || h < 60 {
            return 1;
        }
        let slot = &mut *(lparam as *mut Option<(String, isize)>);
        let area = w * h;
        let replace = match slot {
            None => true,
            Some((_, best_hwnd)) => {
                if *best_hwnd == 0 {
                    true
                } else {
                    let mut best_rect = Rect::default();
                    let _ = GetWindowRect(*best_hwnd, &mut best_rect);
                    area > (best_rect.right - best_rect.left) * (best_rect.bottom - best_rect.top)
                }
            }
        };
        if replace {
            *slot = Some((filter, hwnd));
        }
        1
    }

    fn find_window_by_title(title_filter: &str) -> Option<isize> {
        if title_filter.trim().is_empty() {
            return None;
        }
        let mut slot = Some((title_filter.trim().to_string(), 0isize));
        unsafe {
            let _ = EnumWindows(Some(enum_window_by_title), &mut slot as *mut _ as isize);
        }
        slot.and_then(|(_, hwnd)| is_valid_hwnd(hwnd).then_some(hwnd))
    }

    fn resolve_root_hwnd(window_handle: Option<i64>, window_title: Option<&str>) -> Option<isize> {
        if let Some(handle) = window_handle {
            let hwnd = handle as isize;
            if is_valid_hwnd(hwnd) {
                return Some(hwnd);
            }
        }
        window_title.and_then(|title| find_window_by_title(title))
    }

    fn focus_window(hwnd: isize) {
        unsafe {
            if hwnd == 0 {
                return;
            }
            ShowWindow(hwnd, SW_RESTORE);
            BringWindowToTop(hwnd);
            let fg = GetForegroundWindow();
            if fg == hwnd {
                SetForegroundWindow(hwnd);
                return;
            }
            let mut dummy = 0u32;
            let target_thread = GetWindowThreadProcessId(hwnd, &mut dummy);
            let fg_thread = GetWindowThreadProcessId(fg, &mut dummy);
            let cur_thread = GetCurrentThreadId();
            let mut attached_fg = false;
            let mut attached_target = false;
            if fg_thread != 0 && fg_thread != cur_thread {
                attached_fg = AttachThreadInput(cur_thread, fg_thread, 1) != 0;
            }
            if target_thread != 0 && target_thread != cur_thread {
                attached_target = AttachThreadInput(cur_thread, target_thread, 1) != 0;
            }
            SetForegroundWindow(hwnd);
            if attached_target {
                AttachThreadInput(cur_thread, target_thread, 0);
            }
            if attached_fg {
                AttachThreadInput(cur_thread, fg_thread, 0);
            }
        }
    }

    fn make_lparam(client_x: i32, client_y: i32) -> isize {
        (((client_y as u32) & 0xffff) << 16 | ((client_x as u32) & 0xffff)) as isize
    }

    fn screen_to_client(hwnd: isize, screen_x: i32, screen_y: i32) -> Option<Point> {
        let mut point = Point {
            x: screen_x,
            y: screen_y,
        };
        let ok = unsafe { ScreenToClient(hwnd, &mut point) };
        (ok != 0).then_some(point)
    }

    fn try_button_click(hwnd: isize) -> bool {
        let class = window_class(hwnd);
        if class != "Button" {
            return false;
        }
        unsafe {
            SendMessageW(hwnd, BM_CLICK, 0, 0);
        }
        true
    }

    fn try_message_click(hwnd: isize, screen_x: i32, screen_y: i32, use_post: bool) -> bool {
        if !is_valid_hwnd(hwnd) {
            return false;
        }
        let Some(client) = screen_to_client(hwnd, screen_x, screen_y) else {
            return false;
        };
        let lparam = make_lparam(client.x, client.y);
        unsafe {
            if use_post {
                PostMessageW(hwnd, WM_MOUSEMOVE, 0, lparam);
                std::thread::sleep(Duration::from_millis(40));
                PostMessageW(hwnd, WM_LBUTTONDOWN, MK_LBUTTON, lparam);
                std::thread::sleep(Duration::from_millis(80));
                PostMessageW(hwnd, WM_LBUTTONUP, 0, lparam) != 0
            } else {
                SendMessageW(hwnd, WM_MOUSEMOVE, 0, lparam);
                std::thread::sleep(Duration::from_millis(40));
                SendMessageW(hwnd, WM_LBUTTONDOWN, MK_LBUTTON, lparam);
                std::thread::sleep(Duration::from_millis(80));
                SendMessageW(hwnd, WM_LBUTTONUP, 0, lparam);
                true
            }
        }
    }

    fn click_targets(root_hwnd: Option<isize>, screen_x: i32, screen_y: i32) -> Vec<isize> {
        let mut targets = Vec::new();
        let hit = unsafe {
            WindowFromPoint(Point {
                x: screen_x,
                y: screen_y,
            })
        };
        if is_valid_hwnd(hit) {
            targets.push(hit);
        }
        if let Some(root) = root_hwnd {
            if is_valid_hwnd(root) && !targets.contains(&root) {
                targets.push(root);
            }
        }
        targets
    }

    fn try_window_message_clicks(
        root_hwnd: Option<isize>,
        screen_x: i32,
        screen_y: i32,
    ) -> Vec<String> {
        let mut attempts = Vec::new();
        if let Some(root) = root_hwnd {
            focus_window(root);
            std::thread::sleep(Duration::from_millis(180));
        }

        for hwnd in click_targets(root_hwnd, screen_x, screen_y) {
            if try_button_click(hwnd) {
                attempts.push("BM_CLICK".into());
            }
            if try_message_click(hwnd, screen_x, screen_y, true) {
                attempts.push("PostMessage".into());
            }
            if try_message_click(hwnd, screen_x, screen_y, false) {
                attempts.push("SendMessage".into());
            }
        }
        attempts
    }

    fn window_report(
        action: &str,
        root_hwnd: Option<isize>,
        screen_x: i32,
        screen_y: i32,
        success: bool,
        methods: Vec<String>,
    ) -> WindowInputReport {
        let client = root_hwnd.and_then(|hwnd| screen_to_client(hwnd, screen_x, screen_y));
        let method = if success {
            let method = if methods.is_empty() {
                "window-message".to_string()
            } else {
                methods.join("+")
            };
            format!("{action}:{method}")
        } else {
            format!("{action}:window-message-failed")
        };
        WindowInputReport {
            success,
            method,
            target_x: screen_x,
            target_y: screen_y,
            client_x: client.map(|p| p.x),
            client_y: client.map(|p| p.y),
        }
    }

    fn move_via_window_messages(root_hwnd: Option<isize>, screen_x: i32, screen_y: i32) -> bool {
        if let Some(root) = root_hwnd {
            focus_window(root);
            std::thread::sleep(Duration::from_millis(120));
        }
        for hwnd in click_targets(root_hwnd, screen_x, screen_y) {
            if let Some(client) = screen_to_client(hwnd, screen_x, screen_y) {
                let lparam = make_lparam(client.x, client.y);
                unsafe {
                    if PostMessageW(hwnd, WM_MOUSEMOVE, 0, lparam) != 0 {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn key(vk: u16, scan: u16, flags: u32) -> KeyboardInput {
        KeyboardInput {
            input_type: INPUT_KEYBOARD,
            _padding: 0,
            ki: KeyboardInputData {
                w_vk: vk,
                w_scan: scan,
                dw_flags: flags,
                ..KeyboardInputData::default()
            },
            _tail_padding: [0; 8],
        }
    }

    fn send_keyboard_inputs(mut inputs: Vec<KeyboardInput>) -> bool {
        if inputs.is_empty() {
            return true;
        }
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr() as *mut c_void,
                std::mem::size_of::<KeyboardInput>() as i32,
            ) == inputs.len() as u32
        }
    }

    fn send_ctrl_a() -> bool {
        send_keyboard_inputs(vec![
            key(VK_CONTROL, 0, 0),
            key(VK_A, 0, 0),
            key(VK_A, 0, KEYEVENTF_KEYUP),
            key(VK_CONTROL, 0, KEYEVENTF_KEYUP),
        ])
    }

    fn type_unicode_text(text: &str) -> bool {
        let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
        for unit in text.encode_utf16() {
            inputs.push(key(0, unit, KEYEVENTF_UNICODE));
            inputs.push(key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
        send_keyboard_inputs(inputs)
    }

    fn cursor_pos() -> Option<Point> {
        let mut point = Point::default();
        let ok = unsafe { GetCursorPos(&mut point) };
        (ok != 0).then_some(point)
    }

    fn is_near(x: i32, y: i32) -> bool {
        cursor_pos()
            .map(|p| (p.x - x).abs() <= 6 && (p.y - y).abs() <= 6)
            .unwrap_or(false)
    }

    fn virtual_screen_metrics() -> (i32, i32, i32, i32) {
        unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
                GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
            )
        }
    }

    fn send_input_absolute(x: i32, y: i32) -> bool {
        let (vx, vy, vw, vh) = virtual_screen_metrics();
        let nx = (((x - vx) as f64 * 65535.0) / ((vw - 1).max(1) as f64)).round() as i32;
        let ny = (((y - vy) as f64 * 65535.0) / ((vh - 1).max(1) as f64)).round() as i32;
        let mut input = Input {
            input_type: INPUT_MOUSE,
            _padding: 0,
            mi: MouseInput {
                dx: nx,
                dy: ny,
                dw_flags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                ..MouseInput::default()
            },
        };
        unsafe {
            SendInput(
                1,
                &mut input as *mut _ as *mut c_void,
                std::mem::size_of::<Input>() as i32,
            ) == 1
        }
    }

    fn mouse_event_absolute(x: i32, y: i32) {
        let (vx, vy, vw, vh) = virtual_screen_metrics();
        let nx = (((x - vx) as f64 * 65535.0) / ((vw - 1).max(1) as f64)).round() as u32;
        let ny = (((y - vy) as f64 * 65535.0) / ((vh - 1).max(1) as f64)).round() as u32;
        unsafe {
            mouse_event(
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                nx,
                ny,
                0,
                0,
            );
        }
    }

    fn try_move_once(x: i32, y: i32) -> bool {
        if send_input_absolute(x, y) {
            std::thread::sleep(Duration::from_millis(80));
            if is_near(x, y) {
                return true;
            }
        }

        unsafe {
            SetCursorPos(x, y);
        }
        std::thread::sleep(Duration::from_millis(80));
        if is_near(x, y) {
            return true;
        }

        mouse_event_absolute(x, y);
        std::thread::sleep(Duration::from_millis(80));
        is_near(x, y)
    }

    fn move_physical(x: i32, y: i32) -> InputReport {
        let moved = try_move_once(x, y);
        let final_pos = cursor_pos();
        let (vx, vy, vw, vh) = virtual_screen_metrics();
        let method = if moved {
            "物理鼠标".into()
        } else {
            format!("物理鼠标(失败, 虚拟屏 {vx},{vy} {vw}x{vh})")
        };
        InputReport {
            success: moved,
            method,
            target_x: x,
            target_y: y,
            final_x: final_pos.map(|p| p.x),
            final_y: final_pos.map(|p| p.y),
        }
    }

    fn click_physical(x: i32, y: i32) -> InputReport {
        let report = move_physical(x, y);
        if !report.success {
            return report;
        }
        unsafe {
            mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        }
        std::thread::sleep(Duration::from_millis(70));
        unsafe {
            mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
        }
        report
    }

    fn merge_methods(message_attempts: &[String], physical: &InputReport) -> String {
        if message_attempts.is_empty() {
            return physical.method.clone();
        }
        format!("{} → {}", message_attempts.join("+"), physical.method)
    }

    pub fn window_message_attempts(
        x: i32,
        y: i32,
        window_handle: Option<i64>,
        window_title: Option<&str>,
    ) -> Vec<String> {
        set_thread_dpi_awareness();
        let root = resolve_root_hwnd(window_handle, window_title);
        try_window_message_clicks(root, x, y)
    }

    pub fn click_via_window_messages(
        x: i32,
        y: i32,
        window_handle: Option<i64>,
        window_title: Option<&str>,
    ) -> WindowInputReport {
        set_thread_dpi_awareness();
        let root = resolve_root_hwnd(window_handle, window_title);
        let attempts = try_window_message_clicks(root, x, y);
        window_report("click", root, x, y, !attempts.is_empty(), attempts)
    }

    pub fn move_via_window_messages_only(
        x: i32,
        y: i32,
        window_handle: Option<i64>,
        window_title: Option<&str>,
    ) -> WindowInputReport {
        set_thread_dpi_awareness();
        let root = resolve_root_hwnd(window_handle, window_title);
        let success = move_via_window_messages(root, x, y);
        let methods = if success {
            vec!["PostMessage(WM_MOUSEMOVE)".into()]
        } else {
            Vec::new()
        };
        window_report("move", root, x, y, success, methods)
    }

    pub fn type_text(text: &str) -> bool {
        set_thread_dpi_awareness();
        let _ = send_ctrl_a();
        std::thread::sleep(Duration::from_millis(80));
        type_unicode_text(text)
    }

    pub fn physical_click_at(x: i32, y: i32) -> InputReport {
        set_thread_dpi_awareness();
        click_physical(x, y)
    }

    pub fn physical_move_to(x: i32, y: i32) -> InputReport {
        set_thread_dpi_awareness();
        move_physical(x, y)
    }

    pub fn move_to(
        x: i32,
        y: i32,
        window_handle: Option<i64>,
        window_title: Option<&str>,
    ) -> InputReport {
        set_thread_dpi_awareness();
        let root = resolve_root_hwnd(window_handle, window_title);
        let _ = move_via_window_messages(root, x, y);
        move_physical(x, y)
    }

    pub fn click_at(
        x: i32,
        y: i32,
        window_handle: Option<i64>,
        window_title: Option<&str>,
    ) -> InputReport {
        set_thread_dpi_awareness();
        let root = resolve_root_hwnd(window_handle, window_title);
        let attempts = try_window_message_clicks(root, x, y);
        let mut physical = click_physical(x, y);
        physical.method = merge_methods(&attempts, &physical);
        physical
    }
}

#[cfg(not(windows))]
mod cursor_control {
    #[derive(Clone, Debug)]
    pub struct InputReport {
        pub success: bool,
        pub method: String,
        pub target_x: i32,
        pub target_y: i32,
        pub final_x: Option<i32>,
        pub final_y: Option<i32>,
    }

    #[derive(Clone, Debug)]
    pub struct WindowInputReport {
        pub success: bool,
        pub method: String,
        pub target_x: i32,
        pub target_y: i32,
        pub client_x: Option<i32>,
        pub client_y: Option<i32>,
    }

    pub fn window_message_attempts(
        _x: i32,
        _y: i32,
        _window_handle: Option<i64>,
        _window_title: Option<&str>,
    ) -> Vec<String> {
        Vec::new()
    }

    pub fn click_via_window_messages(
        x: i32,
        y: i32,
        _window_handle: Option<i64>,
        _window_title: Option<&str>,
    ) -> WindowInputReport {
        WindowInputReport {
            success: false,
            method: "unsupported".into(),
            target_x: x,
            target_y: y,
            client_x: None,
            client_y: None,
        }
    }

    pub fn move_via_window_messages_only(
        x: i32,
        y: i32,
        _window_handle: Option<i64>,
        _window_title: Option<&str>,
    ) -> WindowInputReport {
        WindowInputReport {
            success: false,
            method: "unsupported".into(),
            target_x: x,
            target_y: y,
            client_x: None,
            client_y: None,
        }
    }

    pub fn type_text(_text: &str) -> bool {
        false
    }

    pub fn physical_click_at(x: i32, y: i32) -> InputReport {
        physical_move_to(x, y)
    }

    pub fn physical_move_to(x: i32, y: i32) -> InputReport {
        InputReport {
            success: false,
            method: "unsupported".into(),
            target_x: x,
            target_y: y,
            final_x: None,
            final_y: None,
        }
    }

    pub fn move_to(
        x: i32,
        y: i32,
        _window_handle: Option<i64>,
        _window_title: Option<&str>,
    ) -> InputReport {
        InputReport {
            success: false,
            method: "unsupported".into(),
            target_x: x,
            target_y: y,
            final_x: None,
            final_y: None,
        }
    }

    pub fn click_at(
        x: i32,
        y: i32,
        _window_handle: Option<i64>,
        _window_title: Option<&str>,
    ) -> InputReport {
        move_to(x, y, None, None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualRule {
    pub id: String,
    pub name: String,
    pub window_title: String,
    pub match_type: String,
    pub match_value: String,
    #[serde(default = "default_tolerance")]
    pub color_tolerance: u32,
    #[serde(default)]
    pub click: bool,
}

fn default_tolerance() -> u32 {
    24
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainStep {
    pub id: String,
    pub rule_id: String,
    #[serde(default = "default_delay")]
    pub delay_ms: u64,
}

fn default_delay() -> u64 {
    1000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleChain {
    pub id: String,
    pub name: String,
    pub steps: Vec<ChainStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationStep {
    pub id: String,
    pub name: String,
    #[serde(default = "default_action")]
    pub action: String,
    pub window_title: String,
    pub match_type: String,
    pub match_value: String,
    #[serde(default = "default_tolerance")]
    pub color_tolerance: u32,
    #[serde(default)]
    pub click: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_delay")]
    pub delay_ms: u64,
    #[serde(default = "default_delay_unit")]
    pub delay_unit: String,
    #[serde(default)]
    pub time_after_step: bool,
    #[serde(default)]
    pub last_measured_ms: u64,
    #[serde(default)]
    pub offset_x: i32,
    #[serde(default)]
    pub offset_y: i32,
    #[serde(default = "default_input_mode")]
    pub input_mode: String,
    #[serde(default)]
    pub input_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationTemplate {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub steps: Vec<AutomationStep>,
    #[serde(default)]
    pub updated_at: u64,
}

fn default_action() -> String {
    "click".into()
}

fn default_input_mode() -> String {
    "installBase".into()
}

fn default_enabled() -> bool {
    true
}

fn default_delay_unit() -> String {
    "ms".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct VisualRulesFile {
    #[serde(default)]
    rules: Vec<VisualRule>,
    #[serde(default)]
    chains: Vec<RuleChain>,
    #[serde(default)]
    steps: Vec<AutomationStep>,
    #[serde(default)]
    templates: Vec<AutomationTemplate>,
    #[serde(default)]
    active_template_id: String,
}

fn default_visual_rules_file() -> VisualRulesFile {
    let steps = Vec::new();
    let template = AutomationTemplate {
        id: "default".into(),
        name: "默认模板".into(),
        steps: steps.clone(),
        updated_at: current_unix_ms(),
    };
    VisualRulesFile {
        steps,
        templates: vec![template],
        active_template_id: "default".into(),
        ..VisualRulesFile::default()
    }
}

fn current_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn sanitize_template_name(name: &str) -> String {
    let clean = name.trim();
    if clean.is_empty() {
        "未命名模板".into()
    } else {
        clean.chars().take(40).collect()
    }
}

fn new_template_id() -> String {
    format!("template-{}", current_unix_ms())
}

fn ensure_template_state(file: &mut VisualRulesFile) {
    if file.templates.is_empty() {
        let steps = if !file.steps.is_empty() {
            file.steps.clone()
        } else if !file.chains.is_empty() {
            effective_steps_without_templates(file)
        } else {
            Vec::new()
        };
        file.templates.push(AutomationTemplate {
            id: "default".into(),
            name: "默认模板".into(),
            steps: steps.clone(),
            updated_at: current_unix_ms(),
        });
        file.active_template_id = "default".into();
        file.steps = steps;
    }

    let active_exists = file
        .templates
        .iter()
        .any(|template| template.id == file.active_template_id);
    if !active_exists {
        file.active_template_id = file
            .templates
            .first()
            .map(|template| template.id.clone())
            .unwrap_or_default();
    }

    if let Some(active) = file
        .templates
        .iter()
        .find(|template| template.id == file.active_template_id)
    {
        file.steps = active.steps.clone();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualTargetResult {
    pub success: bool,
    pub raw_screen_x: Option<i32>,
    pub raw_screen_y: Option<i32>,
    pub offset_x: Option<i32>,
    pub offset_y: Option<i32>,
    pub screen_x: Option<i32>,
    pub screen_y: Option<i32>,
    pub window_left: Option<i32>,
    pub window_top: Option<i32>,
    pub window_width: Option<i32>,
    pub window_height: Option<i32>,
    pub window_title: Option<String>,
    pub detail: Option<String>,
    pub preview_image: Option<String>,
    pub window_handle: Option<i64>,
    pub message: String,
}

fn portable_root() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn scripts_dir() -> Result<PathBuf, String> {
    let beside_exe = portable_root().join("scripts");
    if beside_exe.join("visual-target.ps1").is_file() {
        return Ok(beside_exe);
    }
    let dev_scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("scripts");
    if dev_scripts.join("visual-target.ps1").is_file() {
        return Ok(dev_scripts);
    }
    Err("找不到 scripts\\visual-target.ps1".into())
}

fn rules_file() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("visual-rules.json"))
}

fn load_rules_file() -> Result<VisualRulesFile, String> {
    let path = rules_file()?;
    if !path.is_file() {
        return Ok(default_visual_rules_file());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn save_rules_file(file: &VisualRulesFile) -> Result<(), String> {
    let path = rules_file()?;
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn localize_visual_message(message: Option<String>, success: bool) -> String {
    let Some(message) = message else {
        return if success {
            "完成".into()
        } else {
            "失败".into()
        };
    };

    match message.as_str() {
        "Preview moved mouse." => "已预览坐标并移动鼠标".into(),
        "Preview move may have failed." => "已找到坐标，但移动鼠标可能失败".into(),
        "Preview coordinates only." => "已预览坐标".into(),
        "Moved and clicked." => "已移动并点击".into(),
        "Moved; click may have failed." => "已移动鼠标，点击可能失败".into(),
        "Mouse moved only." => "已移动鼠标".into(),
        "Target window not found." => "未找到目标窗口".into(),
        "Target not found." => "未找到匹配目标".into(),
        _ => message,
    }
}

fn parse_script_result(stdout: &str) -> Result<VisualTargetResult, String> {
    for line in stdout.lines() {
        let Some(json) = line.strip_prefix("RESULT_JSON:") else {
            continue;
        };
        #[derive(Deserialize)]
        struct Raw {
            success: bool,
            raw_screen_x: Option<i32>,
            raw_screen_y: Option<i32>,
            offset_x: Option<i32>,
            offset_y: Option<i32>,
            screen_x: Option<i32>,
            screen_y: Option<i32>,
            window_left: Option<i32>,
            window_top: Option<i32>,
            window_width: Option<i32>,
            window_height: Option<i32>,
            window_title: Option<String>,
            detail: Option<String>,
            preview_image: Option<String>,
            window_handle: Option<i64>,
            message: Option<String>,
        }
        let raw: Raw = serde_json::from_str(json).map_err(|e| e.to_string())?;
        return Ok(VisualTargetResult {
            success: raw.success,
            raw_screen_x: raw.raw_screen_x,
            raw_screen_y: raw.raw_screen_y,
            offset_x: raw.offset_x,
            offset_y: raw.offset_y,
            screen_x: raw.screen_x,
            screen_y: raw.screen_y,
            window_left: raw.window_left,
            window_top: raw.window_top,
            window_width: raw.window_width,
            window_height: raw.window_height,
            window_title: raw.window_title,
            detail: raw.detail,
            preview_image: raw.preview_image,
            window_handle: raw.window_handle,
            message: localize_visual_message(raw.message, raw.success),
        });
    }
    Err("脚本未返回结果".into())
}

fn apply_visual_input(
    result: &mut VisualTargetResult,
    action: &str,
    click: bool,
    input_text: Option<&str>,
    dry_run: bool,
) {
    if !result.success {
        return;
    }

    let (Some(x), Some(y)) = (result.screen_x, result.screen_y) else {
        return;
    };

    let format_failed = |target_x: i32, target_y: i32, fx: i32, fy: i32| -> String {
        format!("鼠标未到位；目标 ({target_x}, {target_y})，当前 ({fx}, {fy})")
    };

    let window_title = result.window_title.as_deref();

    if dry_run {
        result.message = if result.preview_image.is_some() {
            "已生成截图预览，请看红色标记".into()
        } else {
            "已找到坐标，但没有生成截图预览".into()
        };
        return;
    }

    let type_text = input_text
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .filter(|_| action == "inputText");

    let ps_result =
        run_physical_input_ps(x, y, click || type_text.is_some(), type_text, window_title);
    let (success, detail) = match ps_result {
        Ok(ps) => {
            let cursor_ok = ps
                .cursor_x
                .zip(ps.cursor_y)
                .map(|(cx, cy)| (cx - x).abs() <= 8 && (cy - y).abs() <= 8)
                .unwrap_or(false);
            let ok = if type_text.is_some() {
                ps.typed && cursor_ok
            } else if click {
                ps.clicked && cursor_ok
            } else {
                ps.moved && cursor_ok
            };
            let detail = if ok {
                if type_text.is_some() {
                    format!("同进程PS输入路径 ({x}, {y})")
                } else if click {
                    format!("同进程PS点击 ({x}, {y})")
                } else {
                    format!("同进程PS移动 ({x}, {y})")
                }
            } else {
                format_failed(x, y, ps.cursor_x.unwrap_or(-1), ps.cursor_y.unwrap_or(-1))
            };
            if ok {
                (true, detail)
            } else {
                let report = if click {
                    cursor_control::physical_click_at(x, y)
                } else {
                    cursor_control::physical_move_to(x, y)
                };
                if report.success {
                    (
                        true,
                        format!(
                            "Rust兜底{} ({}, {})",
                            if click { "点击" } else { "移动" },
                            report.target_x,
                            report.target_y
                        ),
                    )
                } else {
                    (
                        false,
                        match (report.final_x, report.final_y) {
                            (Some(fx), Some(fy)) => format_failed(x, y, fx, fy),
                            _ => detail,
                        },
                    )
                }
            }
        }
        Err(err) => {
            eprintln!("physical-input ps failed: {err}");
            let report = if click {
                cursor_control::physical_click_at(x, y)
            } else {
                cursor_control::physical_move_to(x, y)
            };
            if report.success {
                (
                    true,
                    format!(
                        "Rust{} ({}, {})",
                        report.method, report.target_x, report.target_y
                    ),
                )
            } else {
                (
                    false,
                    match (report.final_x, report.final_y) {
                        (Some(fx), Some(fy)) => format_failed(x, y, fx, fy),
                        _ => format!("Rust物理输入失败 ({x}, {y})"),
                    },
                )
            }
        }
    };

    result.success = success;
    result.message = if success {
        format!("已通过{detail}")
    } else {
        detail
    };
}

struct PhysicalInputResult {
    moved: bool,
    clicked: bool,
    typed: bool,
    cursor_x: Option<i32>,
    cursor_y: Option<i32>,
}

fn run_physical_input_ps(
    x: i32,
    y: i32,
    click: bool,
    type_text: Option<&str>,
    window_title: Option<&str>,
) -> Result<PhysicalInputResult, String> {
    let scripts = scripts_dir()?;
    let ps1 = scripts.join("visual-target.ps1");

    let mut args = vec![
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-NoProfile".to_string(),
        "-File".to_string(),
        ps1.to_string_lossy().into_owned(),
        "--physical-x".to_string(),
        x.to_string(),
        "--physical-y".to_string(),
        y.to_string(),
    ];
    if click {
        args.push("--click".to_string());
    }
    if let Some(text) = type_text {
        args.push("--type-text".to_string());
        args.push(text.to_string());
    }
    if let Some(title) = window_title.filter(|t| !t.trim().is_empty()) {
        args.push("--window-title".to_string());
        args.push(title.trim().to_string());
    }

    let output = hidden_command("powershell")
        .args(&args)
        .current_dir(scripts.parent().unwrap_or(&scripts))
        .output()
        .map_err(|e| format!("启动物理输入脚本失败: {e}"))?;

    let stdout = decode_console_output(&output.stdout);
    if !output.status.success() {
        return Err(format!(
            "物理输入脚本退出码 {}: {}",
            output.status.code().unwrap_or(-1),
            stdout.trim()
        ));
    }

    #[derive(Deserialize)]
    struct RawPhysical {
        moved: Option<bool>,
        clicked: Option<bool>,
        typed: Option<bool>,
        cursor_x: Option<i32>,
        cursor_y: Option<i32>,
    }

    for line in stdout.lines() {
        let json = line.strip_prefix("RESULT_JSON:").unwrap_or(line);
        if !json.trim_start().starts_with('{') {
            continue;
        }
        let raw: RawPhysical = serde_json::from_str(json).map_err(|e| e.to_string())?;
        return Ok(PhysicalInputResult {
            moved: raw.moved.unwrap_or(false),
            clicked: raw.clicked.unwrap_or(false),
            typed: raw.typed.unwrap_or(false),
            cursor_x: raw.cursor_x,
            cursor_y: raw.cursor_y,
        });
    }

    Err("物理输入脚本未返回结果".into())
}

fn decode_console_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let (cow, _, _) = encoding_rs::GBK.decode(bytes);
    cow.into_owned()
}

struct VisualScriptOptions {
    action: String,
    match_type: String,
    match_value: String,
    window_title: String,
    tolerance: u32,
    click: bool,
    dry_run: bool,
    offset_x: i32,
    offset_y: i32,
    input_text: Option<String>,
}

fn run_visual_script_blocking(opts: VisualScriptOptions) -> Result<VisualTargetResult, String> {
    let scripts = scripts_dir()?;
    let ps1 = scripts.join("visual-target.ps1");

    let mut args = vec![
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-NoProfile".to_string(),
        "-File".to_string(),
        ps1.to_string_lossy().into_owned(),
        "--match-type".to_string(),
        opts.match_type,
        "--match-value".to_string(),
        opts.match_value,
        "--tolerance".to_string(),
        opts.tolerance.to_string(),
        "--offset-x".to_string(),
        opts.offset_x.to_string(),
        "--offset-y".to_string(),
        opts.offset_y.to_string(),
    ];

    if !opts.window_title.trim().is_empty() {
        args.push("--window-title".to_string());
        args.push(opts.window_title.trim().to_string());
    }
    if opts.dry_run {
        args.push("--dry-run".to_string());
    }

    let output = hidden_command("powershell")
        .args(&args)
        .current_dir(scripts.parent().unwrap_or(&scripts))
        .output()
        .map_err(|e| format!("启动视觉定位脚本失败: {}", e))?;

    let stdout = decode_console_output(&output.stdout);
    let stderr = decode_console_output(&output.stderr);

    if !stderr.trim().is_empty() {
        eprintln!("visual-target stderr: {}", stderr);
    }

    let mut result = parse_script_result(&stdout).or_else(|_| {
        if output.status.success() {
            Ok(VisualTargetResult {
                success: true,
                raw_screen_x: None,
                raw_screen_y: None,
                offset_x: None,
                offset_y: None,
                screen_x: None,
                screen_y: None,
                window_left: None,
                window_top: None,
                window_width: None,
                window_height: None,
                window_title: None,
                detail: None,
                preview_image: None,
                window_handle: None,
                message: stdout.trim().to_string(),
            })
        } else {
            Err(format!(
                "视觉定位脚本退出码 {}: {}",
                output.status.code().unwrap_or(-1),
                if stdout.trim().is_empty() {
                    stderr.trim().to_string()
                } else {
                    stdout.trim().to_string()
                }
            ))
        }
    })?;

    apply_visual_input(
        &mut result,
        &opts.action,
        opts.click,
        opts.input_text.as_deref(),
        opts.dry_run,
    );
    Ok(result)
}

async fn run_visual_script_async(opts: VisualScriptOptions) -> Result<VisualTargetResult, String> {
    tauri::async_runtime::spawn_blocking(move || run_visual_script_blocking(opts))
        .await
        .map_err(|e| format!("视觉定位任务异常: {}", e))?
}

fn visual_script_opts(
    action: &str,
    match_type: &str,
    match_value: &str,
    window_title: &str,
    tolerance: u32,
    click: bool,
    dry_run: bool,
    offset_x: i32,
    offset_y: i32,
    input_text: Option<String>,
) -> VisualScriptOptions {
    VisualScriptOptions {
        action: action.to_string(),
        match_type: match_type.to_string(),
        match_value: match_value.to_string(),
        window_title: window_title.to_string(),
        tolerance,
        click,
        dry_run,
        offset_x,
        offset_y,
        input_text,
    }
}

fn rule_to_step(rule: &VisualRule, id: &str, delay_ms: u64) -> AutomationStep {
    AutomationStep {
        id: id.to_string(),
        name: rule.name.clone(),
        action: "click".into(),
        window_title: rule.window_title.clone(),
        match_type: rule.match_type.clone(),
        match_value: rule.match_value.clone(),
        color_tolerance: rule.color_tolerance,
        click: rule.click,
        enabled: true,
        delay_ms,
        delay_unit: default_delay_unit(),
        time_after_step: false,
        last_measured_ms: 0,
        offset_x: 0,
        offset_y: 0,
        input_mode: "installBase".into(),
        input_text: String::new(),
    }
}

fn effective_steps_without_templates(file: &VisualRulesFile) -> Vec<AutomationStep> {
    if !file.steps.is_empty() {
        return file.steps.clone();
    }
    if !file.chains.is_empty() {
        return file.chains[0]
            .steps
            .iter()
            .filter_map(|s| {
                file.rules
                    .iter()
                    .find(|r| r.id == s.rule_id)
                    .map(|r| rule_to_step(r, &s.id, s.delay_ms))
            })
            .collect();
    }
    file.rules
        .iter()
        .map(|r| rule_to_step(r, &r.id, 1000))
        .collect()
}

fn effective_steps(file: &VisualRulesFile) -> Vec<AutomationStep> {
    if let Some(template) = file
        .templates
        .iter()
        .find(|template| template.id == file.active_template_id)
    {
        return template.steps.clone();
    }
    effective_steps_without_templates(file)
}

fn resolve_input_text(
    action: &str,
    input_mode: &str,
    input_text: &str,
) -> Result<Option<String>, String> {
    if action != "inputText" {
        return Ok(None);
    }

    let text = if input_mode == "custom" {
        input_text.trim().to_string()
    } else {
        get_install_base().to_string_lossy().into_owned()
    };

    if text.trim().is_empty() {
        Err("输入路径不能为空".into())
    } else {
        Ok(Some(text))
    }
}

#[tauri::command]
pub fn get_automation_steps_cmd() -> Result<Vec<AutomationStep>, String> {
    let mut file = load_rules_file()?;
    ensure_template_state(&mut file);
    Ok(effective_steps(&file))
}

#[tauri::command]
pub fn save_automation_steps_cmd(steps: Vec<AutomationStep>) -> Result<(), String> {
    let mut file = load_rules_file().unwrap_or_default();
    ensure_template_state(&mut file);
    if let Some(template) = file
        .templates
        .iter_mut()
        .find(|template| template.id == file.active_template_id)
    {
        template.steps = steps.clone();
        template.updated_at = current_unix_ms();
    }
    file.steps = steps;
    save_rules_file(&file)
}

#[tauri::command]
pub fn get_automation_templates_cmd() -> Result<Vec<AutomationTemplate>, String> {
    let mut file = load_rules_file()?;
    ensure_template_state(&mut file);
    Ok(file.templates)
}

#[tauri::command]
pub fn get_active_automation_template_cmd() -> Result<String, String> {
    let mut file = load_rules_file()?;
    ensure_template_state(&mut file);
    Ok(file.active_template_id)
}

#[tauri::command]
pub fn save_automation_template_cmd(
    name: String,
    steps: Vec<AutomationStep>,
) -> Result<AutomationTemplate, String> {
    let mut file = load_rules_file().unwrap_or_default();
    ensure_template_state(&mut file);
    let name = sanitize_template_name(&name);
    if let Some(template) = file
        .templates
        .iter_mut()
        .find(|template| template.name.eq_ignore_ascii_case(&name))
    {
        template.name = name;
        template.steps = steps.clone();
        template.updated_at = current_unix_ms();
        let saved = template.clone();
        file.active_template_id = saved.id.clone();
        file.steps = steps;
        save_rules_file(&file)?;
        return Ok(saved);
    }

    let template = AutomationTemplate {
        id: new_template_id(),
        name,
        steps: steps.clone(),
        updated_at: current_unix_ms(),
    };
    file.active_template_id = template.id.clone();
    file.steps = steps;
    file.templates.push(template.clone());
    save_rules_file(&file)?;
    Ok(template)
}

#[tauri::command]
pub fn set_active_automation_template_cmd(
    template_id: String,
) -> Result<Vec<AutomationStep>, String> {
    let mut file = load_rules_file().unwrap_or_default();
    ensure_template_state(&mut file);
    let steps = file
        .templates
        .iter()
        .find(|template| template.id == template_id)
        .map(|template| template.steps.clone())
        .ok_or_else(|| format!("找不到模板 {}", template_id))?;
    file.active_template_id = template_id;
    file.steps = steps.clone();
    save_rules_file(&file)?;
    Ok(steps)
}

#[tauri::command]
pub fn delete_automation_template_cmd(
    template_id: String,
) -> Result<Vec<AutomationTemplate>, String> {
    let mut file = load_rules_file().unwrap_or_default();
    ensure_template_state(&mut file);
    if file.templates.len() <= 1 {
        return Err("至少保留一个模板".into());
    }
    let before = file.templates.len();
    file.templates.retain(|template| template.id != template_id);
    if file.templates.len() == before {
        return Err(format!("找不到模板 {}", template_id));
    }
    if file.active_template_id == template_id {
        file.active_template_id = file
            .templates
            .first()
            .map(|template| template.id.clone())
            .unwrap_or_default();
    }
    if let Some(active) = file
        .templates
        .iter()
        .find(|template| template.id == file.active_template_id)
    {
        file.steps = active.steps.clone();
    }
    save_rules_file(&file)?;
    Ok(file.templates)
}

#[tauri::command]
pub async fn run_automation_step_cmd(
    step_id: String,
    dry_run: bool,
) -> Result<VisualTargetResult, String> {
    let file = load_rules_file()?;
    let steps = effective_steps(&file);
    let step = steps
        .into_iter()
        .find(|s| s.id == step_id)
        .ok_or_else(|| format!("找不到步骤 {}", step_id))?;
    if step.action == "closeWindow" {
        return close_target_window_cmd(step.window_title);
    }
    let match_type = match step.match_type.as_str() {
        "color" => "color",
        "point" => "point",
        _ => "text",
    };
    let input_text = resolve_input_text(&step.action, &step.input_mode, &step.input_text)?;
    run_visual_script_async(visual_script_opts(
        &step.action,
        match_type,
        &step.match_value,
        &step.window_title,
        step.color_tolerance.max(1),
        step.click,
        dry_run,
        step.offset_x,
        step.offset_y,
        input_text,
    ))
    .await
}

#[tauri::command]
pub fn get_visual_rules_cmd() -> Result<Vec<VisualRule>, String> {
    Ok(load_rules_file()?.rules)
}

#[tauri::command]
pub fn save_visual_rules_cmd(rules: Vec<VisualRule>) -> Result<(), String> {
    let mut file = load_rules_file().unwrap_or_default();
    file.rules = rules;
    save_rules_file(&file)
}

#[tauri::command]
pub async fn run_visual_target_cmd(
    action: Option<String>,
    match_type: String,
    match_value: String,
    window_title: String,
    color_tolerance: u32,
    click: bool,
    dry_run: bool,
    offset_x: Option<i32>,
    offset_y: Option<i32>,
    input_mode: Option<String>,
    input_text: Option<String>,
) -> Result<VisualTargetResult, String> {
    if match_type != "point" && match_value.trim().is_empty() {
        return Err("请填写匹配文字或颜色".into());
    }
    let match_type = match match_type.as_str() {
        "color" => "color",
        "point" => "point",
        _ => "text",
    };
    let action = action.unwrap_or_else(|| "click".into());
    let input_text = resolve_input_text(
        &action,
        input_mode.as_deref().unwrap_or("installBase"),
        input_text.as_deref().unwrap_or(""),
    )?;
    run_visual_script_async(visual_script_opts(
        &action,
        match_type,
        match_value.trim(),
        &window_title,
        color_tolerance.max(1),
        click,
        dry_run,
        offset_x.unwrap_or(0),
        offset_y.unwrap_or(0),
        input_text,
    ))
    .await
}

#[tauri::command]
pub async fn run_visual_rule_cmd(
    rule_id: String,
    dry_run: bool,
) -> Result<VisualTargetResult, String> {
    let rules = load_rules_file()?.rules;
    let rule = rules
        .into_iter()
        .find(|r| r.id == rule_id)
        .ok_or_else(|| format!("找不到规则 {}", rule_id))?;
    let match_type = match rule.match_type.as_str() {
        "color" => "color",
        "point" => "point",
        _ => "text",
    };
    run_visual_script_async(visual_script_opts(
        "click",
        match_type,
        &rule.match_value,
        &rule.window_title,
        rule.color_tolerance.max(1),
        rule.click,
        dry_run,
        0,
        0,
        None,
    ))
    .await
}

#[tauri::command]
pub fn get_visual_chains_cmd() -> Result<Vec<RuleChain>, String> {
    Ok(load_rules_file()?.chains)
}

#[tauri::command]
pub fn save_visual_chains_cmd(chains: Vec<RuleChain>) -> Result<(), String> {
    let mut file = load_rules_file()?;
    file.chains = chains;
    save_rules_file(&file)
}

#[tauri::command]
pub fn run_visual_chain_cmd(chain_id: String) -> Result<Vec<VisualTargetResult>, String> {
    let file = load_rules_file()?;
    let chain = file
        .chains
        .iter()
        .find(|c| c.id == chain_id)
        .ok_or_else(|| format!("找不到规则链 {}", chain_id))?;

    let mut results = Vec::new();

    for (i, step) in chain.steps.iter().enumerate() {
        let rule = file
            .rules
            .iter()
            .find(|r| r.id == step.rule_id)
            .ok_or_else(|| format!("步骤 {} 引用的规则 {} 不存在", i + 1, step.rule_id))?;

        let match_type = match rule.match_type.as_str() {
            "color" => "color",
            "point" => "point",
            _ => "text",
        };
        let result = run_visual_script_blocking(visual_script_opts(
            "click",
            match_type,
            &rule.match_value,
            &rule.window_title,
            rule.color_tolerance.max(1),
            rule.click,
            false,
            0,
            0,
            None,
        ))?;

        results.push(result);

        if step.delay_ms > 0 && i + 1 < chain.steps.len() {
            std::thread::sleep(std::time::Duration::from_millis(step.delay_ms));
        }
    }

    Ok(results)
}
