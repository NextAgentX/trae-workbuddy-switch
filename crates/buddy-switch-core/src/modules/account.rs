//! 账号存储：读取/写入 `~/.buddy-switch/accounts.json`，与 Python 版共享数据目录。
//!
//! 对照 server.py `load_accounts` / `save_accounts` / `find_account` /
//! `account_display_name` / `account_meta`。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

use crate::modules::config::{atomic_write, now_ms};
use crate::modules::region::{accounts_file_for, region_of, region_spec, Region};

fn load_accounts_from_path(path: &Path) -> Vec<Value> {
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Ok(Value::Array(accounts)) = serde_json::from_str::<Value>(&text) {
            return accounts;
        }
    }
    vec![]
}

fn save_accounts_to_path(path: &Path, accounts: &[Value]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(accounts).unwrap_or_default();
    atomic_write(path, &content)
}

fn find_account_in(accounts: &[Value], account_id: &str) -> Option<Value> {
    accounts
        .iter()
        .find(|account| {
            account.get("id").and_then(Value::as_str) == Some(account_id)
                || account.get("uid").and_then(Value::as_str) == Some(account_id)
        })
        .cloned()
}

fn delete_account_from_path(path: &Path, account_id: &str) -> Result<(), String> {
    let mut accounts = load_accounts_from_path(path);
    let before = accounts.len();
    accounts.retain(|account| account.get("id").and_then(Value::as_str) != Some(account_id));
    if accounts.len() == before {
        return Err("账号不存在".to_string());
    }
    save_accounts_to_path(path, &accounts).map_err(|error| error.to_string())
}

/// 读取账号库（CN）；文件缺失或损坏返回空列表。
pub fn load_accounts() -> Vec<Value> {
    load_accounts_for(Region::Cn)
}

/// 按 region 读取账号库；文件缺失或损坏返回空列表。
pub fn load_accounts_for(region: Region) -> Vec<Value> {
    load_accounts_from_path(&accounts_file_for(region))
}

/// 写回 CN 账号库（原子写），保持原 JSON 数组结构。
pub fn save_accounts(accounts: &[Value]) -> std::io::Result<()> {
    save_accounts_for(Region::Cn, accounts)
}

/// 按 region 写回账号库（原子写）。
pub fn save_accounts_for(region: Region, accounts: &[Value]) -> std::io::Result<()> {
    save_accounts_to_path(&accounts_file_for(region), accounts)
}

/// 按 id 或 uid 在 CN 账号库查找账号。
pub fn find_account(account_id: &str) -> Option<Value> {
    find_account_for(Region::Cn, account_id)
}

/// 按 region 在账号库查找账号。
pub fn find_account_for(region: Region, account_id: &str) -> Option<Value> {
    find_account_in(&load_accounts_for(region), account_id)
}

/// 账号展示名（`email` → `nickname` → **客户端明文快照** → `uid` → `unknown`）。
///
/// # 为什么需要「客户端明文快照」这一档（2026-09-24 用户报障：积分统计页账号名变成一串 UUID）
///
/// 客户端 5.6 起把认证文件里的 `account.nickname` 存成 **`$wbEncrypted` 加密信封**
/// （`{"$wbEncrypted":1,"envelope":"…"}`）。导入时该字段被**原样保留**
/// —— 见 `auth_file::imported_account_from_root` 的注释：强转成字符串会让
/// `access_token` 取不到而「导入恒失败」，即便绕过也会在切换写回时毁掉客户端登录态。
///
/// 于是账号库里的 `nickname` 是**对象**、[`get_str`] 读不到，`email` 通常也是 `null`
/// （新版客户端不再写明文邮箱），展示名一路回落到 **uid**：
///
/// ```text
/// 15bf784e-877b-43dd-904c-853dde62b84c     ← 积分统计页「所有账号」下拉 / 账号列
/// ```
///
/// 而明文名字**确实存在**：客户端自己会在 `{userData}/storage/skeleton/account-snapshot.json`
/// 写一份明文骨架快照（首屏用，与认证文件同源、随登录/切换实时重写）。这一档回落就是
/// 把它接上（[`client_snapshot_nickname_for`]：按 uid **逐字比对**、只读本 region 目录）。
///
/// 放在这里而不是各调用点：本函数是全仓「账号 → 展示名」的**唯一出口**（20+ 处调用，
/// 覆盖积分统计、官方用量明细、签到 / 校园 / 活动 / 差旅 / 切换 / CodeBuddy IDE 与 CLI、
/// 网关日志）。任何一处漏改都会表现为「某个页面还是显示 id」，所以只在这一处补。
///
/// # region 从哪来（为什么没有 region 入参）
///
/// 调用点遍布 6 个 crate，绝大多数作用域里**根本没有 region 变量**。region 由
/// **记录自己的 `domain`** 推出（[`region_of`]）—— 这与
/// `auth_file::write_account_to_auth_file_for` 的安全红线 F 用的是**同一个字段、
/// 同一个函数**，属于「按记录自身声明的登录域判定产品线」，不是跨变体的全局探测。
///
/// 失败模式是**安全**的：`domain` 缺失时 [`region_of`] 判为 CN，若该记录其实是国际版账号，
/// 只会去读国内版快照 ⇒ uid 比对不匹配 ⇒ 如实回落 uid（**不会把别人的名字挂上去**）。
///
/// ⚠️ 仍然**不回落到 `email` 之后**再取 `uid`：回落链的最后一段必须留给调用方/前端，
/// 但本函数的契约是「总能给出一个可渲染的字符串」，故以 `uid` 收尾、`unknown` 兜底。
pub fn account_display_name(acc: &Value) -> String {
    get_str(acc, "email")
        .or_else(|| get_str(acc, "nickname"))
        .or_else(|| {
            // 库里昵称是加密信封 ⇒ 按 uid 去客户端明文快照取（同 [`current_nickname_for`]）。
            let uid = get_str(acc, "uid")?;
            let region = region_of(&get_str(acc, "domain").unwrap_or_default());
            client_snapshot_nickname_for(region, &uid)
        })
        .or_else(|| get_str(acc, "uid"))
        .unwrap_or_else(|| "unknown".to_string())
}

/// 账号库里某个 uid 的**展示名**（`nickname` → `email`）；库里没有该 uid 时返回 `None`。
///
/// # 为什么需要它（2026-09-24 用户报障：区域页签显示一串 UUID）
///
/// 新版 WorkBuddy 客户端把认证文件里的 `account.nickname` / `account.phoneNumber`
/// 改成了**加密信封**（`{"$wbEncrypted":1,"envelope":"…"}`，密钥由客户端经 OS
/// 安全存储保管）。本项目不持有那把钥匙，于是 `display_str(nickname)` 恒为 `null`，
/// 界面只能沿 `nickname → email → uid` 一路回落到 **uid**：
///
/// ```text
/// 国内版 WorkBuddy   已登录: 31da0a95-6637-4f5e-adee-f7f08a6f86fd
/// ```
///
/// 而**账号库里本来就有**这个账号的昵称（导入时写入，也可由用户在卡片上维护）。
/// 「当前账号是谁」的判定用的是 **uid**（前端 `isWorkbuddyCurrent` 也只比 uid / email），
/// 所以按 uid 去库里取昵称是**同源**的，不是猜测。
///
/// 与 Trae 侧同构：那边 `profile::current_account_name_for` 同样以「账号库里的
/// `name`」作为当前账号的展示名、查不到才回落 uid。
///
/// ⚠️ **只回落到 `nickname` / `email`，不回落到 uid** —— 拿 uid 当「昵称」会让调用方
/// 无法区分「查到了名字」与「没查到」，回落链的最后一段留给调用方自己决定。
pub fn library_display_name_for(region: Region, uid: &str) -> Option<String> {
    library_display_name_in(&load_accounts_for(region), uid)
}

/// [`library_display_name_for`] 的纯函数形态（显式入参，测试用，不碰磁盘）。
///
/// 与 [`find_account_in`] 同款拆分理由：单测不该依赖进程级 `BUDDY_SWITCH_HOME`。
fn library_display_name_in(accounts: &[Value], uid: &str) -> Option<String> {
    let uid = uid.trim();
    if uid.is_empty() {
        return None;
    }
    let account = find_account_in(accounts, uid)?;
    get_str(&account, "nickname").or_else(|| get_str(&account, "email"))
}

/// 客户端「骨架快照」里的当前登录账号昵称（**明文**）。
///
/// # 为什么需要它
///
/// 客户端 5.6 起把认证文件里的 `account.nickname` 存成 `$wbEncrypted` 加密信封
/// （`{"$wbEncrypted":1,"envelope":"…"}`），本应用不持有那把钥匙 ⇒ [`display_str`] 读不到
/// ⇒ 界面只能显示 uid（用户报障：`已登录: 15bf784e-877b-43dd-904c-853dde62b84c`）。
///
/// 但客户端**自己**会在 `{userData}/storage/skeleton/account-snapshot.json` 写一份
/// **明文**快照（首屏骨架屏用），字段与认证文件**同源**、随登录/切换实时更新：
///
/// ```json
/// { "primary": { "uid": "15bf784e-…", "nickname": "Andev", "savedAt": 1790245680996 } }
/// ```
///
/// # 两道门禁（都不可省）
///
/// 1. **只读 region 自己的目录**（[`session_data_dir`](crate::modules::session::session_data_dir)：
///    CN `.workbuddy` / Global `.workbuddy-ai`）。**不做跨目录兜底** —— 两版账号体系互相隔离
///    是本项目的硬约束（PRD G1「两版互不污染」），为了多命中一个名字而跨版读文件，
///    等于给「串号」留了一条后路。目录映射若有偏差，正确结果是**读不到**，不是换个目录碰运气。
/// 2. **按 uid 逐字比对** `primary.uid`。快照是客户端按「当前登录账号」写的单条记录，
///    若它记的 uid 与认证文件里的不一致（用户刚切号、快照尚未重写），
///    **一律不采用** —— 宁可显示 uid，也不能把上一个账号的名字挂到当前账号上。
pub fn client_snapshot_nickname_for(region: Region, uid: &str) -> Option<String> {
    let uid = uid.trim();
    if uid.is_empty() {
        return None;
    }
    let path = crate::modules::session::session_data_dir(region)
        .join("storage")
        .join("skeleton")
        .join("account-snapshot.json");
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let primary = value.get("primary")?;
    if display_str(primary, "uid").as_str() != Some(uid) {
        return None;
    }
    display_str(primary, "nickname")
        .as_str()
        .map(|s| s.to_string())
}

