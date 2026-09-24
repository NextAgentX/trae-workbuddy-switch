//! 自动更新：检查公开 GitHub Releases 版本 + 更新源配置。
//!
//! 对照 server.py `load_github_config` / `save_github_config` /
//! `compare_versions` / `update_check`。下载安装走 tauri-plugin-updater（整包更新）。
//!
//! 版本检查不走 GitHub API（避免 60 次/小时/IP 限流）：
//! 1. 主端点：release 资产的 updater manifest（下载不计 API 配额）；
//! 2. 兜底端点：`/releases/latest` 的 302 `Location` 头解析 tag；
//! 3. 成功结果进程级缓存 6 小时，缓存命中不发网络请求。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::modules::config::{
    atomic_write, http_request_raw, http_request_with_proxy, now_secs, store_dir,
};

/// 应用当前版本（来自 Cargo.toml package.version）。
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_OWNER: &str = "NextAgentX";
pub const GITHUB_REPO: &str = "trae-workbuddy-switch";

/// 成功结果缓存有效期（6 小时）。自动轮询（30 分钟）命中缓存，不发网络请求；
/// 设置页手动检查传 force=true 绕过缓存强制刷新。
const CACHE_TTL_SECS: i64 = 6 * 60 * 60;

/// 进程级内存缓存，只缓存 ok=true 的结果；失败不写缓存。
struct CachedCheck {
    checked_at: i64,
    value: Value,
}

static CACHE: Mutex<Option<CachedCheck>> = Mutex::new(None);

pub fn github_config_file() -> PathBuf {
    store_dir().join("github_config.json")
}

/// 读取更新源配置（兼容旧配置文件，但永不返回 token）。
///
/// 配置里的 `owner` / `repo` 原样采信，不做任何改写：本项目的升级源只由
/// [`GITHUB_OWNER`] / [`GITHUB_REPO`] 与用户显式配置决定，不会把用户配置的
/// 其他仓库坐标替换成本项目自己的坐标。
pub fn load_github_config() -> Value {
    let mut owner = GITHUB_OWNER.to_string();
    let mut repo = GITHUB_REPO.to_string();
    let mut proxy = String::new();
    let f = github_config_file();
    let mut should_normalize = false;
    if f.exists() {
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                if let Some(value) = v.get("owner").and_then(|v| v.as_str()) {
                    if !value.trim().is_empty() {
                        owner = value.to_string();
                    }
                }
                if let Some(value) = v.get("repo").and_then(|v| v.as_str()) {
                    if !value.trim().is_empty() {
                        repo = value.to_string();
                    }
                }
                if let Some(value) = v.get("proxy").and_then(|v| v.as_str()) {
                    proxy = value.trim().to_string();
                }
                should_normalize = v.get("token").is_some();
            }
        }
    }
    let normalized = json!({"owner": owner, "repo": repo, "proxy": proxy});
    if should_normalize {
        let _ = atomic_write(
            &f,
            &serde_json::to_string_pretty(&normalized).unwrap_or_default(),
        );
    }
    normalized
}

/// 保存更新源配置；公开仓库不需要也不保存 GitHub token。
pub fn save_github_config(cfg: &Value) -> std::io::Result<()> {
    let owner = cfg
        .get("owner")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(GITHUB_OWNER);
    let repo = cfg
        .get("repo")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(GITHUB_REPO);
    let proxy = cfg
        .get("proxy")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    let clean = json!({"owner": owner, "repo": repo, "proxy": proxy});
    std::fs::create_dir_all(store_dir())?;
    atomic_write(
        &github_config_file(),
        &serde_json::to_string_pretty(&clean).unwrap_or_default(),
    )
}

fn version_tuple(v: &str) -> Vec<i64> {
    v.trim_start_matches('v')
        .split('.')
        .filter_map(|x| x.parse::<i64>().ok())
        .collect()
}

/// 版本比较：a > b 返回 1，a < b 返回 -1，相等返回 0。
pub fn compare_versions(a: &str, b: &str) -> i64 {
    let ta = version_tuple(a);
    let tb = version_tuple(b);
    for i in 0..ta.len().max(tb.len()) {
        let x = ta.get(i).copied().unwrap_or(0);
        let y = tb.get(i).copied().unwrap_or(0);
        if x != y {
            return if x > y { 1 } else { -1 };
        }
    }
    0
}

