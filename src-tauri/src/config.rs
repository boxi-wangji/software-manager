use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct AppConfig {
    install_base: Option<String>,
}

#[derive(Serialize)]
pub struct InstallPathSettings {
    pub default_base: String,
    pub current_base: String,
}

/// 便携版根目录 = exe 所在文件夹
fn portable_root() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_dir() -> Result<PathBuf, String> {
    let dir = portable_root().join("data");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn safe_path_segment(value: &str) -> String {
    let trimmed = value.trim();
    let cleaned: String = trimmed
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    let cleaned = cleaned.trim_matches(['.', ' ']);
    if cleaned.is_empty() {
        "unknown".into()
    } else {
        cleaned.into()
    }
}

fn safe_file_name(file_name: &str) -> String {
    Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .map(safe_path_segment)
        .unwrap_or_else(|| "package.bin".into())
}

pub fn package_cache_dir(id: &str, version: &str) -> Result<PathBuf, String> {
    let dir = data_dir()?
        .join("packages")
        .join(safe_path_segment(id))
        .join(safe_path_segment(version));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn package_cache_path(id: &str, version: &str, file_name: &str) -> Result<PathBuf, String> {
    Ok(package_cache_dir(id, version)?.join(safe_file_name(file_name)))
}

fn config_file() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("config.json"))
}

fn load_config() -> Result<AppConfig, String> {
    let path = config_file()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_file()?;
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

/** 默认: %LOCALAPPDATA%\software-manager\apps */
pub fn default_install_base() -> PathBuf {
    let base = dirs::data_local_dir()
        .map(|d| d.join("software-manager").join("apps"))
        .unwrap_or_else(|| portable_root().join("apps"));
    let _ = fs::create_dir_all(&base);
    base
}

pub fn get_install_base() -> PathBuf {
    load_config()
        .ok()
        .and_then(|c| c.install_base)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(default_install_base)
}

pub fn set_install_base(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("安装路径不能为空".into());
    }
    let mut config = load_config()?;
    config.install_base = Some(trimmed.to_string());
    save_config(&config)
}

pub fn reset_install_base() -> Result<String, String> {
    let mut config = load_config()?;
    config.install_base = None;
    save_config(&config)?;
    Ok(default_install_base().to_string_lossy().into())
}

pub fn get_install_path_settings() -> InstallPathSettings {
    InstallPathSettings {
        default_base: default_install_base().to_string_lossy().into(),
        current_base: get_install_base().to_string_lossy().into(),
    }
}

#[derive(Serialize)]
pub struct AppInstallPaths {
    pub install_dir: String,
    pub download_file: String,
    pub package_file: String,
    pub shortcut: String,
}

pub fn app_install_paths(id: &str, display_name: &str, version: &str, file_name: &str) -> Result<AppInstallPaths, String> {
    let base = get_install_base();
    let install_dir = base.join(id);
    let download_file = install_dir.join(file_name);
    let package_file = package_cache_path(id, version, file_name)?;
    let shortcut = dirs::desktop_dir()
        .map(|d| d.join(format!("{}.lnk", display_name)))
        .unwrap_or_default();
    Ok(AppInstallPaths {
        install_dir: install_dir.to_string_lossy().into(),
        download_file: download_file.to_string_lossy().into(),
        package_file: package_file.to_string_lossy().into(),
        shortcut: shortcut.to_string_lossy().into(),
    })
}

#[tauri::command]
pub fn get_app_install_paths_cmd(
    id: String,
    display_name: String,
    version: String,
    file_name: String,
) -> Result<AppInstallPaths, String> {
    app_install_paths(&id, &display_name, &version, &file_name)
}

#[derive(Serialize)]
pub struct PackageCacheInfo {
    pub cached: bool,
    pub path: String,
    pub size: u64,
}

fn valid_cached_file(path: &Path, expected_size: u64) -> Result<Option<u64>, String> {
    if !path.is_file() {
        return Ok(None);
    }

    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size == 0 || (expected_size > 0 && size != expected_size) {
        return Ok(None);
    }

    Ok(Some(size))
}

#[tauri::command]
pub fn get_package_cache_info_cmd(
    id: String,
    version: String,
    file_name: String,
    expected_size: u64,
) -> Result<PackageCacheInfo, String> {
    let path = package_cache_path(&id, &version, &file_name)?;
    let size = valid_cached_file(&path, expected_size)?.unwrap_or(0);
    Ok(PackageCacheInfo {
        cached: size > 0,
        path: path.to_string_lossy().into(),
        size,
    })
}

#[tauri::command]
pub fn open_cached_package_cmd(path: String) -> Result<(), String> {
    let file = PathBuf::from(path.trim());
    if !file.is_file() {
        return Err("安装包文件不存在，请重新下载".into());
    }
    open::that(&file).map_err(|e| format!("无法打开安装包: {}", e))
}

#[tauri::command]
pub fn get_install_paths_cmd() -> InstallPathSettings {
    get_install_path_settings()
}

#[tauri::command]
pub fn set_install_paths_cmd(path: String) -> Result<InstallPathSettings, String> {
    set_install_base(path)?;
    Ok(get_install_path_settings())
}

#[tauri::command]
pub fn reset_install_paths_cmd() -> Result<InstallPathSettings, String> {
    reset_install_base()?;
    Ok(get_install_path_settings())
}
