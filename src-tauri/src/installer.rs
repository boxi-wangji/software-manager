use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use crate::config::{get_install_base, package_cache_path};
use crate::software::preferred_main_exe;

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

// 下载文件到本地,通过事件上报进度
// on_progress 事件 payload: { downloaded: u64, total: u64, percent: f64 }
#[derive(serde::Serialize, Clone)]
pub struct DownloadProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: f64,
}

#[derive(serde::Serialize, Clone)]
pub struct InstallResult {
    pub id: String,
    pub success: bool,
    pub message: String,
    pub shortcut_path: Option<String>,
    pub package_path: Option<String>,
    pub used_cache: bool,
}

// 下载 + 解压 + 创建快捷方式,一条龙
#[tauri::command]
pub async fn install_software(
    app: AppHandle,
    id: String,
    url: String,
    file_name: String,
    version: String,
    expected_size: u64,
) -> Result<InstallResult, String> {
    // 1. 准备目录:{安装根目录}/{id}
    let base_dir = get_install_base();
    let work_dir = base_dir.join(&id);
    std::fs::create_dir_all(&work_dir).map_err(|e| e.to_string())?;

    // 2. 优先使用 data/packages 里的本地安装包,没有才下载
    let package_path = package_cache_path(&id, &version, &file_name)?;
    let used_cache = ensure_cached_package(&app, &id, &url, &package_path, expected_size).await?;

    // 3. 解压(zip 用 Rust,7z 用 Windows 自带 tar,exe 不解)
    let exe_path = if file_name.ends_with(".zip") {
        extract_zip(&package_path, &work_dir, &id)?
    } else if file_name.ends_with(".7z") {
        extract_7z(&package_path, &work_dir, &id)?
    } else if file_name.ends_with(".exe") {
        // 便携版 exe 要复制到安装目录,缓存包保留在 data/packages
        let exe_path = work_dir.join(&file_name);
        std::fs::copy(&package_path, &exe_path).map_err(|e| format!("复制便携版 exe 失败: {}", e))?;
        exe_path
    } else {
        return Err(format!("不支持的格式: {}", file_name));
    };

    // 4. 创建桌面快捷方式
    let shortcut = create_desktop_shortcut(&app, &id, &exe_path)?;

    Ok(InstallResult {
        id,
        success: true,
        message: if used_cache {
            "使用缓存安装完成".into()
        } else {
            "下载并缓存后安装完成".into()
        },
        shortcut_path: Some(shortcut.to_string_lossy().into()),
        package_path: Some(package_path.to_string_lossy().into()),
        used_cache,
    })
}

#[tauri::command]
pub async fn cache_software_package(
    app: AppHandle,
    id: String,
    url: String,
    file_name: String,
    version: String,
    expected_size: u64,
) -> Result<InstallResult, String> {
    let package_path = package_cache_path(&id, &version, &file_name)?;
    let used_cache = ensure_cached_package(&app, &id, &url, &package_path, expected_size).await?;

    Ok(InstallResult {
        id,
        success: true,
        message: if used_cache {
            "已使用本地缓存".into()
        } else {
            "安装包已缓存".into()
        },
        shortcut_path: None,
        package_path: Some(package_path.to_string_lossy().into()),
        used_cache,
    })
}

fn is_valid_cached_package(path: &Path, expected_size: u64) -> Result<bool, String> {
    if !path.is_file() {
        return Ok(false);
    }

    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    Ok(size > 0 && (expected_size == 0 || size == expected_size))
}