/// updater manifest 候选 URL（按优先级）。
///
/// 1. 合并后的 `latest.json`（含各平台）；
/// 2. 当前系统的 `latest-<os>-<arch>.json`；
/// 3. 兼容旧 Windows 安装包：它们仍请求 `latest-macos-<arch>.json`。
pub fn updater_manifest_urls(owner: &str, repo: &str, os: &str, arch: &str) -> Vec<String> {
    let os_slug = match os {
        "macos" | "darwin" => "macos",
        other => other,
    };
    let mut urls = vec![format!(
        "https://github.com/{owner}/{repo}/releases/latest/download/latest.json"
    )];
    urls.push(format!(
        "https://github.com/{owner}/{repo}/releases/latest/download/latest-{os_slug}-{arch}.json"
    ));
    if os_slug != "macos" {
        urls.push(format!(
            "https://github.com/{owner}/{repo}/releases/latest/download/latest-macos-{arch}.json"
        ));
    }
    urls.dedup();
    urls
}

/// 主端点：拉取 updater manifest。成功返回解析后的 JSON（含 version / pub_date），
/// 失败返回可读错误信息。
async fn fetch_manifest_version(
    owner: &str,
    repo: &str,
    proxy: Option<&str>,
) -> Result<Value, String> {
    let mut headers = HashMap::new();
    headers.insert("Accept".to_string(), "application/json".to_string());
    headers.insert("User-Agent".to_string(), "buddy-switch".to_string());
    let mut last_err = "更新清单解析失败".to_string();
    for url in updater_manifest_urls(owner, repo, std::env::consts::OS, std::env::consts::ARCH) {
        let resp = http_request_with_proxy(&url, "GET", None, Some(&headers), proxy).await;
        let version = resp.get("version").and_then(|v| v.as_str()).unwrap_or("");
        if !version.trim().is_empty() {
            return Ok(resp);
        }
        last_err = resp
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("更新清单解析失败")
            .to_string();
    }
    Err(last_err)
}

/// 兜底端点：请求 `/releases/latest`，读 302 `Location` 头（形如
/// `.../releases/tag/v0.1.13`）解析 tag。不跟随重定向，避免拉到 HTML 页面。
/// 成功返回 tag，失败返回可读错误 + code（状态码或 -1）。
async fn fetch_latest_tag(
    owner: &str,
    repo: &str,
    proxy: Option<&str>,
) -> Result<String, (String, i64)> {
    let url = format!("https://github.com/{owner}/{repo}/releases/latest");
    let mut headers = HashMap::new();
    headers.insert("Accept".to_string(), "text/html".to_string());
    headers.insert("User-Agent".to_string(), "buddy-switch".to_string());
    let (status, resp_headers, body) =
        http_request_raw(&url, "GET", None, Some(&headers), proxy, false).await;

    if status == 0 {
        let msg = if body.trim().is_empty() {
            "网络请求失败".to_string()
        } else {
            body
        };
        return Err((msg, -1));
    }
    if status == 404 {
        // 无正式 release 或仓库不存在。
        return Err(("未找到可用的发布版本".to_string(), 404));
    }
    let location = resp_headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("location"))
        .map(|(_, v)| v.clone());
    let location = match location {
        Some(location) => location,
        None => {
            return Err((
                format!("无法获取发布页跳转地址（HTTP {status}）"),
                status as i64,
            ))
        }
    };
    let tag = location.rsplit('/').next().unwrap_or("").trim().to_string();
    if tag.is_empty() || !location.contains("/releases/tag/") {
        return Err(("无法解析发布版本标签".to_string(), -1));
    }
    Ok(tag)
}

