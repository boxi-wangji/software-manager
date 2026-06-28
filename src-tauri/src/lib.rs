mod github;
mod software;
mod installer;
mod config;
mod custom_software;
mod ocr_install;
mod visual_target;

use github::GithubRelease;
use serde::Deserialize;
use software::{
    is_portable_target, software_list, source_kind_for, SoftwareAsset, SoftwareInfo, SoftwareSource,
    SoftwareTarget,
};
use installer::{cache_software_package, install_software, is_software_installed, uninstall_software};
use config::{
    get_app_install_paths_cmd, get_install_paths_cmd, get_package_cache_info_cmd,
    open_cached_package_cmd, reset_install_paths_cmd, set_install_paths_cmd,
};
use ocr_install::launch_wegame_installer_cmd;
use visual_target::{
    close_target_window_cmd,
    delete_automation_template_cmd, get_active_automation_template_cmd,
    get_automation_steps_cmd, get_automation_templates_cmd, get_visual_chains_cmd,
    get_visual_rules_cmd, pick_screen_color_cmd, run_automation_step_cmd, run_visual_chain_cmd,
    run_visual_rule_cmd, run_visual_target_cmd, save_automation_steps_cmd,
    save_automation_template_cmd, save_visual_chains_cmd, save_visual_rules_cmd,
    set_active_automation_template_cmd,
};
use custom_software::{add_custom_software, remove_custom_software};

// 检测电脑架构：Windows 上返回 x64 或 arm64�?Windows 涓婅繑鍥?x64 �?arm64
#[tauri::command]
fn detect_arch() -> String {
    if cfg!(target_arch = "x86_64") {
        "x64".into()
    } else if cfg!(target_arch = "aarch64") {
        "arm64".into()
    } else {
        "unknown".into()
    }
}

// 查所有软件的最新版信息�?
#[tauri::command]
async fn fetch_all_software() -> Result<Vec<SoftwareInfo>, String> {
    let list = software_list();
    let mut results = Vec::new();

    for target in list {
        match fetch_one_target(&target).await {
            Ok(mut info) => {
                info.ocr_install = target.ocr_install;
                info.source_kind = source_kind_for(&target).into();
                results.push(info);
            }
            Err(e) => {
                // 一个失败不影响其他，返回错误信息占位
                results.push(SoftwareInfo {
                    id: target.id.clone(),
                    display_name: target.display_name.clone(),
                    latest_version: format!("查询失败: {}", e),
                    release_url: String::new(),
                    published_at: String::new(),
                    portable: None,
                    install_kind: target.install_kind.clone(),
                    source_kind: source_kind_for(&target).into(),
                    ocr_install: target.ocr_install,
                });
            }
        }
    }

    Ok(results)
}

async fn fetch_one_target(target: &SoftwareTarget) -> Result<SoftwareInfo, String> {
    match &target.source {
        SoftwareSource::Github(repo) => {
            fetch_one_github_release(
                &target.id,
                &target.display_name,
                repo,
                &target.install_kind,
            )
            .await
        }
        SoftwareSource::WegameOfficial => fetch_wegame_release(target).await,
        SoftwareSource::AmdAdrenalinOfficial => fetch_amd_adrenalin_release(target).await,
    }
}

async fn fetch_one_github_release(
    id: &str,
    display_name: &str,
    repo: &str,
    install_kind: &str,
) -> Result<SoftwareInfo, String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);

    let client = reqwest::Client::builder()
        .user_agent("software-manager")
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.get(&url);
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let release: GithubRelease = resp.json().await.map_err(|e| e.to_string())?;

        // 挑出便携版
    let portable = release
        .assets
        .iter()
        .find(|a| is_portable_target(id, &a.name))
        .map(|a| SoftwareAsset {
            name: a.name.clone(),
            browser_download_url: a.browser_download_url.clone(),
            size: a.size,
        });

    Ok(SoftwareInfo {
        id: id.into(),
        display_name: display_name.into(),
        latest_version: release.tag_name,
        release_url: release.html_url,
        published_at: release.published_at,
        portable,
        install_kind: install_kind.into(),
        source_kind: "github".into(),
        ocr_install: false,
    })
}

#[derive(Deserialize)]
struct WegameHomeConfigs {
    data: Vec<WegameConfigItem>,
}