async fn ensure_cached_package(
    app: &AppHandle,
    id: &str,
    url: &str,
    package_path: &PathBuf,
    expected_size: u64,
) -> Result<bool, String> {
    if is_valid_cached_package(package_path, expected_size)? {
        return Ok(true);
    }

    if let Some(parent) = package_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    if package_path.exists() {
        std::fs::remove_file(package_path).map_err(|e| format!("清理损坏缓存失败: {}", e))?;
    }

    let temp_name = package_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{}.part", name))
        .unwrap_or_else(|| "package.part".into());
    let temp_path = package_path.with_file_name(temp_name);
    if temp_path.exists() {
        std::fs::remove_file(&temp_path).map_err(|e| format!("清理临时下载失败: {}", e))?;
    }

    download_with_progress(app, id, url, &temp_path).await?;

    if !is_valid_cached_package(&temp_path, expected_size)? {
        let actual = std::fs::metadata(&temp_path).map(|m| m.len()).unwrap_or(0);
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("安装包大小不匹配: 期望 {} 字节,实际 {} 字节", expected_size, actual));
    }

    std::fs::rename(&temp_path, package_path).map_err(|e| format!("保存安装包缓存失败: {}", e))?;
    Ok(false)
}

async fn download_with_progress(
    app: &AppHandle,
    id: &str,
    url: &str,
    dest: &PathBuf,
) -> Result<(), String> {
    let resp = reqwest::Client::builder()
        .user_agent("software-manager")
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("下载失败: HTTP {}", resp.status()));
    }

    let total = resp.content_length().unwrap_or(0);
    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(dest).await.map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 { (downloaded as f64 / total as f64) * 100.0 } else { 0.0 };
        let _ = app.emit("download-progress", DownloadProgress {
            id: id.into(),
            downloaded,
            total,
            percent,
        });
    }
    file.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

// 解压 zip,返回解压后的主 exe 路径
fn extract_zip(archive: &PathBuf, work_dir: &PathBuf, id: &str) -> Result<PathBuf, String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match entry.enclosed_name() {
            Some(p) => work_dir.join(p),
            None => continue,
        };
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = std::fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    find_main_exe(work_dir, id)
}

// 解压 7z:用 Windows 自带 tar(Win10+ 内置,Win11 支持 .7z)
fn extract_7z(archive: &PathBuf, work_dir: &PathBuf, id: &str) -> Result<PathBuf, String> {
    let archive_str = archive.to_string_lossy();
    let work_str = work_dir.to_string_lossy();

    let status = hidden_command("tar")
        .args(["-xf", &archive_str, "-C", &work_str])
        .status()
        .map_err(|e| format!("调用 tar 失败: {}", e))?;

    if !status.success() {
        return Err("7z 解压失败,请确认系统是 Windows 10/11 且 tar 可用".into());
    }

    find_main_exe(work_dir, id)
}

fn is_helper_exe(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("uninstall")
        || lower.contains("setup")
        || lower.contains("crash")
        || lower.contains("updater")
        || lower.contains("elevate")
        || lower == "update.exe"
}

// 在目录里找主 exe:优先配置名,否则排除辅助程序后选最浅目录里最大的
fn find_main_exe(dir: &PathBuf, id: &str) -> Result<PathBuf, String> {
    if let Some(name) = preferred_main_exe(id) {
        let path = dir.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }

    let mut candidates: Vec<(usize, u64, PathBuf)> = Vec::new();
    collect_exe_candidates(dir, dir, &mut candidates)?;

    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.1.cmp(&a.1))
            .then(a.2.file_name().cmp(&b.2.file_name()))
    });

    candidates
        .into_iter()
        .map(|(_, _, path)| path)
        .next()
        .ok_or_else(|| "解压完成但没找到 exe".into())
}

fn collect_exe_candidates(
    root: &PathBuf,
    current: &PathBuf,
    out: &mut Vec<(usize, u64, PathBuf)>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(current).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_exe_candidates(root, &path, out)?;
        } else if path.extension().map(|e| e == "exe").unwrap_or(false) {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_helper_exe(name) {
                continue;
            }
            let depth = path
                .strip_prefix(root)
                .map(|p| p.components().count().saturating_sub(1))
                .unwrap_or(usize::MAX);
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            out.push((depth, size, path));
        }
    }
    Ok(())
}

