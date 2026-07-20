mod config;
mod custom_software;
mod github;
mod installer;
mod ocr_install;
mod software;
mod visual_target;

use config::{
    get_app_install_paths_cmd, get_install_paths_cmd, get_package_cache_info_cmd,
    open_cached_package_cmd, reset_install_paths_cmd, set_install_paths_cmd,
};
use custom_software::{
    add_custom_software, clear_custom_software_icon, fetch_custom_software_icon,
    get_custom_software, remove_custom_software, save_custom_software_icon_from_clipboard,
};
use github::GithubRelease;
use installer::{
    cache_software_package, install_software, is_software_installed, uninstall_software,
};
use ocr_install::launch_wegame_installer_cmd;
use serde::{Deserialize, Serialize};
use software::{
    is_portable_target, software_list, source_kind_for, SoftwareAsset, SoftwareInfo,
    SoftwareSource, SoftwareTarget,
};
use visual_target::{
    close_target_window_cmd, delete_automation_template_cmd, get_active_automation_template_cmd,
    get_automation_steps_cmd, get_automation_templates_cmd, get_visual_chains_cmd,
    get_visual_rules_cmd, pick_screen_color_cmd, run_automation_step_cmd, run_visual_chain_cmd,
    run_visual_rule_cmd, run_visual_target_cmd, save_automation_steps_cmd,
    save_automation_template_cmd, save_visual_chains_cmd, save_visual_rules_cmd,
    set_active_automation_template_cmd,
};

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
                info.icon_path = target.icon_path.clone();
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
                    icon_path: target.icon_path.clone(),
                });
            }
        }
    }

    Ok(results)
}

async fn fetch_one_target(target: &SoftwareTarget) -> Result<SoftwareInfo, String> {
    match &target.source {
        SoftwareSource::Github(repo) => {
            fetch_one_github_release(&target.id, &target.display_name, repo, &target.install_kind)
                .await
        }
        SoftwareSource::DirectDownload {
            url,
            page_url,
            asset_match,
            version,
            file_name,
        } => {
            fetch_direct_download_release(target, url, page_url, asset_match, version, file_name)
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
        icon_path: String::new(),
    })
}

async fn fetch_direct_download_release(
    target: &SoftwareTarget,
    url: &str,
    page_url: &str,
    asset_match: &str,
    version: &str,
    configured_file_name: &str,
) -> Result<SoftwareInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent("software-manager")
        .build()
        .map_err(|e| e.to_string())?;
    let resolved = resolve_direct_download(
        &client,
        url,
        page_url,
        asset_match,
        version,
        configured_file_name,
    )
    .await?;
    let size =
        fetch_content_length_with_referer(&client, &resolved.download_url, &resolved.release_url)
            .await
            .unwrap_or(0);

    Ok(SoftwareInfo {
        id: target.id.clone(),
        display_name: target.display_name.clone(),
        latest_version: resolved.version,
        release_url: resolved.release_url,
        published_at: resolved.published_at,
        portable: Some(SoftwareAsset {
            name: resolved.file_name,
            browser_download_url: resolved.download_url,
            size,
        }),
        install_kind: target.install_kind.clone(),
        source_kind: source_kind_for(target).into(),
        ocr_install: false,
        icon_path: target.icon_path.clone(),
    })
}

struct ResolvedDirectDownload {
    download_url: String,
    release_url: String,
    version: String,
    published_at: String,
    file_name: String,
}

#[derive(Serialize, Clone)]
struct DownloadCandidate {
    url: String,
    file_name: String,
    version: String,
    size: u64,
    source_page: String,
    matcher: String,
    score: i32,
}

#[tauri::command]
async fn scan_download_candidates(url: String) -> Result<Vec<DownloadCandidate>, String> {
    let source_url = url.trim().to_string();
    let client = reqwest::Client::builder()
        .user_agent("software-manager")
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(repo) = parse_github_repo(&source_url) {
        let mut candidates = scan_github_release_candidates(&client, &repo, true).await?;
        candidates.truncate(20);
        return Ok(candidates);
    }
    let source_url = normalize_user_url(&source_url)?;
    let mut candidates = scan_download_candidates_internal(&client, &source_url, true).await?;
    candidates.truncate(20);
    Ok(candidates)
}