/// 认证文件的 `account` 对象 → 下发前端的 `status.current.nickname`。
///
/// ## 取值规则（**只有这一处**，两个下发点都调它）
///
/// 1. 认证文件里的 `nickname` 读得到 ⇒ 原样用（归一成「字符串或 null」，见 [`display_str`]）；
/// 2. 读不到（`null`）⇒ 按认证文件里的 **uid** 去客户端**明文快照**取昵称；
/// 3. 快照也没有 ⇒ 去**账号库**取昵称；
/// 4. 都没有 ⇒ `null`，交给前端既有的 `nickname → email → uid` 回落链。
///
/// 为什么第 2 步排在账号库**之前**：两者都是「当前登录账号」的昵称来源，但快照是客户端
/// 每次登录/切换**实时重写**的（与认证文件同一时刻、同一来源），账号库则是历史保存值
/// —— 用户切号后库里可能仍是上一个账号的名字。库里查不到（用户从未点过「从本机导入」）
/// 时这一步更是唯一的明文来源。
///
/// 为什么值得抽成一个函数：第 2/3 条都是**有条件的**回落（只在 `null` 时生效），
/// 若在两个下发点各写一遍，很容易一处写成「无条件覆盖」——
/// 那会让老客户端（`nickname` 仍是明文）的用户看到**过期的旧名字**。
///
/// 背景与「为什么回落是同源的」见 [`library_display_name_for`] / [`client_snapshot_nickname_for`]。
pub fn current_nickname_for(region: Region, acct: &Value) -> Value {
    let from_auth = display_str(acct, "nickname");
    if !from_auth.is_null() {
        return from_auth;
    }
    let uid = match display_str(acct, "uid") {
        Value::String(s) => s,
        // uid 是脏值（对象/数字）⇒ 无从查表，如实落 null，由前端继续回落 email。
        _ => return Value::Null,
    };
    if let Some(name) = client_snapshot_nickname_for(region, &uid) {
        return Value::String(name);
    }
    library_display_name_for(region, &uid)
        .map(Value::String)
        .unwrap_or(Value::Null)
}

/// 账号的展示元数据（不泄露 token）。对照 server.py `account_meta`。
///
/// ⚠️ **展示字段一律过 [`display_str`] 归一**（不要改回 `acc.get(..)` 裸透传）：
/// 这是 issue #2「Win11 打开一片白」的修复点之一，理由见该函数文档。
/// 时间戳字段（`expiresAt` 一族）按契约是**数字**，**不参与**字符串归一。
pub fn account_meta(acc: &Value) -> Value {
    json!({
        "id": display_str(acc, "id"),
        "uid": display_str(acc, "uid"),
        "email": display_str(acc, "email"),
        "nickname": display_str(acc, "nickname"),
        "enterpriseName": display_str(acc, "enterpriseName"),
        // 时间戳：前端 `types.ts` 声明为 `number | null`，且
        // `account-card.tsx` 用 `typeof expiresAt === "number"` 判定过期 —— 不能归一成字符串。
        "expiresAt": acc.get("expiresAt"),
        "refreshExpiresAt": acc.get("refreshExpiresAt"),
        "refreshedAt": acc.get("refreshedAt"),
        "createdAt": acc.get("createdAt"),
        "needsRelogin": acc.get("needs_relogin").and_then(|v| v.as_bool()) == Some(true),
        "needsReloginReason": display_str(acc, "needs_relogin_reason"),
        // 用户备注：自由文本，可能缺失或为 null。前端按「有值才渲染」处理。
        //
        // ⚠️ 这里**没有**「凭据是否加密信封」这个字段：那条判据挂在**失败结果**上
        // （`credits::ENCRYPTED_CREDENTIAL_REASON`，随 `get_credit_expiry_for` 的
        // `reason` 下发）。同一件事只在**一处**编码，才不会两处漂移 ——
        // 账号级布尔要等请求才用得上，而请求本身就会带回原因。
        "remark": display_str(acc, "remark"),
    })
}

/// 账号库记录 → 下发前端的展示元数据（**含昵称回落**）。
///
/// 与 [`account_meta`] 的唯一区别：库里 `nickname` 可能是客户端 5.6 的**加密信封**
/// （导入时**刻意原样保留** —— 见 `auth_file::imported_account_from_root` 的注释：
/// 强转字符串会让 `access_token` 取不到而「导入恒失败」，即便绕过也会在切换写回时
/// 毁掉客户端登录态）。信封读不出名字，卡片标题就只能显示 uid（2026-09-24 用户报障）。
///
/// 因此这里复用 [`current_nickname_for`] 的回落链补一个可读名字：
/// 记录里的明文昵称 → **客户端明文快照**（按 uid 匹配）→ null。
/// 注意回落链里那一档「账号库」对**记录自身**必然落空（库里的昵称就是信封），
/// 所以真正的救兵是客户端快照 —— 这也是它与 `status.current.nickname` 同源的保证。
///
/// ★ **所有下发点一律用本函数，不要直接用 [`account_meta`]**：
/// 漏掉任何一个（备注保存、刷新 token、OAuth 回调、导入返回）都会让该路径下的
/// 卡片标题**退回 uid**，而其余路径正常 —— 表现为「改个备注名字就变回 id」。
pub fn account_meta_for(region: Region, acc: &Value) -> Value {
    let mut meta = account_meta(acc);
    let nickname_missing = meta.get("nickname").map(|v| v.is_null()).unwrap_or(true);
    if nickname_missing {
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("nickname".to_string(), current_nickname_for(region, acc));
        }
    }
    meta
}

