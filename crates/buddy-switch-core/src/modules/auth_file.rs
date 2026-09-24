//! 官方认证文件 `workbuddy-desktop.info` 的路径与读写（四段 JSON）。
//!
//! 对照 server.py `auth_file_path` / `workbuddy_app_path` / `read_auth_file` /
//! `import_from_auth_file`。切换写入（build_account_obj / build_auth_obj /
//! write_account_to_auth_file）在阶段 2 随 switch.rs 落地。
//!
//! **region 化**：新增 `*_for(region, …)` 变体；旧 CN 签名保留为薄包装，内部转调
//! `Region::Cn`，保证 P0-1「CN 行为零变化」。安全红线 F：读取后若凭据域与目标
//! region 不符，必须拒绝使用并返回结构化 [`RegionMismatch`]，且不发起任何上游请求。

use serde_json::{json, Map, Value};
use std::path::PathBuf;

use crate::modules::account::get_str;
use crate::modules::config::{atomic_write, backup_dir, now_ms, utc_iso};
use crate::modules::region::{region_of, region_spec, Region};

pub use crate::modules::region::RegionMismatch;

/// CN 认证文件路径（与改造前完全一致；等价于候选列表第 0 项）。
pub fn auth_file_path() -> PathBuf {
    auth_file_path_for(Region::Cn)
}

/// region 认证文件主路径（候选列表第 0 项）。
pub fn auth_file_path_for(region: Region) -> PathBuf {
    auth_candidates_for(region)
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            crate::modules::config::home_dir().join(region_spec(region).auth_filename)
        })
}

/// region 认证文件探测候选（严格超集；第 0 项保证与既有主路径一致）。
///
/// CN 保留既有主路径为第 0 项（macOS `Library/Application Support/…`、
/// Windows `AppData/Local/…`、Linux `.local/share/…`），其余仅为「主路径不存在
/// 时的回退」，不改变既有命中结果。
pub fn auth_candidates_for(region: Region) -> Vec<PathBuf> {
    let home = crate::modules::config::home_dir();
    let filename = region_spec(region).auth_filename;

    #[cfg(target_os = "macos")]
    {
        vec![home.join(format!(
            "Library/Application Support/CodeBuddyExtension/Data/Public/auth/{filename}"
        ))]
    }
    #[cfg(target_os = "windows")]
    {
        vec![
            home.join(format!(
                "AppData/Local/CodeBuddyExtension/Data/Public/auth/{filename}"
            )),
            home.join(format!(
                "AppData/Roaming/CodeBuddyExtension/Data/Public/auth/{filename}"
            )),
        ]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        vec![
            home.join(format!(
                ".local/share/CodeBuddyExtension/Data/Public/auth/{filename}"
            )),
            home.join(format!(
                ".config/CodeBuddyExtension/Data/Public/auth/{filename}"
            )),
        ]
    }
}

/// WorkBuddy 应用路径（CN）。
pub fn workbuddy_app_path() -> PathBuf {
    workbuddy_app_path_for(Region::Cn)
}

/// 按 region 解析 WorkBuddy 应用路径。
///
/// macOS 走 app bundle 动态探测（`WorkBuddy.app` / `WorkBuddy AI.app`）；
/// Windows 走 exe 动态探测（CN `WorkBuddy.exe`、Global `WorkBuddyAI.exe`，后者
/// 安装目录/可执行文件为**无空格**实测形态）；其余平台返回默认路径。
pub fn workbuddy_app_path_for(region: Region) -> PathBuf {
    #[cfg(target_os = "macos")]
    return crate::modules::process::macos_workbuddy_app_path_for(region);

    #[cfg(target_os = "windows")]
    {
        // 探测顺序：运行进程 Path → 缓存 → 注册表 → 环境变量/盘符扫描。
        // 都找不到时返回 LOCALAPPDATA 默认路径，供启动失败文案写出尝试路径。
        if let Some(exe) = crate::modules::process::windows_workbuddy_exe_path_for(region) {
            return exe;
        }
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        // 国际版实测安装目录/可执行文件为无空格 `WorkBuddyAI`，此处默认路径对齐
        // 现实（该分支仅在动态探测全部落空时用于文案与 `.exists()` 兜底）。
        let (folder, exe) = match region {
            Region::Cn => ("WorkBuddy", "WorkBuddy.exe"),
            Region::Global => ("WorkBuddyAI", "WorkBuddyAI.exe"),
        };
        return std::path::Path::new(&local)
            .join("Programs")
            .join(folder)
            .join(exe);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = region;
        PathBuf::from("/usr/bin/workbuddy")
    }
}

