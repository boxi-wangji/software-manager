use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::config::data_dir;
use crate::software::{SoftwareSource, SoftwareTarget};

#[derive(Serialize, Deserialize, Clone)]
pub struct CustomSoftwareConfig {
    pub id: String,
    pub display_name: String,
    #[serde(default = "default_source_kind")]
    pub source_kind: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub asset_match: String,
    #[serde(default)]
    pub exe_match: String,
    #[serde(default)]
    pub download_url: String,
    #[serde(default)]
    pub page_url: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub silent_install_args: String,
    #[serde(default)]
    pub icon_path: String,
}

fn default_source_kind() -> String {
    "github".into()
}

const RESERVED_SOFTWARE_IDS: &[&str] = &[
    "stranslate",
    "quickclipboard",
    "leagueakari",
    "wegame",
    "amd-adrenalin",
    "winget",
];

fn slugify_source_id(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;

    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "source".into()
    } else {
        out
    }
}

fn generate_unique_custom_id(display_name: &str, list: &[CustomSoftwareConfig]) -> String {
    let mut used: HashSet<String> = RESERVED_SOFTWARE_IDS
        .iter()
        .map(|id| (*id).into())
        .collect();
    for item in list {
        used.insert(item.id.clone());
    }

    let base = slugify_source_id(display_name);
    let mut candidate = base.clone();
    let mut index = 2;
    while used.contains(&candidate) {
        candidate = format!("{base}-{index}");
        index += 1;
    }
    candidate
}

pub fn custom_software_config_path() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("custom_software.json"))
}

pub fn load_custom_software() -> Vec<CustomSoftwareConfig> {
    let path = match custom_software_config_path() {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    if !path.exists() {
        return vec![];
    }

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    serde_json::from_str(&content).unwrap_or_else(|_| vec![])
}

pub fn save_custom_software(list: &Vec<CustomSoftwareConfig>) -> Result<(), String> {
    let path = custom_software_config_path()?;
    let content = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_custom_software(mut config: CustomSoftwareConfig) -> Result<(), String> {
    let mut list = load_custom_software();
    config.id = config.id.trim().to_string();
    config.display_name = config.display_name.trim().to_string();
    config.repo = config.repo.trim().to_string();
    config.asset_match = config.asset_match.trim().to_string();
    config.exe_match = config.exe_match.trim().to_string();
    config.download_url = config.download_url.trim().to_string();
    config.page_url = config.page_url.trim().to_string();
    config.version = config.version.trim().to_string();
    config.file_name = config.file_name.trim().to_string();
    config.silent_install_args = config.silent_install_args.trim().to_string();
    config.icon_path = config.icon_path.trim().to_string();

    if config.source_kind == "github" {
        config.repo = normalize_github_repo(&config.repo).unwrap_or(config.repo);
    }

    if config.display_name.is_empty() {
        config.display_name =
            display_name_from_custom_source(&config).unwrap_or_else(|| "自定义下载源".into());
    }

    if config.id.is_empty() {
        config.id = generate_unique_custom_id(&config.display_name, &list);
    }

    apply_default_silent_install_args(&mut config);

    validate_custom_software(&config)?;
    let icon_id = config.id.clone();
    let should_fetch_icon = config.icon_path.is_empty();
    // 覆盖旧的或追加新的
    if let Some(pos) = list.iter().position(|x| x.id == config.id) {
        list[pos] = config;
    } else {
        list.push(config);
    }
    save_custom_software(&list)?;

    // 图标失败不能影响软件下载源保存；下次启动还会继续补齐。
    if should_fetch_icon {
        let _ = fetch_custom_software_icon(icon_id).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_custom_software(id: String) -> Result<CustomSoftwareConfig, String> {
    load_custom_software()
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "自定义下载源不存在".into())
}

#[tauri::command]
pub async fn fetch_custom_software_icon(id: String) -> Result<String, String> {
    let config = get_custom_config_by_id(&id)?;
    let icon_url = resolve_icon_url(&config).await?;
    let client = reqwest::Client::builder()
        .user_agent("software-manager")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(&icon_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("图标下载失败: HTTP {}", resp.status()));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let ext = icon_ext_from_content_type(&content_type)
        .or_else(|| icon_ext_from_url(&icon_url))
        .unwrap_or("png");
    save_custom_icon_bytes(&id, &bytes, ext)
}

#[tauri::command]
pub async fn fetch_missing_custom_software_icons() -> Vec<String> {
    let missing_ids: Vec<String> = load_custom_software()
        .into_iter()
        .filter(|item| item.icon_path.trim().is_empty())
        .map(|item| item.id)
        .collect();

    let mut updated_ids = Vec::new();
    for id in missing_ids {
        if fetch_custom_software_icon(id.clone()).await.is_ok() {
            updated_ids.push(id);
        }
    }
    updated_ids
}

#[tauri::command]
pub async fn save_custom_software_icon_from_clipboard(id: String) -> Result<String, String> {
    let path = custom_icon_file_path(&id, "png")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$img = [System.Windows.Forms.Clipboard]::GetImage()
if ($null -eq $img) {{ exit 2 }}
$img.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png)
"#,
        path.to_string_lossy().replace('\'', "''")
    );
    let status = hidden_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Sta",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("剪贴板里没有图片；请先截图并复制到剪贴板".into());
    }
    update_custom_icon_path(&id, &path)
}