// 创建桌面快捷方式(.lnk)
fn create_desktop_shortcut(_app: &AppHandle, id: &str, target: &PathBuf) -> Result<PathBuf, String> {
    let shortcut_path = shortcut_path_for_id(id)?;

    // 用 PowerShell 的 WScript.Shell 创建 .lnk
    let target_str = target.to_string_lossy();
    let shortcut_str = shortcut_path.to_string_lossy();
    let ps_script = format!(
        "$s=(New-Object -COM WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.Save()",
        shortcut_str,
        target_str,
        target.parent().map(|p| p.to_string_lossy()).unwrap_or_default()
    );

    let status = hidden_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &ps_script])
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() {
        return Err("创建快捷方式失败".into());
    }

    Ok(shortcut_path)
}

fn display_name_for_id(id: &str) -> &str {
    match id {
        "stranslate" => "STranslate",
        "quickclipboard" => "QuickClipboard",
        "leagueakari" => "LeagueAkari",
        "wegame" => "WeGame",
        "amd-adrenalin" => "AMD Adrenalin Edition",
        _ => id,
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
struct WindowsUninstallEntry {
    #[serde(rename = "DisplayName")]
    display_name: Option<String>,
    #[serde(rename = "DisplayIcon")]
    display_icon: Option<String>,
    #[serde(rename = "InstallLocation")]
    install_location: Option<String>,
    #[serde(rename = "UninstallString")]
    uninstall_string: Option<String>,
    #[serde(rename = "QuietUninstallString")]
    quiet_uninstall_string: Option<String>,
}

fn query_wegame_uninstall_entry() -> Option<WindowsUninstallEntry> {
    #[cfg(not(windows))]
    {
        return None;
    }

    #[cfg(windows)]
    {
        let script = r#"
$paths = @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
$item = Get-ItemProperty -Path $paths -ErrorAction SilentlyContinue |
  Where-Object {
    ($_.DisplayName -match 'WeGame') -or
    ($_.DisplayIcon -match 'WeGame') -or
    ($_.InstallLocation -match 'WeGame')
  } |
  Sort-Object @{ Expression = { if ($_.DisplayName -eq 'WeGame') { 0 } else { 1 } } }, DisplayName |
  Select-Object -First 1 DisplayName, DisplayIcon, InstallLocation, UninstallString, QuietUninstallString
if ($null -ne $item) { $item | ConvertTo-Json -Compress }
"#;

        let output = hidden_command("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            return None;
        }
        serde_json::from_str::<WindowsUninstallEntry>(&text).ok()
    }
}

fn query_amd_adrenalin_uninstall_entry() -> Option<WindowsUninstallEntry> {
    #[cfg(not(windows))]
    {
        return None;
    }

    #[cfg(windows)]
    {
        let script = r#"
$paths = @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
$item = Get-ItemProperty -Path $paths -ErrorAction SilentlyContinue |
  Where-Object {
    ($_.DisplayName -match '^AMD Software') -or
    ($_.DisplayName -match 'AMD.*Adrenalin') -or
    ($_.DisplayIcon -match 'RadeonSoftware\.exe') -or
    ($_.InstallLocation -match '\\AMD\\CNext')
  } |
  Sort-Object @{ Expression = { if ($_.DisplayName -match '^AMD Software') { 0 } else { 1 } } }, DisplayName |
  Select-Object -First 1 DisplayName, DisplayIcon, InstallLocation, UninstallString, QuietUninstallString
if ($null -ne $item) { $item | ConvertTo-Json -Compress }
"#;

        let output = hidden_command("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            return None;
        }
        serde_json::from_str::<WindowsUninstallEntry>(&text).ok()
    }
}

fn existing_wegame_exe_from_pathish(value: &str) -> Option<PathBuf> {
    existing_named_exe_from_pathish(value, "wegame.exe")
}

fn existing_named_exe_from_pathish(value: &str, exe_name: &str) -> Option<PathBuf> {
    let cleaned = value
        .trim()
        .trim_matches('"')
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"');
    if cleaned.is_empty() {
        return None;
    }

    let path = PathBuf::from(cleaned);
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case(exe_name))
        .unwrap_or(false)
        && path.is_file()
    {
        return Some(path);
    }

    None
}

fn common_wegame_exe_exists() -> bool {
    let mut roots = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles(x86)") {
        roots.push(PathBuf::from(program_files));
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(program_files));
    }
    if let Some(local) = dirs::data_local_dir() {
        roots.push(local);
    }

    roots.into_iter().any(|root| {
        [
            root.join("Tencent").join("WeGame").join("wegame.exe"),
            root.join("WeGame").join("wegame.exe"),
        ]
        .into_iter()
        .any(|path| path.is_file())
    })
}

