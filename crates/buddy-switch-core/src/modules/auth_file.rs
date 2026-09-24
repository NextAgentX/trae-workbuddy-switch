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

use crate::modules::account::{get_str, is_envelope, secret_value};
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

    // token 可能是明文字符串，也可能是 WorkBuddy 5.6 的加密信封：信封必须**原样写回**，
    // 由客户端用同一 keyblob 自行解密。降级成空串会静默毁掉客户端登录态。
    obj.insert(
        "accessToken".to_string(),
        secret_value(acc, "access_token").unwrap_or_else(|| json!("")),
    );
    obj.insert(
        "refreshToken".to_string(),
        secret_value(acc, "refresh_token").unwrap_or_else(|| json!("")),
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
    // ★ 按**值**比较，不能 `as_str()`。
    //
    // token 可能是明文，也可能是 WorkBuddy 5.6 的加密信封对象。用 `as_str()` 时信封
    // 被折成 `""`，而期望值那边 `get_str` 也只认字符串 ⇒ 同样折成 `""` ⇒ 两侧都空
    // ⇒ 校验**恒真**，等于给「token 已经被写坏」盖章放行（假阳性比没有校验更坏）。
    let written_token = written
        .get("auth")
        .and_then(|a| a.get("accessToken"))
        .cloned()
        .unwrap_or(Value::Null);
    let expect_token = acc.get("access_token").cloned().unwrap_or(Value::Null);
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
///
/// ★ **导入是解锁信封凭据的唯一收口**（`account::import_local_for` 与网关的
/// `account_strategy` 都走这里）：客户端 5.6 把 `accessToken` / `refreshToken` /
/// `nickname` 存成 at-rest 信封，不解开的话签到 / 积分 / Token 统计全都用不了。
///
/// 解锁**尽力而为**：本机没装客户端、客户端换了加密实现、取钥超时……都只打日志并
/// **保留信封**（退回改动前的行为），绝不让「导入」这个动作整体失败 —— 信封账号
/// 至少还能用于切换。
pub fn import_from_auth_file_for(region: Region) -> Option<Value> {
    let account = imported_account_from_root(read_auth_file_for(region)?)?;
    if !crate::modules::at_rest::is_unlockable(&account) {
        return Some(account);
    }
    match crate::modules::at_rest::unlock_account_for(region, &account) {
        Ok(Some(unlocked)) => Some(unlocked),
        // `None` = 读认证文件与解锁之间字段变了（并发切换），用原值即可。
        Ok(None) => Some(account),
        Err(error) => {
            eprintln!(
                "[at-rest] 信封凭据未解锁（{}）：{error}；保留信封，账号仍可用于切换",
                error.code()
            );
            Some(account)
        }
    }
}

/// 字段取值：**明文优先**（`get_str`：trim + 空串视为缺失，保持历史回落链），
/// 明文取不到时接受 WorkBuddy 5.6 的**加密信封**并原样返回。
///
/// 为什么不直接用 `account::secret_value` 替掉 `get_str`：`secret_value` 不 trim、
/// 也不把空串当缺失 ⇒ 会让「`nickname: ""` 且 `name: "X"`」这类历史数据**停在空昵称上**，
/// 把一条本来会继续往下回落的链子截断。这里保持「明文行为逐字不变、只新增信封这一档」。
fn plain_text_or_envelope(v: &Value, key: &str) -> Option<Value> {
    if let Some(text) = get_str(v, key) {
        return Some(Value::String(text));
    }
    if is_envelope(v, key) {
        return v.get(key).cloned();
    }
    None
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
    // WorkBuddy 5.6 起 `nickname` / `accessToken` / `refreshToken` 可能是
    // `{"$wbEncrypted":1,"envelope":"…"}` 加密信封：这里必须**原样保留**，不能强转字符串 ——
    // 否则 `accessToken` 取不到 ⇒ `access_token.is_none()` ⇒ 导入**恒失败**；
    // 即便绕过这一步，切换写回时也会把信封覆盖成空串（毁掉客户端登录态）。
    let nickname = plain_text_or_envelope(&root, "nickname")
        .or_else(|| plain_text_or_envelope(&root, "name"))
        .or_else(|| plain_text_or_envelope(&account_obj, "nickname"))
        .or_else(|| plain_text_or_envelope(&account_obj, "label"));
    let email = get_str(&root, "email")
        .or_else(|| get_str(&account_obj, "email"))
        .or_else(|| get_str(&auth_obj, "email"));
    let access_token = plain_text_or_envelope(&auth_obj, "accessToken")
        .or_else(|| plain_text_or_envelope(&auth_obj, "access_token"))
        .or_else(|| plain_text_or_envelope(&root, "accessToken"))
        .or_else(|| plain_text_or_envelope(&root, "access_token"));
    let refresh_token = plain_text_or_envelope(&auth_obj, "refreshToken")
        .or_else(|| plain_text_or_envelope(&auth_obj, "refresh_token"))
        .or_else(|| plain_text_or_envelope(&root, "refreshToken"))
        .or_else(|| plain_text_or_envelope(&root, "refresh_token"));
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

    // ── 加密信封（WorkBuddy 5.6）：导入与写回都必须**原样保留**凭据 ──────────

    /// ★ 信封 token 必须能被导入，否则「导入本机账号」在 5.6+ **恒失败**。
    ///
    /// 可证伪：把 `plain_text_or_envelope` 换回 `get_str`，本用例即红
    /// —— `access_token` 取不到 ⇒ `access_token.is_none()` ⇒ 函数返回 `None`。
    #[test]
    fn import_preserves_encrypted_envelope_credentials() {
        let envelope = json!({"$wbEncrypted": 1, "envelope": "eyJzdWl0ZSI6MSw="});
        let account = imported_account_from_root(json!({
            "account": {"uid": "u-1", "nickname": envelope.clone()},
            "auth": {
                "accessToken": envelope.clone(),
                "refreshToken": envelope.clone(),
                "tokenType": "Bearer",
                "domain": "www.codebuddy.cn",
            },
        }))
        .expect("信封凭据也必须能导入（否则 5.6+ 用户永远导不进账号）");

        assert_eq!(account["uid"], "u-1");
        // 凭据原样保留：我方解不开没关系，客户端用同一 keyblob 自己会解；丢掉才是错的。
        assert_eq!(account["access_token"], envelope);
        assert_eq!(account["refresh_token"], envelope);
        // 展示名同样原样保留（切换写回时由客户端解密，覆盖成空串等于把名字丢了）。
        assert_eq!(account["nickname"], envelope);
    }

    /// 对照：明文路径的历史回落链**逐字不变**。
    ///
    /// `root.nickname` 是空串时必须继续往下落到 `account.nickname`。
    /// 若把 `get_str` 整体换成不 trim、也不把空串当缺失的 `account::secret_value`，
    /// 这里会**停在一个空昵称上**（本次刻意不照抄参考实现的原因），故用本用例钉住。
    #[test]
    fn import_keeps_plaintext_fallback_chain_for_blank_nickname() {
        let account = imported_account_from_root(json!({
            "account": {"uid": "u-1", "nickname": "  小明  "},
            "auth": {"accessToken": "plain-token", "domain": "www.codebuddy.cn"},
            "nickname": "",
        }))
        .expect("plaintext payload should import");

        // 空串视为缺失 ⇒ 落到 `account.nickname`；明文仍走 trim。
        assert_eq!(account["nickname"], "小明");
        assert_eq!(account["access_token"], "plain-token");
    }

    /// ★ 切换写回：信封 token 必须**原样写回**，绝不能被抹成空串。
    ///
    /// 可证伪：把 `build_auth_obj` 换回 `get_str(..).unwrap_or_default()`，
    /// 前两条断言即红（写回的是 `""`，等于静默毁掉客户端登录态）。
    #[test]
    fn build_auth_obj_preserves_envelope_token_instead_of_blanking_it() {
        let envelope = json!({"$wbEncrypted": 1, "envelope": "blob"});
        let auth = build_auth_obj(&json!({
            "uid": "u-1",
            "access_token": envelope.clone(),
            "refresh_token": envelope.clone(),
            "token_type": "Bearer",
        }));
        assert_eq!(auth["accessToken"], envelope);
        assert_eq!(auth["refreshToken"], envelope);

        // 阳性对照：明文仍按明文写。
        let auth = build_auth_obj(&json!({"access_token": "AT", "refresh_token": "RT"}));
        assert_eq!(auth["accessToken"], "AT");
        assert_eq!(auth["refreshToken"], "RT");

        // 两者都没有 ⇒ 空串（历史行为；客户端会自行 refresh 重新加密）。
        let auth = build_auth_obj(&json!({"uid": "u-1"}));
        assert_eq!(auth["accessToken"], "");
    }

    /// ★ 端到端：把信封账号切进认证文件后，读回来**仍是信封**（不是空串）。
    ///
    /// 同时钉住「写后校验」那一处的**同源**问题：用 `as_str()` 比较时，写入侧与
    /// 期望侧的信封都会被折成 `""` ⇒ 两侧都空 ⇒ 校验**恒真** ⇒ 给「token 已被写坏」
    /// 盖章放行。这里直接断言文件里的值与账号里的值是**同一个 JSON 值**。
    #[test]
    fn write_back_keeps_envelope_token_in_auth_file() {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-envelope-writeback-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");
        let guard = crate::modules::config::HomeOverrideGuard::set(&dir);

        let envelope = json!({"$wbEncrypted": 1, "envelope": "blob"});
        let acc = json!({
            "id": "a-1",
            "uid": "u-1",
            "nickname": "小明",
            "domain": "www.codebuddy.cn",
            "access_token": envelope.clone(),
            "refresh_token": envelope.clone(),
        });

        write_account_to_auth_file_for(Region::Cn, &acc).expect("信封 token 也必须能写入");
        let written = read_auth_file_for(Region::Cn).expect("写回后必须能读回");
        assert_eq!(written["auth"]["accessToken"], envelope);
        assert_eq!(written["auth"]["refreshToken"], envelope);
        assert_eq!(written["auth"]["accessToken"], acc["access_token"]);

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ★ 真机用例（**默认忽略**，需本机装有客户端且当前登录态是信封）：
    /// 「从本机导入」必须**直接产出明文凭据**，而不是把信封原样存进账号库。
    ///
    /// ```bash
    /// cargo test -p buddy-switch-core --lib -- --ignored import_unlocks_envelope_credentials
    /// ```
    ///
    /// 这是用户看得见的那条链路的收口：导入之后签到 / 积分 / Token 统计应当立刻可用。
    /// 可证伪：把 `import_from_auth_file_for` 里的解锁那几行去掉 ⇒ `access_token`
    /// 变回对象，`as_str()` 为 `None`，第一条断言直接红。
    #[test]
    #[ignore = "需本机安装 WorkBuddy 客户端"]
    fn import_unlocks_envelope_credentials() {
        // ⚠️ 认证文件路径也是 `home_dir()` 派生的（见 `auth_candidates_for`）⇒
        // 隔离 home 之后就读不到真实文件了。所以先取真实内容，再把它**按生产路径**
        // 落到临时 home 里 —— 既确定，又不依赖用户当前登录态，也不写用户的文件。
        let raw_text = auth_candidates_for(Region::Cn)
            .into_iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .expect("应能读到本机认证文件");
        let raw: Value = serde_json::from_str(&raw_text).expect("认证文件应是 JSON");
        if !crate::modules::account::is_envelope(&raw["auth"], "accessToken") {
            panic!("本机当前登录态不是信封，无法验证这条链路");
        }

        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-import-unlock-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");
        let guard = crate::modules::config::HomeOverrideGuard::set(&dir);

        let seeded = auth_candidates_for(Region::Cn)
            .into_iter()
            .next()
            .expect("至少有一个候选路径");
        std::fs::create_dir_all(seeded.parent().expect("parent")).expect("create auth dir");
        std::fs::write(&seeded, &raw_text).expect("seed auth file");
        // 临时 home 里没有 exe 缓存 ⇒ 显式种进去（走注册表/盘符扫描是另一条链路）。
        let exe = crate::modules::at_rest::client_executable_for(Region::Cn);
        crate::modules::config::save_workbuddy_exe_cache_for(Region::Cn, &exe).expect("cache exe");

        // ★ 走**用户点的那条入口**（`import_local_for` = 导入 + 落盘），
        // 而不是只测中间函数：这样连「存进账号库的那份也是明文」一起钉住。
        crate::modules::account::import_local_for(Region::Cn).expect("导入应成功");

        let saved = crate::modules::account::load_accounts_for(Region::Cn);
        let saved = saved.first().expect("导入后账号库应有一条");
        let token = saved["access_token"]
            .as_str()
            .expect("导入后 access_token 应是明文字符串，而不是信封对象");
        assert!(token.starts_with("eyJ"), "应是 JWT：{}", &token[..token.len().min(16)]);
        assert!(
            saved["refresh_token"].as_str().is_some_and(|v| v.starts_with("eyJ")),
            "refresh_token 也应解锁：{}",
            saved["refresh_token"]
        );
        assert!(
            saved["nickname"].as_str().is_some_and(|n| !n.is_empty()),
            "nickname 也应解锁：{}",
            saved["nickname"]
        );

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
