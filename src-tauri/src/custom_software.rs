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

    validate_custom_software(&config)?;
    // 覆盖旧的或追加新的
    if let Some(pos) = list.iter().position(|x| x.id == config.id) {
        list[pos] = config;
    } else {
        list.push(config);
    }
    save_custom_software(&list)
}

#[tauri::command]
pub async fn get_custom_software(id: String) -> Result<CustomSoftwareConfig, String> {
    load_custom_software()
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "自定义下载源不存在".into())
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

pub fn custom_software_targets() -> Vec<SoftwareTarget> {
    let list = load_custom_software();
    list.into_iter()
        .map(|c| SoftwareTarget {
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
        })
        .collect()
}