fn portable_wegame_exe_exists(work_dir: &PathBuf) -> bool {
    find_exe_in_tree(work_dir, "wegame.exe").is_some()
}

fn common_amd_adrenalin_exe_exists() -> bool {
    let mut roots = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(program_files));
    }
    if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
        roots.push(PathBuf::from(program_files_x86));
    }

    roots.into_iter().any(|root| {
        [
            root.join("AMD").join("CNext").join("CNext").join("RadeonSoftware.exe"),
            root.join("AMD").join("CNext").join("RadeonSoftware.exe"),
        ]
        .into_iter()
        .any(|path| path.is_file())
    })
}

fn amd_adrenalin_installed() -> bool {
    if common_amd_adrenalin_exe_exists() {
        return true;
    }

    if let Some(entry) = query_amd_adrenalin_uninstall_entry() {
        if let Some(icon) = entry.display_icon.as_deref() {
            if existing_named_exe_from_pathish(icon, "RadeonSoftware.exe").is_some() {
                return true;
            }
        }
        if let Some(location) = entry.install_location.as_deref() {
            let root = PathBuf::from(location);
            if root.join("RadeonSoftware.exe").is_file()
                || find_exe_in_tree(&root, "RadeonSoftware.exe").is_some()
            {
                return true;
            }
        }
    }

    false
}

fn wegame_installed_outside_portable() -> bool {
    if let Some(entry) = query_wegame_uninstall_entry() {
        let is_wegame_entry = entry
            .display_name
            .as_deref()
            .map(|name| name.to_lowercase().contains("wegame"))
            .unwrap_or(false);

        if let Some(icon) = entry.display_icon.as_deref() {
            if existing_wegame_exe_from_pathish(icon).is_some() {
                return true;
            }
        }

        if let Some(location) = entry.install_location.as_deref() {
            let root = PathBuf::from(location);
            if root.join("wegame.exe").is_file() || find_exe_in_tree(&root, "wegame.exe").is_some() {
                return true;
            }
        }

        if is_wegame_entry && common_wegame_exe_exists() {
            return true;
        }

        for command in [entry.uninstall_string.as_deref(), entry.quiet_uninstall_string.as_deref()]
            .into_iter()
            .flatten()
        {
            if existing_wegame_exe_from_pathish(command).is_some() {
                return true;
            }
        }
    }
    common_wegame_exe_exists()
}

#[cfg(windows)]
fn is_user_admin() -> bool {
    #[link(name = "shell32")]
    extern "system" {
        fn IsUserAnAdmin() -> i32;
    }
    unsafe { IsUserAnAdmin() != 0 }
}

#[cfg(windows)]
fn expand_environment_strings(value: &str) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;

    let wide: Vec<u16> = std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut buf = vec![0u16; 32_768];
    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn ExpandEnvironmentStringsW(lpSrc: *const u16, lpDst: *mut u16, nSize: u32) -> u32;
        }
        let needed = ExpandEnvironmentStringsW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32);
        if needed == 0 || needed as usize > buf.len() {
            return Err("展开卸载命令环境变量失败".into());
        }
        let len = needed.saturating_sub(1) as usize;
        Ok(String::from_utf16_lossy(&buf[..len]))
    }
}