/// 读取认证文件 JSON；不存在或解析失败返回 None。
pub fn read_auth_file() -> Option<Value> {
    read_auth_file_for(Region::Cn)
}

/// 按 region 读取认证文件 JSON。
///
/// 逐个候选探测：仅「文件不存在」才回退到下一候选；文件存在但不可解析视为该
/// 位置权威（返回 None），避免旧版残留文件静默盖过损坏的新文件。
pub fn read_auth_file_for(region: Region) -> Option<Value> {
    for path in auth_candidates_for(region) {
        match std::fs::read_to_string(&path) {
            Ok(text) => return serde_json::from_str(&text).ok(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return None,
        }
    }
    None
}

/// 从认证文档提取凭据域（顶层 `domain` 或 `auth.domain`）。
pub fn credential_domain(root: &Value) -> Option<String> {
    get_str(root, "domain").or_else(|| root.get("auth").and_then(|auth| get_str(auth, "domain")))
}

/// 校验凭据域与目标 region 一致；不一致返回结构化 [`RegionMismatch`]。
///
/// 安全红线 F 的核心：**只做判定，不发起任何上游请求**。
pub fn ensure_region_matches(region: Region, root: &Value) -> Result<(), RegionMismatch> {
    let domain = credential_domain(root).unwrap_or_default();
    let actual = region_of(&domain);
    if actual == region {
        Ok(())
    } else {
        Err(RegionMismatch::new(domain, actual, region))
    }
}

/// 读取并校验 region：不匹配即拒绝（返回 Err，且调用方不得发起上游请求）。
pub fn read_auth_file_checked_for(region: Region) -> Result<Option<Value>, RegionMismatch> {
    match read_auth_file_for(region) {
        Some(root) => {
            ensure_region_matches(region, &root)?;
            Ok(Some(root))
        }
        None => Ok(None),
    }
}

/// 切换前备份当前认证文件，返回备份路径。对照 server.py `backup_auth_file`。
pub fn backup_auth_file() -> Option<PathBuf> {
    backup_auth_file_for(Region::Cn)
}

/// 按 region 备份当前认证文件。
pub fn backup_auth_file_for(region: Region) -> Option<PathBuf> {
    let path = auth_file_path_for(region);
    if !path.exists() {
        return None;
    }
    let dir = backup_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let ts = utc_iso();
    let filename = region_spec(region).auth_filename;
    let stem = filename.strip_suffix(".info").unwrap_or(filename);
    let dest = dir.join(format!("{stem}.{ts}.info"));
    std::fs::copy(&path, &dest).ok()?;
    Some(dest)
}

/// 从账号库记录构造官方 account 字段。对照 server.py `build_account_obj`。
pub fn build_account_obj(acc: &Value) -> Value {
    let mut obj: Map<String, Value> = match acc.get("profile_raw") {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    obj.insert(
        "uid".to_string(),
        acc.get("uid").cloned().unwrap_or_else(|| json!("")),
    );
    obj.insert(
        "nickname".to_string(),
        acc.get("nickname").cloned().unwrap_or_else(|| json!("")),
    );
    setdefault(&mut obj, "type", json!("personal"));
    setdefault(&mut obj, "accountType", json!(""));
    setdefault(&mut obj, "idp", json!(""));
    setdefault(&mut obj, "oneidAccountId", json!(""));
    setdefault(&mut obj, "areaInfoComplete", json!(false));
    setdefault(&mut obj, "isCurrentOneIdEnterprise", json!(false));
    setdefault(&mut obj, "isCurrentOneIdPersonal", json!(false));
    setdefault(&mut obj, "isFirstLogin", json!(false));
    setdefault(&mut obj, "isCreator", json!(false));
    setdefault(&mut obj, "isAdmin", json!(false));
    setdefault(&mut obj, "uin", json!(""));
    setdefault(&mut obj, "phoneNumber", json!(""));
    setdefault(&mut obj, "lastLogin", json!(true));
    setdefault(&mut obj, "pluginEnabled", json!(true));
    setdefault(
        &mut obj,
        "deployStatus",
        json!({"statusCode": 0, "statusMsg": "", "detailMsg": ""}),
    );
    setdefault(
        &mut obj,
        "sso",
        json!({"domain": "", "domainModifiedTimes": 0}),
    );
    Value::Object(obj)
}

/// 从账号库记录构造官方 auth 字段。对照 server.py `build_auth_obj`。
pub fn build_auth_obj(acc: &Value) -> Value {
    let mut obj: Map<String, Value> = Map::new();
    let raw = acc.get("auth_raw");
    if let Some(Value::Object(m)) = raw {
        let inner = match m.get("auth") {
            Some(Value::Object(im)) => im.clone(),
            _ => m.clone(),
        };
        obj.extend(inner);
    }
    let token_type = acc
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();
    let expires_at = acc.get("expiresAt").and_then(|v| v.as_i64());
    let now = now_ms();

    obj.insert(
        "accessToken".to_string(),
        get_str(acc, "access_token").unwrap_or_default().into(),
    );
    obj.insert(
        "refreshToken".to_string(),
        get_str(acc, "refresh_token").unwrap_or_default().into(),
    );
    obj.insert("tokenType".to_string(), token_type.into());
    obj.insert(
        "domain".to_string(),
        get_str(acc, "domain").unwrap_or_default().into(),
    );
    obj.insert("lastRefreshTime".to_string(), json!(now));
    setdefault(
        &mut obj,
        "scope",
        json!("openid profile offline_access email"),
    );

    if let Some(expires_at) = expires_at {
        obj.insert("expiresAt".to_string(), json!(expires_at));
        obj.insert(
            "expiresIn".to_string(),
            json!(((expires_at - now) / 1000).max(0)),
        );
        let refresh_exp = raw
            .and_then(|r| r.get("refreshExpiresAt"))
            .and_then(|v| v.as_i64())
            .unwrap_or(expires_at);
        if !obj.contains_key("refreshExpiresAt") {
            obj.insert("refreshExpiresAt".to_string(), json!(refresh_exp));
        }
        obj.insert(
            "refreshExpiresIn".to_string(),
            json!(((refresh_exp - now) / 1000).max(0)),
        );
    } else {
        setdefault(&mut obj, "expiresIn", json!(0));
        setdefault(&mut obj, "refreshExpiresIn", json!(0));
    }
    setdefault(&mut obj, "notBeforePolicy", json!(0));
    setdefault(&mut obj, "sessionState", json!(""));
    Value::Object(obj)
}

/// 把账号写入官方认证文件（原子写 + 写后校验）。对照 server.py `write_account_to_auth_file`。
pub fn write_account_to_auth_file(acc: &Value) -> Result<(), String> {
    write_account_to_auth_file_for(Region::Cn, acc)
}

/// 按 region 把账号写入官方认证文件（含凭据域校验，安全红线 F）。
pub fn write_account_to_auth_file_for(region: Region, acc: &Value) -> Result<(), String> {
    // 安全红线 F：写入前校验凭据域归属，不匹配即拒绝（不写文件、不发上游请求）。
    let domain = get_str(acc, "domain").unwrap_or_default();
    let actual_region = region_of(&domain);
    if actual_region != region {
        return Err(RegionMismatch::new(domain, actual_region, region).message());
    }

    let path = auth_file_path_for(region);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let existing = read_auth_file_for(region).unwrap_or_else(|| json!({}));
    eprintln!(
        "[auth] write_account: existing is_object={} allAccounts_len={}",
        existing.is_object(),
        existing
            .get("allAccounts")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0)
    );
    let all_accounts = existing
        .get("allAccounts")
        .cloned()
        .or_else(|| existing.get("accounts").cloned())
        .filter(|v| v.is_array())
        .unwrap_or_else(|| json!([]));
    let account_obj = build_account_obj(acc);
    let auth_obj = build_auth_obj(acc);

    // 把目标账号并入 allAccounts（去重：按 uid 或 id）
    let target_uid = get_str(acc, "uid").unwrap_or_default();
    let mut all: Vec<Value> = all_accounts.as_array().cloned().unwrap_or_default();
    all.retain(|a| {
        let primary = a
            .get("uid")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                a.get("id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or("");
        primary != target_uid
    });
    all.push(account_obj.clone());
    eprintln!("[auth] write_account: merged allAccounts len={}", all.len());

    let session = json!({
        "account": &account_obj,
        "auth": &auth_obj,
        "accounts": &all,
        "allAccounts": &all,
    });
    let content = serde_json::to_string_pretty(&session).map_err(|e| e.to_string())?;
    if let Err(e) = atomic_write(&path, &content) {
        eprintln!("[auth] atomic_write FAILED: {e}");
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            return Err(super::error_code::AppError::new(
                super::error_code::ErrorCode::PermissionDenied,
                "无权限写入认证文件：请打开 系统设置→隐私与安全性→App 管理，允许本 App 控制 WorkBuddy 的数据（或为其开启『完全磁盘访问』后重试）",
            )
            .to_wire());
        }
        return Err(e.to_string());
    }

    // 写后校验
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let written_token = written
        .get("auth")
        .and_then(|a| a.get("accessToken"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let expect_token = get_str(acc, "access_token").unwrap_or_default();
    if written_token != expect_token {
        return Err("认证文件写后校验失败，未写入目标账号".to_string());
    }
    Ok(())
}

fn setdefault(map: &mut Map<String, Value>, key: &str, value: Value) {
    if !map.contains_key(key) {
        map.insert(key.to_string(), value);
    }
}

/// 从当前 WorkBuddy 登录态导入账号。对照 server.py `import_from_auth_file`。
pub fn import_from_auth_file() -> Option<Value> {
    import_from_auth_file_for(Region::Cn)
}

/// 按 region 从当前登录态导入账号。
pub fn import_from_auth_file_for(region: Region) -> Option<Value> {
    imported_account_from_root(read_auth_file_for(region)?)
}

fn imported_account_from_root(root: Value) -> Option<Value> {
    let account_obj = root
        .get("account")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let auth_obj = root
        .get("auth")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));

    let uid = get_str(&root, "uid").or_else(|| get_str(&account_obj, "uid"));
    let uid = uid.or_else(|| get_str(&account_obj, "id"));
    let nickname = get_str(&root, "nickname")
        .or_else(|| get_str(&root, "name"))
        .or_else(|| get_str(&account_obj, "nickname"))
        .or_else(|| get_str(&account_obj, "label"));
    let email = get_str(&root, "email")
        .or_else(|| get_str(&account_obj, "email"))
        .or_else(|| get_str(&auth_obj, "email"));
    let access_token = get_str(&auth_obj, "accessToken")
        .or_else(|| get_str(&auth_obj, "access_token"))
        .or_else(|| get_str(&root, "accessToken"))
        .or_else(|| get_str(&root, "access_token"));
    let refresh_token = get_str(&auth_obj, "refreshToken")
        .or_else(|| get_str(&auth_obj, "refresh_token"))
        .or_else(|| get_str(&root, "refreshToken"))
        .or_else(|| get_str(&root, "refresh_token"));
    let token_type = get_str(&auth_obj, "tokenType")
        .or_else(|| get_str(&auth_obj, "token_type"))
        .unwrap_or_else(|| "Bearer".to_string());
    let domain = get_str(&root, "domain").or_else(|| get_str(&auth_obj, "domain"));
    let expires_at = parse_ts(root.get("expiresAt").or_else(|| auth_obj.get("expiresAt")));
    let refresh_expires_at = parse_ts(
        root.get("refreshExpiresAt")
            .or_else(|| auth_obj.get("refreshExpiresAt")),
    );

    if access_token.is_none() {
        return None;
    }

    Some(json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "uid": uid,
        "nickname": nickname,
        "email": email,
        "enterpriseName": get_str(&root, "enterpriseName")
            .or_else(|| get_str(&root, "enterprise_name"))
            .or_else(|| get_str(&account_obj, "enterpriseName"))
            .or_else(|| get_str(&account_obj, "enterprise_name")),
        "enterpriseId": get_str(&root, "enterpriseId")
            .or_else(|| get_str(&root, "enterprise_id"))
            .or_else(|| get_str(&account_obj, "enterpriseId"))
            .or_else(|| get_str(&account_obj, "enterprise_id")),
        "access_token": access_token,
        "refresh_token": refresh_token,
        "token_type": token_type,
        "domain": domain,
        "expiresAt": expires_at,
        "refreshExpiresAt": refresh_expires_at,
        "auth_raw": root,
        "profile_raw": account_obj,
        "createdAt": now_ms(),
    }))
}

