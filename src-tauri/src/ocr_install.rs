use std::path::PathBuf;
use std::process::Command;

use crate::config::{get_install_base, package_cache_path};

#[derive(serde::Serialize)]
pub struct WeGameInstallResult {
    pub success: bool,
    pub message: String,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
    pub installed: bool,
}

fn portable_root() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn scripts_dir() -> Result<PathBuf, String> {
    let beside_exe = portable_root().join("scripts");
    if beside_exe.join("run-wegame-ocr.ps1").is_file() {
        return Ok(beside_exe);
    }

    let dev_scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("scripts");
    if dev_scripts.join("run-wegame-ocr.ps1").is_file() {
        return Ok(dev_scripts);
    }

    Err("找不到 scripts\\run-wegame-ocr.ps1，请确认便携包内带有 scripts 文件夹".into())
}

fn resolve_wegame_package(id: &str, version: &str, file_name: &str) -> Result<PathBuf, String> {
    if id != "wegame" {
        return Err(format!("{} 暂不支持辅助安装", id));
    }

    let package = package_cache_path(id, version, file_name)?;
    if !package.is_file() {
        return Err("请先下载安装包".into());
    }

    Ok(package)
}

fn wegame_install_dir() -> Result<PathBuf, String> {
    let install_dir = get_install_base().join("wegame");
    std::fs::create_dir_all(&install_dir).map_err(|e| e.to_string())?;
    Ok(install_dir)
}

#[tauri::command]
pub fn launch_wegame_installer_cmd(
    id: String,
    version: String,
    file_name: String,
) -> Result<WeGameInstallResult, String> {
    let package = resolve_wegame_package(&id, &version, &file_name)?;
    let install_dir = wegame_install_dir()?;
    let _scripts = scripts_dir()?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let parent = package.parent().ok_or("安装包路径无效")?;
        let child = Command::new(&package)
            .current_dir(parent)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("启动安装器失败: {}", e))?;

        return Ok(WeGameInstallResult {
            success: true,
            message: format!(
                "已启动 WeGame 安装器 (PID {})。建议目录: {}",
                child.id(),
                install_dir.to_string_lossy()
            ),
            exit_code: None,
            pid: Some(child.id()),
            installed: false,
        });
    }

    #[cfg(not(windows))]
    {
        let _ = (package, install_dir);
        Err("安装器启动仅支持 Windows".into())
    }
}