#[cfg(not(windows))]
fn expand_environment_strings(value: &str) -> Result<String, String> {
    Ok(value.to_string())
}

#[cfg(windows)]
fn parse_windows_command_line(command: &str) -> Result<Vec<String>, String> {
    use std::os::windows::ffi::OsStrExt;

    let wide: Vec<u16> = std::ffi::OsStr::new(command)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut argc: i32 = 0;
    unsafe {
        #[link(name = "shell32")]
        extern "system" {
            fn CommandLineToArgvW(lpCmdLine: *const u16, pNumArgs: *mut i32) -> *mut *mut u16;
            fn LocalFree(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        }

        let argv = CommandLineToArgvW(wide.as_ptr(), &mut argc);
        if argv.is_null() || argc <= 0 {
            return Err("解析卸载命令失败".into());
        }

        let mut parts = Vec::new();
        for i in 0..argc {
            let ptr = *argv.offset(i as isize);
            if ptr.is_null() {
                continue;
            }
            let len = (0..).take_while(|&j| *ptr.offset(j) != 0).count();
            let slice = std::slice::from_raw_parts(ptr, len);
            parts.push(String::from_utf16_lossy(slice));
        }
        LocalFree(argv as *mut _);

        if parts.is_empty() {
            return Err("解析卸载命令失败".into());
        }
        Ok(parts)
    }
}

#[cfg(not(windows))]
fn parse_windows_command_line(command: &str) -> Result<Vec<String>, String> {
    Ok(vec![command.to_string()])
}

fn shell_quote_json(value: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

fn launch_uninstall_command(command: &str) -> Result<(), String> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return Err("卸载命令为空".into());
    }

    let expanded = expand_environment_strings(cmd)?;
    let parts = parse_windows_command_line(&expanded)?;
    let file = parts
        .first()
        .filter(|part| !part.trim().is_empty())
        .ok_or("卸载命令格式无效")?;

    if !Path::new(file).is_file() {
        return Err(format!("卸载程序不存在: {}", file));
    }

    #[cfg(windows)]
    {
        if is_user_admin() {
            let mut process = hidden_command(file);
            if let Some(parent) = Path::new(file).parent() {
                if parent.is_dir() {
                    process.current_dir(parent);
                }
            }
            if parts.len() > 1 {
                process.args(&parts[1..]);
            }
            process
                .spawn()
                .map_err(|e| format!("启动卸载程序失败: {} ({})", file, e))?;
            return Ok(());
        }
    }

    let file_json = shell_quote_json(file)?;
    let args_json = parts
        .iter()
        .skip(1)
        .map(|arg| shell_quote_json(arg))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    let script = if parts.len() > 1 {
        format!(
            "$file = {file_json}; \
             $argv = @({args_json}); \
             Start-Process -FilePath $file -ArgumentList $argv -Verb RunAs -WindowStyle Normal"
        )
    } else {
        format!("$file = {file_json}; Start-Process -FilePath $file -Verb RunAs -WindowStyle Normal")
    };
    let status = hidden_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|e| format!("启动卸载程序失败: {}", e))?;
    if !status.success() {
        return Err("启动卸载程序失败，可能被 UAC 取消".into());
    }
    Ok(())
}