#[derive(Deserialize)]
struct WegameConfigItem {
    name: String,
    value: String,
    item_update_time: String,
}

#[derive(Deserialize)]
struct WegameDownload {
    #[serde(rename = "type")]
    kind: String,
    url: serde_json::Value,
}

async fn fetch_wegame_release(target: &SoftwareTarget) -> Result<SoftwareInfo, String> {
    let config_url = format!(
        "https://wegame.gtimg.com/bin_res/ossjson/wegame_home_configs.js?t={}",
        current_timestamp_ms()
    );
    let client = reqwest::Client::builder()
        .user_agent("software-manager")
        .build()
        .map_err(|e| e.to_string())?;

    let text = client
        .get(&config_url)
        .header("Referer", "https://www.wegame.com.cn/home/")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let json = text
        .trim()
        .strip_prefix("var wegame_home_configs = ")
        .and_then(|s| s.strip_suffix(';'))
        .ok_or("无法解析 WeGame 配置 JSON")?;
    let config: WegameHomeConfigs = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let item = config
        .data
        .iter()
        .find(|item| item.name == "new_downloads")
        .or_else(|| config.data.iter().find(|item| item.name == "downloads"))
        .ok_or("WeGame 配置中未找到下载项")?;
    let downloads: Vec<WegameDownload> = serde_json::from_str(&item.value).map_err(|e| e.to_string())?;
    let url = downloads
        .iter()
        .find(|download| download.kind == "pc")
        .and_then(|download| download.url.as_str())
        .map(str::to_string)
        .or_else(|| {
            config
                .data
                .iter()
                .find(|item| item.name == "downloadUrl")
                .and_then(|item| serde_json::from_str::<String>(&item.value).ok())
        })
        .ok_or("WeGame 配置中未找到 PC 下载链接")?;

    let file_name = file_name_from_url(&url).ok_or("无法从 WeGame 下载链接解析文件名")?;
    let size = fetch_content_length(&client, &url).await.unwrap_or(0);

    Ok(SoftwareInfo {
        id: target.id.clone(),
        display_name: target.display_name.clone(),
        latest_version: version_from_wegame_file(&file_name),
        release_url: "https://www.wegame.com.cn/home/".into(),
        published_at: item.item_update_time.clone(),
        portable: Some(SoftwareAsset {
            name: file_name,
            browser_download_url: url,
            size,
        }),
        install_kind: target.install_kind.clone(),
        source_kind: "official".into(),
        ocr_install: target.ocr_install,
    })
}

async fn fetch_amd_adrenalin_release(target: &SoftwareTarget) -> Result<SoftwareInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent("software-manager")
        .build()
        .map_err(|e| e.to_string())?;

    let release = fetch_latest_amd_adrenalin_from_sitemap(&client)
        .await
        .unwrap_or_else(|_| AmdAdrenalinRelease {
            version: "26.6.2".into(),
            release_url: "https://www.amd.com/en/resources/support-articles/release-notes/RN-RAD-WIN-26-6-2.html".into(),
            download_url: "https://drivers.amd.com/drivers/whql-amd-software-adrenalin-edition-26.6.2-win11-c.exe".into(),
            published_at: "2026-06-22".into(),
        });

    let file_name = file_name_from_url(&release.download_url).ok_or("无法解析 AMD 安装包文件名")?;
    let size = fetch_content_length_with_referer(&client, &release.download_url, &release.release_url).await.unwrap_or(0);

    Ok(SoftwareInfo {
        id: target.id.clone(),
        display_name: target.display_name.clone(),
        latest_version: format!("v{}", release.version),
        release_url: release.release_url,
        published_at: release.published_at,
        portable: Some(SoftwareAsset {
            name: file_name,
            browser_download_url: release.download_url,
            size,
        }),
        install_kind: target.install_kind.clone(),
        source_kind: "official".into(),
        ocr_install: target.ocr_install,
    })
}

struct AmdAdrenalinRelease {
    version: String,
    release_url: String,
    download_url: String,
    published_at: String,
}

#[derive(Clone)]
struct AmdReleaseCandidate {
    version: String,
    version_parts: Vec<u32>,
    release_url: String,
    lastmod: String,
    hotfix: bool,
}