async fn scan_github_release_candidates(
    client: &reqwest::Client,
    repo: &str,
    include_size: bool,
) -> Result<Vec<DownloadCandidate>, String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("GitHub Release 查询失败: HTTP {}", resp.status()));
    }
    let release: GithubRelease = resp.json().await.map_err(|e| e.to_string())?;
    let mut candidates: Vec<DownloadCandidate> = release
        .assets
        .into_iter()
        .filter(|asset| looks_like_download_url(&asset.name))
        .map(|asset| DownloadCandidate {
            score: score_download_candidate(&asset.browser_download_url, &asset.name) + 50,
            matcher: derive_candidate_matcher(&asset.name),
            url: asset.browser_download_url,
            file_name: asset.name,
            version: release.tag_name.clone(),
            size: if include_size { asset.size } else { 0 },
            source_page: release.html_url.clone(),
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    Ok(candidates)
}

async fn resolve_direct_download(
    client: &reqwest::Client,
    url: &str,
    page_url: &str,
    asset_match: &str,
    version: &str,
    configured_file_name: &str,
) -> Result<ResolvedDirectDownload, String> {
    let source_url = url.trim();
    let configured_page = page_url.trim();
    let release_url = if configured_page.is_empty() {
        source_url.to_string()
    } else {
        configured_page.to_string()
    };

    if let Some(mut official) =
        resolve_official_download(client, source_url, configured_page).await?
    {
        if !version.trim().is_empty() {
            official.version = version.trim().to_string();
        }
        if !configured_file_name.trim().is_empty() {
            official.file_name = configured_file_name.trim().to_string();
        }
        return Ok(official);
    }

    if !looks_like_download_url(source_url) {
        if let Some(mut scanned) = resolve_scanned_download(client, source_url, asset_match).await?
        {
            if !version.trim().is_empty() {
                scanned.version = version.trim().to_string();
            }
            if !configured_file_name.trim().is_empty() {
                scanned.file_name = configured_file_name.trim().to_string();
            }
            return Ok(scanned);
        }
        return Err(
            "无法从官网页面自动识别下载链接；可点“扫描官网”选择候选项，或改填安装包直链".into(),
        );
    }

    let file_name = if configured_file_name.trim().is_empty() {
        file_name_from_url(source_url).ok_or("无法从下载地址解析文件名")?
    } else {
        configured_file_name.trim().to_string()
    };

    Ok(ResolvedDirectDownload {
        download_url: source_url.to_string(),
        release_url,
        version: if version.trim().is_empty() {
            "自定义下载".into()
        } else {
            version.trim().to_string()
        },
        published_at: String::new(),
        file_name,
    })
}

async fn resolve_scanned_download(
    client: &reqwest::Client,
    source_url: &str,
    asset_match: &str,
) -> Result<Option<ResolvedDirectDownload>, String> {
    let candidates = scan_download_candidates_internal(client, source_url, false).await?;
    let selected = candidates
        .iter()
        .find(|candidate| candidate_matches(candidate, asset_match))
        .or_else(|| candidates.first());

    Ok(selected.map(|candidate| ResolvedDirectDownload {
        download_url: candidate.url.clone(),
        release_url: candidate.source_page.clone(),
        version: candidate.version.clone(),
        published_at: String::new(),
        file_name: candidate.file_name.clone(),
    }))
}

async fn scan_download_candidates_internal(
    client: &reqwest::Client,
    source_url: &str,
    include_size: bool,
) -> Result<Vec<DownloadCandidate>, String> {
    if is_qq_official_source(source_url) {
        if let Some(resolved) = resolve_qq_download(client, source_url).await? {
            return Ok(vec![
                candidate_from_resolved(resolved, include_size, client).await,
            ]);
        }
    }
    if is_wechat_official_source(source_url) {
        if let Some(resolved) = resolve_wechat_download(client, source_url).await? {
            return Ok(vec![
                candidate_from_resolved(resolved, include_size, client).await,
            ]);
        }
    }
    if looks_like_download_url(source_url) {
        let file_name = file_name_from_url(source_url).ok_or("无法从下载地址解析文件名")?;
        let candidate = DownloadCandidate {
            url: source_url.to_string(),
            file_name: file_name.clone(),
            version: guess_version_from_file_name(&file_name)
                .or_else(|| version_from_source_url(source_url))
                .unwrap_or_else(|| "自定义下载".into()),
            size: if include_size {
                fetch_content_length_with_referer(client, source_url, source_url)
                    .await
                    .unwrap_or(0)
            } else {
                0
            },
            source_page: source_url.to_string(),
            matcher: derive_candidate_matcher(&file_name),
            score: score_download_candidate(source_url, &file_name),
        };
        return Ok(vec![candidate]);
    }

    let html = fetch_text(client, source_url).await?;
    let mut urls = Vec::new();
    collect_download_urls_from_text(&html, source_url, &mut urls);

    for script_url in extract_script_urls(&html, source_url).into_iter().take(12) {
        let js = match fetch_text(client, &script_url).await {
            Ok(text) => text,
            Err(_) => continue,
        };
        collect_download_urls_from_text(&js, &script_url, &mut urls);
    }

    urls.sort();
    urls.dedup();

    let mut candidates = Vec::new();
    for candidate_url in urls.into_iter().take(60) {
        let Some(file_name) = file_name_from_url(&candidate_url) else {
            continue;
        };
        let size = if include_size {
            fetch_content_length_with_referer(client, &candidate_url, source_url)
                .await
                .unwrap_or(0)
        } else {
            0
        };
        candidates.push(DownloadCandidate {
            score: score_download_candidate(&candidate_url, &file_name),
            version: guess_version_from_file_name(&file_name)
                .or_else(|| version_from_source_url(source_url))
                .unwrap_or_else(|| "最新版本".into()),
            matcher: derive_candidate_matcher(&file_name),
            url: candidate_url,
            file_name,
            size,
            source_page: source_url.to_string(),
        });
    }

    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    Ok(candidates)
}

async fn candidate_from_resolved(
    resolved: ResolvedDirectDownload,
    include_size: bool,
    client: &reqwest::Client,
) -> DownloadCandidate {
    let size = if include_size {
        fetch_content_length_with_referer(client, &resolved.download_url, &resolved.release_url)
            .await
            .unwrap_or(0)
    } else {
        0
    };
    DownloadCandidate {
        score: score_download_candidate(&resolved.download_url, &resolved.file_name) + 100,
        matcher: derive_candidate_matcher(&resolved.file_name),
        url: resolved.download_url,
        file_name: resolved.file_name,
        version: resolved.version,
        size,
        source_page: resolved.release_url,
    }
}

async fn resolve_official_download(
    client: &reqwest::Client,
    url: &str,
    page_url: &str,
) -> Result<Option<ResolvedDirectDownload>, String> {
    let mut candidates = vec![url.trim().to_string()];
    if !page_url.trim().is_empty() && page_url.trim() != url.trim() {
        candidates.push(page_url.trim().to_string());
    }

    for candidate in candidates {
        if is_qq_official_source(&candidate) {
            if let Some(resolved) = resolve_qq_download(client, &candidate).await? {
                return Ok(Some(resolved));
            }
        }
        if is_wechat_official_source(&candidate) {
            if let Some(resolved) = resolve_wechat_download(client, &candidate).await? {
                return Ok(Some(resolved));
            }
        }
    }

    Ok(None)
}

fn is_qq_official_source(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("im.qq.com")
        || lower.contains("qq-web/im.qq.com")
        || lower.contains("pcconfig.json")
}

fn is_wechat_official_source(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("pc.weixin.qq.com")
        || lower.contains("windows.weixin.qq.com")
        || lower.contains("dldir1v6.qq.com/weixin/")
}

async fn resolve_qq_download(
    client: &reqwest::Client,
    source_url: &str,
) -> Result<Option<ResolvedDirectDownload>, String> {
    if source_url.to_lowercase().contains("pcconfig.json") {
        return fetch_qq_pc_config(client, source_url, "https://im.qq.com/")
            .await
            .map(Some);
    }

    let entry_url = normalize_qq_entry_url(source_url);
    let html = fetch_text(client, &entry_url).await?;
    if let Some(config_url) = find_url_containing(&html, "pcConfig.json") {
        return fetch_qq_pc_config(client, &config_url, &entry_url)
            .await
            .map(Some);
    }

    for script_url in extract_script_urls(&html, &entry_url).into_iter().take(12) {
        let js = match fetch_text(client, &script_url).await {
            Ok(text) => text,
            Err(_) => continue,
        };
        if let Some(config_url) = find_url_containing(&js, "pcConfig.json") {
            return fetch_qq_pc_config(client, &config_url, &entry_url)
                .await
                .map(Some);
        }
        for lazy_url in extract_lazy_asset_urls(&js, &script_url, "download.js")
            .into_iter()
            .take(4)
        {
            let lazy_js = match fetch_text(client, &lazy_url).await {
                Ok(text) => text,
                Err(_) => continue,
            };
            if let Some(config_url) = find_url_containing(&lazy_js, "pcConfig.json") {
                return fetch_qq_pc_config(client, &config_url, &entry_url)
                    .await
                    .map(Some);
            }
        }
    }

    Ok(None)
}

async fn resolve_wechat_download(
    client: &reqwest::Client,
    source_url: &str,
) -> Result<Option<ResolvedDirectDownload>, String> {
    if looks_like_download_url(source_url) {
        let file_name = file_name_from_url(source_url).ok_or("无法从微信下载地址解析文件名")?;
        return Ok(Some(ResolvedDirectDownload {
            download_url: source_url.to_string(),
            release_url: "https://pc.weixin.qq.com/".into(),
            version: version_from_wechat_file(&file_name).unwrap_or_else(|| "微信最新版".into()),
            published_at: String::new(),
            file_name,
        }));
    }

    let entry_url = normalize_wechat_entry_url(source_url);
    let html = fetch_text(client, &entry_url).await?;
    let download_url = find_wechat_windows_download_url(&html, &entry_url)
        .ok_or("微信官网页面中没有找到 Windows 下载链接")?;
    let file_name = file_name_from_url(&download_url).ok_or("无法从微信下载地址解析文件名")?;
    let version = version_from_wechat_file(&file_name)
        .or_else(|| version_from_wechat_page(&html))
        .unwrap_or_else(|| "微信最新版".into());

    Ok(Some(ResolvedDirectDownload {
        download_url,
        release_url: entry_url,
        version,
        published_at: String::new(),
        file_name,
    }))
}

fn normalize_qq_entry_url(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower == "https://im.qq.com"
        || lower == "https://im.qq.com/"
        || lower == "http://im.qq.com"
        || lower == "http://im.qq.com/"
        || lower.starts_with("https://im.qq.com/index#")
        || lower.starts_with("https://im.qq.com/index/#")
    {
        "https://im.qq.com/index/".into()
    } else {
        url.to_string()
    }
}

fn normalize_wechat_entry_url(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower == "https://pc.weixin.qq.com"
        || lower == "https://pc.weixin.qq.com/"
        || lower == "http://pc.weixin.qq.com"
        || lower == "http://pc.weixin.qq.com/"
        || lower == "https://windows.weixin.qq.com"
        || lower == "https://windows.weixin.qq.com/"
    {
        "https://pc.weixin.qq.com/".into()
    } else {
        url.to_string()
    }
}

async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct QqPcConfig {
    #[serde(rename = "Windows")]
    windows: QqWindowsConfig,
}

#[derive(Deserialize)]
struct QqWindowsConfig {
    version: String,
    #[serde(default, rename = "updateDate")]
    update_date: String,
    #[serde(default, rename = "downloadUrl")]
    download_url: String,
    #[serde(default, rename = "ntDownloadUrl")]
    nt_download_url: String,
    #[serde(default, rename = "ntDownloadX64Url")]
    nt_download_x64_url: String,
    #[serde(default, rename = "ntDownloadARMUrl")]
    nt_download_arm_url: String,
}

async fn fetch_qq_pc_config(
    client: &reqwest::Client,
    config_url: &str,
    release_url: &str,
) -> Result<ResolvedDirectDownload, String> {
    let text = fetch_text(client, config_url).await?;
    let config: QqPcConfig = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let windows = config.windows;
    let download_url = if cfg!(target_arch = "aarch64") && !windows.nt_download_arm_url.is_empty() {
        windows.nt_download_arm_url
    } else if cfg!(target_arch = "x86") && !windows.nt_download_url.is_empty() {
        windows.nt_download_url
    } else if !windows.nt_download_x64_url.is_empty() {
        windows.nt_download_x64_url
    } else if !windows.nt_download_url.is_empty() {
        windows.nt_download_url
    } else {
        windows.download_url
    };

    if download_url.trim().is_empty() {
        return Err("QQ 官方配置中没有 Windows 下载链接".into());
    }

    let file_name = file_name_from_url(&download_url).ok_or("无法从 QQ 下载地址解析文件名")?;
    Ok(ResolvedDirectDownload {
        download_url,
        release_url: release_url.to_string(),
        version: if windows.version.trim().is_empty() {
            "QQ 最新版".into()
        } else {
            format!("v{}", windows.version.trim_start_matches('v'))
        },
        published_at: windows.update_date,
        file_name,
    })
}

fn extract_script_urls(html: &str, base_url: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = html;
    while let Some(script_index) = rest.find("<script") {
        let after_script = &rest[script_index..];
        let tag_end = after_script.find('>').unwrap_or(after_script.len());
        let tag = &after_script[..tag_end];
        if let Some(src) = extract_attr(tag, "src") {
            if let Some(url) = absolutize_url(&src, base_url) {
                urls.push(url);
            }
        }
        rest = &after_script[tag_end..];
    }
    urls
}

fn collect_download_urls_from_text(text: &str, base_url: &str, urls: &mut Vec<String>) {
    collect_attr_download_urls(text, base_url, "href", urls);
    collect_attr_download_urls(text, base_url, "src", urls);

    let normalized = text
        .replace("\\/", "/")
        .replace("&amp;", "&")
        .replace("&quot;", "\"");
    let mut rest = normalized.as_str();
    while let Some(index) = rest.find("http") {
        let after = &rest[index..];
        let end = after
            .find(|ch: char| {
                ch == '"'
                    || ch == '\''
                    || ch == '`'
                    || ch == '<'
                    || ch == ')'
                    || ch == ']'
                    || ch == '}'
                    || ch.is_whitespace()
            })
            .unwrap_or(after.len());
        let value = clean_candidate_url(&after[..end]);
        if let Some(url) = absolutize_url(&value, base_url) {
            push_download_url(urls, url);
        }
        rest = &after[end..];
    }
}

fn collect_attr_download_urls(text: &str, base_url: &str, attr: &str, urls: &mut Vec<String>) {
    let pattern = format!("{attr}=");
    let mut rest = text;
    while let Some(index) = rest.find(&pattern) {
        let after_attr = &rest[index + pattern.len()..];
        let tail = after_attr.trim_start();
        let Some(quote) = tail.chars().next() else {
            break;
        };
        if quote != '"' && quote != '\'' {
            rest = after_attr;
            continue;
        }
        let value = tail[1..].split(quote).next().unwrap_or("");
        if let Some(url) = absolutize_url(value, base_url) {
            push_download_url(urls, url);
        }
        rest = &tail[1..];
    }
}

fn push_download_url(urls: &mut Vec<String>, url: String) {
    let cleaned = clean_candidate_url(&url);
    if looks_like_download_url(&cleaned) && !urls.contains(&cleaned) {
        urls.push(cleaned);
    }
}

fn clean_candidate_url(url: &str) -> String {
    url.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches(['\\', ',', ';', '.', ')', ']'])
        .to_string()
}

fn find_wechat_windows_download_url(html: &str, base_url: &str) -> Option<String> {
    let normalized = html.replace("\\/", "/").replace("&amp;", "&");
    let mut best: Option<String> = None;
    let mut rest = normalized.as_str();
    while let Some(index) = rest.find("href=") {
        let after_href = &rest[index + 5..];
        let quote = after_href.chars().next()?;
        if quote != '"' && quote != '\'' {
            rest = after_href;
            continue;
        }
        let value = after_href[1..].split(quote).next().unwrap_or("");
        if let Some(url) = absolutize_url(value, base_url) {
            let lower = url.to_lowercase();
            if lower.ends_with(".exe") && lower.contains("/weixin/") {
                if lower.contains("wechatwin_") || lower.contains("wechatsetup") {
                    if lower.contains("wechatwin_") {
                        return Some(url);
                    }
                    best.get_or_insert(url);
                }
            }
        }
        rest = &after_href[1..];
    }
    best
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let pattern = format!("{name}=");
    let start = tag.find(&pattern)? + pattern.len();
    let tail = &tag[start..].trim_start();
    let quote = tail.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value = tail[1..].split(quote).next()?;
    Some(value.to_string())
}

fn absolutize_url(url: &str, base_url: &str) -> Option<String> {
    if url.starts_with("https://") || url.starts_with("http://") {
        return Some(url.to_string());
    }
    if url.starts_with("//") {
        let scheme = reqwest::Url::parse(base_url)
            .ok()
            .map(|base| base.scheme().to_string())
            .unwrap_or_else(|| "https".into());
        return Some(format!("{scheme}:{url}"));
    }
    if url.starts_with("assets/") {
        if let Some(prefix) = base_url.split("/assets/").next() {
            return Some(format!("{}/{}", prefix.trim_end_matches('/'), url));
        }
    }
    let base = reqwest::Url::parse(base_url).ok()?;
    base.join(url).ok().map(|u| u.to_string())
}

fn extract_lazy_asset_urls(js: &str, base_url: &str, needle: &str) -> Vec<String> {
    let normalized = js.replace("\\/", "/").replace("&quot;", "\"");
    let mut urls = Vec::new();
    let mut rest = normalized.as_str();
    while let Some(index) = rest.find(needle) {
        let before = &rest[..index];
        let quote_start = before
            .rfind('"')
            .or_else(|| before.rfind('\''))
            .map(|i| i + 1);
        let after = &rest[index..];
        let quote_end = after
            .find('"')
            .or_else(|| after.find('\''))
            .map(|i| index + i);

        if let (Some(start), Some(end)) = (quote_start, quote_end) {
            let value = &rest[start..end];
            if let Some(url) = absolutize_url(value, base_url) {
                if !urls.contains(&url) {
                    urls.push(url);
                }
            }
        }

        rest = &rest[index + needle.len()..];
    }
    urls
}

fn find_url_containing(text: &str, needle: &str) -> Option<String> {
    let normalized = text.replace("\\/", "/").replace("&amp;", "&");
    let mut rest = normalized.as_str();
    while let Some(index) = rest.find("http") {
        let after = &rest[index..];
        let end = after
            .find(|ch: char| {
                ch == '"' || ch == '\'' || ch == '`' || ch == '<' || ch == ')' || ch.is_whitespace()
            })
            .unwrap_or(after.len());
        let url = after[..end].trim_end_matches(['\\', ',', ';', ']']);
        if url.contains(needle) {
            return Some(url.to_string());
        }
        rest = &after[end..];
    }
    None
}

fn looks_like_download_url(url: &str) -> bool {
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

fn normalize_user_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("请先填写官网或下载地址".into());
    }
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("https://{trimmed}"))
    }
}

