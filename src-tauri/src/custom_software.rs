use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::data_dir;
use crate::software::{SoftwareSource, SoftwareTarget};

#[derive(Serialize, Deserialize, Clone)]
pub struct CustomSoftwareConfig {
    pub id: String,
    pub display_name: String,
    pub repo: String,
    pub asset_match: String,
    pub exe_match: String,
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
pub async fn add_custom_software(config: CustomSoftwareConfig) -> Result<(), String> {
    let mut list = load_custom_software();
    // 覆盖旧的或追加新的
    if let Some(pos) = list.iter().position(|x| x.id == config.id) {
        list[pos] = config;
    } else {
        list.push(config);
    }
    save_custom_software(&list)
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
            source: SoftwareSource::Github(c.repo),
            install_kind: "portable".into(),
            ocr_install: false,
        })
        .collect()
}
