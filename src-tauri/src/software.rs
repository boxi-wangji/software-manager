// 软件清单配置：每个软件对应一个可识别的发布源。

use crate::custom_software::{custom_software_targets, load_custom_software};

#[derive(serde::Serialize, Clone)]
pub struct SoftwareAsset {
    pub name: String,                 // 文件名
    pub browser_download_url: String, // 下载链接
    pub size: u64,                    // 字节
}

#[derive(serde::Serialize, Clone)]
pub struct SoftwareInfo {
    pub id: String,                      // STranslate / QuickClipboard / LeagueAkari
    pub display_name: String,            // 显示名
    pub latest_version: String,          // v2.0.8
    pub release_url: String,             // GitHub Release 页面
    pub published_at: String,            // 发布时间
    pub portable: Option<SoftwareAsset>, // 挑出来的便携版
    pub install_kind: String, // portable: 自动安装; installer: 官网安装包; store: Microsoft Store
    pub source_kind: String,  // github | official | store
    pub ocr_install: bool,    // installer 是否提供模拟点击安装
    pub silent_install_args: String, // 下载完成后直接执行的静默安装参数
    pub icon_path: String,    // 用户自定义图标路径
}

pub enum SoftwareSource {
    Github(String),
    DirectDownload {
        url: String,
        page_url: String,
        asset_match: String,
        version: String,
        file_name: String,
    },
    WegameOfficial,
    AmdAdrenalinOfficial,
    MicrosoftStore {
        product_id: String,
        package_name: String,
    },
}

pub struct SoftwareTarget {
    pub id: String,
    pub display_name: String,
    pub source: SoftwareSource,
    pub install_kind: String,
    pub ocr_install: bool,
    pub silent_install_args: String,
    pub icon_path: String,
}

pub fn source_kind_for(target: &SoftwareTarget) -> &'static str {
    match target.source {
        SoftwareSource::Github(_) => "github",
        SoftwareSource::DirectDownload { .. }
        | SoftwareSource::WegameOfficial
        | SoftwareSource::AmdAdrenalinOfficial => "official",
        SoftwareSource::MicrosoftStore { .. } => "store",
    }
}

pub fn software_list() -> Vec<SoftwareTarget> {
    let mut list = vec![
        SoftwareTarget {
            id: "stranslate".into(),
            display_name: "STranslate".into(),
            source: SoftwareSource::Github("STranslate/STranslate".into()),
            install_kind: "portable".into(),
            ocr_install: false,
            silent_install_args: String::new(),
            icon_path: String::new(),
        },
        SoftwareTarget {
            id: "quickclipboard".into(),
            display_name: "QuickClipboard".into(),
            source: SoftwareSource::Github("mosheng1/QuickClipboard".into()),
            install_kind: "portable".into(),
            ocr_install: false,
            silent_install_args: String::new(),
            icon_path: String::new(),
        },
        SoftwareTarget {
            id: "leagueakari".into(),
            display_name: "LeagueAkari".into(),
            source: SoftwareSource::Github("LeagueAkari/LeagueAkari".into()),
            install_kind: "portable".into(),
            ocr_install: false,
            silent_install_args: String::new(),
            icon_path: String::new(),
        },
        SoftwareTarget {
            id: "wegame".into(),
            display_name: "WeGame".into(),
            source: SoftwareSource::WegameOfficial,
            install_kind: "installer".into(),
            ocr_install: true,
            silent_install_args: String::new(),
            icon_path: String::new(),
        },
        SoftwareTarget {
            id: "amd-adrenalin".into(),
            display_name: "AMD Adrenalin Edition".into(),
            source: SoftwareSource::AmdAdrenalinOfficial,
            install_kind: "installer".into(),
            ocr_install: false,
            silent_install_args: String::new(),
            icon_path: String::new(),
        },
        SoftwareTarget {
            // Windows 开始菜单显示为 ChatGPT，但实际 AppX 包名是 OpenAI.Codex。
            id: "chatgpt".into(),
            display_name: "ChatGPT".into(),
            source: SoftwareSource::MicrosoftStore {
                product_id: "9PLM9XGG6VKS".into(),
                package_name: "OpenAI.Codex".into(),
            },
            install_kind: "store".into(),
            ocr_install: false,
            silent_install_args: String::new(),
            icon_path: String::new(),
        },
    ];
    list.extend(custom_software_targets());
    list
}

// 每个软件怎么挑便携版:返回 true 表示这是要下的那个文件
// 规则来自我们的验证记录
pub fn is_portable_target(id: &str, file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    match id {
        "stranslate" => lower == "stranslate-win-portable.zip",
        "quickclipboard" => lower.contains("portable") && lower.ends_with(".exe"),
        "leagueakari" => lower.ends_with("-win.7z"),
        _ => {
            let custom_list = load_custom_software();
            if let Some(config) = custom_list.iter().find(|c| c.id == id) {
                asset_matches(&lower, &config.asset_match)
            } else {
                false
            }
        }
    }
}

fn asset_matches(lower_file_name: &str, matcher: &str) -> bool {
    let tokens: Vec<String> = matcher
        .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';' || ch == '|')
        .map(|token| token.trim().to_lowercase())
        .filter(|token| !token.is_empty())
        .collect();
    !tokens.is_empty() && tokens.iter().all(|token| lower_file_name.contains(token))
}

/// 解压后的主程序文件名(避免误选 elevate.exe / Update.exe 等)
pub fn preferred_main_exe(id: &str) -> Option<String> {
    match id {
        "stranslate" => Some("STranslate.exe".into()),
        "leagueakari" => Some("LeagueAkari.exe".into()),
        "wegame" => Some("wegame.exe".into()),
        "amd-adrenalin" => Some("RadeonSoftware.exe".into()),
        _ => {
            let custom_list = load_custom_software();
            if let Some(config) = custom_list.iter().find(|c| c.id == id) {
                Some(config.exe_match.clone())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{software_list, source_kind_for, SoftwareSource};

    #[test]
    fn includes_chatgpt_as_a_microsoft_store_app() {
        let target = software_list()
            .into_iter()
            .find(|target| target.id == "chatgpt")
            .expect("ChatGPT target");

        assert_eq!(source_kind_for(&target), "store");
        assert_eq!(target.install_kind, "store");
        match target.source {
            SoftwareSource::MicrosoftStore {
                product_id,
                package_name,
            } => {
                assert_eq!(product_id, "9PLM9XGG6VKS");
                assert_eq!(package_name, "OpenAI.Codex");
            }
            _ => panic!("ChatGPT must use the Microsoft Store source"),
        }
    }
}