fn parse_github_repo(input: &str) -> Option<String> {
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

fn candidate_matches(candidate: &DownloadCandidate, matcher: &str) -> bool {
    let tokens: Vec<String> = matcher
        .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';' || ch == '|')
        .map(|token| token.trim().to_lowercase())
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return true;
    }
    let haystack = format!("{} {}", candidate.file_name, candidate.url).to_lowercase();
    tokens.iter().all(|token| haystack.contains(token))
}

fn score_download_candidate(url: &str, file_name: &str) -> i32 {
    let lower = format!("{} {}", url, file_name).to_lowercase();
    let mut score = 0;
    if lower.ends_with(".exe") || lower.ends_with(".msi") {
        score += 40;
    }
    if lower.contains("x64")
        || lower.contains("amd64")
        || lower.contains("win64")
        || lower.contains("64")
    {
        score += 18;
    }
    if lower.contains("windows") || lower.contains("win") || lower.contains("pc") {
        score += 12;
    }
    if lower.contains("setup") || lower.contains("install") {
        score += 8;
    }
    if lower.contains("portable") {
        score += 4;
    }
    if lower.contains("x86") || lower.contains("32") {
        score -= 8;
    }
    if lower.contains("arm") {
        score -= 6;
    }
    if lower.contains("beta")
        || lower.contains("alpha")
        || lower.contains("preview")
        || lower.contains("dev")
    {
        score -= 20;
    }
    score
}