#[tauri::command]
pub async fn clear_custom_software_icon(id: String) -> Result<(), String> {
    let mut list = load_custom_software();
    let Some(item) = list.iter_mut().find(|item| item.id == id) else {
        return Err("自定义下载源不存在".into());
    };
    item.icon_path.clear();
    save_custom_software(&list)
}

fn validate_custom_software(config: &CustomSoftwareConfig) -> Result<(), String> {
    if config.id.trim().is_empty() {
        return Err("ID 不能为空".into());
    }
    if config.display_name.trim().is_empty() {
        return Err("显示名称不能为空".into());
    }
    if !config
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("ID 只能包含英文、数字、短横线和下划线".into());
    }

    match config.source_kind.as_str() {
        "direct" => {
            if !(config.download_url.starts_with("http://")
                || config.download_url.starts_with("https://"))
            {
                return Err("直接下载地址必须以 http:// 或 https:// 开头".into());
            }
        }
        _ => {
            if !config.repo.contains('/') {
                return Err("GitHub 仓库格式应为 owner/repo".into());
            }
            if config.asset_match.trim().is_empty() {
                return Err("GitHub 下载文件匹配不能为空".into());
            }
        }
    }

    Ok(())
}

fn get_custom_config_by_id(id: &str) -> Result<CustomSoftwareConfig, String> {
    load_custom_software()
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "自定义下载源不存在".into())
}

async fn resolve_icon_url(config: &CustomSoftwareConfig) -> Result<String, String> {
    if config.source_kind == "github" {
        let repo = normalize_github_repo(&config.repo).unwrap_or_else(|| config.repo.clone());
        let owner = repo.split('/').next().unwrap_or("").trim();
        if !owner.is_empty() {
            return Ok(format!("https://github.com/{}.png?size=128", owner));
        }
    }

    let page = if !config.page_url.trim().is_empty() {
        config.page_url.trim()
    } else {
        config.download_url.trim()
    };
    if page.is_empty() {
        return Err("没有可用于获取图标的官网地址".into());
    }
    let page_url = normalize_http_url(page)?;
    // Cursor 的更新 API 会直接跳转到 200 MB 安装包，不能把它当网页读取。
    if is_cursor_official_source(&page_url) {
        return Ok("https://cursor.com/favicon.ico".into());
    }
    let client = reqwest::Client::builder()
        .user_agent("software-manager")
        .build()
        .map_err(|e| e.to_string())?;
    if is_probably_download_url(&page_url) {
        return favicon_url_for_origin(&page_url);
    }
    let html = client
        .get(&page_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    find_icon_link(&html, &page_url).or_else(|_| favicon_url_for_origin(&page_url))
}

fn find_icon_link(html: &str, base_url: &str) -> Result<String, String> {
    let mut best: Option<(i32, String)> = None;
    let mut rest = html;
    while let Some(index) = rest.find("<link") {
        let after = &rest[index..];
        let end = after.find('>').unwrap_or(after.len());
        let tag = &after[..end];
        let rel = extract_attr(tag, "rel").unwrap_or_default().to_lowercase();
        if rel.contains("icon") {
            if let Some(href) = extract_attr(tag, "href") {
                let score = if rel.contains("apple-touch-icon") {
                    30
                } else if rel.contains("shortcut") {
                    20
                } else {
                    10
                };
                if let Some(url) = absolutize_url(&href, base_url) {
                    if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
                        best = Some((score, url));
                    }
                }
            }
        }
        rest = &after[end..];
    }
    best.map(|(_, url)| url)
        .ok_or_else(|| "官网页面没有声明图标".into())
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let pattern = format!("{name}=");
    let start = tag.find(&pattern)? + pattern.len();
    let tail = tag[start..].trim_start();
    let quote = tail.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    Some(tail[1..].split(quote).next()?.to_string())
}