async fn fetch_latest_amd_adrenalin_from_sitemap(
    client: &reqwest::Client,
) -> Result<AmdAdrenalinRelease, String> {
    let sitemap = client
        .get("https://www.amd.com/en.sitemap.xml")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let mut candidates = parse_amd_release_candidates(&sitemap);
    candidates.sort_by(|a, b| {
        compare_version_parts(&b.version_parts, &a.version_parts)
            .then_with(|| b.hotfix.cmp(&a.hotfix))
            .then_with(|| b.lastmod.cmp(&a.lastmod))
    });

    for candidate in candidates.into_iter().take(10) {
        let page = client
            .get(&candidate.release_url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        if !page.to_lowercase().contains("amd software") || !page.to_lowercase().contains("adrenalin") {
            continue;
        }
        if let Some(download_url) = find_amd_driver_download_url(&page) {
            return Ok(AmdAdrenalinRelease {
                version: candidate.version,
                release_url: candidate.release_url,
                download_url,
                published_at: candidate.lastmod.get(0..10).unwrap_or("").to_string(),
            });
        }
    }

    Err("娌℃湁浠?AMD sitemap 鎵惧�?Adrenalin 涓嬭浇閾炬帴".into())
}

fn parse_amd_release_candidates(sitemap: &str) -> Vec<AmdReleaseCandidate> {
    let mut candidates = Vec::new();
    for block in sitemap.split("</url>") {
        let Some(loc) = extract_between(block, "<loc>", "</loc>") else {
            continue;
        };
        if !loc.contains("/en/resources/support-articles/release-notes/RN-RAD-WIN-") {
            continue;
        }
        let upper = loc.to_uppercase();
        if upper.contains("LEGACY")
            || upper.contains("BOOTCAMP")
            || upper.contains("DEV-PREVIEW")
            || upper.contains("DXCGC")
        {
            continue;
        }
        let Some((version, parts)) = amd_version_from_release_url(loc) else {
            continue;
        };
        let lastmod = extract_between(block, "<lastmod>", "</lastmod>")
            .unwrap_or("")
            .to_string();
        candidates.push(AmdReleaseCandidate {
            version,
            version_parts: parts,
            release_url: loc.to_string(),
            lastmod,
            hotfix: upper.contains("HOTFIX"),
        });
    }
    candidates
}

fn amd_version_from_release_url(url: &str) -> Option<(String, Vec<u32>)> {
    let marker = "RN-RAD-WIN-";
    let tail = url.split(marker).nth(1)?;
    let version_raw: String = tail
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect();
    if version_raw.is_empty() {
        return None;
    }
    let parts: Vec<u32> = version_raw
        .split('-')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect();
    if parts.len() < 3 {
        return None;
    }
    Some((parts.iter().map(u32::to_string).collect::<Vec<_>>().join("."), parts))
}

fn compare_version_parts(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    let len = a.len().max(b.len());
    for i in 0..len {
        let left = *a.get(i).unwrap_or(&0);
        let right = *b.get(i).unwrap_or(&0);
        match left.cmp(&right) {
            std::cmp::Ordering::Equal => continue,
            order => return order,
        }
    }
    std::cmp::Ordering::Equal
}

fn find_amd_driver_download_url(page: &str) -> Option<String> {
    let normalized = page.replace("\\/", "/").replace("&amp;", "&");
    let mut best: Option<String> = None;
    let mut rest = normalized.as_str();
    while let Some(index) = rest.find("https://drivers.amd.com/") {
        let after = &rest[index..];
        let end = after
            .find(|ch: char| ch == '"' || ch == '\'' || ch == '<' || ch.is_whitespace())
            .unwrap_or(after.len());
        let url = after[..end].trim_end_matches(['\\', ')', ']']);
        let lower = url.to_lowercase();
        if lower.ends_with(".exe") && lower.contains("adrenalin") && !lower.contains("cleanup") {
            if lower.contains("minimalsetup") {
                return Some(url.to_string());
            }
            best.get_or_insert_with(|| url.to_string());
        }
        rest = &after[end..];
    }
    best
}

fn extract_between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let after_start = text.split(start).nth(1)?;
    after_start.split(end).next()
}

fn current_timestamp_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

async fn fetch_content_length(client: &reqwest::Client, url: &str) -> Result<u64, String> {
    fetch_content_length_with_referer(client, url, "https://www.wegame.com.cn/home/").await
}