/// 查询最新 Release，与本地版本对比。对照 server.py `update_check`。
///
/// `force=true` 绕过缓存强制刷新（设置页手动检查）；否则 6 小时内成功结果直接返回，
/// 不发网络请求。主端点 manifest 失败时自动走 302 兜底；两个端点都失败返回可读错误。
pub async fn update_check(proxy: Option<&str>, force: bool) -> Value {
    if !force {
        if let Some(cached) = CACHE.lock().unwrap().as_ref() {
            if now_secs() - cached.checked_at < CACHE_TTL_SECS {
                return cached.value.clone();
            }
        }
    }

    let cfg = load_github_config();
    let owner = cfg
        .get("owner")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let repo = cfg
        .get("repo")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let configured_proxy = cfg
        .get("proxy")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let proxy = proxy.or(configured_proxy);
    let release_url = format!("https://github.com/{owner}/{repo}/releases/latest");
    let current = APP_VERSION.to_string();

    // 主端点：updater manifest（release 资产下载，不计 GitHub API 配额）。
    if let Ok(manifest) = fetch_manifest_version(&owner, &repo, proxy).await {
        let version = manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let latest = version.strip_prefix('v').unwrap_or(version).to_string();
        let tag = format!("v{latest}");
        let release_name = manifest
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&tag)
            .to_string();
        let published_at = manifest
            .get("pub_date")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let value = json!({
            "ok": true,
            "current": current,
            "latest": latest,
            "latestTag": tag,
            "hasUpdate": compare_versions(&latest, &current) > 0,
            "releaseName": release_name,
            "releaseUrl": release_url,
            "publishedAt": published_at,
            "checkedAt": now_secs(),
        });
        *CACHE.lock().unwrap() = Some(CachedCheck {
            checked_at: now_secs(),
            value: value.clone(),
        });
        return value;
    }

    // 兜底端点：`/releases/latest` 的 302 Location 头解析 tag（仅 manifest 失败时）。
    match fetch_latest_tag(&owner, &repo, proxy).await {
        Ok(tag) => {
            let latest = tag.strip_prefix('v').unwrap_or(&tag).to_string();
            let value = json!({
                "ok": true,
                "current": current,
                "latest": latest,
                "latestTag": tag,
                "hasUpdate": compare_versions(&latest, &current) > 0,
                "releaseName": tag.clone(),
                "releaseUrl": release_url,
                "checkedAt": now_secs(),
            });
            *CACHE.lock().unwrap() = Some(CachedCheck {
                checked_at: now_secs(),
                value: value.clone(),
            });
            value
        }
        Err((msg, code)) => json!({
            "ok": false,
            "error": msg,
            "message": format!("{msg}（code={code}）"),
            "releaseUrl": release_url,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 配置文件里的仓库坐标必须**原样采信**：本项目与其它项目各自独立，
    /// 绝不允许把用户配置的仓库坐标改写成本项目自己的坐标。
    ///
    /// 这条改写曾以 `migrate_legacy_coordinates` 的形式存在（按 owner 一刀切，
    /// 且因配置优先级高于常量而**落盘覆盖**用户原配置、不可逆），已于 2026-09-23
    /// 整体删除。此测试钉住「不再改写」，防止它以任何形式回归。
    ///
    /// 用隔离的临时 home，绝不触碰真实 `~/.buddy-switch`。**不要**在此再手动
    /// `env_lock()`：`HomeOverrideGuard::set()` 内部已取锁并持到 drop，重复取用会自死锁。
    #[test]
    fn load_github_config_keeps_configured_coordinates_verbatim() {
        let home = std::env::temp_dir().join(format!(
            "buddy-switch-update-cfg-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&home).expect("create isolated home");
        let _guard = crate::modules::config::HomeOverrideGuard::set(&home);

        let file = github_config_file();
        std::fs::create_dir_all(file.parent().expect("config file has parent"))
            .expect("create store dir");

        // 1) 任意第三方坐标必须原样返回，且文件**不被改写**。
        let foreign =
            r#"{"owner":"someone-else","repo":"their-repo","proxy":"http://127.0.0.1:7890"}"#;
        std::fs::write(&file, foreign).expect("seed config");
        let cfg = load_github_config();
        assert_eq!(
            cfg.get("owner").and_then(|v| v.as_str()),
            Some("someone-else")
        );
        assert_eq!(cfg.get("repo").and_then(|v| v.as_str()), Some("their-repo"));
        assert_eq!(
            cfg.get("proxy").and_then(|v| v.as_str()),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(
            std::fs::read_to_string(&file).expect("read back"),
            foreign,
            "坐标原样采信时不应产生任何落盘改写"
        );

        // 2) 归一化路径（旧配置含 token → 剥离）同样不得顺手改写坐标。
        std::fs::write(
            &file,
            r#"{"owner":"someone-else","repo":"their-repo","proxy":"","token":"ghp_secret"}"#,
        )
        .expect("seed legacy config");
        let cfg = load_github_config();
        assert_eq!(
            cfg.get("owner").and_then(|v| v.as_str()),
            Some("someone-else")
        );
        assert_eq!(cfg.get("repo").and_then(|v| v.as_str()), Some("their-repo"));
        assert!(cfg.get("token").is_none(), "token 不得被返回");
        let rewritten = std::fs::read_to_string(&file).expect("read back normalized");
        assert!(!rewritten.contains("ghp_secret"), "token 必须被剥离");
        assert!(rewritten.contains("someone-else"), "归一化不得改写坐标");

        // 3) 无配置文件时回落到本项目自己的坐标。
        std::fs::remove_file(&file).expect("remove config");
        let cfg = load_github_config();
        assert_eq!(cfg.get("owner").and_then(|v| v.as_str()), Some(GITHUB_OWNER));
        assert_eq!(cfg.get("repo").and_then(|v| v.as_str()), Some(GITHUB_REPO));

        std::fs::remove_dir_all(&home).expect("cleanup isolated home");
    }

    #[test]
    fn updater_manifest_urls_macos_skips_duplicate_fallback() {
        let urls = updater_manifest_urls(GITHUB_OWNER, GITHUB_REPO, "macos", "aarch64");
        assert_eq!(
            urls,
            vec![
                "https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest/download/latest.json",
                "https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest/download/latest-macos-aarch64.json",
            ]
        );
    }

    #[test]
    fn updater_manifest_urls_windows_keeps_macos_compat() {
        let urls = updater_manifest_urls(GITHUB_OWNER, GITHUB_REPO, "windows", "x86_64");
        assert_eq!(
            urls,
            vec![
                "https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest/download/latest.json",
                "https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest/download/latest-windows-x86_64.json",
                "https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest/download/latest-macos-x86_64.json",
            ]
        );
    }

    #[test]
    fn compare_versions_orders_semver() {
        assert_eq!(compare_versions("0.1.18", "0.1.18"), 0);
        assert_eq!(compare_versions("0.1.19", "0.1.18"), 1);
        assert_eq!(compare_versions("v0.1.17", "0.1.18"), -1);
    }

    /// 打包版本号形态 `<YYYY>.<M>.<DHHMM>` 必须**严格单调递增**。
    ///
    /// 这是自动更新的正确性前提：若同日两次打包产生相等的版本号，
    /// 客户端会判定「已是最新」，当天推的修复永远收不到。
    /// 版本由 `scripts/stamp-version.sh` 生成，格式说明见该脚本头部。
    #[test]
    fn compare_versions_orders_build_stamped_versions() {
        // 同日分钟级递增
        assert_eq!(compare_versions("2026.9.181216", "2026.9.181215"), 1);
        assert_eq!(compare_versions("2026.9.181215", "2026.9.181216"), -1);
        // 跨日（含月末 → 月初，日从 30 跳到 1）
        assert_eq!(compare_versions("2026.9.190000", "2026.9.182359"), 1);
        assert_eq!(compare_versions("2026.10.010000", "2026.9.302359"), 1);
        // 跨月（9 → 10）与跨年（2026 → 2027）
        assert_eq!(compare_versions("2026.10.010909", "2026.9.300909"), 1);
        assert_eq!(compare_versions("2027.1.010000", "2026.12.312359"), 1);
        // 日的定长补零不破坏跨日顺序（9 日 → 10 日）
        assert_eq!(compare_versions("2026.9.100909", "2026.9.090909"), 1);
        // 与历史 3 段产物（`<YYYY>.<M>.<D>`）的过渡关系。
        //
        // 新格式第三段是 `DHHMM`（6 位数，形如 `180000`），旧格式第三段是 `D`
        // （1~2 位数，形如 `19`），所以**同一月内**新格式恒大于旧格式：
        assert_eq!(compare_versions("2026.9.180000", "2026.9.18"), 1);
        assert_eq!(compare_versions("2026.9.18", "2026.9.180000"), -1);
        assert_eq!(compare_versions("2026.9.180909", "2026.9.18"), 1);
        // 新版本大于上一轮历史产物
        assert_eq!(compare_versions("2026.9.181216", "2026.9.17"), 1);
        //
        // **已知边界（刻意保留，不要"修"）**：第二段是「月」，比较优先级高于第三段。
        // 因此跨月比较时，旧格式里月更大的版本会胜出：
        assert_eq!(compare_versions("2026.9.181216", "2026.12.31"), -1);
        // 这只发生在「旧格式的未来月份 vs 新格式的较早月份」这种真实不会出现的组合上：
        // 一旦开始用新格式，人就不会再产出旧格式；而历史上旧格式的月份必然 ≤ 当前月。
        // 反过来「旧格式的过去月份 vs 新格式的当前月」方向是对的：
        assert_eq!(compare_versions("2026.9.181216", "2026.8.20"), 1);
    }
}