/// 字符串时间戳转 i64（数字原样保留，不做秒/毫秒换算）。
/// 对照 server.py `import_from_auth_file` 的 str→int 逻辑。
fn parse_ts(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok().map(|f| f as i64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn auth_file_path_is_expected_location() {
        let p = auth_file_path();
        let s = p.to_string_lossy();
        assert!(
            s.contains("CodeBuddyExtension"),
            "路径应包含 CodeBuddyExtension: {s}"
        );
        assert!(
            s.ends_with("workbuddy-desktop.info"),
            "文件名应为 workbuddy-desktop.info: {s}"
        );
    }

    /// 三个取值入口必须给出同一个路径。
    ///
    /// ## 为什么必须持 `env_lock()`
    ///
    /// 断言的两侧各自**独立**读取进程级 `BUDDY_SWITCH_HOME`（`auth_file_path`、
    /// `auth_file_path_for`、`auth_candidates_for` 都是无参/重读型路径函数）。
    /// lib 单测在同一进程里并行跑，若有别的用例（`HomeOverrideGuard` 系列）
    /// 在这几次读取之间换掉该变量，就会出现「左侧真实 home、右侧临时 home」
    /// 的**假失败**（2026-09-24 实测踩到：新增用例改 home 后本用例开始红）。
    /// 修法是让本用例与所有改 home 的用例互斥，而不是给断言加容错。
    #[test]
    fn cn_auth_file_path_is_first_candidate() {
        let _lock = crate::modules::config::env_lock();
        assert_eq!(auth_file_path(), auth_file_path_for(Region::Cn));
        assert_eq!(
            auth_file_path_for(Region::Cn),
            auth_candidates_for(Region::Cn)[0]
        );
    }

    #[test]
    fn global_auth_path_uses_ai_filename() {
        let p = auth_file_path_for(Region::Global);
        assert!(
            p.to_string_lossy().ends_with("workbuddy-desktop-ai.info"),
            "国际版文件名应为 workbuddy-desktop-ai.info: {p:?}"
        );
        // CN 与 Global 目录相同、文件名不同。
        assert_eq!(
            auth_file_path_for(Region::Cn).parent(),
            auth_file_path_for(Region::Global).parent()
        );
    }

    #[test]
    fn ensure_region_matches_rejects_cross_region_credential() {
        let global_root = json!({"auth": {"domain": "www.workbuddy.ai"}});
        let err = ensure_region_matches(Region::Cn, &global_root)
            .expect_err("国际版凭据不得用于 CN");
        assert_eq!(err.actual_domain, "www.workbuddy.ai");
        assert_eq!(err.actual_region, Region::Global);
        assert_eq!(err.expected_region, Region::Cn);
        assert_eq!(err.env_var, "WORKBUDDY_AUTH_FILE");
        assert_eq!(err.expected_file, "workbuddy-desktop.info");

        let cn_root = json!({"domain": "www.codebuddy.cn"});
        assert!(ensure_region_matches(Region::Cn, &cn_root).is_ok());
        assert!(ensure_region_matches(Region::Global, &cn_root).is_err());
    }

    #[test]
    fn import_from_auth_file_extracts_fields() {
        let root = json!({
            "account": {"uid": "u-1", "nickname": "小明", "email": "a@b.c"},
            "auth": {
                "accessToken": "AT-1",
                "refreshToken": "RT-1",
                "tokenType": "Bearer",
                "domain": "www.codebuddy.cn",
                "expiresAt": "1791912333558",
            },
            "domain": "www.codebuddy.cn",
        });
        // import_from_auth_file 从真实认证文件读取，此处直接测 parse_ts 与字段提取逻辑
        assert_eq!(parse_ts(root["auth"].get("expiresAt")), Some(1791912333558));
        assert_eq!(parse_ts(root["auth"].get("refreshToken")), None);
        assert_eq!(parse_ts(Some(&json!("1786728333"))), Some(1786728333));
    }

    #[test]
    fn import_without_email_does_not_synthesize_one() {
        let account = imported_account_from_root(json!({
            "account": {"uid": "u-1", "nickname": "同名用户"},
            "auth": {"accessToken": "test-token"}
        }))
        .expect("auth payload should import");

        assert_eq!(account["uid"], "u-1");
        assert_eq!(account["nickname"], "同名用户");
        assert!(account["email"].is_null());
    }
}