/// 取非空字符串字段；空/缺失返回 None。
pub fn get_str(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 展示/标识字段归一：任意 JSON 值 → **「字符串或 null」**。
///
/// # 为什么需要它（issue #2：Win11 打开后一片白）
///
/// 账号相关字段的历史数据里出现过**非字符串**形态 —— 认证文件
/// `account.nickname` 是对象（AI 生成的展示名带 emoji / 特殊 unicode 时，
/// 客户端可能把它存成 `{"zh": "…"}` 这类结构）。
///
/// 而前端拿到这些字段后**直接当 React 子节点渲染**：
/// ```text
/// {workbuddyCurrentName}   // pages/AccountsPage.tsx
/// <h3>{name}</h3>          // components/account-card.tsx
/// ```
/// 对象会让 React 抛 `Objects are not valid as a React child`；当时本仓**没有
/// 错误边界** ⇒ React 在未捕获的渲染错误上卸载**整棵树**（自 React 16 起的行为，
/// 19 的 `createRoot` 默认不变）⇒ 侧栏一起消失 ⇒ 用户看到的是一张**纯白窗口**
/// （而不是「只有 CN 面板空掉」）。
///
/// 所以凡是「要交给前端展示 / 拼进字符串」的字段，出口一律过这道归一：
/// - `String`：原样保留（含空串，语义与历史一致）；
/// - `Number`：转成文本 —— **纯数字昵称是合法数据**（如 `12345`），不能当脏值丢掉；
/// - 其余（对象 / 数组 / 布尔 / null / 缺失）：`null`，交给前端既有的 `||`
///   回落链（`nickname || email || uid || 未知账号`）接管。
///
/// 布尔刻意**不**转成 `"true"`：那会让界面把「字段坏了」显示成一个人名，
/// 属于「静默降级」；落 `null` 才能触发正常的兜底展示。
///
/// ⚠️ **只用于展示与标识字段**。`expiresAt` / `refreshExpiresAt` /
/// `refreshedAt` / `createdAt` 按契约是数字（`src/lib/types.ts` 里
/// `number | null`），**绝不能**过这道归一，否则会破坏过期判定。
pub fn display_str(v: &Value, key: &str) -> Value {
    match v.get(key) {
        Some(Value::String(s)) => Value::String(s.clone()),
        Some(Value::Number(n)) => Value::String(n.to_string()),
        _ => Value::Null,
    }
}

/// 该字段是否为 WorkBuddy 5.6 起的**加密信封**对象（`{"$wbEncrypted":…}`）。
///
/// 密钥由客户端经 OS 安全存储保管，本项目解不开、也不需要解 —— 只要**认得出来**，
/// 就能在「展示」与「凭据」两条出路上分别做对的事（见 [`secret_value`]）。
pub fn is_envelope(v: &Value, key: &str) -> bool {
    matches!(v.get(key), Some(Value::Object(map)) if map.contains_key("$wbEncrypted"))
}

/// **凭据**字段读取：明文字符串原样、加密信封**也原样**，其余类型 `None`。
///
/// # 为什么必须与 [`display_str`] 分成两条出路
///
/// 同一份信封在两个消费者面前要求**相反**：
///
/// | 出路 | 函数 | 规则 | 理由 |
/// | --- | --- | --- | --- |
/// | 展示（给人看） | [`display_str`] | 对象 / 数组 → `null` | 脏值当 React 子节点会卸载整棵树 |
/// | 凭据（喂给 API） | 本函数 | 信封 → **原样保留** | 原样写回客户端它能自己解；折叠成 `null` 等于把凭据丢掉 |
///
/// ⚠️ 本仓曾经把这两条出路**混成一条**（全程 `get_str`）。后果是「白屏修好了、
/// 功能坏了」：`get_str` 只认字符串 ⇒ 信封 token 读成 `None` ⇒
/// ①「导入本机账号」恒失败（`access_token.is_none()` 直接 `return None`）、
/// ② 切换写回把客户端认证文件里的 `accessToken` 抹成 `""`。
///
/// **判别式：看到「某字段变成对象了」先问它下游是 `React 子节点` 还是 `Authorization` 头。**
/// 只归一展示侧、把凭据侧也一并 `null` 掉，就是这条缺陷的形态。
pub fn secret_value(v: &Value, key: &str) -> Option<Value> {
    match v.get(key) {
        Some(s @ Value::String(_)) => Some(s.clone()),
        Some(o @ Value::Object(map)) if map.contains_key("$wbEncrypted") => Some(o.clone()),
        _ => None,
    }
}

/// 返回可用于 UID 缺失场景的真实邮箱。历史展示占位值不参与身份匹配。
fn identity_email(account: &Value) -> Option<String> {
    let email = get_str(account, "email")?;
    if !email.contains('@')
        || email.eq_ignore_ascii_case("unknown")
        || email == "手动添加"
        || get_str(account, "nickname").as_deref() == Some(email.as_str())
        || get_str(account, "uid").as_deref() == Some(email.as_str())
    {
        return None;
    }
    Some(email.to_ascii_lowercase())
}

/// 该记录是否持有**未过期的明文** `access_token`（OAuth 扫码所得、或我方 refresh 写入的形态）。
///
/// 缺 `expiresAt` 时按**有效**处理：不能因为少一个字段就把明文凭据让给信封。
fn has_unexpired_plain_token(acc: &Value) -> bool {
    let Some(Value::String(token)) = acc.get("access_token") else {
        return false;
    };
    if token.trim().is_empty() {
        return false;
    }
    match acc.get("expiresAt").and_then(Value::as_i64) {
        Some(expires_at) => expires_at > now_ms(),
        None => true,
    }
}

/// 按稳定身份将采集结果合并到账号列表，并返回最终持久化的账号。
///
/// 非空 UID 始终优先；仅当新账号没有 UID 时，才使用真实邮箱兜底。
/// 命中已有身份时保留本地 id，避免调用方持有的账号引用失效。
pub fn upsert_collected_account(accounts: &mut Vec<Value>, mut collected: Value) -> Value {
    let collected_uid = get_str(&collected, "uid");
    let collected_email = identity_email(&collected);
    let matches_identity = |existing: &Value| {
        if let Some(uid) = collected_uid.as_deref() {
            return get_str(existing, "uid").as_deref() == Some(uid);
        }
        collected_email
            .as_deref()
            .is_some_and(|email| identity_email(existing).as_deref() == Some(email))
    };

    let matching_indexes: Vec<usize> = accounts
        .iter()
        .enumerate()
        .filter_map(|(index, existing)| matches_identity(existing).then_some(index))
        .collect();

    if let Some(&first_index) = matching_indexes.first() {
        let existing = &accounts[first_index];

        // ── 加密信封的两条保护（WorkBuddy 5.6 起）────────────────────────────
        // 本机重导入（UI 自动 `importLocal`）拿到的是**加密信封** token，而已有
        // 记录可能持有**未过期的明文** token（OAuth 扫码所得，或我方 refresh 写入）。
        // 让信封覆盖明文 ⇒ 签到 / 积分 / 网关这些需要明文 Bearer 的链路**静默失效**
        // （不报错，只是每个请求都退化成空 Bearer）。明文过期后才放行信封接管。
        //
        // 命中时**整体保留**已有记录、本次采集全部丢弃：本次采集只比它多了信封，
        // 而 id / createdAt / remark / 明文凭据都只在已有记录里。
        if is_envelope(&collected, "access_token") && has_unexpired_plain_token(existing) {
            return existing.clone();
        }
        // 展示字段兜底：新采集是信封时保留已有记录的明文展示值 ——
        // 昵称信封我方解不开，用信封覆盖明文等于把名字丢掉。
        for key in ["nickname", "email", "enterpriseName"] {
            if is_envelope(&collected, key) {
                if let Some(existing_value) = existing.get(key) {
                    collected[key] = existing_value.clone();
                }
            }
        }

        if let Some(existing_id) = existing.get("id").cloned() {
            collected["id"] = existing_id;
        }
        if get_str(&collected, "uid").is_none() {
            if let Some(existing_uid) = existing.get("uid").cloned() {
                collected["uid"] = existing_uid;
            }
        }
        if let Some(created_at) = existing.get("createdAt").cloned() {
            collected["createdAt"] = created_at;
        }
        // 备注是**本地标注**，采集结果里永远不会有它；不显式带回就会丢，
        // 且触发场景很常见：重新扫码登录同一个账号、再次「导入本机账号」。
        if let Some(remark) = existing.get("remark").cloned() {
            collected["remark"] = remark;
        }

        for index in matching_indexes.into_iter().rev() {
            accounts.remove(index);
        }
        accounts.insert(first_index.min(accounts.len()), collected.clone());
    } else {
        accounts.push(collected.clone());
    }

    collected
}

/// 使用统一身份规则保存采集到的账号（CN）。
pub fn save_collected_account(collected: Value) -> std::io::Result<Value> {
    save_collected_account_for(Region::Cn, collected)
}

/// 按 region 使用统一身份规则保存采集到的账号。
pub fn save_collected_account_for(region: Region, collected: Value) -> std::io::Result<Value> {
    let mut accounts = load_accounts_for(region);
    let saved = upsert_collected_account(&mut accounts, collected);
    save_accounts_for(region, &accounts)?;
    Ok(saved)
}

/// 按 id 覆盖写入 CN 账号库（不存在则追加）。对照 server.py `_upsert_account`。
pub fn upsert_account(updated: &Value) -> std::io::Result<()> {
    upsert_account_for(Region::Cn, updated)
}

/// 按 region 覆盖写入账号库（不存在则追加）。
pub fn upsert_account_for(region: Region, updated: &Value) -> std::io::Result<()> {
    let mut accounts = load_accounts_for(region);
    let id = updated.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let mut replaced = false;
    for a in accounts.iter_mut() {
        if a.get("id").and_then(|v| v.as_str()) == Some(id) {
            *a = updated.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        accounts.push(updated.clone());
    }
    save_accounts_for(region, &accounts)
}

/// 该账号是否**只有加密信封凭据**（`access_token` 是信封对象）。
///
/// 返回 `Some(可读文案)` 表示「这个账号不能用于需要账号身份的请求」，调用方应
/// **在发请求之前**短路。
///
/// # 为什么必须前置短路（2026-09-24 对照上游 PR #95）
///
/// 信封 token 我方解不开 ⇒ [`build_auth_headers`] / [`build_chat_headers`] 的
/// `get_str(..).unwrap_or_default()` 会把它**静默兜底成空 `Bearer`** ⇒ 上游 401
/// ⇒ 网关的整页 HTML 错误页被回显到界面。「发出去等 401」比「发之前就拦住」坏得多：
/// 前者用户看到的是 openresty 的英文错误页，后者是一句能照着做的中文提示。
///
/// ⚠️ **只认 `access_token`**：`refresh_token` 是信封时由刷新链路自己报
/// 「缺少 refresh token，无法刷新，需重新登录」，不在这里重复拦截 ——
/// 否则会把「还能靠 refresh 救回来」的账号一并封死。
pub fn envelope_token_error(account: &Value) -> Option<String> {
    if !is_envelope(account, "access_token") {
        return None;
    }
    Some(
        "该账号的凭据是 WorkBuddy 加密信封态，无法直接调用签到 / 积分 / Token 统计等接口；\
         切换账号不受影响。点「刷新 Token」可用本机客户端解锁（需已安装 WorkBuddy 桌面端）；\
         也可重新登录该账号（或用 OAuth 扫码添加）以取得明文凭据。"
            .to_string(),
    )
}

/// **刷新 / 保活链路**专用的信封判定：`refresh_token` 是加密信封时给出可读原因。
///
/// # 为什么必须与 [`envelope_token_error`] 分开（2026-09-24 用户截图）
///
/// 两者**判据字段不同、处置也不同**：
///
/// | 函数 | 判据 | 调用方 |
/// | --- | --- | --- |
/// | [`envelope_token_error`] | `access_token` | 签到 / 积分 / 旅行 / 网关（要明文 `Bearer`） |
/// | 本函数 | `refresh_token` | 刷新 / 保活（要明文 refresh token） |
///
/// 信封是**对象**，`get_str` / `as_str()` 取不到值。刷新链路原先只判「取不到字符串」⇒
/// 把「**已加密**」误报成「**缺少** refresh token」：用户看到的是
/// 「Andev：需重新登录（缺少 refresh token，无法刷新）」，而账号库里 `refresh_token`、
/// `refreshExpiresAt` 都在 —— 报错与事实相反，用户照着提示也修不好。
///
/// 两种情形的处置本来也不同：**真缺失** ⇒ 该查导入来源（认证文件 / 导入流程）；
/// **信封** ⇒ 本项目永远解不开，只能重新登录换明文凭据。
///
/// ⚠️ 反向**不**成立：`access_token` 是信封时刷新**仍可能成功**（刷新端点只认
/// `X-Refresh-Token`，见 `upstream::refresh_token`），故这里**不看** `access_token`，
/// 不得据此把还能救回来的账号拦掉（与 [`envelope_token_error`] 的边界一致）。
pub fn envelope_refresh_error(account: &Value) -> Option<String> {
    if !is_envelope(account, "refresh_token") {
        return None;
    }
    Some(
        "凭据是 WorkBuddy 加密信封态（refresh token 已加密、本应用无法解密），\
         需重新登录该账号（或用 OAuth 扫码添加）以取得明文凭据"
            .to_string(),
    )
}

/// 构造与官方对齐的请求头。对照 server.py `build_auth_headers`。
///
/// ⚠️ `access_token` 是加密信封对象时 `get_str` 取不到值，这里会产出**空 `Bearer`**。
/// 调用方必须先用 [`envelope_token_error`] 拦截，不要把空凭据真的发出去。
pub fn build_auth_headers(account: &Value) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert(
        "Authorization".to_string(),
        format!(
            "Bearer {}",
            get_str(account, "access_token").unwrap_or_default()
        ),
    );
    headers.insert("Accept".to_string(), "application/json".to_string());
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    if let Some(uid) = get_str(account, "uid") {
        headers.insert("X-User-Id".to_string(), uid);
    }
    if let Some(eid) =
        get_str(account, "enterpriseId").or_else(|| get_str(account, "enterprise_id"))
    {
        headers.insert("X-Enterprise-Id".to_string(), eid.clone());
        headers.insert("X-Tenant-Id".to_string(), eid);
    }
    if let Some(domain) = get_str(account, "domain") {
        headers.insert("X-Domain".to_string(), domain);
    }
    headers
}

/// 构造 chat 请求头（region 化，含 X-No-* 缺省约定与 `X-Product: SaaS`）。
///
/// 对照参考实现 `chatHeaders`。**安全红线：chat 请求绝不携带 refresh token。**
/// 缺失的身份字段用官方 CLI 的 `X-No-*` 约定表达，而非省略 header。
///
/// 与 [`build_auth_headers`] 的差异（对齐官方客户端行为）：
/// - `Accept` 声明 `text/event-stream`（chat 端点恒为 SSE）；
/// - 附带 `Accept-Language`（CN `zh-CN` / Global `en-US`）；
/// - 附带 `X-CodeBuddy-Request: 1` 与 `X-Agent-Purpose: conversation` 归属头。
pub fn build_chat_headers(region: Region, account: &Value) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    let origin = region_spec(region).billing_base;
    let accept_language = match region {
        Region::Global => "en-US",
        Region::Cn => "zh-CN",
    };
    headers.insert(
        "Accept".to_string(),
        "application/json, text/event-stream".to_string(),
    );
    headers.insert("Accept-Language".to_string(), accept_language.to_string());
    headers.insert("X-CodeBuddy-Request".to_string(), "1".to_string());
    headers.insert(
        "X-Agent-Purpose".to_string(),
        "conversation".to_string(),
    );
    headers.insert("X-Requested-With".to_string(), "XMLHttpRequest".to_string());
    headers.insert("Origin".to_string(), origin.to_string());
    headers.insert("Referer".to_string(), format!("{origin}/"));
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert(
        "Authorization".to_string(),
        format!(
            "Bearer {}",
            get_str(account, "access_token").unwrap_or_default()
        ),
    );
    match get_str(account, "uid") {
        Some(uid) => {
            headers.insert("X-User-Id".to_string(), uid);
        }
        None => {
            headers.insert("X-No-User-Id".to_string(), "1".to_string());
        }
    }
    match get_str(account, "enterpriseId").or_else(|| get_str(account, "enterprise_id")) {
        Some(eid) => {
            headers.insert("X-Enterprise-Id".to_string(), eid);
        }
        None => {
            headers.insert("X-No-Enterprise-Id".to_string(), "1".to_string());
        }
    }
    match get_str(account, "domain") {
        Some(domain) => {
            headers.insert("X-Domain".to_string(), domain);
        }
        None => {
            headers.insert("X-No-Department-Info".to_string(), "1".to_string());
        }
    }
    headers.insert("X-Product".to_string(), "SaaS".to_string());
    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn account_meta_strips_tokens() {
        let acc = json!({
            "id": "a1",
            "uid": "u1",
            "email": "x@y.z",
            "nickname": "小明",
            "enterpriseName": "某公司",
            "access_token": "SECRET_ACCESS",
            "refresh_token": "SECRET_REFRESH",
            "expiresAt": 123456,
            "needs_relogin": true,
            "needs_relogin_reason": "刷新失败",
        });
        let meta = account_meta(&acc);
        assert_eq!(meta["id"], "a1");
        assert_eq!(meta["needsRelogin"], true);
        assert_eq!(meta["needsReloginReason"], "刷新失败");
        assert!(meta.get("access_token").is_none(), "不得泄露 token");
        assert!(meta.get("refresh_token").is_none(), "不得泄露 token");
    }

    /// 明文回落链的**优先级**（不碰磁盘的那几档）。
    ///
    /// ⚠️ 只覆盖**早退**分支：`email` / 明文 `nickname` 命中时不会去读客户端快照。
    /// 需要落盘的几档（加密信封 → 快照 / uid）在下面那条用例里，且必须带隔离 home ——
    /// 否则这条纯断言会变成「读开发者真实 `~/.workbuddy`」的隐式依赖。
    #[test]
    fn account_display_name_priority() {
        assert_eq!(
            account_display_name(&json!({"email": "a@b.c", "nickname": "n"})),
            "a@b.c"
        );
        assert_eq!(
            account_display_name(&json!({"nickname": "n", "uid": "u"})),
            "n"
        );
        assert_eq!(account_display_name(&json!({})), "unknown");
    }

    /// ★ 现场（2026-09-24 用户截图）：积分统计页「所有账号」下拉与账号列显示的是
    /// `15bf784e-877b-43dd-904c-853dde62b84c` / `2fa83368-ebd6-4efe-8cd7-4f43b1c31571`
    /// —— 正是账号库记录里的 **uid**。
    ///
    /// 根因：导入时 `nickname` 被**原样保留**成客户端 5.6 的加密信封
    /// （`{"$wbEncrypted":1,…}`，见 `auth_file::imported_account_from_root`），
    /// `email` 也是 `null` ⇒ `get_str` 全落空 ⇒ 展示名回落到 uid。
    /// 明文名字在客户端自己写的骨架快照里，必须接上这一档。
    ///
    /// 可证伪：删掉 `account_display_name` 里那段 `client_snapshot_nickname_for`，
    /// 第 1/3 段即红（返回 uid 而不是名字）。
    #[test]
    fn account_display_name_falls_back_to_client_snapshot_for_envelope_nickname() {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-display-name-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");
        let guard = crate::modules::config::HomeOverrideGuard::set(&dir);

        let envelope = json!({"$wbEncrypted": 1, "envelope": "eyJzdWl0ZSI6MSw…"});
        // 与真机同形：`email` 为 null、`nickname` 是信封、只有 uid 可读。
        let cn_account = json!({
            "id": "ca53b23f-b7df-44de-9abc-645807ac7249",
            "uid": "15bf784e-877b-43dd-904c-853dde62b84c",
            "email": null,
            "nickname": envelope,
            "domain": "www.codebuddy.cn",
        });

        // 1) 信封 + 快照命中 ⇒ 显示明文名字（本次修复的目标行为）。
        write_client_snapshot(Region::Cn, "15bf784e-877b-43dd-904c-853dde62b84c", json!("Andev"));
        assert_eq!(
            account_display_name(&cn_account),
            "Andev",
            "账号库昵称是加密信封时必须回落到客户端明文快照，否则界面只能显示 uid"
        );

        // 2) 阴性对照：快照记的是**别的** uid（刚切号、快照尚未重写）⇒ 弃用，
        //    如实回落 uid。宁可显示 uid，也不能把上一个账号的名字挂上来。
        write_client_snapshot(Region::Cn, "some-other-uid", json!("别人的名字"));
        assert_eq!(
            account_display_name(&cn_account),
            "15bf784e-877b-43dd-904c-853dde62b84c",
            "快照 uid 不匹配时不得采用"
        );

        // 3) 区域隔离：快照只写在国际版目录 ⇒ 国内版账号不得采用。
        let global_account = json!({
            "id": "6131cab4-6e53-41a4-8a31-64af45963bf9",
            "uid": "2fa83368-ebd6-4efe-8cd7-4f43b1c31571",
            "email": null,
            "nickname": envelope,
            "domain": "www.workbuddy.ai",
        });
        write_client_snapshot(Region::Global, "2fa83368-ebd6-4efe-8cd7-4f43b1c31571", json!("nextagentx"));
        assert_eq!(
            account_display_name(&global_account),
            "nextagentx",
            "国际版账号必须读国际版目录的快照"
        );
        assert_eq!(
            account_display_name(&cn_account),
            "15bf784e-877b-43dd-904c-853dde62b84c",
            "国内版账号不得读国际版目录的快照（两版账号体系互不污染）"
        );

        // 4) `domain` 缺失 ⇒ `region_of(\"\")` 判为 CN；此时读不到名字，如实回落 uid，
        //    **不会**把 CN 快照里别人的名字挂上去。
        write_client_snapshot(Region::Cn, "15bf784e-877b-43dd-904c-853dde62b84c", json!("Andev"));
        let no_domain = json!({
            "uid": "15bf784e-877b-43dd-904c-853dde62b84c",
            "nickname": envelope,
        });
        assert_eq!(account_display_name(&no_domain), "Andev", "缺 domain 时按 CN 处理");

        // 5) 阳性对照（**这条防的是「无条件覆盖」**）：记录里有明文昵称时，
        //    快照与账号库都不得覆盖它 —— 否则老客户端用户会看到过期的旧名字。
        let plaintext = json!({
            "uid": "15bf784e-877b-43dd-904c-853dde62b84c",
            "nickname": "记录里的明文名字",
            "domain": "www.codebuddy.cn",
        });
        assert_eq!(account_display_name(&plaintext), "记录里的明文名字");

        // 6) `email` 仍然优先于一切（历史契约不变）。
        let with_email = json!({
            "uid": "15bf784e-877b-43dd-904c-853dde62b84c",
            "email": "someone@example.com",
            "nickname": envelope,
            "domain": "www.codebuddy.cn",
        });
        assert_eq!(account_display_name(&with_email), "someone@example.com");

        // 7) uid 是脏值（对象）⇒ 无从查快照，但仍有 `unknown` 兜底（可渲染的字符串）。
        let dirty_uid = json!({"uid": {"nested": true}, "nickname": envelope});
        assert_eq!(account_display_name(&dirty_uid), "unknown");

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_str_trims_and_filters_empty() {
        assert_eq!(get_str(&json!({"k": "  v  "}), "k"), Some("v".to_string()));
        assert_eq!(get_str(&json!({"k": "  "}), "k"), None);
        assert_eq!(get_str(&json!({"k": 123}), "k"), None);
    }

    /// [`display_str`] 的形状不变量：**只可能产出字符串或 null**。
    ///
    /// 这是 issue #2「Win11 打开一片白」的根因护栏 —— 前端把这些字段直接当
    /// React 子节点渲染，任何非字符串形态都会让 React 卸载整棵树。
    #[test]
    fn display_str_normalizes_to_string_or_null() {
        // 字符串原样保留（含空串：语义与历史一致，前端 `||` 回落链会接管）
        assert_eq!(display_str(&json!({"k": "v"}), "k"), json!("v"));
        assert_eq!(display_str(&json!({"k": ""}), "k"), json!(""));
        // 数字 → 文本：纯数字昵称是**合法数据**，不能当脏值丢掉
        assert_eq!(display_str(&json!({"k": 12345}), "k"), json!("12345"));
        assert_eq!(display_str(&json!({"k": 1.5}), "k"), json!("1.5"));
        // 对象 / 数组 / 布尔 / null / 缺失 → null（布尔刻意不转 "true"：那会把
        // 「字段坏了」显示成一个人名，属于静默降级）
        assert_eq!(display_str(&json!({"k": {"a": 1}}), "k"), Value::Null);
        assert_eq!(display_str(&json!({"k": [1, 2]}), "k"), Value::Null);
        assert_eq!(display_str(&json!({"k": true}), "k"), Value::Null);
        assert_eq!(display_str(&json!({"k": null}), "k"), Value::Null);
        assert_eq!(display_str(&json!({}), "k"), Value::Null);
    }

    /// 归一的**反向对照**：时间戳字段必须保持数字，不能被顺手字符串化。
    ///
    /// `src/lib/types.ts` 声明 `expiresAt: number | null`，且 `account-card.tsx`
    /// 用 `typeof account.expiresAt === "number"` 判定是否过期 —— 一旦变成
    /// `"123456"`，过期提示会**静默消失**（不报错、只是不再出现）。
    #[test]
    fn account_meta_keeps_timestamps_numeric() {
        let meta = account_meta(&json!({
            "id": "a1",
            "expiresAt": 123456,
            "refreshExpiresAt": 234567,
            "refreshedAt": 345678,
            "createdAt": 456789,
        }));
        assert_eq!(meta["expiresAt"], json!(123456));
        assert_eq!(meta["refreshExpiresAt"], json!(234567));
        assert_eq!(meta["refreshedAt"], json!(345678));
        assert_eq!(meta["createdAt"], json!(456789));
    }

    #[test]
    fn build_chat_headers_uses_no_star_conventions_and_never_carries_refresh_token() {
        let acc = json!({
            "access_token": "AT",
            "refresh_token": "SECRET_REFRESH",
            "uid": "u1",
            "domain": "www.codebuddy.cn",
        });
        let headers = build_chat_headers(Region::Cn, &acc);
        assert_eq!(headers.get("X-User-Id").map(String::as_str), Some("u1"));
        assert_eq!(headers.get("X-Domain").map(String::as_str), Some("www.codebuddy.cn"));
        assert_eq!(headers.get("X-No-Enterprise-Id").map(String::as_str), Some("1"));
        assert_eq!(headers.get("X-Product").map(String::as_str), Some("SaaS"));
        assert_eq!(headers.get("Origin").map(String::as_str), Some("https://www.codebuddy.cn"));
        assert_eq!(headers.get("Authorization").map(String::as_str), Some("Bearer AT"));
        // 安全红线：chat 头绝不携带 refresh token。
        assert!(!headers.contains_key("X-Refresh-Token"));
        assert!(!headers.values().any(|v| v == "SECRET_REFRESH"));
    }

    #[test]
    fn build_chat_headers_marks_missing_identity_with_no_flags() {
        let acc = json!({"access_token": "AT"});
        let headers = build_chat_headers(Region::Global, &acc);
        assert_eq!(headers.get("X-No-User-Id").map(String::as_str), Some("1"));
        assert_eq!(headers.get("X-No-Enterprise-Id").map(String::as_str), Some("1"));
        assert_eq!(headers.get("X-No-Department-Info").map(String::as_str), Some("1"));
        assert_eq!(headers.get("Origin").map(String::as_str), Some("https://www.workbuddy.ai"));
    }

    fn account(id: &str, uid: Option<&str>, nickname: &str, email: Option<&str>) -> Value {
        json!({
            "id": id,
            "uid": uid,
            "nickname": nickname,
            "email": email,
            "access_token": format!("token-{id}"),
            "createdAt": 1,
        })
    }

    /// WorkBuddy 账号库（JSON 数组）的旧记录必须仍可读。
    ///
    /// 账号记录整体是 `serde_json::Value`，因此天然前向/后向兼容；这条测试钉住
    /// 「历史字段缺失、以及未来新增未知字段都不影响读取」，防止有人日后收紧成
    /// 强类型 struct 而把老账号库读空。
    #[test]
    fn workbuddy_accounts_tolerate_legacy_and_unknown_fields() {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-accounts-compat-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("accounts.json");

        // 混合：完整记录 / 仅最小字段的历史记录 / 带未知新字段的记录。
        let text = r#"[
          {
            "id": "a1", "uid": "u1", "email": "full@example.com", "nickname": "完整",
            "access_token": "AT", "refresh_token": "RT",
            "expiresAt": 123456, "createdAt": 1
          },
          { "id": "a2", "uid": "u2", "nickname": "最小历史记录", "access_token": "AT2" },
          { "id": "a3", "uid": "u3", "email": "new@example.com", "access_token": "AT3",
            "brand_new_field": {"nested": true}, "another": [1, 2, 3] }
        ]"#;
        std::fs::write(&path, text).expect("write accounts");

        let accounts = load_accounts_from_path(&path);
        assert_eq!(accounts.len(), 3, "三条记录都必须被读出");
        assert_eq!(
            find_account_in(&accounts, "a2").unwrap()["nickname"],
            "最小历史记录"
        );
        assert_eq!(find_account_in(&accounts, "a3").unwrap()["uid"], "u3");

        // 旧的 `needs_relogin` 布尔标志仍要能映射到线上 camelCase
        let meta = account_meta(&json!({
            "id": "a4", "uid": "u4",
            "needs_relogin": true, "needs_relogin_reason": "刷新失败"
        }));
        assert_eq!(meta["needsRelogin"], true);
        assert_eq!(meta["needsReloginReason"], "刷新失败");

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    /// 账号库里的展示名：`nickname` → `email`，查不到就是 `None`（**不回落到 uid**）。
    ///
    /// 最后一条断言是刻意的：`None` 与「拿 uid 当名字」必须可区分，
    /// 否则调用方（`current_nickname_for`）无法决定要不要继续往下回落。
    #[test]
    fn library_display_name_prefers_nickname_then_email_and_never_uid() {
        let accounts = vec![
            account("a1", Some("uid-1"), "小明", Some("m@example.com")),
            account("a2", Some("uid-2"), "", Some("only-mail@example.com")),
            account("a3", Some("uid-3"), "", None),
        ];

        assert_eq!(
            library_display_name_in(&accounts, "uid-1").as_deref(),
            Some("小明")
        );
        // 昵称为空 ⇒ 用邮箱。
        assert_eq!(
            library_display_name_in(&accounts, "uid-2").as_deref(),
            Some("only-mail@example.com")
        );
        // 昵称与邮箱都没有 ⇒ `None`，**不是** uid。
        assert_eq!(library_display_name_in(&accounts, "uid-3"), None);
        // 库里没有该 uid ⇒ `None`（不能凭空造名字）。
        assert_eq!(library_display_name_in(&accounts, "uid-404"), None);
        // 空 / 全空白 uid 不得命中任何条目。
        assert_eq!(library_display_name_in(&accounts, ""), None);
        assert_eq!(library_display_name_in(&accounts, "   "), None);
        // 脏值（对象）不算名字。
        let dirty = vec![json!({"uid": "uid-4", "nickname": {"zh": "对象昵称"}})];
        assert_eq!(library_display_name_in(&dirty, "uid-4"), None);
    }

    /// ★ 新版客户端的加密昵称 ⇒ `status.current.nickname` 必须回落账号库，而不是显示 uid。
    ///
    /// 现场（2026-09-24 用户截图）：`%APPDATA%\...\workbuddy-desktop.info` 里
    /// `account.nickname` 是 `{"$wbEncrypted":1,"envelope":"…"}`，`display_str` 给 `null`，
    /// 界面于是显示 `已登录: 31da0a95-6637-4f5e-adee-f7f08a6f86fd`。
    #[test]
    fn current_nickname_falls_back_to_library_for_encrypted_auth_file_name() {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-current-nickname-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");
        let guard = crate::modules::config::HomeOverrideGuard::set(&dir);

        save_accounts_for(
            Region::Cn,
            &[
                account("a1", Some("uid-1"), "Jackey", None),
                account("a2", Some("uid-2"), "", Some("fallback@example.com")),
            ],
        )
        .expect("seed accounts");

        // 1) 加密信封（对象）⇒ 读不到 ⇒ 回落账号库。
        let encrypted = json!({
            "uid": "uid-1",
            "nickname": {"$wbEncrypted": 1, "envelope": "eyJzdWl0ZSI6MSw…"},
        });
        assert_eq!(
            current_nickname_for(Region::Cn, &encrypted),
            json!("Jackey"),
            "认证文件读不到昵称时必须回落账号库，否则界面只能显示 uid"
        );

        // 2) 库里只有邮箱 ⇒ 回落到邮箱。
        let encrypted_no_nickname = json!({"uid": "uid-2", "nickname": {"$wbEncrypted": 1}});
        assert_eq!(
            current_nickname_for(Region::Cn, &encrypted_no_nickname),
            json!("fallback@example.com")
        );

        // 3) 库里也没有 ⇒ `null`，把最后一段回落（→ email → uid）留给前端。
        let unknown = json!({"uid": "uid-404", "nickname": {"$wbEncrypted": 1}});
        assert_eq!(current_nickname_for(Region::Cn, &unknown), Value::Null);

        // 4) 阳性对照（**这条防的是「无条件覆盖」**）：老客户端明文昵称必须原样用，
        //    哪怕账号库里对同一个 uid 存着另一个（过期的）名字。
        let plaintext = json!({"uid": "uid-1", "nickname": "认证文件里的名字"});
        assert_eq!(
            current_nickname_for(Region::Cn, &plaintext),
            json!("认证文件里的名字"),
            "认证文件读得到昵称时不得被账号库覆盖"
        );

        // 5) uid 是脏值（对象）⇒ 无从查库，如实落 null。
        let dirty_uid = json!({"uid": {"nested": true}, "nickname": {"$wbEncrypted": 1}});
        assert_eq!(current_nickname_for(Region::Cn, &dirty_uid), Value::Null);

        // 6) 跨区域不得串库：国际版的 uid 不在国内库里 ⇒ null。
        assert_eq!(
            current_nickname_for(Region::Global, &encrypted),
            Value::Null,
            "国内账号库的昵称不得泄漏到国际版视图"
        );

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 写一份客户端「骨架快照」（`{userData}/storage/skeleton/account-snapshot.json`）。
    ///
    /// 路径**必须**走 `session_data_dir`（与生产代码同源），不要手拼 `.workbuddy` /
    /// `.workbuddy-ai` —— 那样一旦目录约定变更，测试会继续绿而生产已失效。
    fn write_client_snapshot(region: Region, uid: &str, nickname: Value) {
        let dir = crate::modules::session::session_data_dir(region)
            .join("storage")
            .join("skeleton");
        std::fs::create_dir_all(&dir).expect("create snapshot dir");
        let body = json!({
            "primary": {"version": 1, "uid": uid, "nickname": nickname, "savedAt": 1},
        });
        std::fs::write(
            dir.join("account-snapshot.json"),
            serde_json::to_string_pretty(&body).expect("serialize snapshot"),
        )
        .expect("write snapshot");
    }

    /// ★ 现场（2026-09-24 用户截图）：客户端 5.6 的认证文件里 `nickname` 是加密信封
    /// （`$wbEncrypted`），而账号库可能**为空**（用户从未点过「从本机导入」）⇒
    /// 界面只能显示 uid（`已登录: 15bf784e-…`）。客户端自己写的**明文**骨架快照
    /// 必须能补上这个名字。
    #[test]
    fn current_nickname_falls_back_to_client_plaintext_snapshot() {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-snapshot-nickname-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");
        let guard = crate::modules::config::HomeOverrideGuard::set(&dir);

        let encrypted = json!({
            "uid": "uid-1",
            "nickname": {"$wbEncrypted": 1, "envelope": "eyJzdWl0ZSI6MSw…"},
        });

        // 1) 账号库为空 ⇒ 唯一的明文来源就是客户端快照。
        write_client_snapshot(Region::Cn, "uid-1", json!("Andev"));
        assert_eq!(
            current_nickname_for(Region::Cn, &encrypted),
            json!("Andev"),
            "认证文件是加密信封、账号库为空时，必须回落到客户端明文快照，否则界面只能显示 uid"
        );

        // 2) 阴性对照：快照记的 uid 与认证文件**不符**（刚切号、快照尚未重写）⇒
        //    一律不采用，不得把上一个账号的名字挂上来；此时回落到账号库。
        save_accounts_for(
            Region::Cn,
            &[account("a1", Some("uid-1"), "库里的名字", None)],
        )
        .expect("seed accounts");
        write_client_snapshot(Region::Cn, "uid-OTHER", json!("别人的名字"));
        assert_eq!(
            current_nickname_for(Region::Cn, &encrypted),
            json!("库里的名字"),
            "快照 uid 不匹配时必须弃用"
        );

        // 3) uid 匹配时快照**优先于**账号库：快照是客户端每次登录/切换实时重写的，
        //    库里可能还留着上一个账号的历史值。
        write_client_snapshot(Region::Cn, "uid-1", json!("快照里的新名字"));
        assert_eq!(
            current_nickname_for(Region::Cn, &encrypted),
            json!("快照里的新名字")
        );

        // 4) ★ 区域隔离：快照只写在国际版目录时，**国内版视图不得采用**
        //    （两版账号体系互不污染，不得为了多命中一个名字而跨版读文件）。
        let cn_snapshot = crate::modules::session::session_data_dir(Region::Cn)
            .join("storage")
            .join("skeleton")
            .join("account-snapshot.json");
        std::fs::remove_file(&cn_snapshot).expect("remove cn snapshot");
        write_client_snapshot(Region::Global, "uid-1", json!("国际版目录里的名字"));
        assert_eq!(
            current_nickname_for(Region::Cn, &encrypted),
            json!("库里的名字"),
            "国内版视图不得读国际版目录的快照"
        );

        // 5) 快照昵称是脏值（对象）⇒ 不算名字，回落账号库。
        write_client_snapshot(Region::Cn, "uid-1", json!({"zh": "对象昵称"}));
        assert_eq!(
            current_nickname_for(Region::Cn, &encrypted),
            json!("库里的名字")
        );

        // 6) 阳性对照（**这条防的是「无条件覆盖」**）：认证文件有明文昵称时，
        //    快照与账号库都不得覆盖它。
        let plaintext = json!({"uid": "uid-1", "nickname": "认证文件里的名字"});
        assert_eq!(
            current_nickname_for(Region::Cn, &plaintext),
            json!("认证文件里的名字")
        );

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_nickname_with_different_uids_is_retained() {
        let mut accounts = vec![account("old", Some("uid-1"), "同名", Some("同名"))];
        let saved =
            upsert_collected_account(&mut accounts, account("new", Some("uid-2"), "同名", None));

        assert_eq!(accounts.len(), 2);
        assert_eq!(saved["id"], "new");
    }

    #[test]
    fn same_uid_refresh_preserves_local_id_and_removes_duplicates() {
        let mut accounts = vec![
            account("stable", Some("uid-1"), "旧名称", Some("old@example.com")),
            account("duplicate", Some("uid-1"), "重复记录", None),
        ];
        let saved = upsert_collected_account(
            &mut accounts,
            account("generated", Some("uid-1"), "新名称", None),
        );

        assert_eq!(accounts.len(), 1);
        assert_eq!(saved["id"], "stable");
        assert_eq!(saved["nickname"], "新名称");
        assert_eq!(saved["access_token"], "token-generated");
    }

    // ── 加密信封（WorkBuddy 5.6）：凭据出路与「不得覆盖未过期明文」────────────

    /// 加密信封的构造器（客户端把 `accessToken` / `nickname` 存成的形态）。
    fn envelope(tag: &str) -> Value {
        json!({"$wbEncrypted": 1, "envelope": tag})
    }

    fn in_a_day() -> i64 {
        crate::modules::config::now_ms() + 86_400_000
    }

    fn an_hour_ago() -> i64 {
        crate::modules::config::now_ms() - 3_600_000
    }

    /// `secret_value` 是**凭据**出路：明文与信封都原样读出，其余一律 `None`。
    ///
    /// 与 `display_str` 的分工是本次修复的核心，故两条出路在同一个用例里对钉：
    /// **同一个信封**，`display_str` 必须给 `null`、`secret_value` 必须原样给出。
    #[test]
    fn secret_value_keeps_plaintext_and_envelope_only() {
        let env = envelope("blob");
        assert_eq!(secret_value(&json!({"k": "v"}), "k"), Some(json!("v")));
        assert_eq!(
            secret_value(&json!({"k": env.clone()}), "k"),
            Some(env.clone())
        );
        // 其余类型没有可用凭据 ⇒ `None`，交给调用方（而不是伪造一个空串）。
        assert_eq!(secret_value(&json!({"k": 123}), "k"), None);
        assert_eq!(secret_value(&json!({"k": true}), "k"), None);
        assert_eq!(secret_value(&json!({"k": ["a"]}), "k"), None);
        assert_eq!(secret_value(&json!({"k": null}), "k"), None);
        assert_eq!(secret_value(&json!({}), "k"), None);
        // 非信封的对象（历史脏值）不是凭据。
        assert_eq!(secret_value(&json!({"k": {"zh": "x"}}), "k"), None);

        // 对照：同一个信封在**展示**出路上必须是 `null`。
        assert_eq!(display_str(&json!({"k": env.clone()}), "k"), Value::Null);
        assert!(is_envelope(&json!({"k": env.clone()}), "k"));
        assert!(!is_envelope(&json!({"k": "v"}), "k"));
        assert!(!is_envelope(&json!({"k": {"zh": "x"}}), "k"));
        assert!(!is_envelope(&json!({}), "k"));
    }

    /// 信封凭据的**可读错误**：只对信封 `access_token` 触发，明文与缺字段都不误报。
    ///
    /// 回归上游 issue #94：信封 token 经 `build_auth_headers` 会被兜底成空 `Bearer`，
    /// 上游 401 之后把整页 HTML 回显到界面。要在**发请求之前**拦住。
    #[test]
    fn envelope_token_error_only_fires_on_envelope_access_token() {
        let encrypted = json!({
            "id": "a1",
            "access_token": envelope("a"),
            "refresh_token": envelope("r"),
        });
        let err = envelope_token_error(&encrypted).expect("信封 access_token 应返回错误");
        assert!(err.contains("加密信封"), "文案应可读：{err}");
        assert!(err.contains("OAuth"), "应给出取得明文凭据的指引：{err}");

        // 阳性对照：明文凭据不得误报。
        let plain = json!({"id": "a2", "access_token": "SECRET", "refresh_token": "R"});
        assert!(envelope_token_error(&plain).is_none(), "明文凭据不应报错");

        // 缺字段的历史账号不在此拦截（保持既有行为）。
        assert!(envelope_token_error(&json!({"id": "a3"})).is_none());

        // ★ 只有 refresh_token 是信封时**不**拦：刷新链路会自己报「需重新登录」，
        //   在这里一并封死会把「还能靠 refresh 救回来」的账号误伤。
        let refresh_only = json!({"id": "a4", "access_token": "SECRET", "refresh_token": envelope("r")});
        assert!(
            envelope_token_error(&refresh_only).is_none(),
            "只有 refresh_token 是信封时不得短路"
        );
    }

    /// 刷新链路的信封判定：**判据是 `refresh_token`，且不得说成「缺少」**。
    ///
    /// 现场（2026-09-24 用户截图）：账号库里 `refresh_token` 是信封对象，
    /// `as_str()` 取不到值 ⇒ 刷新链路报「缺少 refresh token，无法刷新」——
    /// 与账号库里 `refresh_token` / `refreshExpiresAt` 都在的事实相反。
    ///
    /// 可证伪：把本函数改回「不看信封、只由调用方报缺少」（即删掉它），
    /// 第一条断言的 `expect` 直接 panic；把文案改回「缺少 refresh token」，
    /// `!err.contains("缺少")` 这条红。
    #[test]
    fn envelope_refresh_error_reports_encrypted_not_missing() {
        // 1) 信封 refresh_token ⇒ 报「加密信封」，且**必须**给出取得明文凭据的出路。
        let encrypted = json!({
            "id": "a1",
            "access_token": envelope("a"),
            "refresh_token": envelope("r"),
        });
        let err = envelope_refresh_error(&encrypted).expect("信封 refresh_token 应返回原因");
        assert!(err.contains("加密信封"), "文案应可读：{err}");
        assert!(err.contains("OAuth"), "应给出取得明文凭据的指引：{err}");
        assert!(
            !err.contains("缺少"),
            "★ 不得把「已加密」报成「缺少」——这正是本次修复的缺陷形态：{err}"
        );

        // 2) 只有 refresh_token 是信封（access_token 是明文）⇒ 同样报信封。
        //    这条是**边界**：`envelope_token_error` 刻意不管它，刷新链路必须管。
        let refresh_only = json!({"id": "a2", "access_token": "SECRET", "refresh_token": envelope("r")});
        assert!(envelope_refresh_error(&refresh_only).is_some());

        // 3) 阳性对照：明文 refresh_token 不得误报（否则会把可刷新的账号拦死）。
        let plain = json!({"id": "a3", "access_token": envelope("a"), "refresh_token": "R"});
        assert!(
            envelope_refresh_error(&plain).is_none(),
            "明文 refresh_token 不得被信封护栏判定"
        );

        // 4) 真缺失（无该字段 / 空串 / 脏值）不归本函数管：由调用方报「缺少」，
        //    两类原因必须能区分开，否则修法完全不同。
        assert!(envelope_refresh_error(&json!({"id": "a4"})).is_none());
        assert!(envelope_refresh_error(&json!({"id": "a5", "refresh_token": ""})).is_none());
        assert!(envelope_refresh_error(&json!({"id": "a6", "refresh_token": {"zh": "脏值"}})).is_none());
    }

    /// ★ 信封不得覆盖**未过期**的明文 token。
    ///
    /// 现场：WorkBuddy 5.6 的认证文件里 `accessToken` 是信封，而账号库里是
    /// OAuth 扫码（或我方 refresh）写入的**明文**。UI 每次自动 `importLocal`
    /// 都会拿信封重采集同一 uid ⇒ 明文被冲掉 ⇒ 签到 / 积分 / 网关（都需要明文
    /// Bearer）**静默失效**（不报错，只是每个请求退化成空 Bearer）。
    #[test]
    fn envelope_reimport_keeps_unexpired_plain_oauth_token() {
        let mut accounts = vec![json!({
            "id": "a-1",
            "uid": "uid-1",
            "nickname": "明文昵称",
            "access_token": "plain-token",
            "refresh_token": "plain-refresh",
            "expiresAt": in_a_day(),
        })];
        let saved = upsert_collected_account(
            &mut accounts,
            json!({
                "uid": "uid-1",
                "nickname": envelope("nick"),
                "access_token": envelope("a"),
                "refresh_token": envelope("r"),
            }),
        );

        assert_eq!(accounts.len(), 1);
        assert_eq!(saved["access_token"], "plain-token");
        assert_eq!(saved["refresh_token"], "plain-refresh");
        // 展示字段同样保留已有明文：昵称信封我方解不开，覆盖等于把名字丢掉。
        assert_eq!(saved["nickname"], "明文昵称");
        assert_eq!(saved["id"], "a-1");
    }

    /// 对照：明文**已过期**时信封必须接管（切换仍可用，由客户端自行解密）。
    #[test]
    fn envelope_reimport_takes_over_after_plain_token_expired() {
        let mut accounts = vec![json!({
            "id": "a-1",
            "uid": "uid-1",
            "access_token": "stale-plain",
            "expiresAt": an_hour_ago(),
        })];
        let saved = upsert_collected_account(
            &mut accounts,
            json!({"uid": "uid-1", "access_token": envelope("a")}),
        );

        assert_eq!(accounts.len(), 1);
        assert!(is_envelope(&saved, "access_token"), "过期的明文应让信封接管");
        assert_eq!(saved["id"], "a-1");
    }

    /// 对照：**空串**不算「持有明文凭据」，不得挡住信封接管。
    ///
    /// 可证伪：删掉 `has_unexpired_plain_token` 里的 `trim().is_empty()` 判定，
    /// 本用例即红 —— 账号会永远卡在一个空 token 上，再也拿不回可用的凭据。
    #[test]
    fn blank_plain_token_does_not_block_envelope_takeover() {
        let mut accounts = vec![json!({"id": "a-1", "uid": "uid-1", "access_token": ""})];
        let saved = upsert_collected_account(
            &mut accounts,
            json!({"uid": "uid-1", "access_token": envelope("a")}),
        );

        assert!(is_envelope(&saved, "access_token"));
    }

    /// 对照：重新扫码拿到**新的明文**时，应正常替换已有的信封记录。
    #[test]
    fn fresh_plain_oauth_token_replaces_envelope_record() {
        let mut accounts = vec![json!({
            "id": "a-1", "uid": "uid-1", "access_token": envelope("old"),
        })];
        let saved = upsert_collected_account(
            &mut accounts,
            json!({
                "uid": "uid-1",
                "access_token": "fresh-plain",
                "expiresAt": in_a_day(),
            }),
        );

        assert_eq!(accounts.len(), 1);
        assert_eq!(saved["access_token"], "fresh-plain");
        assert_eq!(saved["id"], "a-1");
    }

    /// 展示字段兜底：信封接管 token 的同时，**明文展示值不得被信封覆盖**。
    ///
    /// 与上一条分开是因为那条走「整体保留」分支，这条走「明文已过期 ⇒ 接管 token、
    /// 但展示字段仍保留明文」分支 —— 两条分支的展示字段行为都必须对。
    #[test]
    fn envelope_reimport_keeps_existing_display_name_while_taking_token() {
        let mut accounts = vec![json!({
            "id": "a-1",
            "uid": "uid-1",
            "nickname": "明文昵称",
            "email": "keep@example.com",
            "enterpriseName": "某公司",
            "access_token": "stale",
            "expiresAt": an_hour_ago(),
        })];
        let saved = upsert_collected_account(
            &mut accounts,
            json!({
                "uid": "uid-1",
                "nickname": envelope("nick"),
                "email": envelope("mail"),
                "enterpriseName": envelope("ent"),
                "access_token": envelope("a"),
            }),
        );

        assert!(is_envelope(&saved, "access_token"), "过期的明文应让信封接管");
        assert_eq!(saved["nickname"], "明文昵称");
        assert_eq!(saved["email"], "keep@example.com");
        assert_eq!(saved["enterpriseName"], "某公司");
    }

    #[test]
    fn different_uids_with_same_real_email_are_retained() {
        let mut accounts = vec![account(
            "old",
            Some("uid-1"),
            "账号一",
            Some("shared@example.com"),
        )];
        upsert_collected_account(
            &mut accounts,
            account("new", Some("uid-2"), "账号二", Some("shared@example.com")),
        );

        assert_eq!(accounts.len(), 2);
    }

    #[test]
    fn real_email_is_fallback_only_when_collected_uid_is_missing() {
        let mut accounts = vec![account("stable", None, "旧名称", Some("user@example.com"))];
        let saved = upsert_collected_account(
            &mut accounts,
            account("generated", None, "新名称", Some("USER@example.com")),
        );

        assert_eq!(accounts.len(), 1);
        assert_eq!(saved["id"], "stable");
        assert_eq!(saved["nickname"], "新名称");
    }

    #[test]
    fn legacy_synthetic_email_does_not_merge_accounts() {
        let mut accounts = vec![account("old", None, "同名", Some("同名"))];
        upsert_collected_account(&mut accounts, account("new", None, "同名", Some("同名")));

        assert_eq!(accounts.len(), 2);
    }

    #[test]
    fn persisted_same_name_accounts_can_be_found_and_deleted_independently() {
        let test_dir = std::env::temp_dir().join(format!(
            "buddy-switch-same-name-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let path = test_dir.join("accounts.json");
        let mut accounts = vec![];
        upsert_collected_account(
            &mut accounts,
            account("account-1", Some("uid-1"), "同名用户", None),
        );
        upsert_collected_account(
            &mut accounts,
            account("account-2", Some("uid-2"), "同名用户", None),
        );
        save_accounts_to_path(&path, &accounts).expect("same-name accounts should persist");

        let persisted = load_accounts_from_path(&path);
        assert_eq!(
            find_account_in(&persisted, "account-1").unwrap()["uid"],
            "uid-1"
        );
        assert_eq!(
            find_account_in(&persisted, "account-2").unwrap()["uid"],
            "uid-2"
        );

        delete_account_from_path(&path, "account-1").expect("first account should delete");
        let after_first_delete = load_accounts_from_path(&path);
        assert!(find_account_in(&after_first_delete, "account-1").is_none());
        assert_eq!(
            find_account_in(&after_first_delete, "account-2").unwrap()["uid"],
            "uid-2"
        );

        delete_account_from_path(&path, "account-2").expect("second account should delete");
        assert!(load_accounts_from_path(&path).is_empty());
        std::fs::remove_dir_all(&test_dir).expect("temporary account store should clean up");
    }

    /// 两版账号库必须落到**不同文件**（PRD D2 头号硬约束）。
    ///
    /// `region.rs` 只断言了两个 `accounts_filename` 常量不同，但**常量不同 ≠
    /// `accounts_file_for` 用对了常量**——若该函数写死用 CN 的文件名，常量测试照样绿，
    /// 而两版账号库会互相覆盖。这里直接钉住函数产出的文件名。
    ///
    /// ## 本用例为什么必须持 `env_lock()`
    ///
    /// 断言里出现了**两处**无参全局路径（`accounts_file_for` 与 `store_dir()`），
    /// 它们每次调用都重读进程级 `BUDDY_SWITCH_HOME`。lib 单测在同一进程里并行跑，
    /// 只要有别的用例（如 `current_nickname_falls_back_to_library_for_encrypted_auth_file_name`
    /// 的 `HomeOverrideGuard`）在这两次读取之间换掉该变量，就会出现
    /// 「左侧真实 home、右侧临时 home」的**假失败**（2026-09-24 实测踩到）。
    /// 症状看起来像竞态，实则是**跨用例的全局状态泄漏**。
    /// 修法是让本用例与所有改 home 的用例互斥（取 env 锁），不是给断言加容错。
    #[test]
    fn accounts_file_for_is_region_scoped() {
        let _lock = crate::modules::config::env_lock();
        let cn = accounts_file_for(Region::Cn);
        let global = accounts_file_for(Region::Global);

        assert_eq!(
            cn.file_name().and_then(|n| n.to_str()),
            Some("accounts.json"),
            "CN 账号库文件名"
        );
        assert_eq!(
            global.file_name().and_then(|n| n.to_str()),
            Some("accounts.global.json"),
            "Global 账号库文件名"
        );
        assert_ne!(cn, global, "CN / Global 账号库不得指向同一文件");

        // 同一 store 目录，仅文件名不同。
        assert_eq!(cn.parent(), global.parent());
        assert_eq!(cn, crate::modules::config::store_dir().join("accounts.json"));
        assert_eq!(
            global,
            crate::modules::config::store_dir().join("accounts.global.json")
        );

        // 与既有的 CN 兼容路径保持一致，避免两处逻辑漂移。
        assert_eq!(cn, crate::modules::config::accounts_file());
    }

    /// 备注是**本地标注**，重新采集时必须被带回。
    ///
    /// 可证伪：删掉 `upsert_collected_account` 里 `existing.get("remark")` 那段保留逻辑，
    /// 这条测试立刻变红。
    #[test]
    fn upsert_collected_account_keeps_local_remark() {
        let mut accounts = vec![json!({
            "id": "stable",
            "uid": "uid-1",
            "nickname": "旧名称",
            "access_token": "OLD",
            "remark": "DS4.1 · 10/03 解禁",
        })];
        upsert_collected_account(&mut accounts, account("generated", Some("uid-1"), "新名称", None));

        assert_eq!(accounts.len(), 1);
        assert_eq!(
            accounts[0].get("remark").and_then(Value::as_str),
            Some("DS4.1 · 10/03 解禁"),
            "重新扫码登录 / 再次导入本机账号都不得清空本地备注"
        );
    }

    /// 字段级更新只动 remark，绝不碰 token。
    ///
    /// 这条钉住的是「脱敏 meta 不能拿来整条写回」这条红线：失败模式下
    /// 账号会被写成没有 token 的空壳，而且没有任何报错。
    #[test]
    fn set_remark_only_touches_remark_and_preserves_tokens() {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-remark-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("accounts.json");
        save_accounts_to_path(
            &path,
            &[json!({
                "id": "a1", "uid": "u1", "nickname": "小明",
                "access_token": "AT", "refresh_token": "RT",
            })],
        )
        .expect("seed accounts");

        let meta = set_remark_in_path(Region::Cn, &path, "a1", Some("  10/03 解禁  ")).expect("set remark");
        assert_eq!(meta["remark"], "10/03 解禁", "备注应去首尾空白");
        assert!(meta.get("access_token").is_none(), "meta 不得泄露 token");

        let raw = load_accounts_from_path(&path);
        assert_eq!(raw[0]["remark"], "10/03 解禁");
        assert_eq!(raw[0]["access_token"], "AT", "token 不得被抹掉");
        assert_eq!(raw[0]["refresh_token"], "RT", "token 不得被抹掉");

        // 空备注 = 删除该键，而不是留一个空串。
        set_remark_in_path(Region::Cn, &path, "a1", Some("   ")).expect("clear remark");
        let cleared = load_accounts_from_path(&path);
        assert!(cleared[0].get("remark").is_none(), "空备注应删除字段");
        assert_eq!(cleared[0]["access_token"], "AT");

        // uid 也能定位；不存在的账号必须报错，不能静默成功。
        assert!(set_remark_in_path(Region::Cn, &path, "u1", Some("按 uid 定位")).is_ok());
        assert!(set_remark_in_path(Region::Cn, &path, "nope", None).is_err());

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}

/// 删除账号（按 id，CN）。
pub fn delete_account(account_id: &str) -> Result<(), String> {
    delete_account_for(Region::Cn, account_id)
}

/// 按 region 删除账号（按 id）。
pub fn delete_account_for(region: Region, account_id: &str) -> Result<(), String> {
    delete_account_from_path(&accounts_file_for(region), account_id)
}

/// 就地修改**原始记录**里的备注（字段级更新），返回更新后的脱敏元数据。
///
/// ## 为什么必须是字段级，而不是「读 meta → 改 → 整条写回」
///
/// [`account_meta`] 是**脱敏白名单**，不含 `access_token` / `refresh_token`。
/// 若让调用方拿 meta 改完再走 [`upsert_account_for`]，两个 token 会被一起抹掉，
/// 账号当场失效**且不会有任何报错**。所以读原始记录、只改一个键、再原子写回。
///
/// 备注传空（或全空白）时**删除该键**而不是写空串：账号库是用户可见的文件，
/// 没有备注就不该多出一个字段。
fn set_remark_in_path(
    region: Region,
    path: &Path,
    account_id: &str,
    remark: Option<&str>,
) -> Result<Value, String> {
    let mut accounts = load_accounts_from_path(path);
    let Some(index) = accounts.iter().position(|account| {
        account.get("id").and_then(Value::as_str) == Some(account_id)
            || account.get("uid").and_then(Value::as_str) == Some(account_id)
    }) else {
        return Err("账号不存在".to_string());
    };

    let record = accounts[index]
        .as_object_mut()
        .ok_or_else(|| "账号记录格式异常".to_string())?;
    match remark.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => {
            record.insert("remark".to_string(), json!(text));
        }
        None => {
            record.remove("remark");
        }
    }

    let updated = accounts[index].clone();
    save_accounts_to_path(path, &accounts).map_err(|error| error.to_string())?;
    Ok(account_meta_for(region, &updated))
}

/// 按 region 设置账号备注（按 id 或 uid 定位）。
pub fn set_account_remark_for(
    region: Region,
    account_id: &str,
    remark: Option<&str>,
) -> Result<Value, String> {
    set_remark_in_path(region, &accounts_file_for(region), account_id, remark)
}

/// 设置 CN 账号备注。
pub fn set_account_remark(account_id: &str, remark: Option<&str>) -> Result<Value, String> {
    set_account_remark_for(Region::Cn, account_id, remark)
}

/// 导入本机当前账号（从认证文件读取，CN）。
pub fn import_local() -> Result<Value, String> {
    import_local_for(Region::Cn)
}

/// 按 region 导入本机当前账号（从该 region 认证文件读取）。
pub fn import_local_for(region: Region) -> Result<Value, String> {
    let acc = crate::modules::auth_file::import_from_auth_file_for(region)
        .ok_or("未读取到本地 WorkBuddy 登录信息")?;
    let saved = save_collected_account_for(region, acc).map_err(|e| e.to_string())?;
    Ok(account_meta_for(region, &saved))
}

// 手动添加账号（token 方式）已随 UI 入口「手动添加」一并下线；
// `identity_email` 中的 "手动添加" 占位过滤保留，用于兼容历史手动添加的旧账号。