fn normalize_http_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("https://{trimmed}"))
    }
}

fn absolutize_url(url: &str, base_url: &str) -> Option<String> {
    if url.starts_with("https://") || url.starts_with("http://") {
        return Some(url.to_string());
    }
    if url.starts_with("//") {
        let scheme = reqwest::Url::parse(base_url).ok()?.scheme().to_string();
        return Some(format!("{scheme}:{url}"));
    }
    let base = reqwest::Url::parse(base_url).ok()?;
    base.join(url).ok().map(|url| url.to_string())
}

fn favicon_url_for_origin(url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    let origin = parsed.origin().ascii_serialization();
    Ok(format!("{}/favicon.ico", origin.trim_end_matches('/')))
}

fn is_probably_download_url(url: &str) -> bool {
    let lower = url.split('?').next().unwrap_or(url).to_lowercase();
    [
        ".exe",
        ".msi",
        ".zip",
        ".7z",
        ".rar",
        ".dmg",
        ".appimage",
        ".deb",
        ".rpm",
        ".msixbundle",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

fn is_cursor_official_source(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    matches!(
        parsed.host_str().map(|host| host.to_ascii_lowercase()),
        Some(host)
            if host == "cursor.com"
                || host == "www.cursor.com"
                || host == "api2.cursor.sh"
                || host == "downloads.cursor.com"
    )
}

fn icon_ext_from_content_type(content_type: &str) -> Option<&'static str> {
    let lower = content_type.to_lowercase();
    if lower.contains("png") {
        Some("png")
    } else if lower.contains("jpeg") || lower.contains("jpg") {
        Some("jpg")
    } else if lower.contains("svg") {
        Some("svg")
    } else if lower.contains("webp") {
        Some("webp")
    } else if lower.contains("icon") || lower.contains("ico") {
        Some("ico")
    } else {
        None
    }
}

fn icon_ext_from_url(url: &str) -> Option<&'static str> {
    let lower = url.split('?').next().unwrap_or(url).to_lowercase();
    ["png", "jpg", "jpeg", "svg", "webp", "ico"]
        .into_iter()
        .find(|ext| lower.ends_with(&format!(".{ext}")))
        .map(|ext| if ext == "jpeg" { "jpg" } else { ext })
}

fn custom_icon_file_path(id: &str, ext: &str) -> Result<PathBuf, String> {
    Ok(data_dir()?
        .join("icons")
        .join(format!("{}.{}", slugify_source_id(id), ext)))
}

fn save_custom_icon_bytes(id: &str, bytes: &[u8], ext: &str) -> Result<String, String> {
    let path = custom_icon_file_path(id, ext)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    update_custom_icon_path(id, &path)
}

fn update_custom_icon_path(id: &str, path: &PathBuf) -> Result<String, String> {
    let mut list = load_custom_software();
    let Some(item) = list.iter_mut().find(|item| item.id == id) else {
        return Err("自定义下载源不存在".into());
    };
    item.icon_path = path.to_string_lossy().to_string();
    let icon_path = item.icon_path.clone();
    save_custom_software(&list)?;
    Ok(icon_path)
}

#[cfg(windows)]
fn hidden_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(windows))]
fn hidden_command(program: &str) -> std::process::Command {
    std::process::Command::new(program)
}

fn normalize_github_repo(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_end_matches('/');
    let path = if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("http://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("github.com/") {
        rest
    } else {
        trimmed
    };
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner == "repos" || owner.contains(':') || repo.contains(':') {
        return None;
    }
    Some(format!(
        "{}/{}",
        owner.trim_end_matches(".git"),
        repo.trim_end_matches(".git")
    ))
}

fn display_name_from_custom_source(config: &CustomSoftwareConfig) -> Option<String> {
    if is_cursor_official_source(&config.download_url)
        || is_cursor_official_source(&config.page_url)
    {
        return Some("Cursor".into());
    }
    if !config.file_name.trim().is_empty() {
        return display_name_from_file_name(&config.file_name);
    }
    file_name_from_url(&config.download_url).and_then(|name| display_name_from_file_name(&name))
}