fn derive_candidate_matcher(file_name: &str) -> String {
    let stem = file_name
        .split('?')
        .next()
        .unwrap_or(file_name)
        .rsplit_once('.')
        .map(|(left, _)| left)
        .unwrap_or(file_name);
    let lower = stem.to_lowercase();
    let mut tokens = Vec::new();
    for token in stem.split(|ch: char| !ch.is_ascii_alphanumeric()) {
        let token_lower = token.to_lowercase();
        if token_lower.is_empty()
            || token_lower
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == '.')
            || token_lower.len() == 1
        {
            continue;
        }
        if token_lower.contains('.') {
            continue;
        }
        if matches!(
            token_lower.as_str(),
            "setup" | "install" | "installer" | "release" | "latest" | "windows" | "win" | "pc"
        ) {
            continue;
        }
        if token_lower.len() >= 6 && token_lower.chars().all(|ch| ch.is_ascii_hexdigit()) {
            continue;
        }
        tokens.push(token_lower);
    }
    if lower.contains("x64") || lower.contains("amd64") || lower.contains("win64") {
        tokens.push("x64".into());
    }
    let ext = file_name.rsplit('.').next().unwrap_or("");
    if !ext.is_empty() && ext.len() <= 10 {
        tokens.push(ext.to_lowercase());
    }
    tokens.dedup();
    if tokens.is_empty() {
        file_name.to_string()
    } else {
        tokens.join(" ")
    }
}