async fn fetch_content_length_with_referer(
    client: &reqwest::Client,
    url: &str,
    referer: &str,
) -> Result<u64, String> {
    let resp = client
        .head(url)
        .header("Referer", referer)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0))
}

fn file_name_from_url(url: &str) -> Option<String> {
    let without_query = url.split('?').next().unwrap_or(url);
    without_query
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn version_from_wegame_file(file_name: &str) -> String {
    let stem = file_name.strip_suffix(".exe").unwrap_or(file_name);
    if let Some(version) = stem.split(".std.").nth(1) {
        return format!("v{}", version);
    }
    if let Some(version) = stem.split('.').find(|part| part.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)) {
        return format!("v{}", version);
    }
    "v未知 (latest)".into()
}

#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg(windows)]
fn wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
#[tauri::command]
fn is_elevated_cmd() -> bool {
    #[link(name = "shell32")]
    extern "system" {
        fn IsUserAnAdmin() -> i32;
    }

    unsafe { IsUserAnAdmin() != 0 }
}

#[cfg(not(windows))]
#[tauri::command]
fn is_elevated_cmd() -> bool {
    true
}

#[cfg(windows)]
#[tauri::command]
fn restart_as_admin_cmd(app: tauri::AppHandle) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::ptr::null;

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: isize,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show_cmd: i32,
        ) -> isize;
    }

    let exe = std::env::current_exe().map_err(|e| format!("读取程序路径失败: {e}"))?;
    let verb = wide_null(OsStr::new("runas"));
    let file = wide_null(exe.as_os_str());
    let dir = exe
        .parent()
        .map(|p| wide_null(p.as_os_str()))
        .unwrap_or_else(|| wide_null(OsStr::new(".")));

    let code = unsafe {
        ShellExecuteW(
            0,
            verb.as_ptr(),
            file.as_ptr(),
            null(),
            dir.as_ptr(),
            1,
        )
    };
    if code <= 32 {
        return Err(format!("管理员重启失败，ShellExecuteW={code}"));
    }

    app.exit(0);
    Ok(())
}

#[cfg(not(windows))]
#[tauri::command]
fn restart_as_admin_cmd(_app: tauri::AppHandle) -> Result<(), String> {
    Err("当前平台不需要管理员重启".into())
}

#[tauri::command]
async fn winget_cli_install_cmd(path: String) -> Result<(), String> {
    // 1. Run Add-AppxPackage
    let out = std::process::Command::new("powershell")
        .args(&["-NoProfile", "-Command", &format!("Add-AppxPackage -Path '{}'", path)])
        .output()
        .map_err(|e| format!("PowerShell error: {}", e))?;
    
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("安装失败: {}", err));
    }

    // 2. Run winget settings (this requires admin rights, and might fail if we are not elevated. But the app tries its best)
    let out2 = std::process::Command::new("powershell")
        .args(&["-NoProfile", "-Command", "winget settings --enable BypassCertificatePinningForMicrosoftStore"])
        .output()
        .map_err(|e| format!("Winget error: {}", e))?;
    
    if !out2.status.success() {
        let err = String::from_utf8_lossy(&out2.stderr);
        return Err(format!("安装成功，但配置 BypassCertificatePinningForMicrosoftStore 失败: {}", err));
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            detect_arch,
            fetch_all_software,
            install_software,
            cache_software_package,
            uninstall_software,
            is_software_installed,
            get_install_paths_cmd,
            set_install_paths_cmd,
            reset_install_paths_cmd,
            get_app_install_paths_cmd,
            get_package_cache_info_cmd,
            open_cached_package_cmd,
            launch_wegame_installer_cmd,
            get_visual_rules_cmd,
            save_visual_rules_cmd,
            run_visual_target_cmd,
            run_visual_rule_cmd,
            get_visual_chains_cmd,
            save_visual_chains_cmd,
            run_visual_chain_cmd,
            get_automation_steps_cmd,
            save_automation_steps_cmd,
            get_automation_templates_cmd,
            get_active_automation_template_cmd,
            save_automation_template_cmd,
            set_active_automation_template_cmd,
            delete_automation_template_cmd,
            run_automation_step_cmd,
            add_custom_software,
            remove_custom_software,
            pick_screen_color_cmd,
            close_target_window_cmd,
            is_elevated_cmd,
            restart_as_admin_cmd,
            exit_app,
            winget_cli_install_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
