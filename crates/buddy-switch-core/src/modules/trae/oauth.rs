//! Trae OAuth 登录（浏览器授权 + 本地回调监听）。
//!
//! ## 为什么必须走这条路，而不是继续读本地 JWT
//!
//! 本模块原先只有两条取凭据的路：**手动粘贴 JWT** 与 **从客户端 userData 里提取
//! `Cloud-IDE-JWT`**。后者在 Trae 1.107.1 上已经**实测失效**——`storage.json` 与
//! `state.vscdb` 里都不再有明文 JWT。参考实现的设计文档明确写
//! 「OAuth 登录闭环是获取 refresh_token 的**根本途径**」。
//!
//! ## 协议层（2026-09-16 抓包固化，本轮整体替换）
//!
//! 授权页 `login_channel=native_ide` 的原生流程：
//!
//! 1. 客户端构造授权 URL（`build_authorize_url`）：**22 参数**（TRAE/IDE 线）或
//!    **23 参数**（SOLO 线，多一个从属的 `hide_saas_login=true`）。其中
//!    `login_trace_id` 兼作 CSRF 绑定值、`code_challenge` 是 PKCE S256；
//!    **授权页的域按区域分家**（国内 `https://www.trae.cn`、国际 `https://www.trae.ai`，
//!    取自 [`endpoints_for`]`(variant).console_base`），
//!    **`auth_from` / `client_id` 按产品线分家**（SOLO 线 `solo` + `en1oxy7wnw8j9n`、
//!    TRAE 线 `trae` + `ono9krqynydwx5`，取自 `variant.oauth_line()`）。
//! 2. 用户在浏览器登录并点授权 → 授权页前端调 `GetPCAuthCode`（绑定 challenge）
//!    → 302 回 `auth_callback_url`，参数为 `authCodeInfo`（URL 编码 JSON）+ `userInfo`
//!    + `host` + `userRegion` + `loginTraceID`；
//! 3. 本模块用 `authCodeInfo.AuthCode` + `code_verifier` 调
//!    `${host}/trae/api/v3/oauth/ExchangeToken` 换 token。
//!
//! **旧契约已全部作废**：4 参数 URL、只找 `refreshToken` 的回调解析、
//! `{ClientID, RefreshToken, ClientSecret, UserID}` 请求体、写死的 cloudide 端点、
//! 缺失的 `x-cloudide-token: ""`。逐条对照见架构文档 §1.2 W1–W9。
//!
//! ## 两条交换路径的私钥分工（★ 红线级）
//!
//! - **AuthCode 交换（本模块）**：**不发** DeviceProof，**不持有私钥**。
//!   `DeviceInfo.DevicePublicKey` 直接取信封里的 `publicKeyPEM`
//!   （类型是 [`icube::DeviceIdentity`]，**结构上没有私钥字段**）。
//! - **refreshToken 刷新（`account::exchange_token_for`）**：**必须发** DeviceProof、
//!   必须持有私钥。见 `icube` 模块头。
//!
//! 本模块**不移植**参考 `oauth.rs:579-609` 的「旧端点 + AuthCode + DeviceProof」探测变体：
//! 它需要为 AuthCode 路径持有私钥，与上述红线冲突，且其唯一目的是验证一个已被
//! 固化的逆向结论（参考注释自述「保留以验证逆向结论」）。
//!
//! ## ★ 端口必须固定 17388，且必须应答上游的「在线探测」
//!
//! 这是本模块**最容易踩死的坑**，症状是「授权登录一直卡在认证中」：
//! 授权页在「认证中，正在验证身份」这一步会从浏览器侧探 `127.0.0.1:17388`
//! 确认客户端在线，探不到就不走完授权。因此：
//!
//! - 端口**不能**用 `bind(":0")` 交给系统随机分配（见 [`CALLBACK_PORT`]）；
//! - 端口上收到的**第一条请求通常是探测而不是回调**，必须回 200 + CORS 后继续等；
//! - 响应**必须带 CORS 头**，否则浏览器会拦掉跨源探测的响应（见 [`respond`]）。
//!
//! 三条任一缺失都会回到同一个症状，且**用 curl 测都会显示正常**——只有真浏览器会失败。
//!
//! ## 状态机
//!
//! 每次登录由 `login_id` 标识，进度放在进程内 `OnceLock<Mutex<HashMap>>`。
//! 回调监听**随登录会话生命周期**起停：成功/失败/超时/取消都会释放端口。
//! 会话还携带 `variant`（决定写哪个账号库）、`trace_id`（CSRF 期望值）与
//! `pkce_verifier`（🔴 只存内存，不落盘、不进日志）。
//!
//! ## 安全边界
//!
//! 监听**只绑 `127.0.0.1`**，且**只在拿到凭据前应答探测**：拿到凭据立即关闭。
//! 探测响应体不含任何凭据；回调响应体也不含凭据，只回一张结果卡片
//! （见 [`super::oauth_result_page`]）。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Map, Value};
use sha2::{Digest as _, Sha256};
use tokio::sync::Notify;

use crate::modules::trae::icube::{self, DeviceIdentity};
use crate::modules::trae::oauth_client::oauth_client;
use crate::modules::trae::oauth_result_page::{result_page, PageKind};
use crate::modules::trae::platform::{self, ClientInstallMeta, SystemProfile};
use crate::modules::trae::store;
use crate::modules::trae::variant::TraeVariant;
use crate::modules::trae::{
    account, device, endpoints_for, TRAE_EXCHANGE_TOKEN_LEGACY_PATH, TRAE_EXCHANGE_TOKEN_PATH,
    TRAE_OAUTH_APP_ID, TRAE_OAUTH_LOGIN_TIMEOUT_SECONDS, TRAE_PAGE_APP_VERSION,
    TRAE_PAGE_PLUGIN_VERSION,
};

/// 授权页路径（挂在**该变体的 `console_base`** 后面）。
///
/// ⚠️ 这里**只有路径**，域由 [`crate::modules::trae::endpoints_for`] 的
/// `console_base` 提供 —— 域是**按区域分家**的（CN `www.trae.cn` / 国际 `www.trae.ai`），
/// 曾经把它写成单个常量，于是国际版的登录会打开**国内版**的授权页
/// （用户在错的账号体系上登录，走完也不回调）。
///
/// 路径与客户端的拼法逐字一致（`${loginHost}/authorization`，见 `out/main.js`）。
const AUTHORIZE_PATH: &str = "/authorization";

/// 回调路径。与构造进 `auth_callback_url` 的路径必须一致，否则浏览器跳回来接不住。
const CALLBACK_PATH: &str = "/authorize";

/// 本地回调监听端口（**必须固定**，不能交给系统随机分配）。
///
/// ## 为什么固定端口不是「偷懒」，而是上游的硬约定
///
/// Trae 官网授权页在「认证中，正在验证身份」这一步**会从浏览器侧探测
/// `127.0.0.1:17388`**，只有探到「客户端在线」才会走完授权并把凭据回调过来。
/// 这个端口是 Trae 桌面端发起 OAuth 时的既定值，网站侧写死在它的探测逻辑里。
///
/// 本项目**原先用 `bind("127.0.0.1:0")` 让系统随机分配端口**，于是：
/// 授权页去探 17388 → 那里没人监听 → 判定「客户端不在线」→ **页面永远停在
/// 「认证中」** → 本地监听一直等不到回调 → 直到 `TRAE_OAUTH_LOGIN_TIMEOUT_SECONDS`
/// （300s）才超时。用户看到的现象就是「授权登录一直卡在认证中」。
///
/// ## 端口被占时怎么办（**刻意不换端口**）
///
/// 若 17388 已被真正的 Trae 客户端占用，**绝不回退到随机端口**——
/// 那样只会重新掉进「授权页探不到 → 永久卡认证中」的坑，症状完全相同但更难排查。
/// 正确做法是**明确报错**，让用户知道要关掉占用该端口的程序。
const CALLBACK_PORT: u16 = 17388;

/// 「回调而非探测」的凭据标记。
///
/// 只要请求带上其中任一参数，就说明它是**真正的回调**（而不是授权页的在线探测）。
/// 改造前只认 `refreshToken`，而新协议的主路径是 `authCodeInfo` —— 若沿用旧判别，
/// 一个合法的 AuthCode 回调会被当成探测而 `continue`，会话空等到 300 秒超时。
const CREDENTIAL_MARKERS: [&str; 6] = [
    "authCodeInfo",
    "code",
    "accessToken",
    "access_token",
    "refreshToken",
    "refresh_token",
];

/// 单次登录会话的可变状态。
#[derive(Default)]
struct LoginSession {
    /// 监听端口（用于前端展示与排障）。
    port: u16,
    /// 本次登录属于哪条产品线（决定写哪个账号库）。
    variant: TraeVariant,
    /// CSRF 期望值：授权 URL 里的 `login_trace_id`，应与回调的 `loginTraceID` 相等。
    trace_id: String,
    /// PKCE verifier（🔴 只存内存，不落盘、不进日志）。
    pkce_verifier: String,
    /// 成功后的账号视图。
    result: Option<Value>,
    /// 失败原因（含超时、取消、上游报错）。
    error: Option<String>,
    /// 是否已结束（无论成败）。前端据此停止轮询。
    done: bool,
    /// 已收到的取消信号（终态判定用）。
    cancelled: bool,
    /// 取消通知：让监听任务**立刻**醒来收工，而不是干等到超时。
    ///
    /// 用 `Notify` 而不是「定期轮询 cancelled 标志」的理由：取消要能立刻释放端口，
    /// 而轮询要么延迟释放（短间隔）、要么白烧 CPU（长间隔跑满 300 秒）。
    /// 注意必须用 `notify_one()` 而非 `notify_waiters()`——后者只唤醒**当前**正在
    /// 等待的任务，若取消发生在任务开始 await 之前，信号会被丢掉。
    cancel: Arc<Notify>,
}

static LOGIN_SESSIONS: OnceLock<Mutex<HashMap<String, LoginSession>>> = OnceLock::new();

fn login_sessions() -> &'static Mutex<HashMap<String, LoginSession>> {
    LOGIN_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 会话的只读快照（避免在监听循环里长时间持锁）。
#[derive(Clone)]
struct SessionView {
    cancelled: bool,
    variant: TraeVariant,
    trace_id: String,
    pkce_verifier: String,
}

impl Default for SessionView {
    fn default() -> Self {
        Self {
            cancelled: false,
            variant: TraeVariant::default(),
            trace_id: String::new(),
            pkce_verifier: String::new(),
        }
    }
}

/// 读取会话快照；会话不存在时返回默认值（`cancelled = false`）。
fn session_view(login_id: &str) -> SessionView {
    login_sessions()
        .lock()
        .unwrap()
        .get(login_id)
        .map(|session| SessionView {
            cancelled: session.cancelled,
            variant: session.variant,
            trace_id: session.trace_id.clone(),
            pkce_verifier: session.pkce_verifier.clone(),
        })
        .unwrap_or_default()
}

/// 把回调 URL 里的查询串解析成键值对。
///
/// 刻意手写而不用 `url` crate：回调里只有一小组扁平参数，且**必须容忍**
/// 浏览器对 `auth_callback_url` 的二次编码（参考实现实测会遇到 `%26` 之类）。
/// 用 `url::form_urlencoded` 会因整串被编码而把参数并成一个大键。
///
/// ## 三步顺序不可调整（踩过）
///
/// 1. **先按 `&` 切分**（此时还是原始串）——值里被编码成 `%26` 的 `&` 因此不会被误切。
/// 2. **再整体解码每一段**——形如 `refreshToken%3Dtok` 的段，`=` 是编码过的，
///    必须先还原才能切出键值；在解码**前**切 `=`，整段会被当成一个键，
///    于是 `get("refreshToken")` 恒为 `None`（真实回调上表现为「找不到 refreshToken」，
///    但 URL 看着完好，极难排查）。
/// 3. **最后按第一个 `=` 切键值**——用 `split_once` 而非 `split`：
///    JWT / refresh_token 的值里合法地含有 `=`（base64url 填充），
///    按所有 `=` 切会把值截断成无效凭据。
fn parse_query(raw_query: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pair in raw_query.split('&') {
        if pair.is_empty() {
            continue;
        }
        // 反复解码，处理被二次/三次编码的参数。
        let mut decoded = pair.to_string();
        for _ in 0..3 {
            let next = percent_decode(&decoded);
            if next == decoded {
                break;
            }
            decoded = next;
        }
        let (key, value) = match decoded.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            // 无 `=` 的裸段（如 `?flag`）：按「键存在、值为空」处理，
            // 不要丢弃——调用方可能只判存在性。
            None => (decoded, String::new()),
        };
        out.insert(key, value);
    }
    out
}

/// 最小百分号解码（`%XX` 与 `+`）。非法的 `%` 序列原样保留。
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 从回调参数里抽取 `refreshToken`（容错读，**不是主路径**）。
///
/// 键名有 `refreshToken` / `refresh_token` / `RefreshToken` 三种可能
/// （网页与 API 命名习惯不同），依次尝试；值可能被包在 JSON 风格的双引号里，一并剥掉。
fn extract_refresh_token(params: &HashMap<String, String>) -> Option<String> {
    for key in ["refreshToken", "refresh_token", "RefreshToken"] {
        if let Some(value) = params.get(key) {
            let cleaned = value.trim().trim_matches('"').trim();
            if !cleaned.is_empty() {
                return Some(cleaned.to_string());
            }
        }
    }
    None
}

/// 从回调参数里抽取 `userInfo`（URL 编码的 JSON）。
///
/// 解析失败时返回空对象而不是报错——用户信息只用于「展示名兜底」，
/// 缺失不应阻断登录（uid 还能从 JWT 里解析）。
fn extract_user_info(params: &HashMap<String, String>) -> Value {
    for key in ["userInfo", "user_info", "UserInfo"] {
        if let Some(raw) = params.get(key) {
            let trimmed = raw.trim();
            if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                return parsed;
            }
        }
    }
    json!({})
}

/// 展示名候选键，按「越像人名的越优先」排列。
///
/// 真实回调里是 `userInfo.ScreenName`（抓包固化），旧形态是 `nickname` / `name` /
/// `email`；两类都保留，避免老回调拿不到展示名。
const DISPLAY_NAME_KEYS: [&str; 7] = [
    "ScreenName",
    "NickName",
    "nickname",
    "nickName",
    "name",
    "userName",
    "email",
];