fn uninstall_wegame() -> Result<InstallResult, String> {
    if let Some(entry) = query_wegame_uninstall_entry() {
        let command = entry
            .uninstall_string
            .as_deref()
            .or(entry.quiet_uninstall_string.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("找到了 WeGame，但注册表里没有卸载命令")?;
        launch_uninstall_command(command)?;
        return Ok(InstallResult {
            id: "wegame".into(),
            success: true,
            message: "已启动 WeGame 卸载程序；如果出现 UAC，请手动允许".into(),
            shortcut_path: None,
            package_path: None,
            used_cache: false,
        });
    }

    let work_dir = get_install_base().join("wegame");
    if work_dir.exists() {
        remove_dir_all(&work_dir)?;
        return Ok(InstallResult {
            id: "wegame".into(),
            success: true,
            message: format!("已删除软件管家内的 WeGame 目录: {}", work_dir.display()),
            shortcut_path: None,
            package_path: None,
            used_cache: false,
        });
    }

    Err("未找到 WeGame 卸载信息".into())
}

fn uninstall_amd_adrenalin() -> Result<InstallResult, String> {
    if let Some(entry) = query_amd_adrenalin_uninstall_entry() {
        let command = entry
            .uninstall_string
            .as_deref()
            .or(entry.quiet_uninstall_string.as_deref())
            .ok_or("AMD 卸载项没有卸载命令")?;

        launch_uninstall_command(command)?;
        return Ok(InstallResult {
            id: "amd-adrenalin".into(),
            success: true,
            message: "已启动 AMD Adrenalin 卸载程序".into(),
            shortcut_path: None,
            package_path: None,
            used_cache: false,
        });
    }

    Err("未找到 AMD Adrenalin 卸载信息".into())
}

fn find_exe_in_tree(dir: &std::path::Path, file_name: &str) -> Option<PathBuf> {
    let target = file_name.to_lowercase();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case(file_name) || n.to_lowercase() == target)
                .unwrap_or(false)
            {
                return Some(path);
            }
        }
    }
    None
}

fn portable_app_installed(id: &str, work_dir: &PathBuf) -> bool {
    if let Some(exe_name) = crate::software::preferred_main_exe(id) {
        if find_exe_in_tree(work_dir, &exe_name).is_some() {
            return true;
        }
    }

    work_dir.exists()
        && std::fs::read_dir(work_dir)
            .ok()
            .and_then(|mut d| d.next())
            .and_then(|e| e.ok())
            .is_some()
}

fn shortcut_path_for_id(id: &str) -> Result<PathBuf, String> {
    let desktop = dirs::desktop_dir().ok_or("找不到桌面目录")?;
    Ok(desktop.join(format!("{}.lnk", display_name_for_id(id))))
}

fn remove_dir_all(path: &PathBuf) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|e| format!("删除目录失败: {}", e))?;
    }
    Ok(())
}

// 卸载:删除安装目录 + 桌面快捷方式
#[tauri::command]
pub fn uninstall_software(id: String) -> Result<InstallResult, String> {
    if id == "wegame" {
        return uninstall_wegame();
    }
    if id == "amd-adrenalin" {
        return uninstall_amd_adrenalin();
    }

    let work_dir = get_install_base().join(&id);
    remove_dir_all(&work_dir)?;

    let shortcut = shortcut_path_for_id(&id)?;
    if shortcut.exists() {
        std::fs::remove_file(&shortcut).map_err(|e| format!("删除快捷方式失败: {}", e))?;
    }

    Ok(InstallResult {
        id: id.clone(),
        success: true,
        message: format!("已卸载: {}", work_dir.display()),
        shortcut_path: None,
        package_path: None,
        used_cache: false,
    })
}

// 检查是否已安装(目录存在且非空,或快捷方式存在)
#[tauri::command]
pub fn is_software_installed(id: String) -> Result<bool, String> {
    let work_dir = get_install_base().join(&id);

    if id == "wegame" {
        return Ok(portable_wegame_exe_exists(&work_dir) || wegame_installed_outside_portable());
    }
    if id == "amd-adrenalin" {
        return Ok(amd_adrenalin_installed());
    }

    let has_dir = portable_app_installed(&id, &work_dir);
    let shortcut = shortcut_path_for_id(&id)?;
    Ok(has_dir || shortcut.exists())
}
