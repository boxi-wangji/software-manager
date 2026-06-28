use serde::Deserialize;

// GitHub API 返回的最小结构,只取我们要的字段
#[derive(Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub html_url: String,
    pub published_at: String,
    pub assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
pub struct GithubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}