/// 从任意 JSON 里尽力取一个展示名。
fn pick_display_name(user_info: &Value) -> Option<String> {
    for key in DISPLAY_NAME_KEYS {
        if let Some(value) = user_info.get(key).and_then(|v| v.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// 回调解析结果（内部类型；对外仍是 handlers 的 JSON）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CallbackInfo {
    /// `authCodeInfo.AuthCode`（主路径）或 `code`（标准授权码）。
    auth_code: Option<String>,
    /// `refreshToken`（容错读，老形态回调）。
    refresh_token: Option<String>,
    /// 交换端点主机（回调回传，`https://api.trae.cn` 这类形态）。
    host: Option<String>,
    /// 用户区域。
    user_region: Option<String>,
    /// CSRF 回调值。
    login_trace_id: Option<String>,
    /// 用户 ID。
    user_id: Option<String>,
    /// 展示名。
    display_name: Option<String>,
    /// 头像 URL。
    avatar: Option<String>,
}

/// 取非空字符串参数。
fn param(params: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = params.get(*key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// 解析回调参数。
///
/// ## 主路径
///
/// `authCodeInfo` 是 **URL 编码的 JSON**。`parse_query` 已做
/// 「按 `&` 切 → 反复解码（≤3 次）→ `split_once('=')`」，
/// 因此它的值进来时**已是 JSON 文本**。
///
/// **解码失败必须报错**（`authCodeInfo 解析失败（…）：授权页回调格式异常`），
/// **不得**吞掉变成 `None` —— 那会让「授权页改格式」表现成「回调里没有凭据」，
/// 用户与开发者都会以为是网络问题。
///
/// 解析成功但 `AuthCode` 为空 ⇒ 视为缺失（回落 `code` / `refreshToken` 容错读）。
fn parse_callback(params: &HashMap<String, String>) -> Result<CallbackInfo, String> {
    let mut info = CallbackInfo {
        host: param(params, &["host"]),
        user_region: param(params, &["userRegion", "user_region"]),
        login_trace_id: param(params, &["loginTraceID", "login_trace_id"]),
        refresh_token: extract_refresh_token(params),
        ..Default::default()
    };

    // ---- authCodeInfo（主路径）----
    if let Some(raw) = params.get("authCodeInfo").filter(|raw| !raw.trim().is_empty()) {
        let parsed: Value = serde_json::from_str(raw.trim())
            .map_err(|e| format!("authCodeInfo 解析失败（{e}）：授权页回调格式异常"))?;
        info.auth_code = parsed
            .get("AuthCode")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(str::to_string);
    }
    // 标准授权码兜底（`code`）。
    if info.auth_code.is_none() {
        info.auth_code = param(params, &["code"]);
    }

    // ---- userInfo（容错：解析失败不报错）----
    let user_info = extract_user_info(params);
    info.user_id = user_info
        .get("UserID")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| {
            user_info
                .get("userId")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .or_else(|| param(params, &["UserID", "userId", "user_id"]));
    info.display_name = pick_display_name(&user_info)
        .or_else(|| param(params, &["userName", "user_name", "nickname"]));
    info.avatar = user_info
        .get("AvatarUrl")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| param(params, &["avatar"]));

    Ok(info)
}

/// 回调 CSRF 校验（D5：**不新增逃生开关**）。
///
/// `expected` = 在途会话的 `trace_id`（无会话时 `None`）。四个分支：
///
/// | # | 回调带 loginTraceID | 存在在途会话 | 结果 |
/// |:--|:--|:--|:--|
/// | ① | 是 | 是 | **必须相等**，否则拒绝 |
/// | ② | 是 | 否 | 放行（重启后粘贴回调的场景） |
/// | ③ | 否 | 是 | **拒绝**（新协议回调必带该字段，缺失即不可信） |
/// | ④ | 否 | 否 | 放行（宽容，兼容重启后粘贴） |
fn verify_login_trace(
    params: &HashMap<String, String>,
    expected: Option<&str>,
) -> Result<(), String> {
    let callback_trace = param(params, &["loginTraceID", "login_trace_id"]);
    let expected = expected.map(str::trim).filter(|value| !value.is_empty());
    match (callback_trace, expected) {
        // ① 双向绑定校验。
        (Some(callback), Some(expected)) => {
            if callback == expected {
                Ok(())
            } else {
                Err(classify_error("csrf", ""))
            }
        }
        // ② 有回调值、无在途会话 ⇒ 放行。
        (Some(_), None) => Ok(()),
        // ③ 无回调值、有在途会话 ⇒ 拒绝。
        (None, Some(_)) => Err(classify_error("csrf", "回调缺少 loginTraceID")),
        // ④ 都没有 ⇒ 宽容放行。
        (None, None) => Ok(()),
    }
}

/// 把「阶段 + 上游原文」映射成**可操作**的用户文案（纯函数，可单测）。
///
/// 只做文案映射，**不改控制流**；上游原始码始终保留在消息里
/// （否则用户与开发者都无从判断是协议问题还是凭据问题）。
///
/// 输出**绝不含**令牌 / 私钥 / PKCE verifier 片段（见 `icube` 模块头的脱敏红线）。
fn classify_error(stage: &str, raw: &str) -> String {
    match stage {
        "portBusy" => format!(
            "本地回调端口 {CALLBACK_PORT} 无法监听：{raw}。\
             该端口是 Trae 授权页确认「客户端在线」的固定端口，必须可用；\
             请关闭占用它的程序（通常是 Trae 客户端自身或上一次未退出的登录会话）后重试。"
        ),
        "timeout" => format!(
            "登录超时：{TRAE_OAUTH_LOGIN_TIMEOUT_SECONDS} 秒内没有收到授权回调。\
             请确认 http://127.0.0.1:{CALLBACK_PORT}{CALLBACK_PATH} 可达（防火墙/代理），\
             并检查授权页是否一直停在「认证中」。"
        ),
        "csrf" => format!(
            "回调与本机发起的登录不匹配（loginTraceID 校验失败：{raw}），\
             可能为伪造或重放，已拒绝；请重新发起登录。"
        ),
        "callback" => format!(
            "回调字段缺失（{raw}）；请重新发起登录。若反复失败，请复制完整回调 URL 反馈排查。"
        ),
        "credential" => raw.to_string(),
        "upstream" => classify_upstream(raw),
        _ => raw.to_string(),
    }
}

/// 上游错误码 → 可操作文案。
fn classify_upstream(raw: &str) -> String {
    if raw.contains("20403") {
        format!(
            "设备校验未通过（20403）：缺少 x-cloudide-token 或设备凭证与账号不匹配。原始信息：{raw}"
        )
    } else if raw.contains("20405") {
        format!(
            "设备签名未通过（20405）：DeviceProof 签名格式或设备身份不匹配。原始信息：{raw}"
        )
    } else {
        format!("上游令牌交换失败：{raw}")
    }
}

/// PKCE：返回 `(verifier, challenge)`。
///
/// `verifier` 是 RFC 7636 允许的 64 位 hex（unreserved 字符集），
/// **只存内存**（`LoginSession`），不落盘、不进日志。
/// `challenge = BASE64URL_NOPAD(SHA256(verifier))`。
fn pkce_pair() -> (String, String) {
    let verifier = icube::random_hex(64);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

/// 一次登录用到的**全部客户端侧事实**：设备身份 + 安装元数据 + 系统信息。
///
/// ## ★ 为什么必须绑成一个值（本轮缺陷的结构性护栏）
///
/// 授权 URL 与 `ExchangeToken` 请求体里有**四对同源字段**，而 2026-09-24 的真机
/// 缺陷正是「每一对都各取一份来源」：
///
/// | 同源对 | 授权 URL | 兑换请求体 |
/// |:--|:--|:--|
/// | 机器标识 | `machine_id` / `x_machine_id` | `DeviceInfo.MachineID` |
/// | 应用版本 | `x_app_version` | `ClientVersion` / `IDEVersion` |
/// | 设备型号 | `x_device_brand` | `DeviceModel` |
/// | 系统版本 | `x_os_version` | `OSVersion` |
///
/// 上游对不一致的回应是 `20403/040036: Token device not match`（用户报障原文）。
/// 把三者收进一个值、并让两个构造函数**只从这里取值**，就能让「两处各取一份」
/// 在类型层面不成立 —— 与 `device_id` 那条红线同款思路。
#[derive(Debug, Clone)]
struct ClientFacts {
    /// 设备身份（`device_id` 与 `publicKeyPEM` 的唯一来源）。
    identity: DeviceIdentity,
    /// 安装元数据（`manifest.json`）。取不到时为全空 ⇒ 各字段按 [`ClientInstallMeta`] 回落。
    meta: ClientInstallMeta,
    /// 系统信息。测试里可注入固定值。
    system: SystemProfile,
}

impl ClientFacts {
    /// 读一次本机事实。**设备身份取不到就直接失败**（不做任何降级）。
    fn load_for(variant: TraeVariant) -> Result<ClientFacts, String> {
        // ★ 身份先于端口取：授权 URL 的 `device_id` **必须**与 icube 设备凭证同源，
        // 取不到就直接失败，不做任何降级（见 `build_authorize_url` 的红线说明）。
        let identity = icube::device_identity_for(variant)
            .map_err(|error| classify_error("credential", &error.user_message(variant)))?;
        Ok(ClientFacts {
            identity,
            meta: platform::client_install_meta_for(variant).unwrap_or_default(),
            system: platform::system_profile(),
        })
    }

    /// `machine_id` / `x_machine_id` / `DeviceInfo.MachineID` 的**共同**取值。
    fn machine_id(&self) -> String {
        authorize_machine_id(self.identity.variant, &self.identity)
    }

    /// `x_app_version` / `ClientVersion` / `IDEVersion` 的**共同**取值。
    ///
    /// 客户端三处报的都是**安装包版本**（`manifest.json` → `appVersion`，本机 `0.1.69`），
    /// 不是 `resources/app/package.json` 的内核版本（本机 `1.107.1`）。
    /// 逐级回落：`manifest.json` → 安装目录版本 → 内置常量。**三条路都指向同一个值**，
    /// 因此不会出现「URL 与请求体各说一个数」。
    fn app_version(&self) -> String {
        for candidate in [
            self.meta.app_version.as_str(),
            self.identity.app_version.as_str(),
            TRAE_PAGE_APP_VERSION,
        ] {
            let candidate = candidate.trim();
            if !candidate.is_empty() {
                return candidate.to_string();
            }
        }
        String::new()
    }

    /// 授权 URL 的 `plugin_version`（客户端：`product.json.tronBuildVersion`，
    /// 本机 `2.3.87413`；同值也在安装根 `manifest.json` 的 `buildVersion` 里）。
    fn build_version(&self) -> String {
        let from_client = self.meta.build_version.trim();
        if !from_client.is_empty() {
            return from_client.to_string();
        }
        TRAE_PAGE_PLUGIN_VERSION.to_string()
    }

    /// 授权 URL 的 `x_app_type`（客户端：`product.quality`，本机 `stable`）。
    fn app_type(&self) -> String {
        let from_client = self.meta.channel.trim();
        if !from_client.is_empty() {
            return from_client.to_string();
        }
        "stable".to_string()
    }
}

/// 授权 URL 与 `DeviceInfo` **共用的** `machine_id`（同源，唯一派生点）。
///
/// ## 取值来源：客户端自己的 `telemetry.machineId`
///
/// 真机客户端在授权 URL 的 `machine_id` / `x_machine_id` 与
/// `DeviceInfo.MachineID` 三处报的是**同一个** `machineId`（客户端里 URL 构造器
/// 与请求类注入的是同一个对象 ⇒ 两处必然相等）。真机日志可逐字核对：
///
/// ```text
/// update#initialize: region = CN, deviceId = 7d3f5a9d…7538   ← 即 telemetry.machineId
/// OAuthenticator# openLogin getLoginUrl …&machine_id=7d3f5a9d…7538&…
/// [exchangeTokenByAuthCode] request {…"MachineID":"7d3f5a9d…7538"…}
/// ```
///
/// ## 为什么此前是错的
///
/// 改造前这里用 `device::oauth_login_machine_for` 生成的**自造**值（arch 有意为之：
/// 「授权 URL 在用户点授权之前就要打开，那时不该让客户端装没装决定登录能否发起」），
/// 而请求体用的是 `storage.json` 的 `telemetry.machineId` ⇒ 两处**必然不等**。
/// 上游因此回 `20403/040036 Token device not match`。
///
/// 该理由本身也站不住：`device_id` 本来就来自同一个 `storage.json`
/// （[`icube::device_identity_for`]），客户端没启动过时**更早**就失败了。
///
/// 自造值保留为**兜底**：客户端还没写过 `telemetry.machineId` 时（极早期安装）
/// 至少给出一个稳定的值，而不是空串。
fn authorize_machine_id(variant: TraeVariant, identity: &DeviceIdentity) -> String {
    let from_client = identity.machine_id.trim();
    if !from_client.is_empty() {
        return from_client.to_string();
    }
    device::oauth_login_machine_for(variant).machine_id
}

/// 构造授权 URL（**22 参数**，逐字对齐抓包固化值；SOLO 线再多一个 `hide_saas_login`）。
///
/// 出处：`reference/TraeWorkAssistant-main/src-tauri/src/commands/oauth.rs:321-355`。
/// **不要按语义改写参数顺序或取值** —— 授权页按这些参数进入 `native_ide` 原生流程，
/// 少一个或值不对就会停在 billing status 后不回跳。
///
/// ## ★ 三处取值**必须**按 `variant` 派生（本轮修的两个真实缺陷）
///
/// | 项 | 取值来源 | 国内版 | 国际版 |
/// |:---|:---|:---|:---|
/// | 授权页**域** | `endpoints_for(variant).console_base` | `https://www.trae.cn` | `https://www.trae.ai` |
/// | `auth_from` | `variant.oauth_line().auth_from()` | `solo` | `solo` |
/// | `client_id` | `oauth_client().client_id_for(line)` | `en1oxy7wnw8j9n` | `en1oxy7wnw8j9n` |
///
/// 前两行是「按**区域**分家」（`TraeWork` 与 `Global` 不同），第三行是
/// 「按**产品线**分家」（`TraeWork`/`Global` 同属 SOLO 线，`Trae` 是 TRAE 线）。
/// **两个轴不要混为一谈**：域随区域变，钥匙随产品线变。
///
/// 把域做成**入参**（而不是再写一个常量）是有意的：它让「忘了按区域分家」
/// 在类型层面不可能 —— 调用方必须给出变体，而变体是唯一决定域的东西。
/// 曾经的缺陷形态就是「域写死 CN + 参数按区域分家」，于是国际版登录
/// 打开的是国内版授权页，症状（停在「认证中」不回跳）与端口问题几乎一样。
///
/// ## ★ `device_id` **只能**取自 `identity`（红线，结构性护栏）
///
/// 签名里**没有独立的 `device_id` 形参** ⇒ 类型层面不可能传进一个不同源的值。
/// 依据：参考 `commands/oauth.rs:259-267` 逐字——「登录 URL 的 device_id 必须与
/// icube 设备凭证同源，否则服务端 **20403/20405**；恒覆盖旧值（旧值是随机/
/// `device_map` 对齐的，**与私钥不匹配**）」。自造一个 `device_id` 就是主动写出一次
/// 必然失败的登录，正是本轮要修的缺陷本身。
///
/// `machine_id` 则是**本机自造**的值（`device::oauth_login_machine_for`），
/// 变体级持久稳定 —— 参考自身这两者也不相等（见 arch §10 #2-b）。
fn build_authorize_url(
    variant: TraeVariant,
    facts: &ClientFacts,
    port: u16,
    trace_id: &str,
    code_challenge: &str,
) -> String {
    let identity = &facts.identity;
    let device_id = identity.device_id.as_str();
    // ★ 同源取值：`machine_id` 与 `DeviceInfo.MachineID` 都来自这里（唯一派生点）。
    let machine_id = facts.machine_id();
    let machine_id = machine_id.as_str();
    // ★ 同源取值：系统信息同时喂给本 URL 的 `x_device_brand` / `x_device_type` /
    // `x_os_version` 与请求体的 `DeviceModel` / `OSInfo` / `OSVersion`。
    let system = &facts.system;
    let callback = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
    // 授权页产品线：决定 `auth_from` / `client_id` / `PlatformCode` / 是否追加 `hide_saas_login`。
    // 判定依据是客户端自己的 `packageType` 分派（见 `OAuthLine::from_package_type`）。
    let line = variant.oauth_line();
    let mut url = format!(
        "{console_base}{AUTHORIZE_PATH}?\
        login_version=1\
        &auth_from={auth_from}\
        &login_channel=native_ide\
        &plugin_version={plugin_version}\
        &auth_type=local\
        &client_id={client_id}\
        &redirect=0\
        &login_trace_id={trace_id}\
        &auth_callback_url={redirect_uri}\
        &machine_id={machine_id}\
        &device_id={device_id}\
        &x_device_id={device_id}\
        &x_machine_id={machine_id}\
        &x_device_brand={device_brand}\
        &x_device_type={device_type}\
        &x_os_version={os_version}\
        &x_env=\
        &x_app_version={app_version}\
        &x_app_type={app_type}\
        &code_challenge={code_challenge}\
        &code_challenge_method=S256\
        &channel_name=common",
        console_base = endpoints_for(variant).console_base,
        auth_from = line.auth_from(),
        plugin_version = facts.build_version(),
        client_id = oauth_client().client_id_for(line),
        redirect_uri = urlencoding::encode(&callback),
        // `x_device_brand` 装的是**设备型号**（客户端原文 `x_device_brand: b?.deviceModel`），
        // 不是主机名 —— 改造前填的是 `COMPUTERNAME`，与请求体的 `DeviceModel` 对不上。
        // 值可能含空格/非 ASCII，必须走通用 URL 编码。
        device_brand = urlencoding::encode(&system.device_model),
        device_type = urlencoding::encode(&system.os_name),
        os_version = urlencoding::encode(&system.os_version),
        app_version = facts.app_version(),
        app_type = facts.app_type(),
    );
    // `hide_saas_login` 是 `auth_from=solo` 的**从属**参数（客户端：`A==="solo" && (D+=…)`），
    // 追加在**最末**，与客户端拼串顺序一致。
    if line.hide_saas_login() {
        url.push_str("&hide_saas_login=true");
    }
    url
}

/// 解析交换端点主机：回调回传的 `host` 优先，缺失时回落该变体的 **`account_base`**。
///
/// 归一化只做 `trim()` + 去尾斜杠（**照抄参考** `oauth.rs:548` 的做法，
/// 不自创 scheme 补全规则）。返回 `(host, 是否用了回落值)` ——
/// 回落时要写一条脱敏留痕，便于排查「授权页没回传 host」这类上游变化。
///
/// ## 回落值为什么是 `account_base` 而不是 `icube_base`
///
/// 真机客户端（2026-09-24 实测）兑换时打的是
/// `https://api.trae.cn/trae/api/v3/oauth/ExchangeToken` —— 即
/// `bootConfig.account.trae.normal`（本表的 `account_base`），
/// 而 `icube_base`（`https://api.trae.com.cn`）是**另一台主机**。
/// 客户端源码里 `apiHost` 的取值顺序是「回调回传的 host → `account.trae.normal`」，
/// 从不是 iCube 基址。
fn resolve_exchange_host(callback_host: Option<&str>, variant: TraeVariant) -> (String, bool) {
    match callback_host
        .map(|host| host.trim().trim_end_matches('/'))
        .filter(|host| !host.is_empty())
    {
        Some(host) => (host.to_string(), false),
        None => (endpoints_for(variant).account_base.to_string(), true),
    }
}

/// 构造 AuthCode 交换请求体（纯函数，可单测）。
///
/// ⚠️ AuthCode 场景**不发 DeviceProof**（那是 refreshToken 刷新场景专属结构），
/// 因此这里只收 [`ClientFacts`]（其 `identity` 字段**没有私钥字段**，见模块头）。
/// `DevicePublicKey` 直接取信封里的 `publicKeyPEM`，**不推导**。
///
/// ★ 请求体里**每一个**与授权 URL 重叠的字段都从 `facts` 派生（唯一来源）：
/// `MachineID` ← [`ClientFacts::machine_id`]、`ClientVersion` / `IDEVersion`
/// ← [`ClientFacts::app_version`]、`DeviceModel` / `OSInfo` / `OSVersion`
/// ← `facts.system`。`PlatformCode` 由 `identity.variant.oauth_line()` 派生。
fn build_exchange_payload(
    client_id: &str,
    auth_code: &str,
    code_verifier: &str,
    facts: &ClientFacts,
) -> Value {
    let identity = &facts.identity;
    let system = &facts.system;
    // ★ 与授权 URL 的 `x_app_version` 同源（客户端三处同值）。
    let app_version = facts.app_version();
    json!({
        "ClientID": client_id,
        "AuthCode": auth_code,
        "CodeVerifier": code_verifier,
        "DeviceInfo": {
            "DeviceID": identity.device_id,
            // ★ 与授权 URL 的 `machine_id` / `x_machine_id` **同源**（唯一派生点）。
            // 改造前这里取 `storage.json` 的 `telemetry.machineId`、URL 取自造值 ⇒
            // 上游回 `20403/040036 Token device not match`。
            "MachineID": facts.machine_id(),
            // ★ 按**产品线**派生：SOLO 线 `SOLO_PC` / IDE 线 `IDE_PC`
            // （客户端 `k(){ return gr(product) ? "SOLO_PC" : "IDE_PC" }`）。
            // 改造前写死 `IDE_PC`（照抄参考实现的 IDE 线），SOLO 线上必然对不上。
            "PlatformCode": identity.variant.oauth_line().platform_code(),
            "DeviceType": "PC",
            // 真值是「Windows 账户全名 + 本地化后缀」（本机 `Jackey的电脑`）——
            // 本实现留空，理由见 `platform::system_profile`（无稳定来源，且该字段
            // **不在授权 URL 里**，不参与同源比对）。
            "DeviceName": system.device_name,
            // ★ 与授权 URL 的 `x_device_brand` 同源（客户端装的是 `deviceModel`）。
            "DeviceModel": system.device_model,
            "ClientVersion": app_version,
            "DevicePublicKey": identity.public_key_pem,
            "DeviceBrand": system.device_manufacturer,
            "DeviceCPU": system.cpu_brand,
            // ★ 与授权 URL 的 `x_device_type` / `x_os_version` 同源。
            "OSInfo": system.os_name,
            "OSVersion": system.os_version,
        },
        "IDEVersion": app_version,
    })
}

/// 构造**旧端点兜底**请求体（纯函数，可单测）。
///
/// `code_key` 取 `"AuthCode"` 或 `"Code"`（旧端点对字段名的两种历史拼法）。
/// 该兜底体**既无 `DeviceProof` 也无公钥** ⇒ 同样不需要私钥，
/// 与模块头的红线一致。
fn build_auth_code_fallback_payload(
    client_id: &str,
    auth_code: &str,
    code_verifier: &str,
    device_id: &str,
    platform_code: &str,
    code_key: &str,
) -> Value {
    let mut payload = Map::new();
    payload.insert("ClientID".into(), json!(client_id));
    payload.insert(code_key.to_string(), json!(auth_code));
    payload.insert("CodeVerifier".into(), json!(code_verifier));
    payload.insert("DeviceID".into(), json!(device_id));
    // 与主变体同源（按产品线派生），不写死：兜底链里出现一个「另一条线」的值
    // 只会让本来要排查的真问题被掩盖。
    payload.insert("PlatformCode".into(), json!(platform_code));
    Value::Object(payload)
}

/// 用 AuthCode 交换 token（**两变体链，两步都不需要私钥**）。
///
/// ① 主变体：`${host}/trae/api/v3/oauth/ExchangeToken` + `DeviceInfo`（无 Proof）；
/// ② 兜底：`${icube_base}/cloudide/api/v3/trae/oauth/ExchangeToken`
///    + `{ClientID, AuthCode|Code, CodeVerifier, DeviceID, PlatformCode}`（无 Proof）。
///
/// `host` 显式传入（便于测试指向本地 mock）。
///
/// ## ★ 签名里只有 `&ClientFacts`，没有独立的 device_id / machine_id（结构性护栏）
///
/// `DeviceInfo.DeviceID`、`x-device-id` 与授权 URL 的 `device_id` **三方同源**，
/// 全部取自 `facts.identity.device_id`；`DeviceInfo.MachineID` 与授权 URL 的
/// `machine_id` / `x_machine_id` 同取自 [`ClientFacts::machine_id`]。
/// 类型层面不存在「传进另一个 device_id / machine_id」的可能。
///
/// **不移植**参考 `oauth.rs:579-609` 的「旧端点 + AuthCode + DeviceProof」探测变体
/// （需要私钥、且其唯一目的是验证已固化的逆向结论）。
async fn exchange_auth_code(
    variant: TraeVariant,
    host: &str,
    auth_code: &str,
    code_verifier: &str,
    facts: &ClientFacts,
) -> Result<account::ExchangedToken, String> {
    let auth_device_id = facts.identity.device_id.as_str();
    // ★ 交换请求体里的 `ClientID` 必须与**授权 URL 用的那把钥匙同源**（按产品线分）。
    // 两处取不同的值 = 拿 A 线的钥匙去兑 B 线签发的 AuthCode，上游只会拒绝。
    let line = variant.oauth_line();
    let client_id = oauth_client().client_id_for(line).to_string();
    // ★ `PlatformCode` 同样按产品线派生，与 `client_id` / `auth_from` 同一判定。
    let platform_code = line.platform_code();
    let legacy_url = format!(
        "{}{}",
        endpoints_for(variant).icube_base,
        TRAE_EXCHANGE_TOKEN_LEGACY_PATH
    );
    let variants: Vec<(String, String, Value, bool)> = vec![
        (
            "ExchangeToken/DeviceInfo".to_string(),
            format!(
                "{}{}",
                host.trim_end_matches('/'),
                TRAE_EXCHANGE_TOKEN_PATH
            ),
            build_exchange_payload(&client_id, auth_code, code_verifier, facts),
            true,
        ),
        (
            "ExchangeToken/AuthCode".to_string(),
            legacy_url.clone(),
            build_auth_code_fallback_payload(
                &client_id,
                auth_code,
                code_verifier,
                auth_device_id,
                platform_code,
                "AuthCode",
            ),
            false,
        ),
        (
            "ExchangeToken/Code".to_string(),
            legacy_url,
            build_auth_code_fallback_payload(
                &client_id,
                auth_code,
                code_verifier,
                auth_device_id,
                platform_code,
                "Code",
            ),
            false,
        ),
    ];

    let mut errors: Vec<String> = Vec::new();
    for (tag, url, payload, with_cloudide_token) in &variants {
        match try_exchange_variant(
            tag,
            url,
            payload,
            auth_device_id,
            platform_code,
            *with_cloudide_token,
        )
        .await
        {
            Ok(exchanged) => return Ok(exchanged),
            Err(error) => errors.push(format!("{tag}: {error}")),
        }
    }
    Err(classify_error(
        "upstream",
        &format!("全部交换变体失败 → {}", errors.join(" | ")),
    ))
}

/// 单个 AuthCode 交换变体尝试。
///
/// AuthCode 场景要求 access 与 refresh **都存在**（没有 refresh_token 的账号
/// 之后无法自动续期，等于把问题推迟到 24 小时后爆发）。
async fn try_exchange_variant(
    tag: &str,
    url: &str,
    payload: &Value,
    device_id: &str,
    platform_code: &str,
    with_cloudide_token: bool,
) -> Result<account::ExchangedToken, String> {
    let client = crate::modules::trae::credits::trae_http_client();
    let mut request = client
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "*/*")
        // 设备头（照抄参考 `oauth.rs:663-671`；真机客户端只发 `content-type` +
        // `x-cloudide-token`，这几个头属于「多带无副作用」，留着便于上游侧排查）。
        //
        // ⚠️ 但**值必须对**：`x-platform-code` 曾写死 `IDE_PC`，而 SOLO 线要的是
        // `SOLO_PC`（同 `DeviceInfo.PlatformCode`，按产品线派生）。
        .header("x-device-id", device_id)
        .header("x-app-id", TRAE_OAUTH_APP_ID)
        .header("x-platform-code", platform_code);
    if with_cloudide_token {
        // ★ 必须**存在且为空串**：缺失报 20403，带旧 token 报 20405。
        request = request.header("x-cloudide-token", "");
    }
    let response = request
        .json(payload)
        .send()
        .await
        .map_err(|e| {
            // 与签到/续期同一出口：展开 source 链（见 modules/net.rs）。
            // 授权流程的失败提示往往是用户唯一的线索，不能只剩一行顶层文案。
            //
            // 用 `transport_error`（带码版）而不是 `describe_transport_error`：同一条文案，
            // 额外携带 `net.transport.*` 码，前端才能按当前语言重渲染。文本逐字节相同
            // —— 见 `net::describe_transport_error` 的委托实现。
            format!(
                "请求失败: {}",
                crate::modules::net::transport_error(&e).to_wire()
            )
        })?;

    let status = response.status().as_u16();
    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("解析响应失败 (HTTP {status}): {e}"))?;

    // 火山引擎标准信封：错误在 ResponseMetadata.Error.{Code,Message,StandardCode}。
    if let Some(code) = dig_string(&body, &["ResponseMetadata", "Error", "Code"]) {
        if code != "0" {
            let message = dig_string(&body, &["ResponseMetadata", "Error", "Message"])
                .unwrap_or_else(|| "未知错误".into());
            let standard = dig_string(&body, &["ResponseMetadata", "Error", "StandardCode"])
                .unwrap_or_default();
            return Err(format!("code={code}/{standard}: {message}"));
        }
    }
    // 兼容旧解析（顶层 code/message 形态）。
    if let Some(code) = body.get("code").and_then(|value| value.as_i64()) {
        if code != 0 {
            let message = body
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("未知错误");
            return Err(format!("code={code}: {message}"));
        }
    }

    let access = find_token_value(&body, ACCESS_TOKEN_KEYS).filter(|token| !token.is_empty());
    let refresh = find_token_value(&body, REFRESH_TOKEN_KEYS).filter(|token| !token.is_empty());
    match (access, refresh) {
        (Some(access), Some(refresh)) => Ok(account::ExchangedToken {
            jwt: crate::modules::trae::jwt::authorization_header(&access),
            refresh_token: Some(refresh),
        }),
        (Some(_), None) => Err(format!(
            "{tag}: 响应缺少 RefreshToken（该账号将无法自动续期，已拒绝写库）"
        )),
        (None, _) => Err(format!(
            "{tag}: 响应中未找到 Token 字段（响应键：{}）",
            key_paths(&body).join(" | ")
        )),
    }
}

/// 按路径逐层取值并转成字符串（数字也接受）。
fn dig_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    if let Some(text) = current.as_str() {
        return Some(text.to_string());
    }
    current.as_i64().map(|number| number.to_string())
}

/// access token 的候选键。
const ACCESS_TOKEN_KEYS: &[&str] = &["AccessToken", "access_token", "token", "Jwt", "JWT"];
/// refresh token 的候选键。
const REFRESH_TOKEN_KEYS: &[&str] = &["RefreshToken", "refresh_token"];

/// 在 JSON 树中递归查找指定键（大小写不敏感）的首个非空字符串值。
///
/// 先查 `Result` / `data` / `Data` 容器，再全树深挖 —— 上游的响应包裹层
/// 在不同接口上并不统一，写死一种形态会让「换了个包裹层」表现成「登录失败」。
fn find_token_value(value: &Value, keys: &[&str]) -> Option<String> {
    fn search(value: &Value, keys: &[&str]) -> Option<String> {
        match value {
            Value::Object(map) => {
                for key in keys {
                    if let Some(found) = map.get(*key).and_then(|v| v.as_str()) {
                        if !found.is_empty() {
                            return Some(found.to_string());
                        }
                    }
                }
                for (key, candidate) in map {
                    if keys.iter().any(|k| k.eq_ignore_ascii_case(key)) {
                        if let Some(found) = candidate.as_str() {
                            if !found.is_empty() {
                                return Some(found.to_string());
                            }
                        }
                    }
                }
                for candidate in map.values() {
                    if let Some(found) = search(candidate, keys) {
                        return Some(found);
                    }
                }
                None
            }
            Value::Array(items) => items.iter().find_map(|item| search(item, keys)),
            _ => None,
        }
    }

    if let Some(container) = value
        .get("Result")
        .or_else(|| value.get("data"))
        .or_else(|| value.get("Data"))
    {
        if let Some(found) = search(container, keys) {
            return Some(found);
        }
    }
    search(value, keys)
}

/// 收集 JSON 的键路径（**只有键名、不含值**），用于「找不到 token」时的诊断。
fn key_paths(value: &Value) -> Vec<String> {
    fn walk(value: &Value, prefix: &str, depth: usize, out: &mut Vec<String>) {
        if depth > 6 || out.len() >= 40 {
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    out.push(path.clone());
                    walk(child, &path, depth + 1, out);
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate().take(3) {
                    walk(child, &format!("{prefix}[{index}]"), depth + 1, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, "", 0, &mut out);
    out
}

/// 请求里是否带「凭据标记」。
///
/// 无任一标记 ⇒ 授权页的**在线探测**（回 200 + CORS 后继续等）。
/// 注意 `error=` 的判定**必须在它之前**（见监听循环）。
fn has_credential_marker(params: &HashMap<String, String>) -> bool {
    params
        .keys()
        .any(|key| CREDENTIAL_MARKERS.contains(&key.as_str()))
}

/// 发起一次登录（默认变体，兼容壳）。
pub async fn login_start() -> Result<Value, String> {
    login_start_for(TraeVariant::default()).await
}

/// 发起一次登录：占端口、起监听、返回授权 URL。
///
/// 返回值形状（camelCase，与线上约定一致）：
/// ```json
/// { "loginId", "verificationUri", "expiresIn", "port",
///   "variant", "variantLabel",
///   "deviceCredential": { "available", "sourceApp", "deviceId", "machineId",
///                         "appVersion", "errorKind", "errorMessage" } }
/// ```
/// `deviceCredential` 是**脱敏**诊断（`icube::credential_status_for`），
/// **私钥永不出现**。
pub async fn login_start_for(variant: TraeVariant) -> Result<Value, String> {
    // ★ 身份先于端口取：授权 URL 的 `device_id` **必须**与 icube 设备凭证同源，
    // 取不到就**直接失败**，不做任何降级（见 [`build_authorize_url`] 的红线说明）。
    //
    // 为什么放在绑端口之前：身份缺失是本机环境问题（客户端没启动过 / 没登录过），
    // 与端口无关。先判它，用户拿到的第一句话就是「去启动一次客户端」，
    // 而不是先被一个端口错误引到错误的方向。
    //
    // ★ 一次读齐「客户端侧事实」（身份 + 安装元数据 + 系统信息）：授权 URL 与
    // 兑换请求体都**只**从这里取值，两处不可能各说一套（见 [`ClientFacts`]）。
    let facts = ClientFacts::load_for(variant)?;

    // 绑**固定端口**（见 [`CALLBACK_PORT`] 的说明）：授权页会来探这个端口判断
    // 客户端在线，随机端口会让它永远探不到、流程永久卡在「认证中」。
    //
    // 端口被占时**不做随机端口回退**：回退会重新引入同一个卡死症状，
    // 且用户拿到的错误信息会变成「一切正常但就是没反应」。宁可在这里失败。
    let listener =
        tokio::net::TcpListener::bind::<SocketAddr>(([127, 0, 0, 1], CALLBACK_PORT).into())
            .await
            .map_err(|e| classify_error("portBusy", &e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("读取监听端口失败: {e}"))?
        .port();

    // 授权 URL 的 `machine_id` **不再**在这里单独取值 —— 它由 [`ClientFacts::machine_id`]
    // 与请求体的 `DeviceInfo.MachineID` 共用同一个派生点（改造前两处各取一份，
    // URL 用自造值、请求体用 `telemetry.machineId`，上游回 20403）。
    let trace_id = icube::random_hex(32);
    let (pkce_verifier, code_challenge) = pkce_pair();
    let login_id = format!("trae_{}", uuid::Uuid::new_v4().simple());
    let cancel = Arc::new(Notify::new());

    {
        let mut sessions = login_sessions().lock().unwrap();
        // 顺手回收**已到终态但没人再轮询**的会话。
        //
        // 为什么会有这种残留：用户在轮询出结果前就关掉了对话框（取消 / 超时），
        // 前端的轮询随之停止，`login_poll` 的「读到终态就摘除」就再也不会执行。
        // 单条只占几十字节，但这是个会随使用次数单调增长的表，没有自愈机制。
        // 在「发起新登录」这个天然边界上清理，既不打扰活跃会话，
        // 也保证同一用户连续登录不会无限堆积。
        sessions.retain(|_, session| !session.done);
        sessions.insert(
            login_id.clone(),
            LoginSession {
                port,
                variant,
                trace_id: trace_id.clone(),
                pkce_verifier,
                cancel: cancel.clone(),
                ..Default::default()
            },
        );
    }

    let authorize_url = build_authorize_url(variant, &facts, port, &trace_id, &code_challenge);
    let session_id = login_id.clone();
    // 客户端事实随监听任务一起搬进去：回调到达时要拿**同一份**去填 `DeviceInfo`
    // （`DeviceID` / `MachineID` / `PlatformCode` / 系统信息）。
    let facts = Arc::new(facts);

    // 监听任务：**循环 accept**，直到拿到真正的回调凭据、超时或被取消。
    //
    // ## 为什么是循环而不是「接一条就收工」
    //
    // 固定端口 + 上游会来探测，意味着这个端口上**先到的多半不是回调**：
    // 授权页在「认证中」阶段会发一个不带任何凭据的探测请求（可能还不止一次，
    // 也可能会发 OPTIONS 预检）。若沿用「接一条就当回调处理」的旧写法，
    // 第一个探测请求就会被当成「回调里没有凭据」而**立刻判失败**。
    //
    // 因此：**只把「带凭据参数的请求」当回调**，其余一律回 200 继续等。
    tokio::spawn(async move {
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(TRAE_OAUTH_LOGIN_TIMEOUT_SECONDS);

        loop {
            let accepted = tokio::select! {
                result = listener.accept() => result,
                _ = tokio::time::sleep_until(deadline) => {
                    finish_session(&session_id, None, Some(classify_error("timeout", "")));
                    return;
                }
                // 用户关掉对话框：立刻收工释放端口。**不能只改标志位就 return 出去**——
                // 那样 listener 会一直被持有到超时，端口 300 秒不释放。
                _ = cancel.notified() => {
                    finish_session(&session_id, None, Some("已取消".into()));
                    return;
                }
            };

            let Ok((mut stream, _peer)) = accepted else {
                finish_session(&session_id, None, Some("回调连接失败".into()));
                return;
            };

            // 读请求头即可（只有一行 `GET /authorize?... HTTP/1.1`）。
            // 上限 8KB：恶意/异常客户端不能靠长请求把内存撑爆。
            let mut buf = vec![0u8; 8192];
            let read = match tokio::time::timeout(
                Duration::from_secs(10),
                tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
            )
            .await
            {
                Ok(Ok(n)) => n,
                // **单条连接读失败不终止整个登录**：探测方的连接可能被代理掐断、
                // 或发完就关。继续 accept 才是对的——真正的回调还在后面。
                _ => continue,
            };

            let request = String::from_utf8_lossy(&buf[..read]).into_owned();
            let (method, target) = request
                .lines()
                .next()
                .and_then(|line| {
                    let mut parts = line.split_whitespace();
                    Some((parts.next()?.to_string(), parts.next()?.to_string()))
                })
                .unwrap_or_default();
            let query = target
                .split_once('?')
                .map(|(_, q)| q.to_string())
                .unwrap_or_default();
            let params = parse_query(&query);

            let view = session_view(&session_id);

            // 会话可能已被取消——此时**绝不能继续兑换凭据并写库**。
            //
            // 这道检查覆盖一个真实的竞态：用户点了「授权」之后又关掉对话框（或另开一页
            // 重新发起登录），浏览器随后才把回调打过来。若只看「端口上来了请求」就落库，
            // 用户会看到一个自己已经取消的登录突然多出一个账号。
            if view.cancelled {
                let page = result_page(
                    PageKind::Cancelled,
                    "已取消",
                    &["本次登录已取消，可以关闭本页。".to_string()],
                );
                let _ = respond_html(&mut stream, &page, &method).await;
                return;
            }

            // 用户可能点了「拒绝授权」，此时回调只有 error 没有凭据。
            // 注意：**必须先于「探测请求」判定**——带 `error=` 才是真正的拒绝回调，
            // 而不带任何参数的才是探测。两者都以「无凭据」为特征，
            // 只靠凭据有无区分会把拒绝回调误当成探测、让用户白等。
            if let Some(err) = params.get("error").filter(|e| !e.trim().is_empty()) {
                // 拒绝是**用户自己的动作**，不是故障：用中性文案，不回显上游错误码
                // （`error=` 的值是浏览器可控的，进页面只会变成噪音，且必须转义才安全）。
                let page = result_page(
                    PageKind::Cancelled,
                    "已取消授权",
                    &["你在授权页拒绝了本次登录，可以关闭本页。".to_string()],
                );
                let _ = respond_html(&mut stream, &page, &method).await;
                finish_session(
                    &session_id,
                    None,
                    Some(format!("授权被拒绝: {err}")),
                );
                return;
            }

            if !has_credential_marker(&params) {
                // ---- 到这里就是「上游的在线探测请求」，不是回调 ----
                //
                // 授权页靠这个 200 确认「客户端在线」，探不到就永远停在「认证中」。
                // 回一句 `ok` 并**继续等待**；响应的 CORS 头由 `respond` 统一附加
                // （授权页是 https 源，跨源探测没有 CORS 头会被浏览器拦掉响应，
                // 官网同样会认为客户端不在线）。
                let _ = respond(&mut stream, "ok", &method).await;
                continue;
            }

            // CSRF：期望值来自在途会话（无会话时宽容放行）。
            let expected = (!view.trace_id.is_empty()).then_some(view.trace_id.as_str());
            if let Err(error) = verify_login_trace(&params, expected) {
                let page = result_page(
                    PageKind::Failure,
                    "登录失败",
                    &["登录校验失败，请回到应用重新授权。".to_string()],
                );
                let _ = respond_html(&mut stream, &page, &method).await;
                finish_session(&session_id, None, Some(error));
                return;
            }

            let callback = match parse_callback(&params) {
                Ok(callback) => callback,
                Err(detail) => {
                    let page = result_page(
                        PageKind::Failure,
                        "登录失败",
                        &["登录信息不完整，请回到应用重新授权。".to_string()],
                    );
                    let _ = respond_html(&mut stream, &page, &method).await;
                    finish_session(&session_id, None, Some(classify_error("callback", &detail)));
                    return;
                }
            };

            if callback.auth_code.is_none() && callback.refresh_token.is_none() {
                // 有凭据标记却两种凭据都没有：明确失败，不要空等到 300 秒。
                let page = result_page(
                    PageKind::Failure,
                    "登录失败",
                    &["登录信息不完整，请回到应用重新授权。".to_string()],
                );
                let _ = respond_html(&mut stream, &page, &method).await;
                finish_session(
                    &session_id,
                    None,
                    Some(classify_error(
                        "callback",
                        "回调带了凭据参数，但既无 AuthCode 也无 refreshToken",
                    )),
                );
                return;
            }

            // ★ 结果页在**兑换之后**才回，不在兑换之前。
            //
            // 两个理由，任一都足以定案：
            //
            // 1. 页面要写「账号 [昵称] 登录成功」，而昵称只有兑换成功才拿得到
            //    （参考实现同样是在 `oauth_login` 之后才渲染结果页）；
            // 2. **先回页会让兑换失败时也显示「登录成功」** —— 页面在说谎，
            //    而用户此刻正盯着它，只会以为账号已经加好了。
            //
            // 代价是浏览器多等一次 ExchangeToken 往返（通常 1~2 秒）：
            // 换来的是「页面说的结果 == 实际结果」。
            match perform_login(view.variant, &callback, &view.pkce_verifier, &facts).await {
                Ok(account_view) => {
                    let page = success_page(&account_view, view.variant);
                    // 先落终态再写浏览器页：应用侧的轮询结果不该被「写浏览器响应」这一步拖住
                    // （浏览器提前断开时 write_all 会失败，但那只影响那一页）。
                    finish_session(&session_id, Some(account_view), None);
                    let _ = respond_html(&mut stream, &page, &method).await;
                }
                Err(error) => {
                    let page = result_page(
                        PageKind::Failure,
                        "登录失败",
                        &[error.clone(), "请回到应用重新发起登录。".to_string()],
                    );
                    finish_session(&session_id, None, Some(error));
                    let _ = respond_html(&mut stream, &page, &method).await;
                }
            }
            return;
        }
    });

    Ok(json!({
        "loginId": login_id,
        "verificationUri": authorize_url,
        "expiresIn": TRAE_OAUTH_LOGIN_TIMEOUT_SECONDS,
        "port": port,
        "variant": variant.as_str(),
        "variantLabel": variant.display_name(),
        // 脱敏诊断：只含白名单字段，私钥不可能出现（见 icube::credential_status_value）。
        "deviceCredential": icube::credential_status_for(variant),
    }))
}

/// 极简 HTTP 响应（`text/plain`）。**只用于应答上游的在线探测**。
///
/// ## 为什么必须回 CORS 头
///
/// 授权页是**网页域**（国内版 `https://www.trae.cn`、国际版 `https://www.trae.ai`，
/// 见 [`crate::modules::trae::endpoints_for`] 的 `console_base`），它对本机端口
/// （`http://127.0.0.1:17388`）的在线探测属于**跨源请求**。没有
/// `Access-Control-Allow-Origin` 时浏览器会把响应拦在 JS 之外，授权页因此判不出
/// 「客户端在线」——**表现与端口没监听完全一样，都是永久卡在「认证中」**。
/// 这一条极易漏掉：用 curl 测是通的，只有浏览器会失败。
///
/// ⚠️ 回的是 `*` 而**不是**某个具体域：两个区域的授权页都会来探，写死一个域
/// 会让另一个区域的登录卡在「认证中」（这正是「域按区域分家」后新增的坑，
/// 好在 `*` 天然免疫 —— 别为了"收紧"改成白名单）。
///
/// `OPTIONS` 预检也一并回应（`Access-Control-Allow-Methods` 覆盖到）。
/// 响应的 `Content-Type` 用 `text/plain` 而非 `text/html`：探测方只关心状态码，
/// 而工具链（含各种代理）对 text/html 有额外的嗅探与安全头推断。
///
/// ⚠️ **用户看得见的结果页不走这里**，走 [`respond_html`]：
/// 纯文本会被浏览器渲染成左上角一行小字，用户看不出这是登录流程的一部分
/// （见 [`super::oauth_result_page`] 模块头）。
async fn respond(
    stream: &mut tokio::net::TcpStream,
    message: &str,
    method: &str,
) -> std::io::Result<()> {
    write_response(stream, "text/plain; charset=utf-8", message, method).await
}

/// 结果页响应（`text/html`）。**用户可见的每一页都走这里**。
///
/// 与 [`respond`] 只差 `Content-Type`——`text/plain` 在浏览器里是纯文本，
/// 卡片布局不可能生效。
async fn respond_html(
    stream: &mut tokio::net::TcpStream,
    html: &str,
    method: &str,
) -> std::io::Result<()> {
    write_response(stream, "text/html; charset=utf-8", html, method).await
}

/// 两种响应的公共部分：CORS 头、`Content-Length`（**字节数**，非字符数）、连接关闭。
async fn write_response(
    stream: &mut tokio::net::TcpStream,
    content_type: &str,
    body: &str,
    method: &str,
) -> std::io::Result<()> {
    let response = build_response(content_type, body, method);
    tokio::io::AsyncWriteExt::write_all(stream, response.as_bytes()).await?;
    tokio::io::AsyncWriteExt::flush(stream).await
}

/// 组装完整响应报文（**纯函数**，可单测）。
///
/// ## `Content-Length` 必须是**字节数**
///
/// 结果页正文全是中文（一个汉字 3 字节）。若写成字符数，浏览器会**按声明长度截断**
/// 响应体 ⇒ 卡片只渲染一半、`</html>` 之后的字节被丢掉，而**服务端一切正常**——
/// 这类缺陷用 `read_to_string` 读到 EOF 的集成测试**抓不到**（连接关了，
/// 长度对不对它不看），只有真浏览器会暴露。故这里把它钉成纯函数单测。
///
/// `Cache-Control: no-store`：结果页含账号昵称（个人信息），且回调 URL 是**一次性**的
/// （查询串里带 AuthCode）；不让浏览器留副本。
fn build_response(content_type: &str, body: &str, method: &str) -> String {
    // 预检请求不要 body，回一组头即可。
    let (status, body) = if method.eq_ignore_ascii_case("OPTIONS") {
        ("204 No Content", "")
    } else {
        ("200 OK", body)
    };

    format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Cache-Control: no-store\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: *\r\n\
         Access-Control-Max-Age: 600\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// 应用展示名（三层命名里的「**展示名**」层：带空格的 `Buddy Switch`）。
///
/// ⚠️ 本仓**刻意不做跨语言共享常量**，因此这个字面量在另外两处也各有一份载体，
/// 三处必须同值：
/// `src-tauri/src/tray.rs` 的 `DEFAULT_TOOLTIP`、
/// `src/locales/domains/shell.zh.ts` / `shell.en.ts` 的 `app.name`。
/// 按 i18n 约定它是**产品名，不译**。
const APP_DISPLAY_NAME: &str = "Buddy Switch";

/// 结果页里的产品名：**Trae 模块整体**（含 TraeWork 与 Trae 两条产品线）。
///
/// 与 `TraeVariant::display_name()`（程序位名：`Trae Work` / `Trae`）不同 ——
/// 本页要说的是「账号进了哪个产品的库」，而两条产品线**共用同一本国内库**。
const TRAE_PRODUCT_NAME: &str = "Trae";

/// 成功结果页：账号昵称 + 落库位置（应用 → 产品 → 区域）。
///
/// ## 为什么写「区域」而不是 `variant.display_name()`
///
/// 持久化轴是**区域**（`TraeWork` 与 `Trae` 共用国内库）。页面说「账号已添加到哪」，
/// 就必须按**账号库**那一轴说：写成程序名会出现「已添加到 Trae Work」，
/// 而用户在 Trae 分区里也看得到这个账号（本来就是同一本库）——用户会以为提示在骗人。
///
/// ## 为什么还要带上应用名与产品名（2026-09-24 用户报障后补）
///
/// 这一页是**浏览器里的页**，脱离了应用上下文。只写「国内版」有两个歧义：
///
/// - 没说**是哪个应用**——本应用的 WorkBuddy 分区也有一个「国内版」；
/// - 没说**是哪个产品**——本应用同时管 WorkBuddy 与 Trae 两族账号。
///
/// 用户的原话是「这里显示不对」（截图里圈着「国内版」）⇒ 页面必须自证身份：
/// 「账号已添加到 Buddy Switch 的 Trae（国内版）账号库」。
/// 三段的顺序刻意是**由外到内**（应用 → 产品 → 区域），与用户在界面上选东西的
/// 顺序一致（先选应用分区，再选区域）。
fn success_page(account_view: &Value, variant: TraeVariant) -> String {
    // 昵称可能为空（上游没给）；退回 userId 与前端 `result.name || result.userId` 同款，
    // 两处显示同一个名字，用户才不会怀疑「加错账号了」。
    let name = account_view
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .or_else(|| account_view.get("userId").and_then(Value::as_str))
        .unwrap_or("未知账号")
        .to_string();

    result_page(
        PageKind::Success,
        "登录成功",
        &[
            format!("账号 [{name}] 登录成功"),
            format!(
                "账号已添加到 {APP_DISPLAY_NAME} 的 {TRAE_PRODUCT_NAME}（{}）账号库，\
                 可关闭此页面返回应用。",
                variant.region().display_name()
            ),
        ],
    )
}

/// 用回调里的凭据走完「换 JWT → 落盘」，返回账号视图。
///
/// - **主路径**：`authCodeInfo.AuthCode` ⇒ `exchange_auth_code` ⇒ 落库；
/// - **兼容路径**：回调直接给 `refreshToken`（老形态）⇒ 走 refresh 变体链 ⇒ 落库。
/// `identity` 由 `login_start_for` 在**发起登录时**就取好并随会话带下来：
/// 授权 URL 的 `device_id`、`DeviceInfo.DeviceID`、`x-device-id` 必须是**同一个值**
/// （三方同源），回调到达时再取一次就会多出一个可能不一致的来源。
async fn perform_login(
    variant: TraeVariant,
    callback: &CallbackInfo,
    pkce_verifier: &str,
    facts: &ClientFacts,
) -> Result<Value, String> {
    let (jwt, refresh_token) = if let Some(auth_code) = callback.auth_code.as_deref() {
        let (host, used_fallback) = resolve_exchange_host(callback.host.as_deref(), variant);
        if used_fallback {
            // 脱敏留痕：只有 host 与变体名，没有凭据。
            store::append_log(
                &crate::modules::trae::paths::checkin_log_file_for(variant),
                &format!(
                    // 文案必须跟着 [`resolve_exchange_host`] 的回落值走：它已从
                    // `icube_base` 改为 `account_base`（真机客户端打的就是后者）。
                    // 留旧词会让排障的人去查一台根本没被访问的主机。
                    "OAuth 回调未回传 host，回落【{}】的 account_base（{host}）",
                    variant.display_name()
                ),
            );
        }
        // AuthCode 路径**只接受 `&ClientFacts`**（其 `identity` 没有私钥字段 ⇒ 类型层面
        // 不可能在这条路径上签名），且这里**不发 DeviceProof**（见模块头红线）。
        let exchanged = exchange_auth_code(variant, &host, auth_code, pkce_verifier, facts).await?;
        (exchanged.jwt, exchanged.refresh_token)
    } else if let Some(refresh_token) = callback.refresh_token.as_deref() {
        // 兼容路径（回调直接给 refreshToken）：此刻**还没有**账号绑定 ——
        // 绑定由下面 `login_with_exchanged_tokens_for` 在落库时写入。
        // 故这里传 `None`：`exchange_token_for` 会回落「当前目录的那一条」并留痕。
        //
        // ⚠️ 这个 `None` 是**必须**的，**不是漏改**（R6 裁定）：若改成
        // `Some(&identity.device_id)`，当本机没有该 id 的 `icube-dc` 条目时，行为会从
        // 「用当前目录的那一条」变成「**无设备凭证**」—— 即从「可能签错名」变成
        // 「连签名都没有」，凭空新增一种失败模式。且那一刻绑定尚未落库，
        // `Some` 拿不到任何比 `None` 更可信的东西。
        let exchanged = account::exchange_token_for(variant, refresh_token, None)
            .await
            .map_err(|error| account::refresh_error_message(&error))?;
        // 上游没轮换就沿用回调给的那个（绝不写成 None：那会让刚登录的账号
        // 立刻失去自动续期能力）。
        (
            exchanged.jwt,
            exchanged
                .refresh_token
                .or_else(|| Some(refresh_token.to_string())),
        )
    } else {
        return Err(classify_error("callback", "回调既无 AuthCode 也无 refreshToken"));
    };

    let (raw, _jwt) = account::login_with_exchanged_tokens_for(
        variant,
        jwt,
        refresh_token,
        callback.display_name.clone(),
        // 登录时实际使用的那台设备（授权 URL 的 `device_id` / `DeviceInfo.DeviceID` /
        // `x-device-id` 三方同源的那个值）⇒ 落成账号绑定，续期复用它签名。
        Some(facts.identity.device_id.as_str()),
    )?;
    let uid = account::resolve_user_id(&raw);
    // jwt 已经写进 `raw.jwt`（落盘源就是它），这里的解析只用于日志里的到期时间。
    let exp_hours = crate::modules::trae::jwt::parse(&raw.jwt)
        .exp_hours
        .map(|hours| format!("{hours:.1}h"))
        .unwrap_or_else(|| "?".into());

    store::append_log(
        &crate::modules::trae::paths::checkin_log_file_for(variant),
        &format!(
            "OAuth 登录成功【{}】: user={uid} name={} jwt到期={exp_hours}",
            variant.display_name(),
            raw.name
        ),
    );

    // 视图必须走 `account_view` 单点构造（两套形态的转换只允许发生在那儿）。
    // 刚登录的账号必然无分组、无积分缓存、无冷却，`checkedToday` 也一定是 false，
    // 故这些都是 `None` / `false`；设备标识用 `device_id` 现算，
    // `ensure_for` 落盘发生在首次签到或刷新时，这里不提前写。
    let device_id = device::derive(&uid).device_id;
    Ok(account::account_view(
        &raw,
        &uid,
        None,
        Some(&device_id),
        None,
        None,
        None,
        false,
        None,
    ))
}

/// 记录一次会话的终态（成功 / 失败）。
fn finish_session(login_id: &str, result: Option<Value>, error: Option<String>) {
    let mut sessions = login_sessions().lock().unwrap();
    if let Some(session) = sessions.get_mut(login_id) {
        session.done = true;
        session.result = result;
        session.error = error;
    }
}

/// 轮询一次登录状态。
///
/// 与 WorkBuddy 侧同构地返回 `{ done, account?, error?, port?, variant }`：
/// **永不返回 Err**——轮询是高频调用，把「还没好」表达成异常会让前端要靠抛错驱动循环。
///
/// `variant` 在两个分支都要带出：终态分支必须在 `sessions.remove()` **之前**取值，
/// 否则调用方（`handlers::oauth_login_status`）只能回落到默认变体，
/// 表现为「Trae CN 登录成功却把 Trae Work 的账号列表刷出来」。
pub fn login_poll(login_id: &str) -> Value {
    let mut sessions = login_sessions().lock().unwrap();
    let Some(session) = sessions.get_mut(login_id) else {
        return json!({"done": true, "error": "登录请求不存在或已过期"});
    };

    let variant = session.variant.as_str();

    // 轮询到终态后把会话摘掉：凭据已落盘，内存里没有必要留着。
    if session.done {
        let result = session.result.take();
        let error = session.error.take();
        let port = session.port;
        sessions.remove(login_id);
        return match (result, error) {
            (Some(account), _) => {
                json!({"done": true, "account": account, "port": port, "variant": variant})
            }
            (None, Some(error)) => {
                json!({"done": true, "error": error, "port": port, "variant": variant})
            }
            (None, None) => json!({
                "done": true,
                "error": "登录已结束但未取得结果",
                "port": port,
                "variant": variant
            }),
        };
    }

    json!({"done": false, "port": session.port, "variant": variant})
}

/// 取消一次登录（用户关掉了对话框）。
///
/// 返回是否真的取消到了一个进行中的会话，便于前端给出准确反馈。
///
/// 除了置终态标志，还会 `notify_one()` 唤醒监听任务让它立刻释放端口——
/// 见 [`LoginSession::cancel`] 的说明。
pub fn login_cancel(login_id: &str) -> bool {
    let cancel = {
        let mut sessions = login_sessions().lock().unwrap();
        match sessions.get_mut(login_id) {
            Some(session) if !session.done => {
                session.done = true;
                session.cancelled = true;
                session.error = Some("已取消".into());
                Some(session.cancel.clone())
            }
            _ => None,
        }
    };
    // 在**锁外**通知：`Notified` 的等待方可能马上要再取同一把锁（`finish_session`），
    // 持锁发通知会把自己和它串成一次无谓的等待。
    if let Some(cancel) = cancel {
        cancel.notify_one();
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::trae::test_support::TempEnv;

    /// 造一个合成的 [`DeviceIdentity`]（无私钥字段，AuthCode 路径唯一需要的结构）。
    fn synthetic_identity() -> DeviceIdentity {
        DeviceIdentity {
            variant: TraeVariant::TraeWork,
            device_id: "2292929806738024".into(),
            machine_id: "mach-1".into(),
            app_version: "1.107.1".into(),
            public_key_pem: "-----BEGIN PUBLIC KEY-----\nMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEc5xtFi4XpzYjFuYwN0sBaUzcnrds\n8NWA0zU4mSNC+HoBUNCM5VD4K2SvzuZ8JJjSUdDLpFmR+zYL3aAy7EtDQQ==\n-----END PUBLIC KEY-----".into(),
            source_app: "TRAE SOLO CN".into(),
        }
    }

    /// 造一个指定 `device_id` 的合成身份（授权 URL / 同源护栏用）。
    fn identity_with_device_id(device_id: &str) -> DeviceIdentity {
        DeviceIdentity {
            device_id: device_id.into(),
            ..synthetic_identity()
        }
    }

    /// 造一份「客户端侧事实」（合成身份 + **固定**的安装元数据与系统信息）。
    ///
    /// ★ 刻意用固定值、不读本机：本组用例要断言的是「授权 URL 与兑换请求体**同源**」，
    /// 而不是「本机现在是什么」。读本机会让用例随机器漂，也无法钉住取值来源。
    /// 固定值逐字取自 2026-09-24 真机抓到的客户端原文（`TRAE SOLO CN`）。
    fn synthetic_facts(identity: DeviceIdentity) -> ClientFacts {
        ClientFacts {
            identity,
            meta: ClientInstallMeta {
                app_version: "0.1.69".into(),
                build_version: "2.3.87413".into(),
                channel: "stable".into(),
            },
            system: SystemProfile {
                // 真机是 `Jackey的电脑`；本实现留空（见 `platform::system_profile`）。
                device_name: String::new(),
                device_model: "System Product Name".into(),
                device_manufacturer: "ASUS".into(),
                cpu_brand: "Intel(R) Core(TM) i9-14900K".into(),
                os_name: "windows".into(),
                os_version: "Windows 11 Pro".into(),
            },
        }
    }

    /// 造一个真实形态的 Trae JWT。
    fn make_jwt(user_id: &str) -> String {
        let payload = serde_json::json!({
            "data": { "id": user_id },
            "exp": chrono::Utc::now().timestamp() + 48 * 3600,
        });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("header.{encoded}.signature")
    }

    #[test]
    fn parse_query_handles_flat_params() {
        let params = parse_query("refreshToken=abc123&userInfo={\"nickname\":\"x\"}");
        assert_eq!(params.get("refreshToken").map(String::as_str), Some("abc123"));
    }

    #[test]
    fn parse_query_decodes_double_encoded_values() {
        // 真实场景：浏览器把参数值整体编码一次，值里的 `=` 成了 `%3D`、`&` 成了 `%26`。
        // 参数名不受影响；切分发生在**解码之前**，所以值里的 `&` 不会被误切成新参数。
        let encoded = "refreshToken%3Dtok%2526x%3D1&userInfo=%7B%22nickname%22%3A%22y%22%7D";
        let params = parse_query(encoded);
        assert_eq!(params.get("refreshToken").map(String::as_str), Some("tok&x=1"));
        assert_eq!(
            params.get("userInfo").map(String::as_str),
            Some(r#"{"nickname":"y"}"#)
        );
    }

    #[test]
    fn parse_query_decodes_repeatedly_but_terminates() {
        // 编码层数不确定（浏览器实现有差异），必须解到不动点为止，
        // 且不能因为多解一层就把合法内容吃掉。
        let params = parse_query("a=%25253D");
        assert_eq!(params.get("a").map(String::as_str), Some("="));
        // 不成对的 `%` 必须原样保留，不能被吞掉导致值变短。
        let params = parse_query("b=100%");
        assert_eq!(params.get("b").map(String::as_str), Some("100%"));
        // `+` 视作空格（form 编码约定）。
        let params = parse_query("c=a+b");
        assert_eq!(params.get("c").map(String::as_str), Some("a b"));
    }

    #[test]
    fn extract_refresh_token_accepts_both_namings_and_quotes() {
        let mut params = HashMap::new();
        params.insert("refresh_token".into(), "\"quoted-token\"".into());
        assert_eq!(
            extract_refresh_token(&params).as_deref(),
            Some("quoted-token")
        );

        let mut params = HashMap::new();
        params.insert("RefreshToken".into(), "  spaced  ".into());
        assert_eq!(extract_refresh_token(&params).as_deref(), Some("spaced"));

        assert_eq!(extract_refresh_token(&HashMap::new()), None);
    }

    #[test]
    fn extract_user_info_tolerates_garbage() {
        let mut params = HashMap::new();
        params.insert("userInfo".into(), "not-json".into());
        // 解析失败必须返回空对象而不是报错：登录不应因展示名缺失而失败。
        assert_eq!(extract_user_info(&params), json!({}));

        let mut params = HashMap::new();
        params.insert("userInfo".into(), r#"{"nickname":"小明"}"#.into());
        assert_eq!(
            extract_user_info(&params).get("nickname").and_then(|v| v.as_str()),
            Some("小明")
        );
    }

    #[test]
    fn pick_display_name_prefers_nickname_over_email() {
        let info = json!({"email": "a@b.c", "nickname": "小明"});
        assert_eq!(pick_display_name(&info).as_deref(), Some("小明"));

        // 空串与纯空白不算有效名字
        let info = json!({"nickname": "   ", "name": "真名"});
        assert_eq!(pick_display_name(&info).as_deref(), Some("真名"));

        assert_eq!(pick_display_name(&json!({})), None);

        // 真实回调字段（抓包固化）：`ScreenName` / `NickName` 必须被识别，
        // 且 `ScreenName` 优先（它才是客户端实际展示的名字）。
        let info = json!({"NickName": "昵称", "ScreenName": "屏幕名"});
        assert_eq!(pick_display_name(&info).as_deref(), Some("屏幕名"));
        assert_eq!(
            pick_display_name(&json!({"NickName": "昵称"})).as_deref(),
            Some("昵称")
        );
    }

    // -----------------------------------------------------------------------
    // 授权 URL（22 参数，逐键对拍；**按产品线**分成两种形态）
    // -----------------------------------------------------------------------

    /// ★ 参数**逐键**对拍（键名与固定值全等），并**按产品线**分别对拍。
    ///
    /// 出处：抓包固化 2026-09-16，
    /// `reference/TraeWorkAssistant-main/src-tauri/src/commands/oauth.rs:321-355`。
    ///
    /// ## 为什么必须分成两条线对拍
    ///
    /// 抓包固化的是 **TRAE / IDE 线**（参考自述「真实 Trae **IDE** 登录 URL 实证值」），
    /// 即 `auth_from=trae` + `client_id=ono9krqynydwx5` + **22 参数**。
    /// SOLO 线（TraeWork / 国际版）在客户端里走的是**另一处分支**：
    /// `auth_from=solo` + `client_id=en1oxy7wnw8j9n` + 末尾追加 `hide_saas_login=true`
    /// ⇒ **23 参数**。把 IDE 那套照抄给 SOLO 线，授权页会停在 billing status 后不回跳。
    #[test]
    fn authorize_url_carries_all_native_ide_params() {
        let identity = identity_with_device_id("dev");
        let facts = synthetic_facts(identity.clone());

        // (变体, auth_from, client_id, 是否带 hide_saas_login)
        let cases: [(TraeVariant, &str, &str, bool); 3] = [
            // TRAE / IDE 线：与抓包固化值逐字一致（**零行为变化**的回归护栏）。
            (TraeVariant::Trae, "trae", "ono9krqynydwx5", false),
            // SOLO 线：CN 与 国际版**同属 SOLO 线**（`packageType` 都是 SOLO_*）。
            (TraeVariant::TraeWork, "solo", "en1oxy7wnw8j9n", true),
            (TraeVariant::Global, "solo", "en1oxy7wnw8j9n", true),
        ];

        for (variant, auth_from, client_id, hide_saas) in cases {
            let url = build_authorize_url(variant, &facts, 12345, "trace-1", "challenge-1");
            assert!(
                url.starts_with(&format!(
                    "{}{AUTHORIZE_PATH}",
                    endpoints_for(variant).console_base
                )),
                "{variant:?} 的授权页基址漂了: {url}"
            );
            let query = url.split_once('?').expect("授权 URL 必须带查询串").1;
            let params = parse_query(query);

            // ★ 这些「固定值」全部来自 `synthetic_facts` 注入的**客户端事实**，
            // 不再是散落的常量 —— 断言因此同时钉住了「URL 取值来自哪一份事实」。
            let expected: [(&str, &str); 22] = [
                ("login_version", "1"),
                ("auth_from", auth_from),
                ("login_channel", "native_ide"),
                ("plugin_version", "2.3.87413"),
                ("auth_type", "local"),
                ("client_id", client_id),
                ("redirect", "0"),
                ("login_trace_id", "trace-1"),
                ("auth_callback_url", "http://127.0.0.1:12345/authorize"),
                // ★ `machine_id` 必须**逐字**等于设备身份里的 `telemetry.machineId`
                // （合成身份是 `mach-1`），而不是任何自造值 —— 改造前这里放的是
                // `oauth_device.json` 里那个自造值，上游因此回 20403。
                ("machine_id", "mach-1"),
                ("device_id", "dev"),
                ("x_device_id", "dev"),
                ("x_machine_id", "mach-1"),
                ("x_device_brand", "System Product Name"),
                ("x_device_type", "windows"),
                ("x_os_version", "Windows 11 Pro"),
                ("x_env", ""),
                ("x_app_version", "0.1.69"),
                ("x_app_type", "stable"),
                ("code_challenge", "challenge-1"),
                ("code_challenge_method", "S256"),
                ("channel_name", "common"),
            ];
            for (key, value) in expected {
                assert_eq!(
                    params.get(key).map(String::as_str),
                    Some(value),
                    "{variant:?} 的参数 {key} 不匹配；完整 URL={url}"
                );
            }
            // SOLO 线多一个从属参数 `hide_saas_login`（客户端：`auth_from==="solo"` 时追加）。
            if hide_saas {
                assert_eq!(
                    params.get("hide_saas_login").map(String::as_str),
                    Some("true"),
                    "{variant:?} 缺 hide_saas_login（solo 线必须带）: {url}"
                );
                assert!(
                    url.ends_with("&hide_saas_login=true"),
                    "hide_saas_login 必须追加在**最末**（与客户端拼串顺序一致）: {url}"
                );
            } else {
                assert!(
                    !url.contains("hide_saas_login"),
                    "{variant:?} 不该带 hide_saas_login（那是 solo 线专属）: {url}"
                );
            }
            assert_eq!(
                params.len(),
                if hide_saas { 23 } else { 22 },
                "{variant:?} 参数个数不对（多一个少一个都会让授权页行为改变）: {:?}",
                params.keys().collect::<Vec<_>>()
            );

            // 回调地址必须**编码**：不编码时 `http://` 里的 `:` `/` 会让上层解析错位。
            assert!(
                url.contains("auth_callback_url=http%3A%2F%2F127.0.0.1%3A12345%2Fauthorize"),
                "回调地址未编码: {url}"
            );
            assert!(!url.contains("auth_callback_url=http://"));
            // `x_env=` 必须保留这一对（空值也要出现）。
            assert!(url.contains("&x_env=&"), "x_env 空参数对丢失: {url}");
        }
    }

    /// ★★ 交换请求体的 `ClientID` 必须与授权 URL 用的**同一把钥匙**。
    ///
    /// 两处若各取各的（一处按产品线、一处写死），等于拿 A 线的钥匙去兑 B 线签发的
    /// AuthCode —— 上游只会拒绝，且错误信息不会指向这个根因。
    #[test]
    fn exchange_client_id_matches_authorize_url_key() {
        for variant in [TraeVariant::TraeWork, TraeVariant::Trae, TraeVariant::Global] {
            let url = build_authorize_url(
                variant,
                &synthetic_facts(synthetic_identity()),
                1,
                "t",
                "c",
            );
            let params = parse_query(url.split_once('?').unwrap().1);
            let url_key = params.get("client_id").expect("授权 URL 必须带 client_id");
            assert_eq!(
                url_key.as_str(),
                oauth_client().client_id_for(variant.oauth_line()),
                "{variant:?}：授权 URL 的 client_id 与「按产品线取值」的入口不一致"
            );
        }
    }

    /// ★★ 授权页的**两个轴**不能混为一谈（本轮两个缺陷的判别式）。
    ///
    /// - **区域轴**决定**域**：`TraeWork`(CN) 与 `Global`(国际) 的域必须不同；
    /// - **产品线轴**决定 `auth_from` / `client_id`：`TraeWork` 与 `Global` 同属
    ///   SOLO 线 ⇒ 这两项**必须相同**；`Trae` 属 TRAE 线 ⇒ **必须不同**。
    ///
    /// 混轴的两种写法都曾真实发生过：域写死（区域轴漏了）、钥匙写死（产品线轴漏了）。
    #[test]
    fn authorize_url_splits_by_the_right_axis() {
        let url_of = |variant| {
            build_authorize_url(variant, &synthetic_facts(synthetic_identity()), 1, "t", "c")
        };
        let key_of = |variant| {
            let url = url_of(variant);
            parse_query(url.split_once('?').unwrap().1)
                .remove("client_id")
                .expect("client_id 必须存在")
        };

        // 区域轴：域必须分家。
        assert_ne!(
            endpoints_for(TraeVariant::TraeWork).console_base,
            endpoints_for(TraeVariant::Global).console_base,
            "区域轴漏了：两条区域的授权页域撞了"
        );
        // 产品线轴：同线的两个区域取值必须相同，跨线必须不同。
        assert_eq!(
            key_of(TraeVariant::TraeWork),
            key_of(TraeVariant::Global),
            "TraeWork 与 国际版同属 SOLO 线，client_id 必须相同"
        );
        assert_ne!(
            key_of(TraeVariant::TraeWork),
            key_of(TraeVariant::Trae),
            "产品线轴漏了：SOLO 线与 TRAE 线的 client_id 撞了"
        );
    }

    #[test]
    fn callback_path_matches_authorize_url() {
        // 构造与解析必须共用同一个路径常量，否则浏览器跳回来接不住。
        let url = build_authorize_url(
            TraeVariant::TraeWork,
            &synthetic_facts(synthetic_identity()),
            1,
            "t",
            "c",
        );
        assert!(url.contains(&format!("%2F{}", CALLBACK_PATH.trim_start_matches('/'))));
    }

    /// ★★ 授权页**域**必须随区域走（2026-09-21 修的真实缺陷）。
    ///
    /// 缺陷形态：域写死成 `https://www.trae.cn/authorization` 这一个常量，
    /// 而 URL 的**其余部分**（参数、设备身份、回调地址）全都随变体走 ⇒
    /// 只有「域」这一项没分家，国际版登录打开的是**国内版**授权页。
    /// 后果不是「报错」而是「静默走错账号体系」：用户在 CN 页面上登录，
    /// 即使走完也不会把凭据回调回国际版客户端，表现是永久停在「认证中」，
    /// 与「回调端口没监听」症状几乎一样 —— 极易被误判成端口/防火墙问题。
    ///
    /// 依据（**国际版客户端自述**，不是推断）：本机 `%LOCALAPPDATA%\Programs\TRAE SOLO`
    /// （`packageType = SOLO_I18N`）的 `product.json` → `bootConfig.consoleHost
    /// = "https://www.trae.ai"`；客户端 `out/main.js` 的 OAuth 构造段取
    /// `loginHost = bootConfig.consoleHost` 后拼 `/authorization?login_version=1…`。
    /// CN 侧同源：`TRAE SOLO CN` 与 `Trae CN` 的 `bootConfig.consoleHost`
    /// 都是 `https://www.trae.cn`（本机实测）。
    ///
    /// ⚠️ 本用例只覆盖**区域轴**；**产品线轴**（`auth_from` / `client_id`）由
    /// [`authorize_url_splits_by_the_right_axis`] 与
    /// [`authorize_url_carries_all_native_ide_params`] 覆盖 —— 两个轴要分开断言，
    /// 合成一条会让「哪一轴漏了」变得不可判读。
    #[test]
    fn 授权页域随区域分家() {
        let work = build_authorize_url(
            TraeVariant::TraeWork,
            &synthetic_facts(synthetic_identity()),
            1,
            "t",
            "c",
        );
        let global = build_authorize_url(
            TraeVariant::Global,
            &synthetic_facts(synthetic_identity()),
            1,
            "t",
            "c",
        );

        assert!(
            work.starts_with("https://www.trae.cn/authorization?"),
            "国内版授权页域漂了: {work}"
        );
        assert!(
            global.starts_with("https://www.trae.ai/authorization?"),
            "国际版授权页域漂了（必须用国际版客户端自述的 consoleHost）: {global}"
        );

        // 反例护栏：国际版 URL 里**任何位置**都不得出现国内域。
        // 只比前缀不够 —— 域可能出现在别处（例如某天有人把 console_base 塞进查询串）。
        assert!(
            !global.contains("trae.cn"),
            "国际版授权 URL 里出现了国内域: {global}"
        );
        assert!(
            !work.contains("trae.ai"),
            "国内版授权 URL 里出现了国际域: {work}"
        );

        // 两个区域的 URL 必须**只**在域上不同：其余 22 参数逐字相同。
        // 这条钉住「域分家」没有顺带改坏参数（参数是抓包固化值，改了就登录不上）。
        let strip = |url: &str| url.split_once('?').unwrap().1.to_string();
        assert_eq!(
            strip(&work),
            strip(&global),
            "两个区域的授权 URL 除域之外还出现了差异（参数是固化值，不能按区域改）"
        );
    }

    /// ★ 同源护栏（T4 验收 1b）：授权 URL 的 `device_id` / `x_device_id`
    /// **必须都等于** `identity.device_id`，且**不存在**第二个来源。
    ///
    /// 这是本轮最核心的一条红线：参考 `commands/oauth.rs:259-267` 明说
    /// 「登录 URL 的 device_id 必须与 icube 设备凭证同源，否则 20403/20405；
    /// 旧值是随机/`device_map` 对齐的，**与私钥不匹配**」。
    ///
    /// 结构性保证（比本断言更强）：`build_authorize_url` 的签名里**没有独立的
    /// `device_id` 形参** ⇒ 类型层面就传不进一个不同源的值。
    #[test]
    fn authorize_url_device_id_is_same_source_as_identity() {
        let identity = identity_with_device_id("2292929806738024");
        let url = build_authorize_url(
            TraeVariant::TraeWork,
            &synthetic_facts(identity.clone()),
            17388,
            "trace-1",
            "chal-1",
        );
        let params = parse_query(url.split_once('?').unwrap().1);

        assert_eq!(
            params.get("device_id").map(String::as_str),
            Some(identity.device_id.as_str()),
            "授权 URL 的 device_id 与设备身份不同源 ⇒ 必然 20403/20405"
        );
        assert_eq!(
            params.get("x_device_id").map(String::as_str),
            Some(identity.device_id.as_str()),
            "x_device_id 与 device_id 必须是同一个值"
        );
        // `machine_id` 是另一条身份：必须取自设备身份里的 `telemetry.machineId`
        // （合成身份 = `mach-1`）—— **不是** `oauth_device.json` 里那个自造值。
        // 改造前这里放的是自造值，于是与请求体的 `DeviceInfo.MachineID` 不一致，
        // 上游回 `20403/040036 Token device not match`。
        assert_eq!(
            params.get("machine_id").map(String::as_str),
            Some(identity.machine_id.as_str()),
            "授权 URL 的 machine_id 必须与设备身份同源（自造值 ⇒ 必然 20403）"
        );
        assert_ne!(
            params.get("machine_id"),
            params.get("device_id"),
            "machine_id 与 device_id 被写成了同一个值（它们是两条身份）"
        );
        // 说明：源码级「不存在自造 device_id 的调用路径」由 `cargo test` 之外的
        // `grep` 门禁核对（自引用断言写不出来——测试源码本身就会命中关键字）。
    }

    /// ★★ 本轮缺陷的**直接护栏**：授权 URL 与兑换请求体里，同一件事实只能有一个值。
    ///
    /// ## 现场（2026-09-24 用户报障）
    ///
    /// 上游回 `20403/040036: Token device not match`。根因是下面每一对都各取了一份来源：
    ///
    /// | 同源对 | 改造前授权 URL | 改造前请求体 |
    /// |:--|:--|:--|
    /// | 机器标识 | 自造值（`oauth_device.json`） | `telemetry.machineId` |
    /// | 应用版本 | IDE 线常量 `3.3.100` | 安装目录版本 `1.107.1` |
    /// | 设备型号 | `COMPUTERNAME` | 空串 |
    /// | 系统版本 | 写死 `Windows` | 空串 |
    ///
    /// 真机客户端的做法是「两处取同一个对象」—— 本用例把这条不变量钉死：
    /// **任一对改回「两处各取一份」都会立刻变红**，失败信息直接给出两侧的值。
    #[test]
    fn authorize_url_and_exchange_body_share_the_same_device_facts() {
        for variant in [TraeVariant::TraeWork, TraeVariant::Trae, TraeVariant::Global] {
            let facts = synthetic_facts(identity_with_device_id("dev-same-source"));
            let url = build_authorize_url(variant, &facts, 17388, "trace-1", "chal-1");
            let params = parse_query(url.split_once('?').expect("授权 URL 必须带查询串").1);
            let body = build_exchange_payload("en1oxy7wnw8j9n", "ac-1", "verifier-1", &facts);
            let device = body.get("DeviceInfo").expect("必须有 DeviceInfo").clone();

            let param = |key: &str| {
                params
                    .get(key)
                    .map(String::as_str)
                    .unwrap_or_else(|| panic!("{variant:?} 的授权 URL 缺 {key}: {url}"))
                    .to_string()
            };
            let field = |key: &str| {
                device
                    .get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| panic!("{variant:?} 的 DeviceInfo 缺 {key}: {device:?}"))
                    .to_string()
            };
            let top = |key: &str| {
                body.get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| panic!("{variant:?} 的请求体缺 {key}"))
                    .to_string()
            };

            // ① 机器标识：`machine_id` / `x_machine_id` ↔ `DeviceInfo.MachineID`
            assert_eq!(param("machine_id"), field("MachineID"), "{variant:?} 的 machine_id 与 MachineID 不同源");
            assert_eq!(param("x_machine_id"), field("MachineID"), "{variant:?} 的 x_machine_id 与 MachineID 不同源");
            // ② 应用版本：`x_app_version` ↔ `ClientVersion` ↔ `IDEVersion`
            assert_eq!(param("x_app_version"), field("ClientVersion"), "{variant:?} 的 x_app_version 与 ClientVersion 不同源");
            assert_eq!(param("x_app_version"), top("IDEVersion"), "{variant:?} 的 x_app_version 与 IDEVersion 不同源");
            // ③ 设备型号：`x_device_brand` ↔ `DeviceInfo.DeviceModel`
            assert_eq!(param("x_device_brand"), field("DeviceModel"), "{variant:?} 的 x_device_brand 与 DeviceModel 不同源");
            // ④ 系统版本：`x_os_version` ↔ `DeviceInfo.OSVersion`
            assert_eq!(param("x_os_version"), field("OSVersion"), "{variant:?} 的 x_os_version 与 OSVersion 不同源");
            // ⑤ 操作系统名：`x_device_type` ↔ `DeviceInfo.OSInfo`
            assert_eq!(param("x_device_type"), field("OSInfo"), "{variant:?} 的 x_device_type 与 OSInfo 不同源");
        }
    }

    // -----------------------------------------------------------------------
    // PKCE
    // -----------------------------------------------------------------------

    /// ★ `challenge` 必须等于独立复算的 `BASE64URL_NOPAD(SHA256(verifier))`。
    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        let (verifier, challenge) = pkce_pair();
        assert_eq!(verifier.len(), 64);
        assert!(verifier.chars().all(|c| c.is_ascii_hexdigit()));
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, expected);
        // 同会话恒定、跨会话必变。
        let (verifier2, challenge2) = pkce_pair();
        assert_ne!(verifier, verifier2, "两次登录的 verifier 必须不同");
        assert_ne!(challenge, challenge2);
    }

    // -----------------------------------------------------------------------
    // CSRF（D5 四分支）
    // -----------------------------------------------------------------------

    fn csrf_params(value: Option<&str>) -> HashMap<String, String> {
        let mut params = HashMap::new();
        if let Some(value) = value {
            params.insert("loginTraceID".into(), value.to_string());
        }
        params
    }

    #[test]
    fn csrf_rejects_mismatch() {
        let error = verify_login_trace(&csrf_params(Some("other")), Some("expected"))
            .expect_err("不匹配必须拒绝");
        assert!(error.contains("不匹配"), "{error}");
    }

    #[test]
    fn csrf_rejects_missing_with_pending() {
        let error = verify_login_trace(&csrf_params(None), Some("expected"))
            .expect_err("有在途会话却缺 loginTraceID，必须拒绝");
        assert!(error.contains("loginTraceID"), "{error}");
    }

    #[test]
    fn csrf_allows_missing_without_pending() {
        assert!(verify_login_trace(&csrf_params(None), None).is_ok());
        // 空串等价于「没有在途会话」。
        assert!(verify_login_trace(&csrf_params(None), Some("")).is_ok());
    }

    #[test]
    fn csrf_allows_match() {
        assert!(verify_login_trace(&csrf_params(Some("expected")), Some("expected")).is_ok());
        // 有回调值但无在途会话（重启后粘贴回调）⇒ 放行。
        assert!(verify_login_trace(&csrf_params(Some("anything")), None).is_ok());
    }

    // -----------------------------------------------------------------------
    // 回调解析
    // -----------------------------------------------------------------------

    /// 固化样例：`authCodeInfo` 是 URL 编码的 JSON（`parse_query` 解码后是 JSON 文本）。
    fn captured_callback_params() -> HashMap<String, String> {
        parse_query(
            "authCodeInfo=%7B%22AuthCode%22%3A%22ac-123%22%2C%22ExpireAt%22%3A1789000000%7D\
             &userInfo=%7B%22UserID%22%3A%223604620555324748%22%2C%22ScreenName%22%3A%22%E5%B0%8F%E6%98%8E%22%2C%22AvatarUrl%22%3A%22https%3A%2F%2Fx%2Fa.png%22%7D\
             &host=https%3A%2F%2Fapi.trae.cn\
             &userRegion=cn\
             &loginTraceID=trace-abc",
        )
    }

    #[test]
    fn parse_callback_reads_authcodeinfo_and_userinfo() {
        let info = parse_callback(&captured_callback_params()).expect("固化样例必须能解析");
        assert_eq!(info.auth_code.as_deref(), Some("ac-123"));
        assert_eq!(info.host.as_deref(), Some("https://api.trae.cn"));
        assert_eq!(info.user_region.as_deref(), Some("cn"));
        assert_eq!(info.login_trace_id.as_deref(), Some("trace-abc"));
        assert_eq!(info.user_id.as_deref(), Some("3604620555324748"));
        assert_eq!(info.display_name.as_deref(), Some("小明"));
        assert_eq!(info.avatar.as_deref(), Some("https://x/a.png"));
        assert_eq!(info.refresh_token, None);
    }

    /// ★ `authCodeInfo` 解不开时必须**报错**，不得吞掉变成「没有凭据」。
    #[test]
    fn parse_callback_reports_missing_authcodeinfo_instead_of_silence() {
        let mut params = HashMap::new();
        params.insert("authCodeInfo".into(), "{not-json".into());
        let error = parse_callback(&params).expect_err("坏 JSON 必须报错");
        assert!(error.contains("authCodeInfo 解析失败"), "{error}");
        assert!(error.contains("授权页回调格式异常"), "{error}");

        // 空 `AuthCode` 视为缺失（不是报错），回落 `code`。
        let mut params = HashMap::new();
        params.insert("authCodeInfo".into(), r#"{"AuthCode":""}"#.into());
        params.insert("code".into(), "code-fallback".into());
        let info = parse_callback(&params).unwrap();
        assert_eq!(info.auth_code.as_deref(), Some("code-fallback"));

        // `authCodeInfo` 完全缺席时也不报错（兼容路径）。
        let mut params = HashMap::new();
        params.insert("refreshToken".into(), "rt-1".into());
        let info = parse_callback(&params).unwrap();
        assert_eq!(info.auth_code, None);
        assert_eq!(info.refresh_token.as_deref(), Some("rt-1"));
    }

    #[test]
    fn parse_callback_prefers_auth_code_over_refresh_token() {
        let mut params = captured_callback_params();
        params.insert("refreshToken".into(), "rt-should-be-ignored".into());
        let info = parse_callback(&params).unwrap();
        assert_eq!(info.auth_code.as_deref(), Some("ac-123"));
        // 容错读仍保留（供兼容路径使用），但主路径优先 AuthCode。
        assert_eq!(info.refresh_token.as_deref(), Some("rt-should-be-ignored"));
    }

    // -----------------------------------------------------------------------
    // 凭据标记（探测 vs 回调）
    // -----------------------------------------------------------------------

    /// ★ 六种凭据标记都必须被识别为「回调」，否则合法回调会被当成探测空等到超时。
    #[test]
    fn credential_markers_cover_all_new_and_legacy_entry_points() {
        for marker in CREDENTIAL_MARKERS {
            let mut params = HashMap::new();
            params.insert(marker.to_string(), "x".into());
            assert!(has_credential_marker(&params), "{marker} 未被识别为凭据");
        }
        assert!(!has_credential_marker(&HashMap::new()));
        // 纯探测：没有凭据参数。
        let probe = parse_query("");
        assert!(!has_credential_marker(&probe));
    }

    // -----------------------------------------------------------------------
    // 交换体（AuthCode：无 DeviceProof、公钥直取）
    // -----------------------------------------------------------------------

    #[test]
    fn build_exchange_payload_has_deviceinfo_and_no_deviceproof() {
        let identity = synthetic_identity();
        let facts = synthetic_facts(identity.clone());
        let payload = build_exchange_payload("ono9krqynydwx5", "ac-1", "verifier-1", &facts);

        let device_info = payload.get("DeviceInfo").expect("必须有 DeviceInfo");
        // ★ `PlatformCode` 按**产品线**派生：合成身份是 TraeWork ⇒ SOLO 线 ⇒ `SOLO_PC`。
        // 反例就是本轮修掉的真实缺陷（写死 IDE_PC ⇒ 上游 20403）。
        assert_eq!(
            device_info.get("PlatformCode").and_then(|v| v.as_str()),
            Some("SOLO_PC")
        );
        assert_eq!(
            device_info.get("DeviceType").and_then(|v| v.as_str()),
            Some("PC")
        );
        assert_eq!(
            device_info.get("DeviceID").and_then(|v| v.as_str()),
            Some(identity.device_id.as_str())
        );
        assert_eq!(
            device_info.get("MachineID").and_then(|v| v.as_str()),
            Some(identity.machine_id.as_str())
        );
        // ★ 公钥必须**逐字**等于 identity 里的那个（证明是直取，不是推导）。
        assert_eq!(
            device_info.get("DevicePublicKey").and_then(|v| v.as_str()),
            Some(identity.public_key_pem.as_str())
        );
        // ★ 应用版本三处同源：取自**安装包版本**（`manifest.json` 的 `appVersion`），
        // **不是**安装目录 `resources/app/package.json` 的内核版本（本机 1.107.1）。
        assert_eq!(
            device_info.get("ClientVersion").and_then(|v| v.as_str()),
            Some("0.1.69")
        );
        assert_eq!(
            payload.get("IDEVersion").and_then(|v| v.as_str()),
            Some("0.1.69")
        );
        assert_eq!(payload.get("AuthCode").and_then(|v| v.as_str()), Some("ac-1"));
        assert_eq!(
            payload.get("CodeVerifier").and_then(|v| v.as_str()),
            Some("verifier-1")
        );
        // ★ AuthCode 场景**不发 DeviceProof**（那是 refreshToken 刷新场景专属）。
        assert!(
            payload.get("DeviceProof").is_none(),
            "AuthCode 请求体里出现了 DeviceProof"
        );
        assert_eq!(payload.as_object().unwrap().len(), 5);
    }

    /// 旧端点兜底体：只有 5 个字段，**既无 Proof 也无公钥** ⇒ 同样不需要私钥。
    #[test]
    fn build_auth_code_fallback_payload_has_no_proof_and_no_public_key() {
        for code_key in ["AuthCode", "Code"] {
            let payload = build_auth_code_fallback_payload(
                "ono9krqynydwx5",
                "ac-1",
                "verifier-1",
                "dev-1",
                "SOLO_PC",
                code_key,
            );
            let map = payload.as_object().unwrap();
            assert_eq!(map.len(), 5, "兜底体字段集漂了: {map:?}");
            assert!(map.contains_key("ClientID"));
            assert!(map.contains_key(code_key), "缺少 {code_key}");
            assert!(map.contains_key("CodeVerifier"));
            assert!(map.contains_key("DeviceID"));
            // `PlatformCode` 由调用方按产品线传入（**不再**是写死的 IDE 线常量）。
            assert_eq!(
                map.get("PlatformCode").and_then(|v| v.as_str()),
                Some("SOLO_PC")
            );
            assert!(payload.get("DeviceProof").is_none());
            assert!(payload.get("DeviceInfo").is_none());
            assert!(!serde_json::to_string(&payload).unwrap().contains("PUBLIC KEY"));
        }
    }

    /// ★ 结构护栏：`exchange_auth_code` 只接受 `&ClientFacts`（其 `identity` 字段
    /// **没有私钥字段**），因此「AuthCode 路径误用私钥」是编译错误而不是评审项。
    ///
    /// 这条注释即断言：若有人把签名改成接受 `&DeviceCredential`，本文件将无法编译
    /// （`synthetic_identity()` 只造得出 `DeviceIdentity`）。
    #[test]
    fn exchange_auth_code_never_requests_a_private_key() {
        let identity: DeviceIdentity = synthetic_identity();
        let _: &DeviceIdentity = &identity;
        // `DeviceIdentity` 的字段集里不含任何私钥字段（编译期即保证）。
        let debug = format!("{identity:?}");
        assert!(!debug.contains("PRIVATE"), "{debug}");
    }

    #[test]
    fn resolve_exchange_host_falls_back_to_variant_account_base() {
        // 正常：原样使用（只去尾斜杠）。
        let (host, used) = resolve_exchange_host(Some("https://api.trae.cn"), TraeVariant::TraeWork);
        assert_eq!(host, "https://api.trae.cn");
        assert!(!used);
        let (host, used) = resolve_exchange_host(Some("  https://api.trae.cn/  "), TraeVariant::TraeWork);
        assert_eq!(host, "https://api.trae.cn");
        assert!(!used);
        // 缺失 / 空串：回落该变体的 **account_base**（真机客户端兑换打的就是它，
        // 见 `resolve_exchange_host` 的说明）。曾经回落成 `icube_base`（另一台主机）。
        for missing in [None, Some(""), Some("   ")] {
            let (host, used) = resolve_exchange_host(missing, TraeVariant::Global);
            assert_eq!(host, "https://grow-normal.trae.ai");
            assert!(used, "回落必须被标记，以便留痕");
        }
        // 阳性对照：CN 侧的 account_base 与 icube_base **不是**同一个值，
        // 否则本用例根本分不出「回落到哪一台主机」。
        let (cn, _) = resolve_exchange_host(None, TraeVariant::TraeWork);
        assert_eq!(cn, "https://api.trae.cn");
        assert_ne!(cn, endpoints_for(TraeVariant::TraeWork).icube_base);
    }

    // -----------------------------------------------------------------------
    // 错误分类
    // -----------------------------------------------------------------------

    /// 一条消息里最长的一串 base64url 字符（用于断言「没有令牌片段」）。
    fn longest_base64url_run(text: &str) -> usize {
        let mut best = 0usize;
        let mut current = 0usize;
        for ch in text.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                current += 1;
                best = best.max(current);
            } else {
                current = 0;
            }
        }
        best
    }

    #[test]
    fn classify_error_maps_20403_and_20405_and_port_busy() {
        let cases = [
            ("portBusy", "Address already in use (os error 10048)"),
            ("timeout", ""),
            ("csrf", "回调缺少 loginTraceID"),
            ("callback", "authCodeInfo 解析失败"),
            ("upstream", "code=20403/xxx: Device not match"),
            ("upstream", "code=20405/xxx: Device proof required"),
        ];
        let messages: Vec<String> = cases
            .iter()
            .map(|(stage, raw)| classify_error(stage, raw))
            .collect();

        assert!(messages[0].contains("17388"), "{}", messages[0]);
        assert!(messages[0].contains("关闭占用"), "{}", messages[0]);
        assert!(messages[1].contains("超时"), "{}", messages[1]);
        assert!(messages[1].contains("17388"), "{}", messages[1]);
        assert!(messages[2].contains("不匹配"), "{}", messages[2]);
        assert!(messages[3].contains("authCodeInfo"), "{}", messages[3]);
        assert!(messages[4].contains("20403"), "{}", messages[4]);
        assert!(messages[4].contains("x-cloudide-token"), "{}", messages[4]);
        assert!(messages[5].contains("20405"), "{}", messages[5]);
        assert!(messages[5].contains("DeviceProof"), "{}", messages[5]);

        // ★ 任何一类文案都不得含 20+ 字符的连续 base64url 片段（令牌/verifier 的形态）。
        for message in &messages {
            assert!(
                longest_base64url_run(message) < 20,
                "错误文案疑似含令牌片段: {message}"
            );
            assert!(!message.contains("BEGIN"), "{message}");
        }
    }

    // -----------------------------------------------------------------------
    // 会话生命周期（含取消）
    // -----------------------------------------------------------------------
    //
    // 这些用例共享进程级的 `LOGIN_SESSIONS`，且 lib 单测并行跑，
    // 因此**一律不做「表里只有几条」这类绝对断言**，只断言自己那几个 id 的存亡。
    // 否则任何一个并发用例插入会话都会把它们弄红。

    /// 需要独占 17388 端口的用例的串行闸门。
    ///
    /// ## 为什么必须有它（固定端口引入的新约束）
    ///
    /// 回调端口从「系统随机分配」改成「固定 17388」之后，任何两个真的调
    /// [`login_start`] 的用例**同时跑必然有一个绑不上端口而失败**——
    /// 这是端口的物理属性，不是脏数据或竞态。
    ///
    /// 锁是**同步** `Mutex`：持锁期间会 `.await`，因此用 `std::sync::Mutex`
    /// 并不理想（跨 await 持 std 锁），但这里锁只保护「本用例自己的端口占用」，
    /// 不会被其他任务的锁顺序牵成死锁，且 tokio 单线程测试运行器下不会自锁。
    /// 用 `std::sync::Mutex` + `lock().unwrap_or_else(|e| e.into_inner())`，
    /// 保证某条用例 panic 后（毒化）其它用例仍能继续跑。
    static PORT_GATE: Mutex<()> = Mutex::new(());

    fn lock_the_callback_port() -> std::sync::MutexGuard<'static, ()> {
        PORT_GATE.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 建一个隔离的临时 home，返回持有者；drop 时删目录并还原 home 变量。
    ///
    /// 隔离测试环境：**同时**隔离 home 与 `APPDATA`，drop 时全部还原。
    ///
    /// 实现已上收到 [`crate::modules::trae::test_support::TempEnv`] ——
    /// 「改环境变量」这件事在本仓库极易写出假绿/随机红（少拿锁、还原与删目录顺序反、
    /// 只隔离两个变量中的一个），因此**全模块只保留一份实现**。
    /// 本函数只是给 oauth 用例保留的短名字，顺带固定「铺设备凭证 fixture」这一档
    /// （本模块的用例都要走 `icube::device_identity_for`，那是登录的硬依赖）。
    ///
    /// ## 为什么是「返回 guard」而不是「包一层闭包」
    ///
    /// `login_start` 是 async 的，用例必须 `.await` 它。若沿用 `FnOnce() -> T` 的
    /// 闭包形态，就得在闭包里再 `block_on` 一次，而 `#[tokio::test]` 默认是
    /// current_thread 运行时——在 runtime 里再启 runtime 会直接 panic
    /// （`Cannot start a runtime from within a runtime`）。
    /// `#[tokio::test]` 的 future **不需要 `Send`**，所以可以把 guard 持到用例结束。
    fn temp_env() -> TempEnv {
        TempEnv::with_device_fixture()
    }

    /// 直接登记一个指定 id 的会话（绕过 `login_start` 的端口绑定）。
    fn insert_session(id: &str, done: bool) -> Arc<Notify> {
        let cancel = Arc::new(Notify::new());
        login_sessions().lock().unwrap().insert(
            id.to_string(),
            LoginSession {
                port: 1,
                done,
                cancel: cancel.clone(),
                ..Default::default()
            },
        );
        cancel
    }

    /// 会话是否已被取消（**测试专用**：生产代码走 `session_view(..).cancelled`）。
    ///
    /// 会话不存在时返回 `false`：那只可能是「轮询已摘除会话」的正常收尾后路径，
    /// 不该把它当成「取消」而拒绝一次真实回调。
    fn session_cancelled(login_id: &str) -> bool {
        session_view(login_id).cancelled
    }

    /// 登记一个**已终态且带账号结果**的会话，用于验证轮询出参的变体归属。
    fn insert_finished_session(id: &str, variant: TraeVariant, account: Value) {
        login_sessions().lock().unwrap().insert(
            id.to_string(),
            LoginSession {
                port: 1,
                variant,
                result: Some(account),
                done: true,
                ..Default::default()
            },
        );
    }

    /// ★ 轮询出终态时，附带的账号列表必须来自**该会话所属变体**的库。
    ///
    /// 回归护栏：`handlers::oauth_login_status` 曾经固定用默认变体取列表，症状是
    /// 「Trae CN 登录成功却把 Trae Work 的列表刷出来」——用户会以为账号丢了，
    /// 而真正的错误是读错了库。
    #[test]
    fn login_status_accounts_follow_the_session_variant() {
        let _env = temp_env();
        // 两个变体各放一个账号：取错库时数量与 userId 都会露馅。
        account::login_with_exchanged_tokens_for(
            TraeVariant::TraeWork,
            make_jwt("work-user"),
            None,
            Some("Work".to_string()),
            None,
        )
        .expect("Trae Work 账号应能落库");
        account::login_with_exchanged_tokens_for(
            TraeVariant::Global,
            make_jwt("cn-user"),
            None,
            Some("CN".to_string()),
            None,
        )
        .expect("Trae CN 账号应能落库");

        let login_id = "test-status-variant";
        insert_finished_session(login_id, TraeVariant::Global, json!({"userId": "cn-user"}));

        let value = crate::modules::trae::handlers::oauth_login_status(login_id);
        assert_eq!(
            value.get("variant").and_then(Value::as_str),
            Some("global")
        );
        let accounts = value
            .get("accounts")
            .and_then(Value::as_array)
            .expect("终态必须附带账号列表");
        assert_eq!(accounts.len(), 1, "取错变体的库会把另一条线的账号也带出来");
        assert_eq!(
            accounts[0].get("userId").and_then(Value::as_str),
            Some("cn-user"),
            "账号列表不是从会话所属变体的库里取的"
        );
    }

    /// ★ 操作层入口（`handlers::oauth_login_start_for`）必须把变体透传到会话。
    ///
    /// 通道层（HTTP / Tauri）只做「解析参数 → 调本函数」，因此这条是两条通道
    /// 共用的变体透传护栏：它一旦断掉，Trae CN 的登录会静默写成 Trae Work。
    #[tokio::test]
    async fn handlers_entry_propagates_variant_to_the_session() {
        let _gate = lock_the_callback_port();
        let _env = temp_env();

        let started = crate::modules::trae::handlers::oauth_login_start_for(TraeVariant::Global)
            .await
            .expect("发起登录不应失败");
        assert_eq!(started["variant"].as_str(), Some("global"));
        assert_eq!(started["variantLabel"].as_str(), Some("国际版"));
        // 授权 URL 的 `device_id` 必须与**该变体**的 icube 设备身份同源
        // （fixture 里两条产品线的候选目录给了不同 deviceId ⇒ 读错变体会露馅）。
        let uri = started["verificationUri"].as_str().unwrap();
        let params = parse_query(uri.split_once('?').unwrap().1);
        let cn_identity = icube::device_identity_for(TraeVariant::Global).expect("fixture 身份应可读");
        assert_eq!(
            params.get("device_id").map(String::as_str),
            Some(cn_identity.device_id.as_str())
        );

        let login_id = started["loginId"].as_str().unwrap().to_string();
        assert!(login_cancel(&login_id));
    }

    /// ★ 设备身份取不到时 `login_start_for` **必须明确失败**，绝不降级。
    ///
    /// 参考在凭证缺失时会回落「`device_map` 最小键条目 → 15 位随机数字」，
    /// 但参考自己的注释已说明这类值与私钥不匹配 ⇒ **必然 20403/20405**。
    /// 本设计**不移植**该回落：宁可给一句「去启动一次客户端」，也不要产出一个
    /// 注定失败、且用户完全看不懂原因的登录。
    #[tokio::test]
    async fn login_start_fails_loudly_when_device_identity_missing() {
        let _gate = lock_the_callback_port();
        let _env = temp_env();
        // 把该变体的候选目录全部删掉 ⇒ 模拟「客户端没启动过 / 没登录过」。
        let appdata = std::env::var_os("APPDATA").expect("temp_env 应已设置 APPDATA");
        for name in crate::modules::trae::platform::data_dir_names_for(TraeVariant::TraeWork) {
            let _ = std::fs::remove_dir_all(std::path::PathBuf::from(&appdata).join(name));
        }

        let error = login_start_for(TraeVariant::TraeWork)
            .await
            .expect_err("设备身份缺失时必须失败，不得回落自造 device_id");
        assert!(
            error.contains("Trae Work"),
            "错误文案必须点明是哪条产品线（否则用户不知道该启动哪个客户端）: {error}"
        );
        assert!(
            error.contains("设备凭证"),
            "错误文案应指出缺的是设备凭证: {error}"
        );
        // 端口必须已释放：失败路径不该白占 300 秒。
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT)).is_ok(),
            "失败路径没有释放回调端口"
        );
    }

    #[test]
    fn session_cancelled_missing_session_is_false() {
        // 会话不存在 ≠ 已取消。若这里返 true，一次正常回调会被误判成取消而丢账号。
        assert!(!session_cancelled("definitely-not-a-session"));
    }

    #[test]
    fn cancel_marks_session_and_poll_reports_it_once() {
        let id = "test-cancel-single";
        insert_session(id, false);

        assert!(!session_cancelled(id), "刚登记的会话不应是已取消");
        assert!(login_cancel(id), "取消一个进行中的会话应返回 true");
        assert!(session_cancelled(id), "取消后必须能读到已取消标志");

        // 第一次轮询：终态 + 明确文案
        let first = login_poll(id);
        assert_eq!(first.get("done").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            first.get("error").and_then(|v| v.as_str()),
            Some("已取消")
        );
        // 终态分支也要带出 variant。
        assert_eq!(
            first.get("variant").and_then(|v| v.as_str()),
            Some("trae_work")
        );

        // 终态会被摘除，再轮询就是「不存在」——这正是前端停止轮询后的正常收尾路径。
        let second = login_poll(id);
        assert_eq!(second.get("done").and_then(|v| v.as_bool()), Some(true));
        assert!(second.get("error").is_some());
    }

    #[test]
    fn cancel_is_rejected_for_already_done_session() {
        let id = "test-cancel-done";
        insert_session(id, true);
        // 已终态的会话不该被「再取消一次」，否则前端会收到一个假的成功回执。
        assert!(!login_cancel(id));
        assert!(!session_cancelled(id), "取消被拒时不应留下已取消标志");
        // 轮询到的仍是「已结束但无结果」，而不是「已取消」。
        let polled = login_poll(id);
        assert_eq!(polled.get("done").and_then(|v| v.as_bool()), Some(true));
        assert_ne!(
            polled.get("error").and_then(|v| v.as_str()),
            Some("已取消"),
            "已终态会话的失败原因不应被改写成「已取消」"
        );
    }

    /// 取消必须**唤醒**监听任务，而不是等超时。
    ///
    /// 这是本轮修掉的一个真实缺陷：原实现只把 `done`/`cancelled` 置位就返回，
    /// 监听任务仍在 `accept()` 上干等，端口长达 300 秒不释放。
    #[tokio::test]
    async fn cancel_wakes_the_waiting_listener() {
        let cancel = Arc::new(Notify::new());
        // 模拟监听任务：等通知，最多等 2 秒（远小于 300 秒的登录超时）。
        let waiter = {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = cancel.notified() => true,
                    _ = tokio::time::sleep(Duration::from_secs(2)) => false,
                }
            })
        };

        // 给任务一点时间真正进入 `notified()` 的等待。
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.notify_one();

        assert!(
            waiter.await.expect("监听任务不应 panic"),
            "取消未唤醒监听任务：端口会被白占到超时"
        );
    }

    /// `notify_one` 的许可会存下来，取消先于等待发生也不会丢信号。
    #[tokio::test]
    async fn cancel_before_waiting_is_not_lost() {
        let cancel = Arc::new(Notify::new());
        cancel.notify_one(); // 先发信号，任务还没开始等
        let woken = tokio::time::timeout(Duration::from_secs(1), cancel.notified()).await;
        assert!(woken.is_ok(), "提前发出的取消信号被丢掉了");
    }

    #[tokio::test]
    async fn login_start_binds_the_fixed_callback_port_and_reports_it() {
        let _gate = lock_the_callback_port();
        let _env = temp_env();
        let started = login_start().await.expect("发起登录不应失败");
        let login_id = started
            .get("loginId")
            .and_then(|v| v.as_str())
            .expect("缺少 loginId")
            .to_string();
        let port = started
            .get("port")
            .and_then(|v| v.as_u64())
            .expect("缺少 port") as u16;
        let uri = started
            .get("verificationUri")
            .and_then(|v| v.as_str())
            .expect("缺少 verificationUri")
            .to_string();

        // ★ 端口必须是**固定**的 17388，不能是系统随机分配的。
        assert_eq!(port, CALLBACK_PORT, "回调端口必须是上游约定的固定端口");
        assert!(
            uri.contains(&format!(
                "auth_callback_url=http%3A%2F%2F127.0.0.1%3A{port}%2Fauthorize"
            )),
            "授权 URL 里的回调端口与回传端口不一致: {uri}"
        );
        assert_eq!(
            started.get("expiresIn").and_then(|v| v.as_u64()),
            Some(TRAE_OAUTH_LOGIN_TIMEOUT_SECONDS)
        );

        // 新增契约：变体、PKCE、CSRF 必须出现在响应 / 授权 URL 里。
        assert_eq!(
            started.get("variant").and_then(|v| v.as_str()),
            Some("trae_work"),
            "默认变体必须是 trae_work"
        );
        assert_eq!(
            started.get("variantLabel").and_then(|v| v.as_str()),
            Some("Trae Work")
        );
        let params = parse_query(uri.split_once('?').unwrap().1);
        assert_eq!(
            params.get("code_challenge_method").map(String::as_str),
            Some("S256"),
            "PKCE 方法必须是 S256"
        );
        let challenge = params.get("code_challenge").cloned().unwrap_or_default();
        assert_eq!(challenge.len(), 43, "S256 challenge 是 32B 的 base64url");
        let trace = params.get("login_trace_id").cloned().unwrap_or_default();
        assert_eq!(trace.len(), 32, "login_trace_id 必须是 32 位 hex");
        assert!(trace.chars().all(|c| c.is_ascii_hexdigit()));

        // 端口必须真的在监听（能连上）。
        let connect = tokio::net::TcpStream::connect(("127.0.0.1", port)).await;
        assert!(connect.is_ok(), "回调端口 {port} 没有在监听: {connect:?}");
        drop(connect);

        // 刚发起时还没结果。
        let polled = login_poll(&login_id);
        assert_eq!(polled.get("done").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(polled.get("port").and_then(|v| v.as_u64()), Some(port as u64));
        assert_eq!(
            polled.get("variant").and_then(|v| v.as_str()),
            Some("trae_work")
        );

        // 收尾，避免把监听任务留给后续用例。
        assert!(login_cancel(&login_id));
    }

    /// ★ 设备凭证诊断必须出现在发起响应里，且**不含私钥**。
    #[tokio::test]
    async fn login_start_response_carries_variant_and_device_credential_diagnostics() {
        let _gate = lock_the_callback_port();
        let _env = temp_env();
        let started = login_start_for(TraeVariant::Global)
            .await
            .expect("发起登录不应失败");
        let login_id = started
            .get("loginId")
            .and_then(|v| v.as_str())
            .expect("缺少 loginId")
            .to_string();

        assert_eq!(
            started.get("variant").and_then(|v| v.as_str()),
            Some("global")
        );
        assert_eq!(
            started.get("variantLabel").and_then(|v| v.as_str()),
            Some("国际版")
        );
        let credential = started
            .get("deviceCredential")
            .expect("必须带设备凭证诊断");
        for key in [
            "available",
            "sourceApp",
            "deviceId",
            "machineId",
            "appVersion",
            "errorKind",
            "errorMessage",
        ] {
            assert!(
                credential.as_object().unwrap().contains_key(key),
                "诊断缺少字段 {key}: {credential}"
            );
        }
        let text = serde_json::to_string(&started).unwrap();
        assert!(!text.contains("BEGIN"), "发起响应里出现了 PEM");
        assert!(!text.contains("PRIVATE KEY"), "发起响应里出现了私钥");
        // 授权 URL 里的 machine_id / device_id 与身份 A 同源。
        let uri = started.get("verificationUri").and_then(|v| v.as_str()).unwrap();
        let params = parse_query(uri.split_once('?').unwrap().1);
        assert_eq!(
            params.get("device_id"),
            params.get("x_device_id"),
            "x_device_id 必须与 device_id 同源"
        );
        assert_eq!(
            params.get("machine_id"),
            params.get("x_machine_id"),
            "x_machine_id 必须与 machine_id 同源"
        );

        assert!(login_cancel(&login_id));
    }

    /// ★ 上游的「在线探测」请求**不能**把登录判为失败。
    #[tokio::test]
    async fn probe_request_does_not_finish_the_session() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let _gate = lock_the_callback_port();
        let _env = temp_env();

        let started = login_start().await.expect("发起登录不应失败");
        let login_id = started
            .get("loginId")
            .and_then(|v| v.as_str())
            .expect("缺少 loginId")
            .to_string();
        let port = started
            .get("port")
            .and_then(|v| v.as_u64())
            .expect("缺少 port") as u16;

        // 模拟授权页的裸探测：没有任何凭据参数。
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("应能连上回调端口");
        stream
            .write_all(b"GET /authorize HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .await
            .expect("写出探测请求不应失败");
        let mut response = String::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            stream.read_to_string(&mut response),
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 200"), "探测必须收到 200: {response}");
        // 跨源探测没有 CORS 头会被浏览器拦掉响应，授权页照样判「不在线」。
        assert!(
            response
                .to_ascii_lowercase()
                .contains("access-control-allow-origin"),
            "探测响应缺少 CORS 头，浏览器会拦截它: {response}"
        );

        // **关键断言**：探测之后会话必须仍然活着，而不是被当成失败回调收尾。
        let polled = login_poll(&login_id);
        assert_eq!(
            polled.get("done").and_then(|v| v.as_bool()),
            Some(false),
            "探测请求把登录会话误判成已结束: {polled}"
        );

        assert!(login_cancel(&login_id));
    }

    /// `OPTIONS` 预检也要被应答（回 204），同样不能结束会话。
    #[tokio::test]
    async fn options_preflight_is_answered_without_finishing_the_session() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let _gate = lock_the_callback_port();
        let _env = temp_env();

        let started = login_start().await.expect("发起登录不应失败");
        let login_id = started
            .get("loginId")
            .and_then(|v| v.as_str())
            .expect("缺少 loginId")
            .to_string();
        let port = started
            .get("port")
            .and_then(|v| v.as_u64())
            .expect("缺少 port") as u16;

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("应能连上回调端口");
        stream
            .write_all(
                b"OPTIONS /authorize HTTP/1.1\r\nHost: 127.0.0.1\r\n\
                  Access-Control-Request-Method: GET\r\nConnection: close\r\n\r\n",
            )
            .await
            .expect("写出预检请求不应失败");
        let mut response = String::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            stream.read_to_string(&mut response),
        )
        .await;

        assert!(
            response.starts_with("HTTP/1.1 204"),
            "预检应回 204: {response}"
        );

        let polled = login_poll(&login_id);
        assert_eq!(
            polled.get("done").and_then(|v| v.as_bool()),
            Some(false),
            "预检请求把登录会话误判成已结束: {polled}"
        );

        assert!(login_cancel(&login_id));
    }

    // -----------------------------------------------------------------------
    // 本地 mock 上游：完整回调链路（不发真实网络请求）
    // -----------------------------------------------------------------------

    /// 起一个本地 mock HTTP 上游，返回 `(端口, 捕获的原始请求)`。
    ///
    /// 每个连接只服务一次：读完请求（含 body）→ 回固定 JSON → 关闭。
    /// `accepts` 是任务循环的上限，防止测试泄漏后台任务。
    async fn mock_upstream(
        accepts: usize,
        response_body: String,
    ) -> (u16, Arc<Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("mock 上游应能绑定端口");
        let port = listener.local_addr().unwrap().port();
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = captured.clone();

        tokio::spawn(async move {
            for _ in 0..accepts {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut buf: Vec<u8> = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let read = match stream.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&chunk[..read]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(headers_end) = text.find("\r\n\r\n") {
                        let head_len = headers_end + 4;
                        let content_length = text[..headers_end]
                            .lines()
                            .find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                if key.trim().eq_ignore_ascii_case("content-length") {
                                    value.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0);
                        if buf.len() >= head_len + content_length {
                            break;
                        }
                    }
                }
                sink.lock().unwrap().push(String::from_utf8_lossy(&buf).into_owned());
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        (port, captured)
    }

    /// ★ `x-cloudide-token` 必须**存在且为空串**（20403 的直接护栏）。
    ///
    /// 走真实 TCP：某些 HTTP 库会丢弃空值头，只有在原始请求字节上才看得出来。
    #[tokio::test]
    async fn exchange_request_sends_empty_cloudide_token_header() {
        let jwt = make_jwt("3604620555324748");
        let body = json!({"Result": {"Token": jwt, "RefreshToken": "rt-1"}}).to_string();
        let (port, captured) = mock_upstream(1, body).await;

        let identity = synthetic_identity();
        let facts = synthetic_facts(identity.clone());
        let exchanged = exchange_auth_code(
            TraeVariant::TraeWork,
            &format!("http://127.0.0.1:{port}"),
            "ac-1",
            "verifier-1",
            &facts,
        )
        .await
        .expect("主变体应交换成功");
        assert_eq!(exchanged.refresh_token.as_deref(), Some("rt-1"));
        assert!(exchanged.jwt.starts_with("Cloud-IDE-JWT "));

        let requests = captured.lock().unwrap().clone();
        assert_eq!(requests.len(), 1, "主变体成功就不该再打兜底变体");
        let request = &requests[0];
        let lower = request.to_ascii_lowercase();
        assert!(
            lower.contains("content-type: application/json"),
            "缺少 content-type: {request}"
        );
        // ★ 头必须存在，且值为空串。
        let header_line = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("x-cloudide-token:"))
            .expect("必须发送 x-cloudide-token 头（缺失报 20403）");
        assert_eq!(
            header_line.split_once(':').unwrap().1.trim(),
            "",
            "x-cloudide-token 必须为空串: {header_line:?}"
        );
        // 请求体是 AuthCode 主变体（有 DeviceInfo、无 DeviceProof）。
        assert!(request.contains("\"DeviceInfo\""), "{request}");
        assert!(!request.contains("\"DeviceProof\""), "{request}");
        assert!(request.contains("\"CodeVerifier\":\"verifier-1\""), "{request}");

        // ★ 三方同源（T4 验收 1b）：`DeviceInfo.DeviceID` 与请求头 `x-device-id`
        // 都必须等于授权 URL 用的那个 `identity.device_id`。
        assert!(
            request.contains(&format!("\"DeviceID\":\"{}\"", identity.device_id)),
            "DeviceInfo.DeviceID 与设备身份不同源: {request}"
        );
        let device_header = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("x-device-id:"))
            .expect("必须发送 x-device-id 头");
        assert_eq!(
            device_header.split_once(':').unwrap().1.trim(),
            identity.device_id,
            "x-device-id 与 DeviceInfo.DeviceID 不同源"
        );
    }

    /// ★ 只带 `code` 的回调是**合法凭据入口**，必须被兑换而不是被当成探测。
    ///
    /// 这条替换了旧用例 `callback_without_refresh_token_fails_fast_instead_of_hanging`：
    /// 新契约下 `code` / `authCodeInfo` 是主路径，旧断言「只带 code 必须立刻失败」
    /// 与需求直接冲突。**原意图（不允许空等到 300s）被完整保住**：
    /// 本用例断言会话在很短时间内进入终态并落库。
    #[tokio::test]
    async fn callback_with_code_is_exchanged_not_treated_as_probe() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let _gate = lock_the_callback_port();
        let _env = temp_env();
        let jwt = make_jwt("3604620555324748");
        let body = json!({"Result": {"Token": jwt, "RefreshToken": "rt-1"}}).to_string();
        let (mock_port, _captured) = mock_upstream(2, body).await;

        let started = login_start_for(TraeVariant::TraeWork)
            .await
            .expect("发起登录不应失败");
        let login_id = started["loginId"].as_str().unwrap().to_string();
        let port = started["port"].as_u64().unwrap() as u16;
        let uri = started["verificationUri"].as_str().unwrap();
        // 从授权 URL 里取 CSRF 期望值（授权页会原样回传它）。
        let trace = parse_query(uri.split_once('?').unwrap().1)
            .get("login_trace_id")
            .cloned()
            .expect("授权 URL 必须带 login_trace_id");

        let target = format!(
            "/authorize?code=ac-1&loginTraceID={trace}&host=http%3A%2F%2F127.0.0.1%3A{mock_port}"
        );
        let request =
            format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("应能连上回调端口");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            stream.read_to_string(&mut response),
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");

        // ★ 浏览器那一页必须是**渲染得出来的结果卡片**，而不是纯文本。
        // 纯文本（`text/plain`）会被浏览器当正文渲染成左上角一行小字，
        // 用户看不出这是登录流程的一部分还是页面坏了。
        assert!(
            response.contains("Content-Type: text/html; charset=utf-8"),
            "回调结果页必须回 text/html，否则卡片布局不生效: {response}"
        );
        assert!(response.contains("<!DOCTYPE html>"), "{response}");
        assert!(response.contains("登录成功"), "{response}");
        // 落库区域必须写在页面上（持久化轴是区域，不是程序名）。
        assert!(
            response.contains("国内版"),
            "结果页必须点明账号落在哪个区域库: {response}"
        );
        // ★ 结果页**不得**出现凭据（本链路不变式：回调响应体不含凭据）。
        assert!(
            !response.contains(&jwt),
            "回调响应体泄漏了 JWT: {response}"
        );
        assert!(
            !response.contains("authCodeInfo") && !response.contains("refreshToken"),
            "回调响应体出现了凭据标记: {response}"
        );

        // 必须在很短时间内进入终态（不是空等到 300 秒超时）。
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let polled = login_poll(&login_id);
            if polled.get("done").and_then(|v| v.as_bool()) == Some(true) {
                assert!(
                    polled.get("account").is_some(),
                    "带 code 的回调应完成兑换并落库: {polled}"
                );
                assert_eq!(
                    polled.get("variant").and_then(|v| v.as_str()),
                    Some("trae_work")
                );
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "带 code 的回调被当成探测，会话会空等到超时: {polled}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        // 账号必须落进 **Trae Work** 的库。
        assert_eq!(account::entries_for(TraeVariant::TraeWork).len(), 1);
        assert!(account::entries_for(TraeVariant::Global).is_empty());
    }

    /// ★ 兑换失败时，浏览器那一页**不能说「登录成功」**。
    ///
    /// 这是本次改造修掉的真实缺陷形态：结果页原先在兑换**之前**就回掉了，
    /// 于是只要回调带着凭据到达，页面一律显示「登录成功，可以关闭本页并返回应用」——
    /// 哪怕紧随其后的 ExchangeToken 直接失败、账号根本没落库。
    /// 用户此刻正盯着那一页，只会以为账号已经加好了。
    ///
    /// 反向验证：把结果页改回「兑换前先回成功页」，本用例的
    /// `!response.contains("登录成功")` 必须失败。
    #[tokio::test]
    async fn failed_exchange_renders_failure_page_not_success_page() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let _gate = lock_the_callback_port();
        let _env = temp_env();
        // 三个交换变体全都会拿到这个 body：既无 Token 也无 RefreshToken ⇒ 全部失败。
        let (mock_port, _captured) = mock_upstream(3, json!({"Result": {}}).to_string()).await;

        let started = login_start_for(TraeVariant::TraeWork)
            .await
            .expect("发起登录不应失败");
        let login_id = started["loginId"].as_str().unwrap().to_string();
        let port = started["port"].as_u64().unwrap() as u16;
        let uri = started["verificationUri"].as_str().unwrap();
        let trace = parse_query(uri.split_once('?').unwrap().1)
            .get("login_trace_id")
            .cloned()
            .expect("授权 URL 必须带 login_trace_id");

        let target = format!(
            "/authorize?code=ac-1&loginTraceID={trace}&host=http%3A%2F%2F127.0.0.1%3A{mock_port}"
        );
        let request =
            format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("应能连上回调端口");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(10),
            stream.read_to_string(&mut response),
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(
            response.contains("Content-Type: text/html; charset=utf-8"),
            "{response}"
        );
        assert!(
            response.contains("登录失败"),
            "兑换失败必须渲染失败页: {response}"
        );
        assert!(
            !response.contains("登录成功"),
            "★ 兑换失败却在页面上说「登录成功」= 页面在骗用户: {response}"
        );

        // 会话同样必须收尾（页面与前端轮询两条路都要给出失败）。
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let polled = login_poll(&login_id);
            if polled.get("done").and_then(|v| v.as_bool()) == Some(true) {
                assert!(polled.get("error").is_some(), "{polled}");
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "兑换失败没有收尾: {polled}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// ★ 成功页取名与落库位置：昵称 → userId → 占位；
    /// 落库位置按**应用 → 产品 → 区域**三段写全，区域按**持久化轴**取。
    ///
    /// 纯函数，不碰端口与 home（避免 lib 单测并行时的进程级环境串味）。
    #[test]
    fn success_page_prefers_name_then_user_id_and_uses_persistence_region() {
        let named = success_page(
            &json!({"name": "小明", "userId": "u-1"}),
            TraeVariant::TraeWork,
        );
        assert!(named.contains("账号 [小明] 登录成功"), "{named}");
        // ★ 整句逐字对拍：应用名 + 产品名 + 区域，缺一段都算「页面没说清账号进了哪」。
        // 现场（2026-09-24 用户报障）：只写「国内版」时用户读不出是哪个应用的哪个产品
        // —— 本应用的 WorkBuddy 分区同样有「国内版」。
        assert!(
            named.contains("账号已添加到 Buddy Switch 的 Trae（国内版）账号库，可关闭此页面返回应用。"),
            "成功页必须写全「应用 → 产品 → 区域」: {named}"
        );
        // ★ 反面：**不得**出现程序位名（`Trae Work` / `Trae CN`）。
        // 持久化轴是区域，两条产品线共用一本国内库 ⇒ 写程序名会让另一条线的用户
        // 以为提示在骗人（见 `success_page` 的说明）。
        assert!(
            !named.contains("Trae Work") && !named.contains("Trae CN"),
            "成功页出现了程序位名，会让共用同一本库的另一条线用户误判: {named}"
        );

        // 上游没给昵称时退回 userId —— 与前端 `result.name || result.userId` 同款。
        let unnamed = success_page(&json!({"name": "", "userId": "u-1"}), TraeVariant::TraeWork);
        assert!(unnamed.contains("账号 [u-1] 登录成功"), "{unnamed}");

        let empty = success_page(&json!({}), TraeVariant::Global);
        assert!(empty.contains("未知账号"), "{empty}");
        // 区域随变体分家：国际版必须是「国际版」，且同样写全三段。
        assert!(
            empty.contains("账号已添加到 Buddy Switch 的 Trae（国际版）账号库"),
            "{empty}"
        );
    }

    /// ★★ `Content-Length` 必须是**字节数**，否则浏览器按声明长度**截断**结果页。
    ///
    /// 为什么必须单测：结果页正文全是中文（一个汉字 3 字节），写成字符数会让卡片
    /// **只渲染一半**，而服务端一切正常。集成测试用 `read_to_string` 读到 EOF，
    /// **抓不到长度声明错误**（连接关了，它不看 Content-Length）——只有真浏览器会暴露。
    ///
    /// 反向验证：把 `body.len()` 改成 `body.chars().count()`，本用例必须红。
    #[test]
    fn content_length_is_byte_count_not_char_count() {
        let body = "账号 [小明] 登录成功";
        let response = build_response("text/html; charset=utf-8", body, "GET");

        let declared: usize = response
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .expect("响应必须带 Content-Length")
            .trim()
            .parse()
            .expect("Content-Length 必须是数字");

        assert_eq!(
            declared,
            body.as_bytes().len(),
            "Content-Length 声明 {declared} 与正文字节数 {} 不符（写成字符数了？）",
            body.as_bytes().len()
        );
        assert_ne!(
            declared,
            body.chars().count(),
            "正文含多字节字符，字符数与字节数不该相等——本用例失去了鉴别力"
        );
        // 报文里正文必须完整（长度声明对了，正文被拼丢了同样白搭）。
        assert!(response.ends_with(body), "{response}");
    }

    /// `OPTIONS` 预检：204、空正文、长度 0，且 CORS 头照旧。
    #[test]
    fn options_preflight_response_has_no_body() {
        let response = build_response("text/html; charset=utf-8", "<html>不该出现</html>", "OPTIONS");
        assert!(response.starts_with("HTTP/1.1 204 No Content"), "{response}");
        assert!(response.contains("Content-Length: 0"), "{response}");
        assert!(!response.contains("不该出现"), "204 不该带正文: {response}");
        assert!(
            response.to_ascii_lowercase().contains("access-control-allow-origin"),
            "预检同样需要 CORS 头: {response}"
        );
    }

    /// 结果页响应必须同时具备：`text/html`、`no-store`、CORS（探测与结果页共用组装）。
    #[test]
    fn html_response_declares_html_type_and_forbids_caching() {
        let response = build_response("text/html; charset=utf-8", "<html></html>", "GET");
        assert!(
            response.contains("Content-Type: text/html; charset=utf-8"),
            "{response}"
        );
        assert!(
            response.to_ascii_lowercase().contains("cache-control: no-store"),
            "结果页含账号昵称且 URL 一次性，不该被缓存: {response}"
        );
        assert!(
            response.to_ascii_lowercase().contains("access-control-allow-origin"),
            "{response}"
        );
    }

    /// ★ `error=` 回调必须**立刻**收尾（且优先于探测判定）。
    #[tokio::test]
    async fn error_callback_finishes_session_before_probe_check() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let _gate = lock_the_callback_port();
        let _env = temp_env();

        let started = login_start().await.expect("发起登录不应失败");
        let login_id = started["loginId"].as_str().unwrap().to_string();
        let port = started["port"].as_u64().unwrap() as u16;

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream
            .write_all(
                b"GET /authorize?error=access_denied HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let mut response = String::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            stream.read_to_string(&mut response),
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let polled = login_poll(&login_id);
            if polled.get("done").and_then(|v| v.as_bool()) == Some(true) {
                assert!(
                    polled
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .contains("access_denied"),
                    "{polled}"
                );
                return;
            }
            assert!(tokio::time::Instant::now() < deadline, "拒绝回调没有立刻收尾: {polled}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// ★ CSRF：回调不带 `loginTraceID` 而本机有在途会话 ⇒ 必须拒绝（且不落库）。
    #[tokio::test]
    async fn callback_without_login_trace_id_is_rejected_while_session_pending() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let _gate = lock_the_callback_port();
        let _env = temp_env();

        let started = login_start().await.expect("发起登录不应失败");
        let login_id = started["loginId"].as_str().unwrap().to_string();
        let port = started["port"].as_u64().unwrap() as u16;

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream
            .write_all(b"GET /authorize?code=ac-1 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            stream.read_to_string(&mut response),
        )
        .await;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let polled = login_poll(&login_id);
            if polled.get("done").and_then(|v| v.as_bool()) == Some(true) {
                assert!(
                    polled
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .contains("loginTraceID"),
                    "{polled}"
                );
                assert!(polled.get("account").is_none(), "被拒绝的回调不得落库: {polled}");
                return;
            }
            assert!(tokio::time::Instant::now() < deadline, "CSRF 未拒绝: {polled}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// 取消后端口必须**很快**停止监听（而不是等到 300 秒超时）。
    #[tokio::test]
    async fn cancel_releases_the_callback_port() {
        let _gate = lock_the_callback_port();
        let _env = temp_env();
        let started = login_start().await.expect("发起登录不应失败");
        let login_id = started
            .get("loginId")
            .and_then(|v| v.as_str())
            .expect("缺少 loginId")
            .to_string();
        let port = started
            .get("port")
            .and_then(|v| v.as_u64())
            .expect("缺少 port") as u16;

        assert!(login_cancel(&login_id));

        // 监听任务被唤醒到真正 drop listener 之间有一小段调度延迟，
        // 因此允许重试到一个很短的截止时间；到期仍能连上才算失败。
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let refused = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_err();
            if refused {
                return;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("取消后端口 {port} 仍在监听：监听任务没有被唤醒，端口会被白占 300 秒");
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// 发起新登录时会回收「已到终态但没人再轮询」的会话。
    #[tokio::test]
    async fn login_start_prunes_terminal_sessions() {
        let _gate = lock_the_callback_port();
        let _env = temp_env();
        let stale = "test-prune-stale";
        insert_session(stale, true); // 已终态、无人轮询——正是要回收的那种

        let started = login_start().await.expect("发起登录不应失败");
        let fresh = started
            .get("loginId")
            .and_then(|v| v.as_str())
            .expect("缺少 loginId")
            .to_string();

        let sessions = login_sessions().lock().unwrap();
        assert!(!sessions.contains_key(stale), "终态残留会话没有被回收");
        assert!(sessions.contains_key(&fresh), "新会话必须已登记");
        drop(sessions);

        assert!(login_cancel(&fresh));
    }

    // -----------------------------------------------------------------------
    // QA 补强：真实链路护栏（端口被占 / 三种探测请求串行）
    // -----------------------------------------------------------------------

    /// 对一个已监听的本地端口发一条原始 HTTP 请求，读回完整响应文本。
    async fn send_raw(port: u16, request: &[u8]) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("应能连上回调端口");
        stream
            .write_all(request)
            .await
            .expect("写出请求不应失败");
        let mut response = String::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            stream.read_to_string(&mut response),
        )
        .await;
        response
    }

    /// ★ 固定端口被占时，`login_start` 必须**明确报错**，绝不能悄悄回退随机端口。
    #[tokio::test]
    async fn login_start_fails_loudly_when_fixed_port_is_occupied() {
        let _gate = lock_the_callback_port();
        let _env = temp_env();

        // 前置：先占住固定端口。若此处就失败，说明本机 17388 已被外部进程占用，
        // 那是环境问题而非本用例要验证的行为，直接以明确信息暴露。
        let squatter = std::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
            .expect("测试前置失败：本机 17388 已被占用，无法验证端口冲突路径");

        let result = login_start().await;
        let error = result
            .expect_err("端口被占时 login_start 必须返回 Err，绝不能回退到随机端口");

        assert!(
            error.contains(&CALLBACK_PORT.to_string()),
            "错误文案必须点明端口号 {CALLBACK_PORT}，否则用户无从排查：{error}"
        );
        assert!(
            error.contains("17388"),
            "错误文案应含具体端口 17388：{error}"
        );
        assert!(
            error.contains("关闭占用"),
            "错误文案必须给出下一步动作：{error}"
        );

        drop(squatter);
    }

    /// ★ 三种上游探测请求（根路径 / 授权路径 / OPTIONS 预检）串行打完后，
    /// 会话必须**仍然存活**，且每条响应都带 CORS 头。
    #[tokio::test]
    async fn probe_chain_root_authorize_options_survive_with_cors() {
        let _gate = lock_the_callback_port();
        let _env = temp_env();

        let started = login_start().await.expect("发起登录不应失败");
        let login_id = started
            .get("loginId")
            .and_then(|v| v.as_str())
            .expect("缺少 loginId")
            .to_string();
        let port = started
            .get("port")
            .and_then(|v| v.as_u64())
            .expect("缺少 port") as u16;

        // 1) 根路径、无 query —— 工程师未覆盖的探测形态。
        let root = send_raw(port, b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").await;
        assert!(root.starts_with("HTTP/1.1 200"), "根路径探测应回 200: {root}");
        assert!(
            root.to_ascii_lowercase().contains("access-control-allow-origin"),
            "根路径探测缺少 CORS 头，浏览器会拦掉响应: {root}"
        );

        // 2) 授权路径、无 query。
        let authorize = send_raw(
            port,
            b"GET /authorize HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(
            authorize.starts_with("HTTP/1.1 200"),
            "授权路径探测应回 200: {authorize}"
        );
        assert!(
            authorize.to_ascii_lowercase().contains("access-control-allow-origin"),
            "授权路径探测缺少 CORS 头: {authorize}"
        );

        // 3) OPTIONS 预检。
        let preflight = send_raw(
            port,
            b"OPTIONS /authorize HTTP/1.1\r\nHost: 127.0.0.1\r\n\
              Access-Control-Request-Method: GET\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(
            preflight.starts_with("HTTP/1.1 204"),
            "预检应回 204: {preflight}"
        );
        assert!(
            preflight.to_ascii_lowercase().contains("access-control-allow-origin"),
            "预检响应缺少 CORS 头: {preflight}"
        );

        // ★ 核心行为：三次探测之后会话必须仍在进行中，而不是被当成回调收尾。
        let polled = login_poll(&login_id);
        assert_eq!(
            polled.get("done").and_then(|v| v.as_bool()),
            Some(false),
            "任一探测请求把登录会话误判成已结束: {polled}"
        );

        assert!(login_cancel(&login_id));
    }
}