fn file_name_from_url(url: &str) -> Option<String> {
    let without_query = url.split('?').next().unwrap_or(url);
    without_query
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn display_name_from_file_name(file_name: &str) -> Option<String> {
    let stem = file_name
        .rsplit_once('.')
        .map(|(left, _)| left)
        .unwrap_or(file_name);
    let words: Vec<String> = stem
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|part| {
            let lower = part.to_lowercase();
            if lower.is_empty()
                || lower.chars().all(|ch| ch.is_ascii_digit())
                || matches!(
                    lower.as_str(),
                    "setup"
                        | "installer"
                        | "install"
                        | "download"
                        | "release"
                        | "stable"
                        | "latest"
                        | "x64"
                        | "x86"
                        | "win"
                        | "windows"
                )
            {
                return None;
            }
            Some(format!(
                "{}{}",
                lower[..1].to_uppercase(),
                lower.get(1..).unwrap_or("")
            ))
        })
        .collect();
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

#[tauri::command]
pub async fn remove_custom_software(id: String) -> Result<(), String> {
    let mut list = load_custom_software();
    list.retain(|x| x.id != id);
    save_custom_software(&list)
}

/// 为已识别的官方 Inno Setup 安装包补上安全的默认静默参数。
/// 已由用户填写的参数永远不会被覆盖。
pub fn ensure_default_silent_install_profiles() -> Result<(), String> {
    let mut list = load_custom_software();
    let mut changed = false;

    for config in &mut list {
        let before = config.silent_install_args.clone();
        apply_default_silent_install_args(config);
        if config.silent_install_args != before {
            changed = true;
        }
    }

    if changed {
        save_custom_software(&list)?;
    }
    Ok(())
}

fn apply_default_silent_install_args(config: &mut CustomSoftwareConfig) {
    if !config.silent_install_args.trim().is_empty() || config.source_kind != "direct" {
        return;
    }

    let identity = [
        config.id.as_str(),
        config.display_name.as_str(),
        config.download_url.as_str(),
        config.page_url.as_str(),
        config.asset_match.as_str(),
    ]
    .join(" ")
    .to_ascii_lowercase();

    if identity.contains("visual-studio-code")
        || identity.contains("visual studio code")
        || identity.contains("code.visualstudio.com")
    {
        config.silent_install_args =
            r#"/VERYSILENT /SUPPRESSMSGBOXES /MERGETASKS="desktopicon,!associatewithfiles""#.into();
    } else if identity.contains("cursor") || identity.contains("cursor.com") {
        config.silent_install_args = "/VERYSILENT /SUPPRESSMSGBOXES".into();
    }
}

pub fn custom_software_targets() -> Vec<SoftwareTarget> {
    let list = load_custom_software();
    list.into_iter()
        .map(|mut c| {
            apply_default_silent_install_args(&mut c);
            SoftwareTarget {
                id: c.id,
                display_name: c.display_name,
                source: if c.source_kind == "direct" {
                    SoftwareSource::DirectDownload {
                        url: c.download_url,
                        page_url: c.page_url,
                        asset_match: c.asset_match,
                        version: c.version,
                        file_name: c.file_name,
                    }
                } else {
                    SoftwareSource::Github(c.repo)
                },
                install_kind: "download".into(),
                ocr_install: false,
                silent_install_args: c.silent_install_args,
                icon_path: c.icon_path,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{apply_default_silent_install_args, find_icon_link, CustomSoftwareConfig};

    #[test]
    fn prefers_apple_touch_icon_from_official_page() {
        let html = r#"
            <link rel="shortcut icon" href="/assets/favicon.ico" sizes="128x128" />
            <link rel="apple-touch-icon" href="/assets/apple-touch-icon.png" />
        "#;

        let icon = find_icon_link(html, "https://code.visualstudio.com/thank-you?dv=win64user")
            .expect("official page icon");

        assert_eq!(
            icon,
            "https://code.visualstudio.com/assets/apple-touch-icon.png"
        );
    }

    #[test]
    fn supplies_vscode_silent_install_profile() {
        let mut config = CustomSoftwareConfig {
            id: "visual-studio-code".into(),
            display_name: "Visual Studio Code".into(),
            source_kind: "direct".into(),
            repo: String::new(),
            asset_match: "vscodeusersetup x64 exe".into(),
            exe_match: String::new(),
            download_url: "https://code.visualstudio.com/thank-you?dv=win64user".into(),
            page_url: String::new(),
            version: String::new(),
            file_name: String::new(),
            silent_install_args: String::new(),
            icon_path: String::new(),
        };

        apply_default_silent_install_args(&mut config);

        assert_eq!(
            config.silent_install_args,
            r#"/VERYSILENT /SUPPRESSMSGBOXES /MERGETASKS="desktopicon,!associatewithfiles""#
        );
    }
}
