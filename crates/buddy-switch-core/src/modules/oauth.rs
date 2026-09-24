//! OAuth 扫码登录采集（复刻 cockpit 流程）。
//!
//! 对照 server.py `oauth_start` / `oauth_poll`。
//!
//! **region 化**：新增 `oauth_start_for` / `oauth_poll_for`；旧 CN 签名保留为薄包装。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::modules::account;
use crate::modules::config::{
    http_request, norm_ts, now_ms, now_secs, OAUTH_TIMEOUT_SECONDS, WORKBUDDY_API_PREFIX,
};
use crate::modules::region::{region_spec, Region};

#[derive(Default)]
struct OAuthInfo {
    region: Region,
    state: String,
    expires_at: i64,
    done: bool,
    result: Option<Value>,
    error: Option<String>,
}

static OAUTH_STATES: OnceLock<Mutex<HashMap<String, OAuthInfo>>> = OnceLock::new();

fn oauth_states() -> &'static Mutex<HashMap<String, OAuthInfo>> {
    OAUTH_STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 发起 CN 登录。
pub async fn oauth_start() -> Result<Value, String> {
    oauth_start_for(Region::Cn).await
}

/// 发起指定 region 的登录：向官方申请 state，返回 loginId / verificationUri / expiresIn。
pub async fn oauth_start_for(region: Region) -> Result<Value, String> {
    let spec = region_spec(region);
    let base = spec.billing_base;
    let platform = spec.platform;
    let login_id = format!("wb_{}", uuid::Uuid::new_v4().simple());
    let url = format!("{base}{WORKBUDDY_API_PREFIX}/auth/state?platform={platform}");
    let resp = http_request(&url, "POST", Some(json!({})), None).await;
    let data = resp.get("data").cloned().unwrap_or_else(|| json!({}));
    let state = data
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if state.is_empty() {
        let snippet = serde_json::to_string(&resp)
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect::<String>();
        return Err(format!("auth/state 响应缺少 state: {snippet}"));
    }
    let auth_url = data
        .get("authUrl")
        .and_then(|v| v.as_str())
        .or_else(|| data.get("auth_url").and_then(|v| v.as_str()))
        .or_else(|| data.get("url").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{base}/login?state={state}"));

    let mut map = oauth_states().lock().unwrap();
    map.insert(
        login_id.clone(),
        OAuthInfo {
            region,
            state,
            expires_at: now_secs() + OAUTH_TIMEOUT_SECONDS,
            ..Default::default()
        },
    );
    drop(map);

    Ok(json!({
        "loginId": login_id,
        "verificationUri": auth_url,
        "expiresIn": OAUTH_TIMEOUT_SECONDS,
    }))
}

/// 轮询一次 CN 官方 token 接口。
pub async fn oauth_poll(login_id: &str) -> Value {
    oauth_poll_for(Region::Cn, login_id).await
}

/// 轮询一次指定 region 的官方 token 接口。成功则拉取账号信息并入库。
pub async fn oauth_poll_for(region: Region, login_id: &str) -> Value {
    let base = region_spec(region).billing_base;
    let state = {
        let mut map = oauth_states().lock().unwrap();
        let Some(info) = map.get_mut(login_id) else {
            return json!({"done": true, "error": "登录请求不存在"});
        };
        // 登录会话在发起时绑定 region；用其他 region 轮询同一 loginId 属调用方错误，
        // 直接拒绝，避免把 CN 的登录态写进国际版账号库（反之亦然）。
        if info.region != region {
            return json!({"done": true, "error": "登录请求与目标版本不匹配"});
        }
        if info.done {
            return json!({"done": true, "result": info.result.clone(), "error": info.error.clone()});
        }
        if now_secs() > info.expires_at {
            info.done = true;
            info.error = Some("登录超时".to_string());
            return json!({"done": true, "error": "登录超时"});
        }
        info.state.clone()
    };

    let url = format!("{base}{WORKBUDDY_API_PREFIX}/auth/token?state={state}");
    let resp = http_request(&url, "GET", None, None).await;
    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 && code != 200 {
        return json!({"done": false});
    }
    let data = resp.get("data").cloned().unwrap_or_else(|| json!({}));
    let access_token = data
        .get("accessToken")
        .and_then(|v| v.as_str())
        .or_else(|| data.get("access_token").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    if access_token.is_empty() {
        return json!({"done": false});
    }

    // 拉取账号信息
    let account_url = format!("{base}{WORKBUDDY_API_PREFIX}/login/account?state={state}");
    let mut headers = HashMap::new();
    headers.insert(
        "Authorization".to_string(),
        format!("Bearer {access_token}"),
    );
    let domain = data.get("domain").and_then(|v| v.as_str()).unwrap_or("");
    if !domain.is_empty() {
        headers.insert("X-Domain".to_string(), domain.to_string());
    }
    let acc_resp = http_request(&account_url, "GET", None, Some(&headers)).await;
    let acc_data = acc_resp.get("data").cloned().unwrap_or_else(|| json!({}));

    let uid = acc_data
        .get("uid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let nickname = acc_data
        .get("nickname")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let email = oauth_profile_email(&acc_data);

    let expires_at = norm_ts(data.get("expiresAt").or_else(|| data.get("expires_at")));
    let expires_at = match expires_at {
        Some(v) => Some(v),
        None => data
            .get("expiresIn")
            .and_then(|v| v.as_i64())
            .map(|e| now_ms() + e * 1000),
    };
    let refresh_expires_at = norm_ts(
        data.get("refreshExpiresAt")
            .or_else(|| data.get("refresh_expires_at")),
    );
    let refresh_expires_at = match refresh_expires_at {
        Some(v) => Some(v),
        None => data
            .get("refreshExpiresIn")
            .and_then(|v| v.as_i64())
            .map(|e| now_ms() + e * 1000),
    };

    let account = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "uid": uid,
        "nickname": nickname,
        "email": email,
        "enterpriseName": acc_data.get("enterpriseName"),
        "enterpriseId": acc_data.get("enterpriseId"),
        "access_token": access_token,
        "refresh_token": data.get("refreshToken").and_then(|v| v.as_str())
            .or_else(|| data.get("refresh_token").and_then(|v| v.as_str()))
            .map(|s| s.to_string()),
        "token_type": data.get("tokenType").and_then(|v| v.as_str())
            .or_else(|| data.get("token_type").and_then(|v| v.as_str()))
            .unwrap_or("Bearer")
            .to_string(),
        "domain": domain.to_string(),
        "expiresAt": expires_at,
        "refreshExpiresAt": refresh_expires_at,
        "auth_raw": data,
        "profile_raw": acc_data,
        "createdAt": now_ms(),
    });

    let account = match account::save_collected_account_for(region, account) {
        Ok(saved) => saved,
        Err(error) => {
            let error = format!("保存账号失败: {error}");
            let mut map = oauth_states().lock().unwrap();
            if let Some(info) = map.get_mut(login_id) {
                info.done = true;
                info.error = Some(error.clone());
            }
            return json!({"done": true, "error": error});
        }
    };

    let result = account::account_meta_for(region, &account);
    let mut map = oauth_states().lock().unwrap();
    if let Some(info) = map.get_mut(login_id) {
        info.done = true;
        info.result = Some(result.clone());
    }
    drop(map);

    json!({"done": true, "result": result})
}

fn oauth_profile_email(profile: &Value) -> Option<String> {
    profile
        .get("email")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_profile_without_email_does_not_use_nickname_or_uid() {
        let profile = json!({"uid": "u-1", "nickname": "同名用户"});
        assert_eq!(oauth_profile_email(&profile), None);
    }

    #[test]
    fn oauth_profile_keeps_factual_email() {
        let profile = json!({"email": " user@example.com "});
        assert_eq!(
            oauth_profile_email(&profile).as_deref(),
            Some("user@example.com")
        );
    }

    /// 轮询必须校验 loginId 绑定的 region（PRD G1）：用另一版轮询同一 loginId
    /// 必须在任何 HTTP 请求**之前**被拒绝，否则会把 CN 的登录态写进国际版账号库
    /// （反之亦然）。
    ///
    /// 这里直接往 `OAUTH_STATES` 里种一条记录来构造交错，因此不依赖网络，
    /// 也不会真的发起请求。用完清理，避免污染其它用例。
    #[tokio::test]
    async fn poll_rejects_login_id_bound_to_other_region() {
        let cn_login = format!("wb_test_{}", uuid::Uuid::new_v4().simple());
        let global_login = format!("wb_test_{}", uuid::Uuid::new_v4().simple());
        let unbound = "wb_test_never_inserted";

        {
            let mut map = oauth_states().lock().unwrap();
            map.insert(
                cn_login.clone(),
                OAuthInfo {
                    region: Region::Cn,
                    state: "state-cn".to_string(),
                    expires_at: now_secs() + OAUTH_TIMEOUT_SECONDS,
                    ..Default::default()
                },
            );
            map.insert(
                global_login.clone(),
                OAuthInfo {
                    region: Region::Global,
                    state: "state-global".to_string(),
                    expires_at: now_secs() + OAUTH_TIMEOUT_SECONDS,
                    ..Default::default()
                },
            );
        }

        // Global 轮询 CN 发起的 loginId → 拒绝。
        let crossed = oauth_poll_for(Region::Global, &cn_login).await;
        assert_eq!(crossed["done"], json!(true));
        assert_eq!(crossed["error"], json!("登录请求与目标版本不匹配"));

        // 反向：CN 轮询 Global 发起的 loginId → 同样拒绝。
        let crossed_rev = oauth_poll_for(Region::Cn, &global_login).await;
        assert_eq!(crossed_rev["done"], json!(true));
        assert_eq!(crossed_rev["error"], json!("登录请求与目标版本不匹配"));

        // 未知 loginId → 登录请求不存在（同样在 I/O 之前返回）。
        let unknown = oauth_poll_for(Region::Cn, unbound).await;
        assert_eq!(unknown["done"], json!(true));
        assert_eq!(unknown["error"], json!("登录请求不存在"));

        let mut map = oauth_states().lock().unwrap();
        map.remove(&cn_login);
        map.remove(&global_login);
    }
}