fn guess_version_from_file_name(file_name: &str) -> Option<String> {
    let stem = file_name
        .rsplit_once('.')
        .map(|(left, _)| left)
        .unwrap_or(file_name);
    for part in stem.split(|ch: char| !(ch.is_ascii_digit() || ch == '.')) {
        let version = part.trim_matches('.');
        if version.contains('.') && version.chars().any(|ch| ch.is_ascii_digit()) {
            return Some(format!("v{}", version));
        }
    }
    None
}

fn version_from_source_url(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    for (key, value) in parsed.query_pairs() {
        if key.eq_ignore_ascii_case("version") {
            let normalized = value.trim().to_lowercase();
            return match normalized.as_str() {
                "stable" | "release" | "latest" => Some("稳定版".into()),
                "beta" => Some("测试版".into()),
                "dev" | "preview" | "insider" => Some("预览版".into()),
                other if !other.is_empty() => Some(other.to_string()),
                _ => None,
            };
        }
    }
    None
}

fn version_from_wechat_file(file_name: &str) -> Option<String> {
    let lower = file_name.to_lowercase();
    let marker = "wechatwin_";
    let start = lower.find(marker)? + marker.len();
    let version = file_name[start..]
        .trim_end_matches(".exe")
        .split(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .next()
        .unwrap_or("")
        .trim_matches('.');
    if version.is_empty() {
        None
    } else {
        Some(format!("v{}", version))
    }
}

fn version_from_wechat_page(html: &str) -> Option<String> {
    let marker = "下载 ";
    let after = html.split(marker).nth(1)?;
    let version = after
        .split(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .find(|part| part.chars().any(|ch| ch.is_ascii_digit()))?
        .trim_matches('.');
    if version.is_empty() {
        None
    } else {
        Some(format!("v{}", version))
    }
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
    let downloads: Vec<WegameDownload> =
        serde_json::from_str(&item.value).map_err(|e| e.to_string())?;
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
        icon_path: target.icon_path.clone(),
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
    let size =
        fetch_content_length_with_referer(&client, &release.download_url, &release.release_url)
            .await
            .unwrap_or(0);

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
        icon_path: target.icon_path.clone(),
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
        if !page.to_lowercase().contains("amd software")
            || !page.to_lowercase().contains("adrenalin")
        {
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
    Some((
        parts
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join("."),
        parts,
    ))
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
    if let Some(version) = stem.split('.').find(|part| {
        part.chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    }) {
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

    let code = unsafe { ShellExecuteW(0, verb.as_ptr(), file.as_ptr(), null(), dir.as_ptr(), 1) };
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
        .args(&[
            "-NoProfile",
            "-Command",
            &format!("Add-AppxPackage -Path '{}'", path),
        ])
        .output()
        .map_err(|e| format!("PowerShell error: {}", e))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("安装失败: {}", err));
    }

    // 2. Run winget settings (this requires admin rights, and might fail if we are not elevated. But the app tries its best)
    let out2 = std::process::Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-Command",
            "winget settings --enable BypassCertificatePinningForMicrosoftStore",
        ])
        .output()
        .map_err(|e| format!("Winget error: {}", e))?;

    if !out2.status.success() {
        let err = String::from_utf8_lossy(&out2.stderr);
        return Err(format!(
            "安装成功，但配置 BypassCertificatePinningForMicrosoftStore 失败: {}",
            err
        ));
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
            scan_download_candidates,
            add_custom_software,
            get_custom_software,
            remove_custom_software,
            fetch_custom_software_icon,
            save_custom_software_icon_from_clipboard,
            clear_custom_software_icon,
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
