// 软件清单配置:每个软件对应一个 GitHub 仓库
// 第一版只支持 GitHub 源,官网源留第二版
// 加新软件只需要往这个数组里加一条

use crate::custom_software::{custom_software_targets, load_custom_software};

#[derive(serde::Serialize, Clone)]
pub struct SoftwareAsset {
    pub name: String,           // 文件名
    pub browser_download_url: String, // 下载链接
    pub size: u64,              // 字节
}

#[derive(serde::Serialize, Clone)]
pub struct SoftwareInfo {
    pub id: String,             // STranslate / QuickClipboard / LeagueAkari
    pub display_name: String,   // 显示名
    pub latest_version: String, // v2.0.8
    pub release_url: String,    // GitHub Release 页面
    pub published_at: String,   // 发布时间
    pub portable: Option<SoftwareAsset>, // 挑出来的便携版
    pub install_kind: String,    // portable: 自动安装; installer: 只下载/缓存安装包
    pub source_kind: String,     // github | official
    pub ocr_install: bool,       // installer 是否提供模拟点击安装
}

pub enum SoftwareSource {
    Github(String),
    WegameOfficial,
    AmdAdrenalinOfficial,
}

pub struct SoftwareTarget {
    pub id: String,
    pub display_name: String,
    pub source: SoftwareSource,
    pub install_kind: String,
    pub ocr_install: bool,
}

pub fn source_kind_for(target: &SoftwareTarget) -> &'static str {
    match target.source {
        SoftwareSource::Github(_) => "github",
        SoftwareSource::WegameOfficial | SoftwareSource::AmdAdrenalinOfficial => "official",
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
        },
        SoftwareTarget {
            id: "quickclipboard".into(),
            display_name: "QuickClipboard".into(),
            source: SoftwareSource::Github("mosheng1/QuickClipboard".into()),
            install_kind: "portable".into(),
            ocr_install: false,
        },
        SoftwareTarget {
            id: "leagueakari".into(),
            display_name: "LeagueAkari".into(),
            source: SoftwareSource::Github("LeagueAkari/LeagueAkari".into()),
            install_kind: "portable".into(),
            ocr_install: false,
        },
        SoftwareTarget {
            id: "wegame".into(),
            display_name: "WeGame".into(),
            source: SoftwareSource::WegameOfficial,
            install_kind: "installer".into(),
            ocr_install: true,
        },
        SoftwareTarget {
            id: "amd-adrenalin".into(),
            display_name: "AMD Adrenalin Edition".into(),
            source: SoftwareSource::AmdAdrenalinOfficial,
            install_kind: "installer".into(),
            ocr_install: false,
        },
        SoftwareTarget {
            id: "winget".into(),
            display_name: "Winget CLI".into(),
            source: SoftwareSource::Github("microsoft/winget-cli".into()),
            install_kind: "installer".into(),
            ocr_install: false,
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
        "winget" => lower.ends_with(".msixbundle") && !lower.contains("dependencies"),
        _ => {
            let custom_list = load_custom_software();
            if let Some(config) = custom_list.iter().find(|c| c.id == id) {
                lower.contains(&config.asset_match.to_lowercase())
            } else {
                false
            }
        }
    }
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
