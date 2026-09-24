//! Trae 账号库、分组与账号视图。
//!
//! ## 两种形态，两套命名约定（重要）
//!
//! 1. **持久化形态**：`trae/checkin_accounts.json` / `trae/groups.json`。
//!    字段名与参考实现逐字对齐（含 `UserID` 这个非常规的大写键），因为这两个文件
//!    是**跨工具共享**的——参考实现的 `device_proxy.py`、用户的既有备份都按这个
//!    形状读写。改动键名会让既有数据静默丢失。
//! 2. **线上形态**（Tauri invoke / HTTP 响应）：一律 camelCase，用 `json!` 显式构造。
//!    这是本仓库的统一约定（见 `modules::account::account_meta`）。
//!
//! 两套形态之间的转换集中在 [`account_view`]，**不要在别处临时拼装**：
//! 任一通道漏一个字段，前端就会在某些入口拿到 `undefined`，而这在编译期看不出来。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::modules::trae::credits::CooldownsFile;
use crate::modules::trae::icube::{self, DeviceCredential, ProofSigFormat};
use crate::modules::trae::jwt;
use crate::modules::trae::paths;
use crate::modules::trae::store;
use crate::modules::trae::variant::TraeVariant;
use crate::modules::trae::{
    device, TRAE_EXCHANGE_TOKEN_LEGACY_PATH, TRAE_EXCHANGE_TOKEN_PATH, TRAE_OAUTH_APP_ID,
    TRAE_PAGE_PLATFORM_CODE,
};

/// 账号库中的一条账号记录（持久化形态，键名与参考实现一致）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawAccount {
    /// 展示名。
    #[serde(default)]
    pub name: String,
    /// 用户 ID。参考实现用大写 `UserID`，保留以兼容既有数据。
    #[serde(rename = "UserID", default)]
    pub user_id: Option<String>,
    /// 完整的 `Cloud-IDE-JWT <token>` 或裸 token。
    #[serde(default)]
    pub jwt: String,
    /// OAuth 刷新令牌；仅 OAuth 登录的账号具备。
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// 首次写入时间。
    #[serde(default)]
    pub added_at: Option<String>,
    /// 最近更新（改名 / 换 JWT / 刷新令牌）时间。
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// 账号库文件（**G-b**：设备绑定放「文件容器」，不放账号记录）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountsFile {
    #[serde(default)]
    pub accounts: Vec<RawAccount>,
    /// `uid → iCubeAuthInfo://icube-dc:<deviceId>` 内嵌的 `<deviceId>`。
    ///
    /// ## 为什么放在容器键而不是 `RawAccount.device_id`
    ///
    /// 它是**本机绑定**，不是账号的可携带属性：
    /// 1. `export_accounts_for` 逐条 `serde_json::to_value(record)` ⇒ 放在记录里会被写进
    ///    **给另一台机器导入**的导出文件，把本机 `deviceId` 泄漏出去；
    /// 2. `merge_import_record` 的覆盖分支是 `*existing = replaced` ⇒ 放在记录里会被
    ///    一份不含该字段的导入文件**静默抹掉**。
    ///
    /// 容器键两个问题都不存在：合并逻辑的签名只收 `&mut Vec<RawAccount>`，**结构上碰不到**它。
    ///
    /// ## 为什么需要它
    ///
    /// 续期（DeviceProof 签名）**必须**用「这个账号当初登录/导入时那台设备」的私钥。
    /// 客户端换过设备、或本变体有多个候选 userData 目录时，「活跃目录里的那一条」
    /// 未必是账号绑定的那台 —— 用错私钥会被服务端拒绝（20403/20405）。
    ///
    /// ## 三条不变式（改「uid ↔ 记录」对应关系的路径必须全部遵守）
    ///
    /// | # | 路径 | 规则 |
    /// |:--|:--|:--|
    /// | I-1 | [`crate::modules::trae::export_import::import_accounts_for`] | 回存**整个** `AccountsFile`，不得字面重建容器（否则清空全部绑定） |
    /// | I-2 | [`delete_for`] | 删账号时一并 `remove(uid)` |
    /// | I-3 | [`update_for`] | 换 JWT 导致 uid 变化时删**旧 uid** 的绑定；新 uid 留空 |
    ///
    /// 空表不落盘（`skip_serializing_if`）：不产生绑定时，账号库文件与改造前**逐字一致**，
    /// 既有备份 / 参考实现的 `device_proxy.py` 读到的形状不变。
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub device_bindings: HashMap<String, String>,
}

/// 把 `uid` 绑定到某个 `device_id`（**只写不删**：`None` / 空串不改变既有绑定）。
///
/// 删除一律由三条不变式各自的调用点显式 `remove` —— 「`None` 该不该清掉旧绑定」
/// 在不同路径上答案不同（I-3 要清、导入缺 `device_id` 时不该清），
/// 由本函数统一猜会让其中一条悄悄变错。
fn bind_device(file: &mut AccountsFile, uid: &str, device_id: Option<&str>) {
    if let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty()) {
        file.device_bindings
            .insert(uid.to_string(), device_id.to_string());
    }
}

/// 分组。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub order: i32,
}

/// 分组文件：分组定义 + `user_id -> group_id` 成员关系。
///
/// 成员关系与分组定义放在同一文件：两者必须同步更新（删分组要同时摘掉成员），
/// 分文件会让「删了分组但成员关系残留」成为可能。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupsFile {
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub membership: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// 读写
// ---------------------------------------------------------------------------

/// 读取账号库（默认变体，兼容壳）。
pub fn load_accounts() -> AccountsFile {
    load_accounts_for(TraeVariant::default())
}

/// 读取账号库（按变体分家）。
///
/// **两条产品线的账号库必须分开读**：它们的 `user_id` 空间不同、凭据形态不同，
/// 混读的后果不是报错而是"看起来正常"—— 界面上会列出另一条产品线的账号。
pub fn load_accounts_for(variant: TraeVariant) -> AccountsFile {
    store::read_json(&paths::accounts_file_for(variant))
}

/// 写入账号库（默认变体，兼容壳）。
pub fn save_accounts(accounts: &AccountsFile) -> Result<(), String> {
    save_accounts_for(TraeVariant::default(), accounts)
}

/// 写入账号库（按变体分家）。
pub fn save_accounts_for(variant: TraeVariant, accounts: &AccountsFile) -> Result<(), String> {
    store::write_json(&paths::accounts_file_for(variant), accounts)
}

/// 读取分组（默认变体，兼容壳）。
pub fn load_groups() -> GroupsFile {
    load_groups_for(TraeVariant::default())
}

/// 读取分组（按变体分家）。
pub fn load_groups_for(variant: TraeVariant) -> GroupsFile {
    store::read_json(&paths::groups_file_for(variant))
}

/// 写入分组（默认变体，兼容壳）。
pub fn save_groups(groups: &GroupsFile) -> Result<(), String> {
    save_groups_for(TraeVariant::default(), groups)
}

/// 写入分组（按变体分家）。
pub fn save_groups_for(variant: TraeVariant, groups: &GroupsFile) -> Result<(), String> {
    store::write_json(&paths::groups_file_for(variant), groups)
}

/// 解析账号的 userId：优先记录里已存的 `UserID`，其次从 JWT 现算。
///
/// 回落顺序不可颠倒：旧的 `checkin_accounts.json` 可能存在 `UserID` 与 JWT 不一致的
/// 脏数据（用户手改过），而以记录为准能保持分组、设备映射、积分历史这些**按 uid 索引**
/// 的数据不断链。
pub fn resolve_user_id(account: &RawAccount) -> String {
    account
        .user_id
        .clone()
        .filter(|uid| !uid.trim().is_empty())
        .unwrap_or_else(|| jwt::user_id_of(&account.jwt).unwrap_or_default())
}

/// 读取账号并附带其解析后的 uid，过滤掉无法确定 uid 的脏记录（默认变体，兼容壳）。
pub fn entries() -> Vec<(String, RawAccount)> {
    entries_for(TraeVariant::default())
}

/// 读取账号并附带其解析后的 uid，过滤掉无法确定 uid 的脏记录（按变体分家）。
///
/// 返回 `Vec` 而非 `HashMap`：账号库的顺序即用户可见顺序，不能被打乱。
pub fn entries_for(variant: TraeVariant) -> Vec<(String, RawAccount)> {
    load_accounts_for(variant)
        .accounts
        .into_iter()
        .map(|account| (resolve_user_id(&account), account))
        .filter(|(uid, _)| !uid.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// 区域轴读取（新代码请优先用这一族）
// ---------------------------------------------------------------------------
//
// `*_for(variant)` 那一族仍然可用，但它们内部都按 `variant.region()` 落到**同一本区域库**
// （国内两条产品线共用一本，见 `paths::scoped_file` 的说明）。想明确表达「按区域」时
// 用下面这族，避免读者误以为参数还分家。

/// 读取某**区域**的账号库。
pub fn load_accounts_for_region(region: super::region::TraeRegion) -> AccountsFile {
    store::read_json(&paths::accounts_file_for_region(region))
}

/// 写入某**区域**的账号库。
///
/// ⚠️ 必须**整体回存** `AccountsFile`，不得字面重建容器 —— 否则 `device_bindings`
/// 会被清空（见该字段的三条不变式）。
pub fn save_accounts_for_region(
    region: super::region::TraeRegion,
    accounts: &AccountsFile,
) -> Result<(), String> {
    store::write_json(&paths::accounts_file_for_region(region), accounts)
}

/// 读取某区域的账号并附带解析后的 uid（过滤掉无法确定 uid 的脏记录）。
///
/// gateway 的账号池按**区域**取号：国内两套产品线共用一本库，池也必须按区域建，
/// 否则会出现「两个池服务同一批账号」（选号、冷却、日志归属全部对不上）。
pub fn entries_for_region(region: super::region::TraeRegion) -> Vec<(String, RawAccount)> {
    load_accounts_for_region(region)
        .accounts
        .into_iter()
        .map(|account| (resolve_user_id(&account), account))
        .filter(|(uid, _)| !uid.is_empty())
        .collect()
}

/// 查找单个账号（默认变体，兼容壳）。
pub fn find(user_id: &str) -> Option<RawAccount> {
    find_for(TraeVariant::default(), user_id)
}

/// 查找单个账号（按变体分家）。
///
/// **注意跨变体查不到是正确行为**：同一个 `user_id` 在两条产品线里
/// 是两个不同的账号，绝不能让查找横跨账号库。
pub fn find_for(variant: TraeVariant, user_id: &str) -> Option<RawAccount> {
    entries_for(variant)
        .into_iter()
        .find(|(uid, _)| uid == user_id)
        .map(|(_, account)| account)
}

/// 按 uid 取账号库里的**展示名**（[`RawAccount::name`]）；查不到或名字为空白 → `None`。
///
/// ## 用途：界面上「已登录: `<谁>`」该给人看的东西
///
/// 「当前登录账号」对外是**两个不同的事实**，不能合并成一个字段：
///
/// | 事实 | 载体 | 谁在用 |
/// |:--|:--|:--|
/// | 身份 | `currentAccount`（uid） | 前端的相等比较（卡片上「是不是当前账号」） |
/// | 展示 | 本函数的结果 | 状态条第二行「已登录: …」 |
///
/// 拿展示名去做身份比较会在**改名**后立刻失配（把当前账号显示成"未启用"）；
/// 反过来，把 uid 直接摆到界面上就是改造前的样子 —— 16 位数字，用户认不出是谁。
///
/// ## 调用方拿到 `None` 时必须回落 uid，不得显示「未知账号」
///
/// 「客户端正登录着一个本机账号库里还没有的账号」是**正常状态**：用户刚在客户端里
/// 手动登录、还没做采集/导入。此时 uid 仍是有效且可行动的线索（用户能拿它去客户端
/// 里对照），报成「未知账号」反而把唯一可用的信息抹掉了。
/// 这一条与 WorkBuddy 侧 `presenceText` 的回落链（`nickname → email → uid`）同构。
///
/// 名字为空白同样视为**无名**：`name` 是 `#[serde(default)]`，
/// 参考工具写出的老文件里有空串记录。
pub fn display_name_for(variant: TraeVariant, user_id: &str) -> Option<String> {
    find_for(variant, user_id)
        .map(|account| account.name.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// 按 uid 取展示名（默认变体，兼容壳）。
pub fn display_name(user_id: &str) -> Option<String> {
    display_name_for(TraeVariant::default(), user_id)
}

// ---------------------------------------------------------------------------
// 视图（线上形态：camelCase）
// ---------------------------------------------------------------------------

/// 把一条账号记录 + 各附属数据聚合成前端消费的视图对象。
///
/// 聚合的字段与来源：
/// - `jwtExpHours` / `jwtExpTimestamp` / `jwtStatus`：现算 JWT 到期
/// - `groupId`：分组成员关系
/// - `deviceIdMasked`：设备映射（脱敏）
/// - `remainingCredits` / `creditsExpireAt`：剩余积分缓存
/// - `cooldownType` / `cooldownUntil` / `cooldownReason`：冷却状态（仅未到期时暴露）
/// - `checkedToday`：今日签到摘要
#[allow(clippy::too_many_arguments)]
pub fn account_view(
    account: &RawAccount,
    user_id: &str,
    group_id: Option<String>,
    device_id: Option<&str>,
    remaining_credits: Option<f64>,
    credits_expire_at: Option<i64>,
    cooldown: Option<&crate::modules::trae::credits::CooldownEntry>,
    checked_today: bool,
    latest_credits: Option<i64>,
) -> Value {
    let info = jwt::parse(&account.jwt);
    let has_refresh_token = account
        .refresh_token
        .as_ref()
        .map(|token| !token.is_empty())
        .unwrap_or(false);
    // 自动刷新条件：有 refresh_token 且 JWT 在 24h 内到期（或无 exp 无法判定）。
    let needs_refresh = has_refresh_token
        && info
            .exp_hours
            .map(|hours| hours <= crate::modules::trae::TRAE_JWT_WARN_HOURS)
            .unwrap_or(true);

    let now = chrono::Local::now().timestamp();
    let active_cooldown = cooldown.filter(|entry| {
        entry.until > now && !entry.error_type.is_empty()
    });

    json!({
        "userId": user_id,
        "name": account.name,
        "groupId": group_id,
        "jwt": account.jwt,
        "jwtExpHours": info.exp_hours,
        "jwtExpTimestamp": info.exp_timestamp,
        "jwtStatus": info.status(),
        "checkedToday": checked_today,
        "credits": latest_credits,
        "remainingCredits": remaining_credits,
        "creditsExpireAt": credits_expire_at,
        "deviceIdMasked": device_id.map(store::mask),
        "cooldownType": active_cooldown.map(|entry| entry.error_type.clone()),
        "cooldownUntil": active_cooldown.map(|entry| entry.until),
        "cooldownReason": active_cooldown
            .map(|entry| entry.reason.clone())
            .filter(|reason| !reason.is_empty()),
        "hasRefreshToken": has_refresh_token,
        "jwtAutoRefresh": needs_refresh,
        "addedAt": account.added_at,
        "updatedAt": account.updated_at,
    })
}

/// 构建全部账号视图（按变体分家），含各附属数据的一次性加载。
///
/// 附属数据一次性读入后再遍历账号，而不是「每个账号各读一遍文件」：
/// 账套规模通常是几十个，逐账号读文件会产生 O(n) 次磁盘 IO 且可能读到不一致的快照。
///
/// **本函数必须整条链路用同一个 `variant`**：账号库、分组、设备映射、剩余积分、
/// 冷却、签到摘要、积分明细——七处全部按变体分家。漏掉任何一处，
/// 就会把另一条产品线的数据混进这个视图（例如把 Trae CN 的积分显示在 Trae Work 账号上）。
pub fn list_account_views_for(variant: TraeVariant) -> Vec<Value> {
    let accounts = load_accounts_for(variant);
    let groups = load_groups_for(variant);
    let device_map = device::load_map_for(variant);
    let remaining = crate::modules::trae::credits::load_remaining_for(variant);
    let cooldowns: CooldownsFile = store::read_json(&paths::cooldowns_file_for(variant));

    // 「今日已签到」的**唯一来源**是当日台账（跨运行累积、按 userId 记），并兜上当日积分明细
    // （升级当天台账还是空的，见 `credits::checked_in_today_for` 的两条理由）。
    //
    // 曾经这里读的是 `checkin_summary.json`（最近一次运行的结果）并按 `name` 匹配，
    // 两个缺陷叠在一起：
    //   ① 摘要每轮整体覆盖 ⇒ 上一轮签过、本轮因 `skip_checked_in` 未被处理的账号
    //      会丢掉标记。实测：Jackey 已 claim 成功（`credits_history` 有 delta=150），
    //      却因为随后一轮只处理了 JackDev 而显示「未签到」；
    //   ② 用显示名做键 ⇒ 同名账号互相冒充。
    let checked_user_ids: std::collections::HashSet<String> =
        crate::modules::trae::credits::checked_in_today_for(variant);

    let credits_history = crate::modules::trae::credits::load_history_for(variant);

    accounts
        .accounts
        .iter()
        .filter_map(|account| {
            let uid = resolve_user_id(account);
            if uid.is_empty() {
                return None;
            }
            let latest_credits = credits_history
                .records
                .iter()
                .filter(|record| record.user_id == uid)
                // 同日期取较大值、跨日期取较新日期：避免展示历史峰值而非当前余额。
                .fold(None::<&crate::modules::trae::credits::CreditRecord>, |best, record| {
                    match best {
                        None => Some(record),
                        Some(current) => {
                            if record.date > current.date
                                || (record.date == current.date && record.credits > current.credits)
                            {
                                Some(record)
                            } else {
                                Some(current)
                            }
                        }
                    }
                })
                .map(|record| record.credits);
            Some(account_view(
                account,
                &uid,
                groups.membership.get(&uid).cloned(),
                device_map.get(&uid).map(|entry| entry.device_id.as_str()),
                remaining.credits.get(&uid).copied(),
                remaining.expire_times.get(&uid).copied(),
                cooldowns.cooldowns.get(&uid),
                checked_user_ids.contains(&uid),
                latest_credits,
            ))
        })
        .collect()
}

/// 分组视图（默认变体，兼容壳）。
pub fn list_group_views() -> Vec<Value> {
    list_group_views_for(TraeVariant::default())
}

/// 分组视图（按变体分家），含成员数，按 `order` 升序、其次按创建顺序。
pub fn list_group_views_for(variant: TraeVariant) -> Vec<Value> {
    let groups = load_groups_for(variant);
    let mut views: Vec<Value> = groups
        .groups
        .iter()
        .map(|group| {
            let count = groups
                .membership
                .values()
                .filter(|gid| *gid == &group.id)
                .count();
            json!({
                "id": group.id,
                "name": group.name,
                "color": group.color,
                "order": group.order,
                "count": count,
            })
        })
        .collect();
    views.sort_by_key(|view| {
        view.get("order")
            .and_then(|v| v.as_i64())
            .unwrap_or(i64::MAX)
    });
    views
}

// ---------------------------------------------------------------------------
// 账号写操作
// ---------------------------------------------------------------------------

/// 手动添加账号（从 JWT 解析 uid；默认变体，兼容壳）。
pub fn add_manual(name: &str, raw_jwt: &str, group_id: Option<String>) -> Result<String, String> {
    add_manual_for(TraeVariant::default(), name, raw_jwt, group_id)
}

/// 手动添加账号（按变体分家）。
///
/// 重复账号直接报错而不是静默覆盖：静默覆盖会丢掉用户已有的 refresh_token。
///
/// **去重只在变体内进行**：同一个 uid 在另一条产品线里是合法的新账号，
/// 跨库去重会让用户无法把同一账号分别加进两条产品线。
pub fn add_manual_for(
    variant: TraeVariant,
    name: &str,
    raw_jwt: &str,
    group_id: Option<String>,
) -> Result<String, String> {
    let jwt_value = raw_jwt.trim();
    if jwt_value.is_empty() {
        return Err("JWT 不能为空".into());
    }
    let uid = jwt::user_id_of(jwt_value)
        .filter(|uid| !uid.is_empty())
        .ok_or("无法从 JWT 解析 UserID，请检查格式")?;

    let mut accounts = load_accounts_for(variant);
    if accounts
        .accounts
        .iter()
        .any(|account| resolve_user_id(account) == uid)
    {
        return Err("该账号已存在".into());
    }

    let name = if name.trim().is_empty() {
        // 未提供名字时用 uid 尾 6 位兜底，保证列表里可区分。
        format!("账号{}", &uid[uid.len().saturating_sub(6)..])
    } else {
        name.trim().to_string()
    };

    let now = store::now_iso();
    accounts.accounts.push(RawAccount {
        name,
        user_id: Some(uid.clone()),
        jwt: jwt_value.to_string(),
        refresh_token: None,
        added_at: Some(now.clone()),
        updated_at: Some(now),
    });
    save_accounts_for(variant, &accounts)?;

    if let Some(group) = group_id.filter(|g| !g.trim().is_empty()) {
        move_to_group_for(variant, &uid, Some(group))?;
    }
    Ok(uid)
}

/// 从客户端登录态导入当前账号（对齐 WorkBuddy 的「导入本机账号」）。
///
/// 与 [`add_manual`] 的差别只在凭据来源：这里不要求用户粘贴 JWT，
/// 而是从 Trae 客户端 userData 的持久化登录态里提取（见 `profile::extract_local_jwt`）。
///
/// **重复时是覆盖而不是报错**：WorkBuddy 的「导入本机账号」语义是「把本机现在登录的
/// 账号同步进来」，用户点它的意图就是「让库里的记录跟上当前登录态」，
/// 因此同 uid 存在时更新 JWT、保留既有 name / refresh_token / added_at。
pub fn import_local() -> Result<RawAccount, String> {
    import_local_for(TraeVariant::default())
}

/// 按**产品线变体**从客户端登录态导入当前账号。
///
/// 旧签名 [`import_local`] 是它的薄封装（固定用 [`TraeVariant::default`]），
/// 既有调用点零改动、行为不变。
///
/// 变体影响**两件事**（两处都必须用它，缺一即串味）：
/// 1. 读哪个 userData 目录（见 [`crate::modules::trae::profile::extract_local_jwt_for`]）；
/// 2. 写进哪个账号库（见 [`paths::accounts_file_for`]）—— 两条产品线的账号库彼此独立。
///
/// 落库时同时写 `uid → device_id` 绑定（[`AccountsFile::device_bindings`]，**G-b**）：
/// 导入读的是哪个目录，续期就该用**那个目录那台设备**的私钥签名 —— 客户端换过设备、
/// 或本变体有多个候选目录时，「活跃目录里的那一条」未必是这台。
pub fn import_local_for(variant: TraeVariant) -> Result<RawAccount, String> {
    // 用 `import_local_login_for` 而不是 `extract_local_jwt_for`：除了 uid 与凭据，
    // 还需要它一并给出的 `device_id` —— 那是**同一个来源目录**里的设备身份，
    // 续期要用它的私钥签名。分两次取会让「凭据来自 A、设备身份来自 B」。
    let login = crate::modules::trae::profile::import_local_login_for(variant)?;
    let uid = login.user_id;
    let jwt_value = login.authorization;
    let device_id = login.device_id;

    let mut accounts = load_accounts_for(variant);
    let now = store::now_iso();

    let record = match accounts
        .accounts
        .iter_mut()
        .find(|account| resolve_user_id(account) == uid)
    {
        Some(existing) => {
            existing.jwt = jwt_value;
            existing.user_id = Some(uid.clone());
            existing.updated_at = Some(now);
            existing.clone()
        }
        None => {
            let record = RawAccount {
                name: format!("账号{}", &uid[uid.len().saturating_sub(6)..]),
                user_id: Some(uid.clone()),
                jwt: jwt_value,
                refresh_token: None,
                added_at: Some(now.clone()),
                updated_at: Some(now),
            };
            accounts.accounts.push(record.clone());
            record
        }
    };

    // 记录**这次导入用的是哪台设备**（`Some` 才写）。
    // 取不到时**不动**既有绑定：宁可留旧值（下次续期仍可能对），也不要用一个空值把
    // 「本来正确的绑定」清掉 —— 后者会让续期退回「猜活跃目录」，且用户无从察觉。
    bind_device(&mut accounts, &uid, device_id.as_deref());
    save_accounts_for(variant, &accounts)?;
    Ok(record)
}

/// 删除账号（默认变体，兼容壳）。
pub fn delete(user_id: &str, delete_profile: bool) -> Result<(), String> {
    delete_for(TraeVariant::default(), user_id, delete_profile)
}

/// 删除账号（按变体分家）。
///
/// `delete_profile = true` 时一并删除登录态快照目录。默认不删：
/// 快照可能包含用户还未备份的登录态，误删成本高于磁盘占用。
///
/// **删快照也必须限定变体**：两条产品线的快照根目录不同
/// （见 [`paths::profiles_dir_for`]），否则会删到另一条产品线的快照。
///
/// **I-2**：删账号必须**一并删掉它的设备绑定**。否则 uid 被重新登录 / 复用时，
/// 会拿**属于旧设备**的绑定去签名 ⇒ 续期被上游拒绝（20403/20405），
/// 而用户看到的是「刚加的账号就是刷新不了」。
pub fn delete_for(
    variant: TraeVariant,
    user_id: &str,
    delete_profile: bool,
) -> Result<(), String> {
    let mut accounts = load_accounts_for(variant);
    let before = accounts.accounts.len();
    accounts
        .accounts
        .retain(|account| resolve_user_id(account) != user_id);
    // 只删**这个** uid 的绑定：同一本账号库里其他 uid 的绑定不受影响。
    let binding_removed = accounts.device_bindings.remove(user_id).is_some();
    // 账号条数与绑定**任一**有变化就要回存：账号本就不存在、但绑定残留时也得清掉。
    if accounts.accounts.len() != before || binding_removed {
        save_accounts_for(variant, &accounts)?;
    }

    // 无论账号是否存在，都清理分组成员关系，避免「孤儿成员」在重建同名账号时复活。
    let mut groups = load_groups_for(variant);
    if groups.membership.remove(user_id).is_some() {
        save_groups_for(variant, &groups)?;
    }

    if delete_profile {
        if let Some(dir) = paths::profile_dir_for(variant, user_id) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    Ok(())
}

/// 更新账号（改名 / 换 JWT；默认变体，兼容壳）。
pub fn update(user_id: &str, name: Option<String>, raw_jwt: Option<String>) -> Result<(), String> {
    update_for(TraeVariant::default(), user_id, name, raw_jwt)
}

/// 更新账号（按变体分家）。
///
/// **I-3**：换 JWT 可能换掉账号主体（`:543-546` 同步改 uid），此时**旧 uid 的绑定必须删掉**
/// —— 它属于**旧账号**，留着就是错配；新 uid **不搬**旧绑定（旧设备的私钥对新账号不适用），
/// 留空后续期回落活跃目录并留痕。uid 未变则绑定原样保留。
pub fn update_for(
    variant: TraeVariant,
    user_id: &str,
    name: Option<String>,
    raw_jwt: Option<String>,
) -> Result<(), String> {
    let mut accounts = load_accounts_for(variant);
    let previous_uid = user_id.to_string();
    let mut next_uid = previous_uid.clone();
    {
        let account = accounts
            .accounts
            .iter_mut()
            .find(|account| resolve_user_id(account) == user_id)
            .ok_or("账号不存在")?;

        if let Some(name) = name {
            let name = name.trim().to_string();
            if !name.is_empty() {
                account.name = name;
            }
        }
        if let Some(raw_jwt) = raw_jwt {
            let raw_jwt = raw_jwt.trim().to_string();
            if !raw_jwt.is_empty() {
                // JWT 可能换了账号：同步 uid，否则分组/积分/设备会错挂到旧 uid 上。
                if let Some(uid) = jwt::user_id_of(&raw_jwt) {
                    account.user_id = Some(uid.clone());
                    next_uid = uid;
                }
                account.jwt = raw_jwt;
            }
        }
        account.updated_at = Some(store::now_iso());
    }
    // I-3：uid 变了 ⇒ 旧绑定属于旧账号，删除（**不**搬给新 uid）。
    if next_uid != previous_uid {
        accounts.device_bindings.remove(&previous_uid);
    }
    save_accounts_for(variant, &accounts)
}

/// 创建分组，返回新分组 ID（默认变体，兼容壳）。
pub fn group_create(name: &str, color: &str) -> Result<String, String> {
    group_create_for(TraeVariant::default(), name, color)
}

/// 创建分组，返回新分组 ID（按变体分家）。
pub fn group_create_for(
    variant: TraeVariant,
    name: &str,
    color: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("分组名不能为空".into());
    }
    let mut groups = load_groups_for(variant);
    if groups.groups.iter().any(|group| group.name == name) {
        return Err("同名分组已存在".into());
    }
    let id = format!("g_{}", chrono::Local::now().timestamp_millis());
    let order = groups.groups.len() as i32 + 1;
    groups.groups.push(Group {
        id: id.clone(),
        name: name.to_string(),
        color: color.to_string(),
        order,
    });
    save_groups_for(variant, &groups)?;
    Ok(id)
}

/// 更新分组字段（默认变体，兼容壳）。
pub fn group_update(
    id: &str,
    name: Option<String>,
    color: Option<String>,
    order: Option<i32>,
) -> Result<(), String> {
    group_update_for(TraeVariant::default(), id, name, color, order)
}

/// 更新分组字段（按变体分家）。
pub fn group_update_for(
    variant: TraeVariant,
    id: &str,
    name: Option<String>,
    color: Option<String>,
    order: Option<i32>,
) -> Result<(), String> {
    let mut groups = load_groups_for(variant);
    let group = groups
        .groups
        .iter_mut()
        .find(|group| group.id == id)
        .ok_or("分组不存在")?;
    if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
        group.name = name.trim().to_string();
    }
    if let Some(color) = color {
        group.color = color;
    }
    if let Some(order) = order {
        group.order = order;
    }
    save_groups_for(variant, &groups)
}

/// 删除分组；成员回落为「未分组」（即从 membership 中移除）；默认变体，兼容壳。
pub fn group_delete(id: &str) -> Result<(), String> {
    group_delete_for(TraeVariant::default(), id)
}

/// 删除分组（按变体分家）；成员回落为「未分组」（即从 membership 中移除）。
pub fn group_delete_for(variant: TraeVariant, id: &str) -> Result<(), String> {
    let mut groups = load_groups_for(variant);
    groups.groups.retain(|group| group.id != id);
    groups.membership.retain(|_, gid| gid != id);
    save_groups_for(variant, &groups)
}

/// 把账号移入/移出分组（`None` 表示移出到「未分组」）；默认变体，兼容壳。
pub fn move_to_group(user_id: &str, group_id: Option<String>) -> Result<(), String> {
    move_to_group_for(TraeVariant::default(), user_id, group_id)
}

/// 把账号移入/移出分组（按变体分家；`None` 表示移出到「未分组」）。
pub fn move_to_group_for(
    variant: TraeVariant,
    user_id: &str,
    group_id: Option<String>,
) -> Result<(), String> {
    let mut groups = load_groups_for(variant);
    match group_id.filter(|gid| !gid.trim().is_empty()) {
        Some(group_id) => {
            if !groups.groups.iter().any(|group| group.id == group_id) {
                return Err("目标分组不存在".into());
            }
            groups.membership.insert(user_id.to_string(), group_id);
        }
        None => {
            groups.membership.remove(user_id);
        }
    }
    save_groups_for(variant, &groups)
}

// ---------------------------------------------------------------------------
// 执行范围
// ---------------------------------------------------------------------------

/// 签到/切换的执行范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// 全部账号。
    All,
    /// 指定分组。
    Group(String),
    /// 显式选中的账号。
    Selected(Vec<String>),
}

impl Scope {
    /// 解析 `all` / `group:<id>` / `selected`。
    ///
    /// 未知输入返回 `Err` 而非静默当成 `All`：把范围解析错误当成「全量执行」
    /// 会让用户以为只签了一组，实际签了全部账号，属于不可接受的静默行为放大。
    pub fn parse(raw: &str) -> Result<Scope, String> {
        let raw = raw.trim();
        match raw {
            "all" | "" => Ok(Scope::All),
            "selected" => Ok(Scope::Selected(Vec::new())),
            other => match other.strip_prefix("group:") {
                Some(group_id) if !group_id.is_empty() => Ok(Scope::Group(group_id.to_string())),
                _ => Err(format!("未知的执行范围: {other}")),
            },
        }
    }
}

/// 按范围解析目标 userId 列表（默认变体，兼容壳）。
pub fn resolve_user_ids(scope: &Scope, user_ids: Option<Vec<String>>) -> Vec<String> {
    resolve_user_ids_for(TraeVariant::default(), scope, user_ids)
}

/// 按范围解析目标 userId 列表（按变体分家）。
///
/// `Selected` 会与 `user_ids` 参数合并；`All` / `Group` 忽略 `user_ids`
/// （前端切换范围时可能残留上一次的勾选，忽略比取交集更符合直觉）。
///
/// **`All` 指的是「该变体的全部账号」**，不是「全部账号」——
/// 这正是「Trae CN 签到不该动到 Trae Work 账号」的落点。
pub fn resolve_user_ids_for(
    variant: TraeVariant,
    scope: &Scope,
    user_ids: Option<Vec<String>>,
) -> Vec<String> {
    let groups = load_groups_for(variant);
    match scope {
        Scope::All => entries_for(variant).into_iter().map(|(uid, _)| uid).collect(),
        Scope::Group(group_id) => entries_for(variant)
            .into_iter()
            .filter(|(uid, _)| groups.membership.get(uid) == Some(group_id))
            .map(|(uid, _)| uid)
            .collect(),
        Scope::Selected(explicit) => {
            let mut wanted = explicit.clone();
            if let Some(extra) = user_ids {
                for uid in extra {
                    if !wanted.contains(&uid) {
                        wanted.push(uid);
                    }
                }
            }
            wanted
        }
    }
}

// ---------------------------------------------------------------------------
// 令牌交换（AuthCode 落库 / refreshToken 刷新）
// ---------------------------------------------------------------------------

/// `ExchangeToken` 的响应摘要：新 JWT + 可能轮换过的 refresh_token。
#[derive(Debug, Clone)]
pub(crate) struct ExchangedToken {
    /// 完整 `Cloud-IDE-JWT <token>` 头值。
    pub(crate) jwt: String,
    /// 上游若轮换了 refresh_token 则带上；否则 `None`（沿用旧的）。
    pub(crate) refresh_token: Option<String>,
}

/// refresh 交换的失败详情。
///
/// - `server_rejected = true`：服务端**明确拒绝**（旧形态数字 `code != 0`）
///   ⇒ refresh_token 已确定失效 ⇒ 文案「凭据已失效，请重新登录该账号」；
/// - `server_rejected = false`：网络 / 解析 / 错误信封等本地与协议级失败
///   ⇒ 文案「暂时失败，可稍后重试」。
///
/// ★ **不得**把异构响应（无 `code`/`message` 的信封）误判为「凭据失效」——
/// 参考 `oauth.rs:859` 记录了这正是旧实现的缺陷（`unwrap_or(-1)` 误判）。
#[derive(Debug, Clone)]
pub(crate) struct RefreshExchangeError {
    /// 失败原因（可进日志与错误文案；**不含任何令牌正文**）。
    pub(crate) message: String,
    /// 是否属于「服务端明确拒绝」。
    pub(crate) server_rejected: bool,
}

impl RefreshExchangeError {
    fn transport(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            server_rejected: false,
        }
    }
}

/// 一个刷新变体（**纯数据**，便于单测断言顺序与签名路径）。
#[derive(Debug, Clone)]
pub(crate) struct RefreshVariant {
    /// 诊断标签（错误消息里逐变体列出）。
    pub(crate) tag: String,
    /// 完整请求 URL。
    pub(crate) url: String,
    /// DeviceProof 的签名路径（无 Proof 的变体为空串）。
    pub(crate) sign_path: &'static str,
    /// 请求体。
    pub(crate) payload: Value,
    /// 是否带 `x-cloudide-token: ""`（空串头）。
    pub(crate) with_cloudide_token: bool,
}

/// refresh 请求体（固化协议）：`{ClientID, RefreshToken, DeviceID, PlatformCode, DeviceProof}`。
pub(crate) fn build_refresh_payload(
    client_id: &str,
    refresh_token: &str,
    device_id: &str,
    proof: Value,
) -> Value {
    json!({
        "ClientID": client_id,
        "RefreshToken": refresh_token,
        "DeviceID": device_id,
        "PlatformCode": TRAE_PAGE_PLATFORM_CODE,
        "DeviceProof": proof,
    })
}

/// 旧协议兜底请求体：`{ClientID, RefreshToken, ClientSecret, UserID}`。
///
/// **逐字保留**（与改造前 `account.rs:769-774` 一致）：它是参考项目自己排在最后、
/// 并标注「保留至固化协议验证期结束」的那条路径，改动它等于同时改掉唯一的历史退路。
pub(crate) fn build_legacy_refresh_payload(client_id: &str, refresh_token: &str) -> Value {
    json!({
        "ClientID": client_id,
        "RefreshToken": refresh_token,
        "ClientSecret": crate::modules::trae::oauth_client::DEFAULT_CLIENT_SECRET,
        "UserID": "",
    })
}

/// 构造 refresh 的**四步变体链**（顺序即优先级，纯函数 ⇒ 可单测）。
///
/// | 步 | 端点 | 请求体 | DeviceProof | 需要私钥 |
/// |:--|:---|:---|:---|:---|
/// | 1 | `${icube_base}/trae/api/v3/oauth/ExchangeToken` | 固化体 | P1363 | 是 |
/// | 2 | 同 1 | 同 1（仅签名编码不同） | DER | 是 |
/// | 3 | `${icube_base}/cloudide/api/v3/trae/oauth/ExchangeToken`（旧端点） | 同 1 | P1363 | 是 |
/// | 4 | 同 3（旧端点） | `{ClientID, RefreshToken, ClientSecret, UserID}` | 无 | 否 |
///
/// - 步 1–3 **仅在取到设备凭证**时进入（`credential` 为 `None` 时整段跳过）；
/// - 步 4 **无条件**存在（无凭证时它是唯一路径）；
/// - `sign_path` 按端点区分（步 1/2 用新路径、步 3 用旧路径），与参考
///   `oauth.rs:882-897` 逐字一致 —— 签名原文里的路径与请求 URL 的路径必须匹配，
///   否则服务端按另一条路径验签必然 20405。
fn build_refresh_variants(
    variant: TraeVariant,
    client_id: &str,
    refresh_token: &str,
    credential: Option<&DeviceCredential>,
) -> Vec<RefreshVariant> {
    let icube_base = crate::modules::trae::endpoints_for(variant).icube_base;
    let new_url = format!("{icube_base}{TRAE_EXCHANGE_TOKEN_PATH}");
    let legacy_url = format!("{icube_base}{TRAE_EXCHANGE_TOKEN_LEGACY_PATH}");

    let mut variants = Vec::with_capacity(4);
    if let Some(credential) = credential {
        for (format, url, sign_path, tag) in [
            (
                ProofSigFormat::P1363,
                new_url.as_str(),
                TRAE_EXCHANGE_TOKEN_PATH,
                "Refresh/Proof/P1363",
            ),
            (
                ProofSigFormat::Der,
                new_url.as_str(),
                TRAE_EXCHANGE_TOKEN_PATH,
                "Refresh/Proof/DER",
            ),
            (
                ProofSigFormat::P1363,
                legacy_url.as_str(),
                TRAE_EXCHANGE_TOKEN_LEGACY_PATH,
                "Refresh/LegacyEndpoint/Proof/P1363",
            ),
        ] {
            // 签名失败（私钥形态变化）时**跳过该变体**而不是整体失败：
            // 后面还有 DER 与旧协议兜底，多一条路总比直接放弃好。
            let Ok(proof) =
                icube::device_proof(credential, sign_path, client_id, refresh_token, format)
            else {
                continue;
            };
            variants.push(RefreshVariant {
                tag: tag.to_string(),
                url: url.to_string(),
                sign_path,
                payload: build_refresh_payload(
                    client_id,
                    refresh_token,
                    &credential.device_id,
                    proof,
                ),
                with_cloudide_token: true,
            });
        }
    }

    // 旧协议兜底：无条件存在（无凭证时唯一路径）。
    variants.push(RefreshVariant {
        tag: "Refresh/Legacy".to_string(),
        url: legacy_url,
        sign_path: "",
        payload: build_legacy_refresh_payload(client_id, refresh_token),
        with_cloudide_token: false,
    });

    variants
}

/// 读某账号**绑定的**设备身份（[`refresh_jwt_for`] 的唯一取值点）。
///
/// 抽成具名函数而不是内联在 `refresh_jwt_for` 里：一是与 `snapshot_data_dir_for`
/// 同一套「唯一取值点」写法，二是让「绑定读得到 / 读不到」这条**可单测**
/// （`refresh_jwt_for` 本体要发网络请求，测不动）。
/// 「读到的值确实被传下去」由签名保证：`exchange_token_for` 必须收第三个实参。
fn bound_device_id(variant: TraeVariant, user_id: &str) -> Option<String> {
    load_accounts_for(variant)
        .device_bindings
        .get(user_id)
        .cloned()
}

/// 解析本次续期要用的**设备凭证**（**同源优先**，[`exchange_token_for`] 的唯一取值点）。
///
/// - `Some(id)` ⇒ [`icube::device_credential_by_device_id`] **按 id 精确取**，不依赖活跃度；
/// - `None`（旧账号 / 无绑定）⇒ [`icube::device_credential_for`] 回落「当前目录的那一条」，
///   并**写一条留痕日志** —— 这条路径拿到的私钥可能与账号当初用的那台不一致，
///   出问题时必须能从日志里看出来「这次是猜的」。
///
/// 抽出来是为了**可测**：`exchange_token_for` 的其余部分要发网络请求，
/// 而「到底取了哪台设备的私钥」正是 T13-4 要钉住的那件事。
fn resolve_refresh_credential(
    variant: TraeVariant,
    device_id: Option<&str>,
) -> Result<DeviceCredential, icube::IcubeError> {
    match device_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(device_id) => icube::device_credential_by_device_id(variant, device_id),
        None => {
            // 留痕：不阻塞流程，但必须留下「这次没有绑定、是猜的」这个事实。
            store::append_log(
                &paths::checkin_log_file_for(variant),
                &format!(
                    "设备凭证回落：账号无绑定 deviceId，改用【{}】当前目录的那一条\
                     （续期若报 20403/20405，请重新导入本机账号以重建绑定）",
                    variant.display_name()
                ),
            );
            icube::device_credential_for(variant)
        }
    }
}

/// 调 `ExchangeToken` 把 refresh_token 换成 access token（**四步变体链**）。
///
/// **不做任何落盘**：调用方决定「这条凭据属于谁、要不要写回」。
/// 抽出来是为了让两个调用方共享同一份线上协议：
/// - [`refresh_jwt_for`]（已存在账号的续期）：先用旧记录定位账号，再校验 uid 一致后写回；
/// - `oauth::perform_login` 的**兼容路径**（回调直接给 refreshToken，老形态）。
///
/// ## 为什么这是本轮的核心修复之一
///
/// 改造前这里只发 `{ClientID, RefreshToken, ClientSecret, UserID}` —— 那正是参考项目
/// **自己排在最后、并标注「保留至固化协议验证期结束」的旧协议兜底**，且全仓
/// `DeviceProof` 零命中 ⇒ 自动续期走的是一条上游很可能已不接受的路径。
/// ## 设备凭证的来源（**同源优先**）
///
/// `device_id` = 该账号**绑定的**那台设备（[`AccountsFile::device_bindings`]，G-b）：
/// - `Some(id)` ⇒ [`icube::device_credential_by_device_id`] **按 id 精确取**
///   （不依赖活跃度）—— 客户端换过设备、或本变体有多个候选目录时，
///   「活跃目录里的那一条」未必是这台，用错私钥会被上游拒（20403/20405）；
/// - `None`（旧账号 / 无绑定）⇒ [`icube::device_credential_for`] 回落「当前目录的那一条」，
///   并**写一条留痕日志**：这条路径拿到的私钥可能与账号当初用的那台不一致，
///   出问题时必须能从日志里看出来「这次是猜的」。
pub(crate) async fn exchange_token_for(
    variant: TraeVariant,
    refresh_token: &str,
    device_id: Option<&str>,
) -> Result<ExchangedToken, RefreshExchangeError> {
    let credential = resolve_refresh_credential(variant, device_id);
    let credential_note = credential
        .as_ref()
        .err()
        .map(|error| format!("{}（kind={}）", error.user_message(variant), error.kind()));
    let credential = credential.ok();

    // `x-device-id`：有设备凭证时与 DeviceProof 的 DeviceID 同源（参考
    // `load_or_create_oauth_device` 正是把 oauth 设备 id 覆盖成 icube 的 deviceId）。
    //
    // ★ 无凭证时**不发这个头**，绝不回落到任何「本机自造」的 device_id：
    // 自造值与签名私钥不匹配，上游只会回 20403/20405（参考自己的注释已经写明）。
    // 少一个头是「诚实地说我没有设备身份」，编一个出来是「撒谎」。
    let header_device_id = credential
        .as_ref()
        .map(|credential| credential.device_id.clone());

    // ★ `ClientID` 按**产品线**取（SOLO 与 TRAE 各有独立的一把钥匙，见
    // `OAuthLine::default_client_id`）。它与授权 URL / AuthCode 交换用的是同一个
    // 取值函数 —— 三处必须同源，否则等于拿 A 线的钥匙兑 B 线的 token。
    let client_id = crate::modules::trae::oauth_client::oauth_client()
        .client_id_for(variant.oauth_line());
    let variants = build_refresh_variants(variant, client_id, refresh_token, credential.as_ref());

    let mut errors: Vec<String> = Vec::new();
    let mut last_error: Option<RefreshExchangeError> = None;
    for plan in &variants {
        match try_refresh_variant(plan, header_device_id.as_deref()).await {
            Ok((access_token, refresh_token, _body)) => {
                return Ok(ExchangedToken {
                    jwt: jwt::authorization_header(&access_token),
                    refresh_token,
                });
            }
            Err(error) => {
                // 旧形态数字 code != 0：服务端明确拒绝，立即采纳不再探测。
                if error.server_rejected {
                    return Err(error);
                }
                // 签名路径一并进错误消息：20405 的首要排查项就是「签名原文里的路径
                // 与请求 URL 的路径是否匹配」，不给出来用户只能靠猜。
                errors.push(if plan.sign_path.is_empty() {
                    format!("{}: {}", plan.tag, error.message)
                } else {
                    format!(
                        "{}（签名路径 {}）: {}",
                        plan.tag, plan.sign_path, error.message
                    )
                });
                last_error = Some(error);
            }
        }
    }

    // 全部失败：**逐变体列出**（否则用户无从判断是协议问题还是凭据问题）。
    let mut message = format!("全部刷新变体失败 → {}", errors.join(" | "));
    if let Some(note) = credential_note {
        message.push_str(&format!("；未取到设备凭证：{note}"));
    }
    Err(RefreshExchangeError {
        message,
        server_rejected: last_error.map(|e| e.server_rejected).unwrap_or(false),
    })
}

/// 把刷新失败映射成**可操作**的用户文案（纯函数，可单测）。
///
/// 「凭据已失效」与「稍后重试」是两种完全不同的用户动作，混为一谈会让用户
/// 反复重试一条已经失效的凭据，或者反过来把一次网络抖动当成账号失效而重新登录。
pub(crate) fn refresh_error_message(error: &RefreshExchangeError) -> String {
    if error.server_rejected {
        format!("凭据已失效，请重新登录该账号（{}）", error.message)
    } else {
        format!("刷新暂时失败，可稍后重试（{}）", error.message)
    }
}

/// 单个刷新变体尝试：设备头请求 → 火山信封错误解析 → token 提取。
///
/// 与 AuthCode 场景的差异：
/// - refresh_token 可能**不轮换**：响应缺 `RefreshToken` 视为成功（调用方保留旧值）；
/// - 仅旧形态数字 `code != 0` 判为「服务端明确拒绝」（`server_rejected = true`）；
///   火山错误信封（20403/20405 等设备/协议错误）视为变体失败、继续探测。
async fn try_refresh_variant(
    plan: &RefreshVariant,
    device_id: Option<&str>,
) -> Result<(String, Option<String>, Value), RefreshExchangeError> {
    let client = crate::modules::trae::credits::trae_http_client();
    let mut request = client
        .post(&plan.url)
        .header("content-type", "application/json")
        .header("accept", "*/*")
        // 设备头（F-70 情报：ExchangeToken 专属错误码 20403=Device not match /
        // 20405=Device proof required ⇒ 交换与设备绑定强相关）。
        // `x-device-id` 只在**取到 icube 设备凭证**时带（见 `exchange_token_for`）。
        .header("x-app-id", TRAE_OAUTH_APP_ID)
        .header("x-platform-code", TRAE_PAGE_PLATFORM_CODE);
    if let Some(device_id) = device_id {
        request = request.header("x-device-id", device_id);
    }
    if plan.with_cloudide_token {
        // 实测：`x-cloudide-token` 必须为**空字符串**——带旧 token 报 20405，
        // 完全不带该头报 20403。
        request = request.header("x-cloudide-token", "");
    }
    let response = request
        .json(&plan.payload)
        .send()
        .await
        .map_err(|e| {
            // 与签到路径同一出口：展开 source 链，否则只剩「请求失败: error sending
            // request for url (…)」，用户无从判断是重试还是修网络。
            RefreshExchangeError::transport(format!(
                "请求失败: {}",
                crate::modules::net::describe_transport_error(&e)
            ))
        })?;

    let status = response.status().as_u16();
    let body: Value = response
        .json()
        .await
        .map_err(|e| RefreshExchangeError::transport(format!("解析响应失败 (HTTP {status}): {e}")))?;

    // 火山信封错误：协议级失败（可能是 DeviceProof/设备问题），继续探测下一变体。
    if let Some(code) = dig_string(&body, &["ResponseMetadata", "Error", "Code"]) {
        if code != "0" {
            let message = dig_string(&body, &["ResponseMetadata", "Error", "Message"])
                .unwrap_or_else(|| "未知错误".into());
            let standard = dig_string(&body, &["ResponseMetadata", "Error", "StandardCode"])
                .unwrap_or_default();
            return Err(RefreshExchangeError::transport(format!(
                "code={code}/{standard}: {message}"
            )));
        }
    }
    // 旧形态数字 code：**唯一可信的**「服务端明确拒绝」信号。
    if let Some(code) = body.get("code").and_then(|value| value.as_i64()) {
        if code != 0 {
            let message = body
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("未知错误");
            return Err(RefreshExchangeError {
                message: format!("code={code}: {message}"),
                server_rejected: true,
            });
        }
    }

    let access = find_token_value(&body, ACCESS_TOKEN_KEYS).filter(|token| !token.is_empty());
    let refresh = find_token_value(&body, REFRESH_TOKEN_KEYS).filter(|token| !token.is_empty());
    match access {
        Some(access) => Ok((access, refresh, body)),
        None => Err(RefreshExchangeError::transport(format!(
            "响应中未找到 AccessToken 字段（响应键：{}）",
            key_paths(&body).join(" | ")
        ))),
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

/// access token 的候选键（精确名优先，其次大小写不敏感）。
const ACCESS_TOKEN_KEYS: &[&str] = &["AccessToken", "access_token", "token", "Jwt", "JWT"];
/// refresh token 的候选键。
const REFRESH_TOKEN_KEYS: &[&str] = &["RefreshToken", "refresh_token"];

/// 在 JSON 树中递归查找指定键（大小写不敏感）的首个非空字符串值。
fn find_token_value(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            // 精确匹配优先，其次大小写不敏感。
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
                if let Some(found) = find_token_value(candidate, keys) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| find_token_value(item, keys)),
        _ => None,
    }
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

/// 用 AuthCode 交换得到的 `(jwt, refresh_token)` 落库（OAuth 登录的收尾动作）。
///
/// 与 [`add_manual`] 的差别就是**本模块原先最大的缺口**：粘贴式账号
/// `refresh_token` 恒为 `None`，因此 JWT 到期后只能重新粘贴；OAuth 登录的账号
/// 从这里起就带着 refresh_token，[`refresh_jwt`] 之后能自动续期。
///
/// ## 已存在同 uid 时的合并语义（每一步都必须有理由）
///
/// | 字段 | 动作 | 为什么 |
/// |:---|:---|:---|
/// | `jwt` | **覆盖** | 本次登录的唯一产物，不覆盖等于没登录 |
/// | `refresh_token` | 传了才覆盖 | 上游可能不轮换；用 `None` 清空会让刚登录的账号立刻失去自动续期 |
/// | `user_id` | 覆盖并补全 | 旧记录可能只靠 JWT 现算 uid，落成显式值能抗 JWT 解析失败 |
/// | `name` | 传了才覆盖 | 用户在库里改过备注名时，不该被上游昵称冲掉 |
/// | `added_at` | 保留 | 是「首次添加」，不是「本次更新」 |
/// | `updated_at` | 刷新 | 语义就是最近更新时间 |
///
/// `name` 那一行容易写错成「总是覆盖」——上游昵称常常是一串无意义默认值，
/// 覆盖掉用户自己起的名字是纯损失。
///
/// **变体在这里有两处作用，缺一即错**：写进该变体的账号库；uid 与 JWT 都来自该变体
/// 的交换结果。若写错变体，就会产生「账号出现了但凭据对该产品线无效」——
/// 报错指向上游，用户永远查不到原因。
///
/// 返回 `(落盘后的账号记录, 新 JWT)`。
///
/// `device_id` 是**登录时实际使用的那台设备**（OAuth 三方同源的那个 `identity.device_id`），
/// 落库时写进 [`AccountsFile::device_bindings`]（G-b）—— 续期要复用它签名。
/// 传 `None` 时不写（**不动**既有绑定，见 [`bind_device`]）。
pub fn login_with_exchanged_tokens_for(
    variant: TraeVariant,
    exchanged_jwt: String,
    refresh_token: Option<String>,
    display_name: Option<String>,
    device_id: Option<&str>,
) -> Result<(RawAccount, String), String> {
    // uid 以上游签发的 JWT 为准，而不是信任本地传入的任何 ID：
    // 这条路径上本来没有可信的 uid 来源（账号可能还不存在）。
    let uid = jwt::parse(&exchanged_jwt)
        .user_id
        .filter(|uid| !uid.trim().is_empty())
        .ok_or("无法从新 JWT 解析 UserID，已拒绝写库")?;

    let refresh_token = refresh_token
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty());

    let mut accounts = load_accounts_for(variant);
    let now = store::now_iso();
    let trimmed_name = display_name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());

    let record = match accounts
        .accounts
        .iter_mut()
        .find(|account| resolve_user_id(account) == uid)
    {
        Some(existing) => {
            existing.jwt = exchanged_jwt.clone();
            if let Some(token) = refresh_token {
                existing.refresh_token = Some(token);
            }
            existing.user_id = Some(uid.clone());
            if let Some(name) = trimmed_name {
                existing.name = name;
            }
            existing.updated_at = Some(now);
            existing.clone()
        }
        None => {
            let record = RawAccount {
                // 上游没给昵称时用 uid 尾 6 位兜底，与 `add_manual` 同一约定，
                // 保证列表里能区分多个「账号」。
                name: trimmed_name
                    .unwrap_or_else(|| format!("账号{}", &uid[uid.len().saturating_sub(6)..])),
                user_id: Some(uid.clone()),
                jwt: exchanged_jwt.clone(),
                refresh_token,
                added_at: Some(now.clone()),
                updated_at: Some(now),
            };
            accounts.accounts.push(record.clone());
            record
        }
    };

    // 绑定**这次登录用的那台设备**：续期必须复用它签名（换设备/多候选目录时，
    // 「活跃目录里的那一条」未必是这台）。
    bind_device(&mut accounts, &uid, device_id);
    save_accounts_for(variant, &accounts)?;
    Ok((record, exchanged_jwt))
}

/// 刷新后的 uid 一致性校验（纯函数，便于单测）。
///
/// 不一致说明 refresh_token 属于另一个账号 —— 此时**拒绝写回**，
/// 否则会把账号 A 的凭据写进账号 B 的记录里（用户看到的是「刷新成功但账号串了」）。
///
/// 新 JWT 解析不出 uid 时**放行**：上游可能不总是带 `data.id`，
/// 而这里没有更可信的来源；拒绝反而会让正常续期失败。
fn ensure_refreshed_uid_matches(expected: &str, actual: Option<&str>) -> Result<(), String> {
    match actual {
        Some(actual) if actual != expected => Err(format!(
            "刷新后 user_id 不匹配: 期望={expected}, 实际={actual}；已拒绝写回"
        )),
        _ => Ok(()),
    }
}

/// 用 `refresh_token` 换新 JWT（ExchangeToken）；默认变体，兼容壳。
pub async fn refresh_jwt(user_id: &str) -> Result<String, String> {
    refresh_jwt_for(TraeVariant::default(), user_id).await
}

/// 用 `refresh_token` 换新 JWT（ExchangeToken；按变体分家）。
///
/// **只对 OAuth 登录的账号可用**：代理/浏览器抓取得到的 JWT 没有 refresh_token，
/// 报错文案直接指向补救动作（重新抓取），而不是让用户对着「刷新失败」猜。
///
/// 刷新后校验 uid 一致：不一致说明 refresh_token 属于另一个账号，
/// 此时**拒绝写回**，否则会把账号 A 的凭据写进账号 B 的记录里。
///
/// **设备身份取自账号绑定**（[`AccountsFile::device_bindings`]，G-b）：
/// 续期必须用「这个账号当初登录 / 导入时那台设备」的私钥签名。旧账号没有绑定 ⇒
/// 透传 `None`，由 [`exchange_token_for`] 回落当前目录并**留痕**。
pub async fn refresh_jwt_for(variant: TraeVariant, user_id: &str) -> Result<String, String> {
    let account = find_for(variant, user_id).ok_or("账号不存在")?;
    let refresh_token = account
        .refresh_token
        .as_ref()
        .filter(|token| !token.is_empty())
        .ok_or("该账号无 refresh_token（抓取得到的账号不支持自动刷新），请重新获取 JWT")?
        .clone();
    // 绑定可能不存在（旧账号 / 刚换过 uid）—— 那正是回落分支存在的意义，不是错误。
    let device_binding = bound_device_id(variant, user_id);

    let exchanged = exchange_token_for(variant, &refresh_token, device_binding.as_deref())
        .await
        .map_err(|error| refresh_error_message(&error))?;
    let new_jwt = exchanged.jwt;
    let info = jwt::parse(&new_jwt);
    ensure_refreshed_uid_matches(user_id, info.user_id.as_deref())?;

    apply_refreshed_tokens_for(variant, user_id, &new_jwt, exchanged.refresh_token)?;

    store::append_log(
        &paths::checkin_log_file_for(variant),
        &format!(
            "JWT 自动刷新成功: user={user_id} 新到期={}",
            info.exp_hours
                .map(|hours| format!("{hours:.1}h"))
                .unwrap_or_else(|| "?".into())
        ),
    );

    Ok(new_jwt)
}

/// 把一次刷新的结果写回账号库（**纯落盘，不发网络**，故可直接单测）。
///
/// ## 三条语义各自都容易写错，且错了都不会编译报错
///
/// 1. `jwt` **必须覆盖**——本次刷新的唯一产物，不覆盖等于没刷新；
/// 2. `Result.RefreshToken` 存在时**必须覆盖**——上游可能轮换 refresh token，
///    丢弃轮换值 ⇒ 下次刷新拿旧 token 必然失败。**这是「账号过一阵子掉线」的
///    另一半根因**（另一半是 refresh 走的协议已过时，见 [`exchange_token_for`]）；
///    `None` 时**保留旧值**（上游可能不轮换，清空会让账号立刻失去自动续期）；
/// 3. 必须写回**该变体**的账号库——改造前这里调的是无参 `save_accounts`，
///    会把 Trae CN 的续期结果写进 Trae Work 的库（静默串号）。
///
/// 重新读文件而不是复用调用方的快照：刷新期间用户可能在前端改过这个账号。
fn apply_refreshed_tokens_for(
    variant: TraeVariant,
    user_id: &str,
    new_jwt: &str,
    rotated_refresh_token: Option<String>,
) -> Result<(), String> {
    let mut accounts = load_accounts_for(variant);
    let target = accounts
        .accounts
        .iter_mut()
        .find(|account| resolve_user_id(account) == user_id)
        .ok_or("账号不存在")?;
    target.jwt = new_jwt.to_string();
    if let Some(refresh_token) = rotated_refresh_token {
        target.refresh_token = Some(refresh_token);
    }
    target.updated_at = Some(store::now_iso());
    save_accounts_for(variant, &accounts)
}

/// 上游签到/积分接口的完整 URL 构造（集中一处，便于测试与替换上游）。
///
/// 端点按**变体**查表派生（见 [`crate::modules::trae::variant`]）。
/// 实测两条 CN 产品线的取值逐字相同，故本函数的输出与改造前一致。
pub fn checkin_url() -> String {
    checkin_url_for(TraeVariant::default())
}

/// 按变体派生的 [`checkin_url`]。
pub fn checkin_url_for(variant: TraeVariant) -> String {
    let base = crate::modules::trae::endpoints_for(variant).account_base;
    format!("{base}{}", crate::modules::trae::TRAE_CHECKIN_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn scope_parse_accepts_all_group_selected() {
        assert_eq!(Scope::parse("all").unwrap(), Scope::All);
        assert_eq!(Scope::parse("").unwrap(), Scope::All);
        assert_eq!(
            Scope::parse("group:g_1").unwrap(),
            Scope::Group("g_1".into())
        );
        assert_eq!(Scope::parse(" selected ").unwrap(), Scope::Selected(vec![]));
    }

    #[test]
    fn scope_parse_rejects_unknown_instead_of_defaulting_to_all() {
        // 关键护栏：未知范围不能被当成 All，否则「只签一组」会静默变成「全签」。
        assert!(Scope::parse("grup:g_1").is_err());
        assert!(Scope::parse("group:").is_err());
        assert!(Scope::parse("whatever").is_err());
    }

    #[test]
    fn resolve_user_id_prefers_stored_then_jwt() {
        let stored = RawAccount {
            name: "n".into(),
            user_id: Some("stored-uid".into()),
            jwt: String::new(),
            refresh_token: None,
            added_at: None,
            updated_at: None,
        };
        assert_eq!(resolve_user_id(&stored), "stored-uid");

        // 记录里没有 uid 时从 JWT 现算
        let jwt_value = crate::modules::trae::jwt::authorization_header("x");
        let _ = jwt_value;
        let mut fallback = stored.clone();
        fallback.user_id = None;
        assert_eq!(resolve_user_id(&fallback), "");

        // 空白 UserID 视为缺失，不能产出 "  " 这种 uid
        let mut blank = stored.clone();
        blank.user_id = Some("   ".into());
        assert_eq!(resolve_user_id(&blank), "");
    }

    /// 展示名取自账号库的 `name`，且**绝不**把 uid 当展示名返回。
    ///
    /// 反例（改坏会红）：
    /// - 名字空白时返回 `Some("")` → 状态条渲染成「已登录: 」；
    /// - 查不到时回落 `user_id` → 界面又变回那串 16 位数字，正是本次要修的；
    /// - 查找横跨变体 → 国际版会读到国内库的名字。
    #[test]
    fn display_name_reads_the_library_and_refuses_blanks() {
        let _env = crate::modules::trae::test_support::TempEnv::with_device_fixture();

        let mut file = AccountsFile::default();
        file.accounts.push(RawAccount {
            name: "JackDev".into(),
            user_id: Some("u-cn".into()),
            jwt: String::new(),
            refresh_token: None,
            added_at: None,
            updated_at: None,
        });
        file.accounts.push(RawAccount {
            name: "   ".into(),
            user_id: Some("u-blank".into()),
            jwt: String::new(),
            refresh_token: None,
            added_at: None,
            updated_at: None,
        });
        save_accounts_for(TraeVariant::Trae, &file).unwrap();

        assert_eq!(
            display_name_for(TraeVariant::Trae, "u-cn").as_deref(),
            Some("JackDev")
        );
        // 空白名 = 无名（不是 `Some("")`）。
        assert_eq!(display_name_for(TraeVariant::Trae, "u-blank"), None);
        // 库里没有这个 uid ⇒ 无名，回落与否交给调用方。
        assert_eq!(display_name_for(TraeVariant::Trae, "u-absent"), None);
        // 国内两条产品线**共用一本库** ⇒ 换程序位也读得到。
        assert_eq!(
            display_name_for(TraeVariant::TraeWork, "u-cn").as_deref(),
            Some("JackDev")
        );
        // 国际版是另一本库 ⇒ 读不到（跨区域冒充会在这里红）。
        assert_eq!(display_name_for(TraeVariant::Global, "u-cn"), None);
    }

    /// 参考实现格式的 `checkin_accounts.json` 必须能被直接读入（跨工具共享契约）。
    ///
    /// 这是「与既有 Trae 账号数据兼容」的核心断言：用户从参考工具迁移过来时，
    /// 磁盘上已有的账号库不能因为字段命名差异而读空。
    /// 反例（会让本测试失败的真实回归）：
    /// - 把 `UserID` 改成 `user_id` 且不加 alias → `user_id` 解析为 None，uid 落空；
    /// - 给任一字段去掉 `#[serde(default)]` → 旧文件缺该键时整个文件解析失败、回落空列表。
    #[test]
    fn reference_format_accounts_file_deserializes() {
        // 逐字取自参考实现 `device_proxy.py` 写出的形状（含非常规大写 UserID）。
        let raw = r#"{
          "accounts": [
            {
              "name": "主账号",
              "UserID": "7512345678901234567",
              "jwt": "Cloud-IDE-JWT eyJhbGciOiJIUzI1NiJ9.eyJ1c2VyX2lkIjoiNzUxMjM0NTY3ODkwMTIzNDU2NyJ9.x",
              "refresh_token": "rt-abc",
              "added_at": "2026-01-01T10:00:00",
              "updated_at": "2026-02-01T10:00:00"
            },
            {
              "name": "仅抓取的账号",
              "UserID": "7512345678901234568",
              "jwt": "Cloud-IDE-JWT eyJhbGciOiJIUzI1NiJ9.eyJ1c2VyX2lkIjoiNzUxMjM0NTY3ODkwMTIzNDU2OCJ9.y"
            }
          ]
        }"#;

        let file: AccountsFile =
            serde_json::from_str(raw).expect("参考实现格式必须可解析，否则既有账号库会读空");
        assert_eq!(file.accounts.len(), 2);

        // 大写 UserID 落到 user_id 字段
        assert_eq!(
            file.accounts[0].user_id.as_deref(),
            Some("7512345678901234567")
        );
        assert_eq!(file.accounts[0].name, "主账号");
        assert_eq!(file.accounts[0].refresh_token.as_deref(), Some("rt-abc"));
        // 缺失的可选字段（第二条没有 refresh_token / added_at / updated_at）必须容忍
        assert!(file.accounts[1].refresh_token.is_none());
        assert!(file.accounts[1].added_at.is_none());
        assert!(file.accounts[1].updated_at.is_none());

        // 解析出的 uid 必须与参考实现一致（以记录里的 UserID 为准）
        for account in &file.accounts {
            assert!(
                !resolve_user_id(account).is_empty(),
                "参考格式账号必须能解析出 uid"
            );
        }
    }

    /// 带未知字段的账号库必须可解析（前向兼容）。
    ///
    /// 参考工具后续版本可能加入新字段；本仓库不能用 `deny_unknown_fields`
    /// 让整个账号库解析失败——那是升级时的静默数据丢失。
    #[test]
    fn accounts_file_tolerates_unknown_fields() {
        let raw = r#"{
          "accounts": [
            {"name": "a", "UserID": "u1", "jwt": "j", "future_field": {"nested": 1}},
            {"name": "b", "UserID": "u2", "jwt": "j", "some_new_flag": true}
          ],
          "schema_version": 99
        }"#;
        let file: AccountsFile = serde_json::from_str(raw).expect("未知字段不得导致解析失败");
        assert_eq!(file.accounts.len(), 2);
        assert_eq!(file.accounts[1].user_id.as_deref(), Some("u2"));
    }

    /// 账号库序列化回磁盘后必须保留大写 `UserID`。
    ///
    /// 若写回时变成 `user_id`，参考实现的 `device_proxy.py` 就读不到了——
    /// 反向兼容同样必须成立（本工具不只是消费者，也是生产者）。
    #[test]
    fn accounts_file_roundtrip_keeps_reference_keys() {
        let account = RawAccount {
            name: "往返".into(),
            user_id: Some("7512345678901234567".into()),
            jwt: "j".into(),
            refresh_token: Some("rt".into()),
            added_at: Some("2026-01-01T00:00:00".into()),
            updated_at: None,
        };
        let file = AccountsFile {
            accounts: vec![account],
            // 无绑定：本用例还要证明「空表不落盘」—— 序列化结果与改造前**逐字一致**。
            device_bindings: HashMap::new(),
        };
        let text = serde_json::to_string(&file).expect("序列化");
        // 关键：键名必须是参考实现的大写 UserID，而不是 user_id
        assert!(
            text.contains("\"UserID\""),
            "写回必须保留大写 UserID 键，实际: {text}"
        );
        assert!(
            !text.contains("\"user_id\""),
            "不得写回 snake_case 键，实际: {text}"
        );
        // 新增的容器键在**空表时不得出现**：否则既有备份 / 参考实现的 `device_proxy.py`
        // 读到的形状就变了（N-8：只新增一个可选键，不动既有键，也不无端多一个空键）。
        assert!(
            !text.contains("device_bindings"),
            "无绑定时不得写出 device_bindings 键，实际: {text}"
        );
        // 且能再次读回
        let back: AccountsFile = serde_json::from_str(&text).expect("回读");
        assert_eq!(back.accounts[0].user_id, file.accounts[0].user_id);
        assert!(back.device_bindings.is_empty());
    }

    /// 空账号库与缺 `accounts` 键的账号库都必须解析成功并给出空列表。
    #[test]
    fn accounts_file_handles_empty_and_missing_array() {
        assert!(serde_json::from_str::<AccountsFile>("{}")
            .expect("缺 accounts 键应回落空列表")
            .accounts
            .is_empty());
        assert!(serde_json::from_str::<AccountsFile>(r#"{"accounts": []}"#)
            .expect("空数组")
            .accounts
            .is_empty());
    }

    #[test]
    fn account_view_exposes_camel_case_wire_fields() {
        let account = RawAccount {
            name: "测试账号".into(),
            user_id: Some("1234567890123456".into()),
            jwt: "not-a-jwt".into(),
            refresh_token: Some("rt".into()),
            added_at: Some("2026-01-01T00:00:00".into()),
            updated_at: None,
        };
        let view = account_view(
            &account,
            "1234567890123456",
            Some("g_1".into()),
            Some("123456789012345"),
            Some(12.5),
            Some(1_800_000_000),
            None,
            true,
            Some(30),
        );
        // 线上形态必须是 camelCase —— 前端 TS 类型按此定义。
        for key in [
            "userId",
            "groupId",
            "jwtExpHours",
            "jwtExpTimestamp",
            "jwtStatus",
            "checkedToday",
            "remainingCredits",
            "creditsExpireAt",
            "deviceIdMasked",
            "hasRefreshToken",
            "jwtAutoRefresh",
            "addedAt",
            "updatedAt",
        ] {
            assert!(view.get(key).is_some(), "缺少线上字段 {key}");
        }
        // 持久化键名（大写 UserID）绝不能泄漏到线上形态
        assert!(view.get("UserID").is_none());
        assert!(view.get("user_id").is_none());
        assert_eq!(view.get("checkedToday").unwrap().as_bool(), Some(true));
        assert_eq!(view.get("remainingCredits").unwrap().as_f64(), Some(12.5));
        assert_eq!(view.get("deviceIdMasked").unwrap().as_str(), Some("1234…2345"));
        // 无 JWT 时必须给出 unknown 而不是崩溃
        assert_eq!(view.get("jwtStatus").unwrap().as_str(), Some("unknown"));
    }

    /// ★ 回归：`checkedToday` 必须取自**当日台账**，不能取自「最近一次签到摘要」。
    ///
    /// 现场（2026-09-21 用户报障「多账号签到只有一个显示成功」）：
    /// Jackey 15:03:57 claim 成功（`credits_history` 里确有 delta=150），
    /// 紧接着 15:04:36 那一轮只处理了 JackDev（摘要被整体覆盖成只含 JackDev），
    /// 旧实现按 `name` 匹配摘要 ⇒ Jackey 的徽章翻回「未签到」，
    /// 且下一轮 `skip_checked_in` 会把已经签过的账号再探一遍。
    #[test]
    fn checked_today_comes_from_the_ledger_not_the_last_run_summary() {
        let _env = crate::modules::trae::test_support::TempEnv::empty();
        let variant = TraeVariant::default();
        let jack_dev = "3604620555324748";
        let jackey = "1189017012674171";

        let mut file = load_accounts_for(variant);
        file.accounts.push(raw(jack_dev, "JackDev"));
        file.accounts.push(raw(jackey, "Jackey"));
        save_accounts_for(variant, &file).expect("造账号库");

        // 今天两个账号都签过 ⇒ 台账两条（跨运行累积）。
        crate::modules::trae::credits::mark_checked_in_for(variant, jack_dev).unwrap();
        crate::modules::trae::credits::mark_checked_in_for(variant, jackey).unwrap();
        // 而「最近一次摘要」只含最后一轮处理过的 JackDev —— 旧实现唯一的数据源。
        crate::modules::trae::credits::save_summary_for(
            variant,
            &crate::modules::trae::credits::CheckinSummary {
                time: Some(store::now_iso()),
                results: vec![json!({
                    "name": "JackDev",
                    "userId": jack_dev,
                    "ok": true,
                    "action": "skip_already",
                })],
                already: 1,
                ..Default::default()
            },
        )
        .expect("造摘要");

        let views = list_account_views_for(variant);
        let checked = |uid: &str| {
            views
                .iter()
                .find(|view| view.get("userId").and_then(Value::as_str) == Some(uid))
                .and_then(|view| view.get("checkedToday").and_then(Value::as_bool))
                .unwrap_or(false)
        };
        assert!(checked(jack_dev), "JackDev 在台账里 ⇒ 已签到");
        assert!(
            checked(jackey),
            "Jackey 今天已 claim 成功（摘要里没有它，但台账里有）⇒ 必须仍是已签到"
        );
    }

    /// 台账按 `userId` 记 ⇒ **同名**账号不会互相冒充（旧实现按 `name` 匹配摘要）。
    #[test]
    fn checked_today_is_keyed_by_user_id_not_display_name() {
        let _env = crate::modules::trae::test_support::TempEnv::empty();
        let variant = TraeVariant::default();
        let checked_uid = "1111111111111111";
        let unchecked_uid = "2222222222222222";

        let mut file = load_accounts_for(variant);
        file.accounts.push(raw(checked_uid, "同名"));
        file.accounts.push(raw(unchecked_uid, "同名"));
        save_accounts_for(variant, &file).expect("造账号库");

        crate::modules::trae::credits::mark_checked_in_for(variant, checked_uid).unwrap();

        let views = list_account_views_for(variant);
        let checked = |uid: &str| {
            views
                .iter()
                .find(|view| view.get("userId").and_then(Value::as_str) == Some(uid))
                .and_then(|view| view.get("checkedToday").and_then(Value::as_bool))
                .unwrap_or(false)
        };
        assert!(checked(checked_uid), "签过的那个必须已签到");
        assert!(!checked(unchecked_uid), "同名的另一个不得被冒充成已签到");
    }

    #[test]
    fn account_view_hides_expired_cooldown() {
        let account = RawAccount {
            name: "n".into(),
            user_id: Some("u".into()),
            jwt: "x".into(),
            refresh_token: None,
            added_at: None,
            updated_at: None,
        };
        let expired = crate::modules::trae::credits::CooldownEntry {
            error_type: "Server".into(),
            until: chrono::Local::now().timestamp() - 10,
            reason: "旧错误".into(),
            error_count: 1,
        };
        let view = account_view(&account, "u", None, None, None, None, Some(&expired), false, None);
        // 已过期的冷却不能继续显示为「冷却中」，否则 UI 会永久禁用账号操作。
        assert!(view.get("cooldownType").unwrap().is_null());
        assert!(view.get("cooldownUntil").unwrap().is_null());

        let active = crate::modules::trae::credits::CooldownEntry {
            error_type: "SessionDead".into(),
            until: 9_999_999_999,
            reason: String::new(),
            error_count: 0,
        };
        let view = account_view(&account, "u", None, None, None, None, Some(&active), false, None);
        assert_eq!(view.get("cooldownType").unwrap().as_str(), Some("SessionDead"));
        // 原因为空时不下发空串，前端用 null 判定「无原因」
        assert!(view.get("cooldownReason").unwrap().is_null());
    }

    #[test]
    fn account_view_auto_refresh_flag_requires_refresh_token() {
        let mut account = RawAccount {
            name: "n".into(),
            user_id: Some("u".into()),
            jwt: "x".into(),
            refresh_token: None,
            added_at: None,
            updated_at: None,
        };
        // 没有 refresh_token：即便 JWT 无法解析也不该提示「可自动刷新」
        let view = account_view(&account, "u", None, None, None, None, None, false, None);
        assert_eq!(view.get("hasRefreshToken").unwrap().as_bool(), Some(false));
        assert_eq!(view.get("jwtAutoRefresh").unwrap().as_bool(), Some(false));

        account.refresh_token = Some("rt".into());
        let view = account_view(&account, "u", None, None, None, None, None, false, None);
        assert_eq!(view.get("hasRefreshToken").unwrap().as_bool(), Some(true));
        assert_eq!(view.get("jwtAutoRefresh").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn checkin_url_uses_trae_api_base() {
        assert_eq!(
            checkin_url(),
            "https://api.trae.cn/trae/api/v2/ug/checkin_credits/claim"
        );
        // 变体化之后两条产品线的 CN 签到端点必须仍然逐字相同
        // （实测：产品线不改变端点，region 才改变端点）。
        for variant in TraeVariant::all() {
            assert_eq!(
                checkin_url_for(variant),
                "https://api.trae.cn/trae/api/v2/ug/checkin_credits/claim",
                "变体 {variant:?} 的签到端点与 CN 实测值不一致"
            );
        }
    }

    /// ★ 核心护栏：两条产品线的账号库**真的物理分开**。
    ///
    /// 这条测试走完整的「写 A → 写 B → 各自读回」路径，而不是只比路径字符串：
    /// 路径不同但读写函数用错变体（例如 `save_accounts_for` 内部仍读默认变体）
    /// 时，只有端到端断言才抓得住。
    ///
    /// 若这个测试红了，症状就是用户会看到的
    /// 「Trae CN 的账号出现在 Trae Work 的列表里」（或反之）。
    #[test]
    fn 两条产品线的账号库互不污染() {
        let temp = std::env::temp_dir().join(format!(
            "buddy-switch-trae-variant-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&temp);
        let _guard = crate::modules::config::HomeOverrideGuard::set(&temp);

        let work = TraeVariant::TraeWork;
        let cn = TraeVariant::Global;

        // 各写一条**不同**账号进各自的书。
        let mut work_file = load_accounts_for(work);
        work_file.accounts.push(RawAccount {
            name: "work-account".into(),
            user_id: Some("1111111111111111".into()),
            jwt: "Cloud-IDE-JWT work.jwt".into(),
            refresh_token: None,
            added_at: None,
            updated_at: None,
        });
        save_accounts_for(work, &work_file).expect("写 TraeWork 账号库");

        let mut cn_file = load_accounts_for(cn);
        cn_file.accounts.push(RawAccount {
            name: "cn-account".into(),
            user_id: Some("2222222222222222".into()),
            jwt: "Cloud-IDE-JWT cn.jwt".into(),
            refresh_token: None,
            added_at: None,
            updated_at: None,
        });
        save_accounts_for(cn, &cn_file).expect("写 Trae 账号库");

        // 各自只读到自己那条 —— 绝不能互相看见。
        let work_uids: Vec<String> = entries_for(work).into_iter().map(|(uid, _)| uid).collect();
        let cn_uids: Vec<String> = entries_for(cn).into_iter().map(|(uid, _)| uid).collect();
        assert_eq!(work_uids, vec!["1111111111111111".to_string()]);
        assert_eq!(cn_uids, vec!["2222222222222222".to_string()]);

        // 跨变体查找必须**查不到**：同一 uid 在另一条产品线里是另一个账号。
        assert!(find_for(work, "2222222222222222").is_none(), "跨变体查到了账号");
        assert!(find_for(cn, "1111111111111111").is_none(), "跨变体查到了账号");
        // 本变体内查得到。
        assert!(find_for(work, "1111111111111111").is_some());
        assert!(find_for(cn, "2222222222222222").is_some());

        // 两个文件必须真的落在磁盘上的不同文件里。
        let work_path = paths::accounts_file_for(work);
        let cn_path = paths::accounts_file_for(cn);
        assert!(work_path.is_file(), "TraeWork 账号库未落盘: {work_path:?}");
        assert!(cn_path.is_file(), "Trae 账号库未落盘: {cn_path:?}");
        assert_ne!(work_path, cn_path);

        let _ = std::fs::remove_dir_all(&temp);
    }

    /// `resolve_user_ids(All)` 必须限定在**该变体内**。
    ///
    /// 这是「Trae CN 签到不该动到 Trae Work 账号」的落点：
    /// 若 `All` 跨了变体，一次「全部签到」会把另一条产品线的账号也签一遍。
    #[test]
    fn 全部范围只覆盖本变体的账号() {
        let temp = std::env::temp_dir().join(format!(
            "buddy-switch-trae-scope-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&temp);
        let _guard = crate::modules::config::HomeOverrideGuard::set(&temp);

        let work = TraeVariant::TraeWork;
        let cn = TraeVariant::Global;

        save_accounts_for(
            work,
            &AccountsFile {
                accounts: vec![RawAccount {
                    name: "w".into(),
                    user_id: Some("w-uid".into()),
                    jwt: String::new(),
                    refresh_token: None,
                    added_at: None,
                    updated_at: None,
                }],
                device_bindings: HashMap::new(),
            },
        )
        .expect("写 TraeWork");
        save_accounts_for(
            cn,
            &AccountsFile {
                accounts: vec![RawAccount {
                    name: "c".into(),
                    user_id: Some("c-uid".into()),
                    jwt: String::new(),
                    refresh_token: None,
                    added_at: None,
                    updated_at: None,
                }],
                device_bindings: HashMap::new(),
            },
        )
        .expect("写 Trae");

        assert_eq!(
            resolve_user_ids_for(work, &Scope::All, None),
            vec!["w-uid".to_string()]
        );
        assert_eq!(
            resolve_user_ids_for(cn, &Scope::All, None),
            vec!["c-uid".to_string()]
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    // -----------------------------------------------------------------------
    // refreshToken 刷新：四步变体链（W13 的修复本体）
    // -----------------------------------------------------------------------

    /// 测试用 EC P-256 私钥（openssl 生成，**仅供测试**）。
    const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgfMhArVaVsHbRHQHS\n\
hLkrYGiVNhsErnjKIgS7/EIHdsihRANCAARznG0WLhenNiMW5jA3SwFpTNyet2zw\n\
1YDTNTiZI0L4egFQ0IzlUPgrZK/O5nwkmNJR0MukWZH7NgvdoDLsS0NB\n\
-----END PRIVATE KEY-----";

    fn test_credential() -> DeviceCredential {
        DeviceCredential {
            variant: TraeVariant::TraeWork,
            device_id: "2292929806738024".into(),
            private_key_pem: TEST_PRIVATE_KEY_PEM.to_string(),
            source_app: "TRAE SOLO CN".into(),
        }
    }

    /// 造一个真实形态的 Trae JWT（`Cloud-IDE-JWT` + 载荷 `data.id`）。
    fn make_jwt(user_id: &str) -> String {
        use base64::Engine as _;
        let payload = serde_json::json!({
            "data": { "id": user_id },
            "exp": chrono::Utc::now().timestamp() + 48 * 3600,
        });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("Cloud-IDE-JWT header.{encoded}.signature")
    }

    fn with_temp_home<T>(f: impl FnOnce() -> T) -> T {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-account-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("临时 home 应能创建");
        let guard = crate::modules::config::HomeOverrideGuard::set(&dir);
        let out = f();
        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    /// ★ 链的顺序与签名路径必须与参考 `oauth.rs:878-929` 逐字一致。
    #[test]
    fn refresh_chain_order_is_new_p1363_then_new_der_then_old_p1363_then_legacy() {
        let credential = test_credential();
        let variants =
            build_refresh_variants(TraeVariant::TraeWork, "cid", "rt", Some(&credential));

        let tags: Vec<&str> = variants.iter().map(|v| v.tag.as_str()).collect();
        assert_eq!(
            tags,
            vec![
                "Refresh/Proof/P1363",
                "Refresh/Proof/DER",
                "Refresh/LegacyEndpoint/Proof/P1363",
                "Refresh/Legacy",
            ],
            "四步链的顺序漂了"
        );

        let new_url = format!(
            "{}{}",
            crate::modules::trae::TRAE_OAUTH_BASE_CN,
            crate::modules::trae::TRAE_EXCHANGE_TOKEN_PATH
        );
        let legacy_url = format!(
            "{}{}",
            crate::modules::trae::TRAE_OAUTH_BASE_CN,
            crate::modules::trae::TRAE_EXCHANGE_TOKEN_LEGACY_PATH
        );

        // 步 1/2 用**新端点 + 新签名路径**。
        assert_eq!(variants[0].url, new_url);
        assert_eq!(variants[0].sign_path, TRAE_EXCHANGE_TOKEN_PATH);
        assert_eq!(variants[1].url, new_url);
        assert_eq!(variants[1].sign_path, TRAE_EXCHANGE_TOKEN_PATH);
        // 步 3 用**旧端点 + 旧签名路径**（路径必须与 URL 匹配，否则验签必失败）。
        assert_eq!(variants[2].url, legacy_url);
        assert_eq!(variants[2].sign_path, TRAE_EXCHANGE_TOKEN_LEGACY_PATH);
        // 步 4 是旧协议兜底。
        assert_eq!(variants[3].url, legacy_url);

        // 步 1–3 带空 `x-cloudide-token`，步 4 不带（与参考一致）。
        assert!(variants[0].with_cloudide_token);
        assert!(variants[1].with_cloudide_token);
        assert!(variants[2].with_cloudide_token);
        assert!(!variants[3].with_cloudide_token);

        // 两个 P1363 步骤的签名必须**不同**（同一时间戳下也如此，因为有随机 nonce）。
        let sig = |index: usize| {
            variants[index]
                .payload
                .get("DeviceProof")
                .and_then(|proof| proof.get("Signature"))
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string()
        };
        assert!(!sig(0).is_empty());
        assert!(!sig(2).is_empty());
    }

    /// 固化 refresh 请求体：`{ClientID, RefreshToken, DeviceID, PlatformCode, DeviceProof}`。
    #[test]
    fn build_refresh_payload_has_deviceproof_and_platform_code() {
        let proof = serde_json::json!({"Signature":"s","Timestamp":1,"Nonce":"n"});
        let payload = build_refresh_payload("ono9krqynydwx5", "rt", "dev-1", proof);
        let keys: Vec<&str> = payload
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        for expected in [
            "ClientID",
            "RefreshToken",
            "DeviceID",
            "PlatformCode",
            "DeviceProof",
        ] {
            assert!(keys.contains(&expected), "缺少字段 {expected}: {keys:?}");
        }
        assert_eq!(keys.len(), 5, "请求体字段集漂了: {keys:?}");
        assert_eq!(
            payload.get("PlatformCode").and_then(|v| v.as_str()),
            Some("IDE_PC")
        );
        assert_eq!(
            payload.get("DeviceID").and_then(|v| v.as_str()),
            Some("dev-1")
        );
        assert!(payload.get("DeviceProof").is_some());
    }

    /// `DeviceProof.Timestamp` 必须是 **JSON int**（字符串会被服务端 schema 拒绝）。
    #[test]
    fn refresh_payload_timestamp_is_json_int() {
        let credential = test_credential();
        let variants =
            build_refresh_variants(TraeVariant::TraeWork, "cid", "rt", Some(&credential));
        for variant in variants.iter().take(3) {
            let timestamp = variant
                .payload
                .get("DeviceProof")
                .and_then(|proof| proof.get("Timestamp"))
                .expect("带 Proof 的变体必须有 Timestamp");
            assert!(
                timestamp.is_u64() || timestamp.is_i64(),
                "{} 的 Timestamp 不是 JSON int: {timestamp}",
                variant.tag
            );
        }
    }

    /// 旧协议兜底体必须与改造前**逐字一致**（`account.rs:769-774`），保证兜底不漂移。
    #[test]
    fn build_legacy_refresh_payload_matches_old_contract() {
        let payload = build_legacy_refresh_payload("ono9krqynydwx5", "rt");
        assert_eq!(
            payload,
            serde_json::json!({
                "ClientID": "ono9krqynydwx5",
                "RefreshToken": "rt",
                "ClientSecret": "-",
                "UserID": "",
            })
        );
    }

    /// ★ 无设备凭证时**只**试旧协议兜底（并且调用方能从错误里看出是「没凭证」）。
    #[test]
    fn refresh_chain_uses_legacy_only_when_credential_missing() {
        let variants = build_refresh_variants(TraeVariant::TraeWork, "cid", "rt", None);
        assert_eq!(variants.len(), 1, "无凭证时不该出现带 Proof 的变体");
        assert_eq!(variants[0].tag, "Refresh/Legacy");
        assert!(!variants[0].with_cloudide_token);
        assert!(variants[0].payload.get("DeviceProof").is_none());
        assert_eq!(
            variants[0].payload.get("ClientSecret").and_then(|v| v.as_str()),
            Some("-")
        );

        // 错误消息必须能指出「没取到设备凭证」（否则用户无从判断是协议还是凭证问题）。
        //
        // ⚠️ 这里**刻意不再锚某个具体词**（原来断言的是 `contains("未找到")`）：
        // 那是**代理断言** —— 它用「文案里出现『未找到』」代理「文案说清了原因是凭证」。
        // 2026-09-24 把文案改成「已检测到【Trae Work】客户端（…），但它**从未启动过** ——
        // 设备凭证是客户端首次启动时才写入的…」之后它就**恒为假**：
        // **不是文案变差了，是锚选错了**（用户报障的原话正是「已经安装了，为什么检查不到
        // 安装的客户端」，旧文案确实没把原因说清）。
        //
        // 现在锚两处**稳定标记**：
        //   ① `kind()` —— 结构化判据，调用方与日志真正依赖的那一个（换文案不会动它）；
        //   ② 文案必须**点明产品线**且**提到「设备凭证」** —— 领域名词，跨改写稳定。
        let credential_error = crate::modules::trae::icube::IcubeError::DataDirMissing;
        let message = credential_error.user_message(TraeVariant::TraeWork);
        assert!(
            message.contains(TraeVariant::TraeWork.display_name()),
            "文案必须点明是哪条产品线: {message}"
        );
        assert!(
            message.contains("设备凭证"),
            "文案必须把原因说到「设备凭证」上（否则用户会以为是网络/协议问题）: {message}"
        );
        assert_eq!(credential_error.kind(), "dataDirMissing");
    }

    /// ★ 服务端明确拒绝 vs 本地/协议级失败必须区分开。
    #[test]
    fn refresh_error_distinguishes_server_rejection_from_transport_failure() {
        let rejected = RefreshExchangeError {
            message: "code=1001: invalid refresh token".into(),
            server_rejected: true,
        };
        let transport = RefreshExchangeError::transport("请求失败: connection reset");
        assert!(rejected.server_rejected);
        assert!(!transport.server_rejected);

        // 异构响应（无 code/message）不得被判为「凭据失效」——
        // 参考 `oauth.rs:859` 记录了旧实现 `unwrap_or(-1)` 正是这么误判的。
        let heterogeneous = RefreshExchangeError::transport("响应中未找到 AccessToken 字段");
        assert!(!heterogeneous.server_rejected);
    }

    /// ★ 「凭据失效」与「稍后重试」必须是两种不同的用户动作。
    #[test]
    fn refresh_jwt_for_maps_rejection_to_relogin_hint() {
        let rejected = RefreshExchangeError {
            message: "code=1001: invalid".into(),
            server_rejected: true,
        };
        let message = refresh_error_message(&rejected);
        assert!(message.contains("凭据已失效"), "{message}");
        assert!(message.contains("重新登录"), "{message}");

        let transient = RefreshExchangeError::transport("请求失败: timeout");
        let message = refresh_error_message(&transient);
        assert!(message.contains("暂时失败"), "{message}");
        assert!(message.contains("稍后重试"), "{message}");
        assert!(!message.contains("凭据已失效"), "{message}");
    }

    /// 刷新后 uid 不一致必须**拒绝写回**（否则会把 A 的凭据写进 B 的记录）。
    #[test]
    fn refresh_jwt_rejects_uid_mismatch_without_writing_back() {
        assert!(ensure_refreshed_uid_matches("1111111111111111", Some("1111111111111111")).is_ok());
        // 解析不出 uid 时放行（上游可能不带 data.id）。
        assert!(ensure_refreshed_uid_matches("1111111111111111", None).is_ok());
        let error = ensure_refreshed_uid_matches("1111111111111111", Some("2222222222222222"))
            .expect_err("uid 不一致必须拒绝");
        assert!(error.contains("已拒绝写回"), "{error}");
        assert!(error.contains("1111111111111111"));
        assert!(error.contains("2222222222222222"));
    }

    /// ★ AuthCode 落库：覆盖 jwt、保留 added_at、`name` 传了才覆盖。
    #[test]
    fn login_with_exchanged_tokens_for_merges_without_clobbering_name() {
        with_temp_home(|| {
            let variant = TraeVariant::TraeWork;
            let uid = "1111111111111111";
            let jwt = make_jwt(uid);

            // 首次落库：新账号。
            let (record, returned) =
                login_with_exchanged_tokens_for(
                    variant,
                    jwt.clone(),
                    Some("rt-1".into()),
                    Some("小明".into()),
                    None,
                )
                .expect("首次落库不应失败");
            assert_eq!(returned, jwt);
            assert_eq!(record.name, "小明");
            assert_eq!(record.refresh_token.as_deref(), Some("rt-1"));
            let added_at = record.added_at.clone().expect("新账号必须有 added_at");
            assert_eq!(entries_for(variant).len(), 1);

            // 再次落库：`name` 不传 ⇒ 保留用户自己的名字；jwt/refresh_token 覆盖。
            let jwt2 = make_jwt(uid);
            let (updated, _) =
                login_with_exchanged_tokens_for(variant, jwt2.clone(), Some("rt-2".into()), None, None)
                    .expect("重复落库不应失败");
            assert_eq!(updated.name, "小明", "name 不该被上游空值冲掉");
            assert_eq!(updated.refresh_token.as_deref(), Some("rt-2"));
            assert_eq!(updated.jwt, jwt2);
            assert_eq!(
                updated.added_at.as_deref(),
                Some(added_at.as_str()),
                "added_at 是「首次添加」，不得被覆盖"
            );
            assert_eq!(entries_for(variant).len(), 1, "不该产生重复账号");

            // refresh_token 传 None ⇒ **保留**旧值（清空会让账号失去自动续期）。
            let (kept, _) =
                login_with_exchanged_tokens_for(variant, make_jwt(uid), None, None, None).unwrap();
            assert_eq!(kept.refresh_token.as_deref(), Some("rt-2"));

            // 另一条产品线**看不到**这条账号（变体隔离）。
            assert!(entries_for(TraeVariant::Global).is_empty());
        });
    }

    /// 无法从 JWT 解析 uid ⇒ 拒绝写库（不能让一条无主凭据进库）。
    #[test]
    fn login_with_exchanged_tokens_for_rejects_unparsable_jwt() {
        with_temp_home(|| {
            let error =
                login_with_exchanged_tokens_for(
                    TraeVariant::TraeWork,
                    "not-a-jwt".into(),
                    None,
                    None,
                    None,
                )
                .expect_err("解析不出 uid 必须拒绝");
            assert!(error.contains("UserID"), "{error}");
            assert!(entries_for(TraeVariant::TraeWork).is_empty());
        });
    }

    /// ★ 上游轮换 `RefreshToken` 时必须**写回**（否则下次刷新用旧值必掉线）。
    ///
    /// 这是「账号过一阵子掉线」的另一半根因的护栏：另一半是 refresh 走的协议过时
    /// （见 `exchange_token_for` 的四步链）。
    #[test]
    fn rotated_refresh_token_is_written_back() {
        with_temp_home(|| {
            let variant = TraeVariant::TraeWork;
            let uid = "3333333333333333";
            login_with_exchanged_tokens_for(variant, make_jwt(uid), Some("rt-old".into()), None, None)
                .expect("造账号不应失败");

            // 上游轮换了 refresh token ⇒ 必须覆盖。
            apply_refreshed_tokens_for(variant, uid, &make_jwt(uid), Some("rt-new".into()))
                .expect("写回不应失败");
            let stored = find_for(variant, uid).expect("账号应还在");
            assert_eq!(
                stored.refresh_token.as_deref(),
                Some("rt-new"),
                "丢弃轮换后的 refresh_token ⇒ 下次刷新必掉线"
            );

            // 上游没轮换（None）⇒ **保留**旧值，不得清空。
            apply_refreshed_tokens_for(variant, uid, &make_jwt(uid), None).expect("写回不应失败");
            let stored = find_for(variant, uid).expect("账号应还在");
            assert_eq!(
                stored.refresh_token.as_deref(),
                Some("rt-new"),
                "上游未轮换时清空 refresh_token 会让账号立刻失去自动续期"
            );

            // 变体隔离：另一条产品线不得出现这条账号。
            assert!(find_for(TraeVariant::Global, uid).is_none());
        });
    }

    /// 取 `source` 里每个 `callee(` 调用点的**顶层实参**（已 `trim`）。
    ///
    /// 只做括号配平（`()` / `[]` / `{}`），并跳过**注释行**（行首非空白以 `//` 开头）。
    /// 不处理字符串字面量里的括号 —— 本项目在调用点不写这种实参；万一将来写了，
    /// 本函数会**多切**，断言随之**报红**而不是静默放过（宁可吵，不可哑）。
    fn call_arguments(source: &str, callee: &str) -> Vec<Vec<String>> {
        let mut found = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = source[from..].find(callee) {
            let at = from + rel;
            from = at + callee.len();

            // 标识符边界：`xxx_exchange_token_for` 不算。
            if let Some(prev) = source[..at].chars().next_back() {
                if prev.is_alphanumeric() || prev == '_' {
                    continue;
                }
            }
            // 定义处（`… fn exchange_token_for(`）不算 —— 那是形参表，不是调用。
            if source[..at].trim_end().ends_with("fn") {
                continue;
            }
            // 注释行不算（docblock 里会引用签名，例如 `…(variant, refresh_token, device_id)`）。
            let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
            if source[line_start..at].trim_start().starts_with("//") {
                continue;
            }
            // 后面必须紧跟 `(`（允许空白）。
            let tail = &source[at + callee.len()..];
            let trimmed = tail.trim_start();
            if !trimmed.starts_with('(') {
                continue;
            }
            let open = at + callee.len() + (tail.len() - trimmed.len());

            let mut depth = 0i32;
            let mut args: Vec<String> = Vec::new();
            let mut current = String::new();
            for ch in source[open..].chars() {
                match ch {
                    '(' | '[' | '{' => {
                        depth += 1;
                        if depth > 1 {
                            current.push(ch);
                        }
                    }
                    ')' | ']' | '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        current.push(ch);
                    }
                    ',' if depth == 1 => {
                        args.push(current.trim().to_string());
                        current.clear();
                    }
                    _ => current.push(ch),
                }
            }
            args.push(current.trim().to_string());
            found.push(args);
        }
        found
    }

    /// ★【T13-4 / A】`exchange_token_for` 的第三个实参**不得是硬编码 `None`**。
    ///
    /// ## 为什么需要这条：签名挡不住「主动放弃绑定」
    ///
    /// `exchange_token_for(variant, refresh_token, device_id)` 的签名只保证
    /// 「**传了**第三个实参」，不保证「传的是**从绑定读出来的那个值**」。
    /// 有人把调用点改成 `…, None)` ⇒ 编译过、`bound_device_id` 的用例照样绿、
    /// 回落用例照样绿 ⇒ **绑定被静默忽略 → 退回猜活跃目录 → 上游 20403/20405**，
    /// 而**没有任何测试会红**。这条断言就是补这个洞。
    ///
    /// ## 为什么是结构断言
    ///
    /// `refresh_jwt_for` 本体要发网络请求，跑不起来（同
    /// `switch_passes_the_same_dir_to_restore_and_verification` 的理由）。
    ///
    /// ## 与 R1-4 那条的区别：**锚点不是名字**
    ///
    /// R1-4 锚定的是**一个业务函数体**（`switch_account`）与**一个局部变量名**
    /// （`restore_dir`）—— 改名即失配。本条锚定的是**被调 API 的调用形态**：
    /// 扫本文件里**所有** `exchange_token_for(…)` 调用点，逐个看第三个实参。
    /// 于是「把这段逻辑挪进别的函数」「换个变量名」都不影响它；
    /// 只有「改被调函数名 / 改参数个数」会让它失配 —— 而那两件事**必须**有人来看这里，
    /// 所以「一个调用点都没找到」时本断言**报红**，绝不静默通过。
    ///
    /// ## 已知边界（耦合点登记）
    ///
    /// - 只扫 `account.rs`：`oauth.rs` 的兼容路径**刻意**传 `None`（那一刻账号绑定
    ///   尚未落库，已裁定接受），不在范围内；
    /// - 认的是**字面量** `None`：写成 `Option::<&str>::None`、或先 `let none = None;`
    ///   再传变量，本断言抓不到 —— 但那已不是「顺手写个 `None`」，需要刻意绕；
    /// - 依赖「注释行以 `//` 开头」来跳过 docblock 里的签名引用（本文件风格如此）。
    ///
    /// ## 反向验证
    ///
    /// 把调用点的第三实参改成字面量 `None` ⇒ 本用例必须报红。
    #[test]
    fn refresh_never_passes_a_hardcoded_none_device_id() {
        let calls = call_arguments(include_str!("account.rs"), "exchange_token_for");
        assert!(
            !calls.is_empty(),
            "一个 `exchange_token_for` 调用点都没找到 —— 要么被改名/加参数，要么调用点被搬走；\
             两种情况都必须有人来复核本断言，故此处报红"
        );
        for args in &calls {
            assert_eq!(
                args.len(),
                3,
                "`exchange_token_for` 应当是 3 参调用，实际实参：{args:?}"
            );
            assert_ne!(
                args[2], "None",
                "第三个实参不得是硬编码 `None`：那会让账号绑定被静默忽略\
                 （退回猜活跃目录 ⇒ 上游 20403/20405），且现有测试全都不会红"
            );
        }
    }

    /// ★ refresh 的 URL 必须由**该变体的 `icube_base`** 拼成，与回调 `host` 无关。
    ///
    /// refresh 是**离线触发**（没有浏览器回调），根本没有 host 可传；
    /// `exchange_token_for(variant, refresh_token, device_id)` 的签名里也确实没有 host 形参
    /// （编译期护栏：多一个 `host` 参数本用例就编不过）。
    #[test]
    fn refresh_url_is_built_from_variant_icube_base() {
        let credential = test_credential();
        for variant in TraeVariant::all() {
            let base = crate::modules::trae::endpoints_for(variant).icube_base;
            let variants = build_refresh_variants(variant, "cid", "rt", Some(&credential));
            assert_eq!(variants.len(), 4, "有凭证时必须是四步链");
            for plan in &variants {
                assert!(
                    plan.url.starts_with(&base),
                    "变体 {variant:?} 的 refresh URL 不是由 icube_base（{base}）拼成的: {}",
                    plan.url
                );
                assert!(
                    !plan.url.contains("127.0.0.1"),
                    "refresh URL 里出现了回调 host，说明复用了浏览器回调地址: {}",
                    plan.url
                );
            }
        }
    }

    /// 起一个本地 mock HTTP 上游（只服务一次），返回 `(端口, 捕获的原始请求)`。
    ///
    /// 与 `oauth.rs` 测试里的同名助手**刻意各写一份**：AuthCode 与 refresh 是两条
    /// 独立的请求路径，各自都要有一条「空串头」护栏；共用一个助手会让「某一边忘了
    /// 加头」表现成「两边一起绿」——那就不再是护栏了。
    async fn mock_upstream_once(response_body: String) -> (u16, Arc<Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("mock 上游应能绑定端口");
        let port = listener.local_addr().unwrap().port();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let sink = captured.clone();

        tokio::spawn(async move {
            let Ok((mut stream, _peer)) = listener.accept().await else {
                return;
            };
            // 读到「请求头 + Content-Length 指定的正文」为止，避免把 body 截断。
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
            sink.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf).into_owned());
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
        });

        (port, captured)
    }

    /// ★ refresh 请求头必须带 `x-cloudide-token: ""`（空串）—— 20403 的直接护栏。
    ///
    /// 与 `oauth.rs` 的 `exchange_request_sends_empty_cloudide_token_header` 构成
    /// **两条路径各自**的护栏：空串头是两条路径共同的硬要求，缺失报 20403、
    /// 带旧 token 报 20405。
    #[tokio::test]
    async fn refresh_request_carries_empty_cloudide_token_header() {
        let body = json!({"Result": {"AccessToken": "at-1", "RefreshToken": "rt-2"}}).to_string();
        let (port, captured) = mock_upstream_once(body).await;

        let credential = test_credential();
        let plan = RefreshVariant {
            tag: "Refresh/Proof/P1363".into(),
            url: format!("http://127.0.0.1:{port}{TRAE_EXCHANGE_TOKEN_PATH}"),
            sign_path: TRAE_EXCHANGE_TOKEN_PATH,
            payload: build_refresh_payload(
                "cid",
                "rt-1",
                &credential.device_id,
                icube::device_proof(
                    &credential,
                    TRAE_EXCHANGE_TOKEN_PATH,
                    "cid",
                    "rt-1",
                    ProofSigFormat::P1363,
                )
                .expect("合成私钥应能签名"),
            ),
            with_cloudide_token: true,
        };

        let (access, refresh, _) = try_refresh_variant(&plan, Some(&credential.device_id))
            .await
            .expect("mock 上游应返回成功");
        assert_eq!(access, "at-1");
        assert_eq!(refresh.as_deref(), Some("rt-2"));

        let requests = captured.lock().unwrap().clone();
        let request = &requests[0];
        let header = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("x-cloudide-token:"))
            .expect("refresh 必须发送 x-cloudide-token 头（缺失报 20403）");
        assert_eq!(
            header.split_once(':').unwrap().1.trim(),
            "",
            "x-cloudide-token 必须为空串: {header:?}"
        );
        // 设备头与 DeviceProof 的 DeviceID 同源。
        let device = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("x-device-id:"))
            .expect("有设备凭证时必须发送 x-device-id");
        assert_eq!(
            device.split_once(':').unwrap().1.trim(),
            credential.device_id
        );
        assert!(request.contains("\"DeviceProof\""), "{request}");
        assert!(request.contains("\"PlatformCode\":\"IDE_PC\""), "{request}");
        assert!(
            request.contains(&format!("\"DeviceID\":\"{}\"", credential.device_id)),
            "{request}"
        );
    }

    // -----------------------------------------------------------------------
    // T13-4：账号 ↔ 设备身份绑定（G-b）
    //
    // 绑定键是 uid ⇒ 「账号被删 / uid 变了」时必须清理（I-2 / I-3）；
    // 而 `import_accounts_for` 一旦字面重建容器就会清空全部绑定（I-1，护栏在
    // `export_import.rs` 的 `import_accounts_keeps_device_bindings`）。
    // -----------------------------------------------------------------------

    /// 造一份 Trae Work 的候选目录 fixture，并给**指定下标**的候选挂上 `device_id`。
    ///
    /// 活跃度由网格构造器用 `File::set_modified` **显式钉死**，不依赖写入顺序 ——
    /// 否则 Windows 约 15.6ms 的时间粒度会让「谁更活跃」变成掷骰子（P0-2 的成因）。
    #[cfg(windows)]
    fn work_grid_with_devices(
        env: &crate::modules::trae::test_support::TempEnv,
        cells: &[(bool, bool, bool)],
        devices: &[(usize, &str)],
    ) -> crate::modules::trae::icube::test_support::SelectionGrid {
        use crate::modules::trae::icube::test_support as fixtures;
        let grid = fixtures::write_selection_grid(
            &env.appdata(),
            TraeVariant::TraeWork,
            cells,
            chrono::Utc::now().timestamp() + 3600,
        );
        for (index, device_id) in devices {
            fixtures::attach_device_entry(&env.appdata(), grid.cells[*index].name, device_id);
        }
        grid
    }

    /// 造一条最小可用的账号记录。
    fn raw(uid: &str, name: &str) -> RawAccount {
        RawAccount {
            name: name.to_string(),
            user_id: Some(uid.to_string()),
            jwt: make_jwt(uid),
            refresh_token: None,
            added_at: None,
            updated_at: None,
        }
    }

    /// ★【T13-4 ①】导入必须把**来源目录那台设备**记成账号绑定。
    ///
    /// fixture 是用户机器形态：首位**活跃但无登录态**、次位**不活跃但有登录态 + 设备身份**。
    /// ⇒ 「绑定取自活跃目录」的退化实现会去首位找设备身份（那里没有）⇒ 绑定落空。
    #[cfg(windows)]
    #[test]
    fn import_local_for_writes_the_device_binding() {
        let env = crate::modules::trae::test_support::TempEnv::empty();
        let bound = "22929298067000007";
        let grid = work_grid_with_devices(
            &env,
            &[(true, false, true), (true, true, false)],
            &[(1, bound)],
        );
        assert!(
            grid.cells[0].active && !grid.cells[0].logged_in,
            "前置：活跃目录没有登录态"
        );
        assert!(
            !grid.cells[1].active && grid.cells[1].logged_in,
            "前置：登录态在次位候选"
        );

        let record = import_local_for(TraeVariant::TraeWork).expect("导入必须成功");
        let uid = resolve_user_id(&record);
        assert_eq!(uid, grid.cells[1].user_id, "必须导入次位（装着登录态）那个账号");

        let stored = load_accounts_for(TraeVariant::TraeWork);
        assert_eq!(
            stored.device_bindings.get(&uid).map(String::as_str),
            Some(bound),
            "绑定必须是**来源目录**那台设备（而不是活跃目录的设备）"
        );
    }

    /// 取不到设备身份时**不得**把既有绑定清空 —— 宁可留旧值（下次续期仍可能对），
    /// 也不要用一个空值把本来正确的绑定抹掉（那会让续期静默退回「猜活跃目录」）。
    #[cfg(windows)]
    #[test]
    fn import_local_for_keeps_the_existing_binding_when_device_identity_is_missing() {
        let env = crate::modules::trae::test_support::TempEnv::empty();
        // 有登录态，但**一条 `icube-dc` 都没有**（客户端从未注册设备身份）。
        let grid = work_grid_with_devices(
            &env,
            &[(true, true, true), (false, false, false)],
            &[],
        );
        let uid = grid.cells[0].user_id.clone();

        let mut file = load_accounts_for(TraeVariant::TraeWork);
        file.device_bindings.insert(uid.clone(), "dev-old".into());
        save_accounts_for(TraeVariant::TraeWork, &file).expect("造绑定");

        import_local_for(TraeVariant::TraeWork).expect("有登录态时导入必须成功");

        assert_eq!(
            bound_device_id(TraeVariant::TraeWork, &uid).as_deref(),
            Some("dev-old"),
            "取不到设备身份时不得清空既有绑定"
        );
    }

    /// ★【T13-4 ④】`refresh_jwt_for` 的设备身份来自**账号绑定**（唯一取值点 [`bound_device_id`]）。
    ///
    /// 「读到的值确实被传下去」由签名保证：`exchange_token_for` 必须收第三个实参，
    /// 少传一个就编不过 —— 所以这里钉的是**值的来源**，不是传参动作本身。
    #[test]
    fn refresh_reads_the_account_device_binding() {
        with_temp_home(|| {
            let variant = TraeVariant::TraeWork;
            assert_eq!(bound_device_id(variant, "u1"), None, "无绑定时必须是 None");

            let mut file = load_accounts_for(variant);
            file.device_bindings.insert("u1".into(), "dev-1".into());
            save_accounts_for(variant, &file).expect("写绑定");

            assert_eq!(bound_device_id(variant, "u1").as_deref(), Some("dev-1"));
            // 变体分家：另一条线读不到这条绑定。
            assert_eq!(
                bound_device_id(TraeVariant::Global, "u1"),
                None,
                "绑定必须按变体分家"
            );
        });
    }

    /// ★【T13-4 ②】绑定生效时取的是**绑定那台设备**的凭证，且**与活跃度无关**：
    /// 活跃目录翻转后仍取同一台。
    ///
    /// 判据用 `source_app`（凭证来自哪个候选目录）而不是私钥字节 ——
    /// 合成 fixture 里各目录的设备信封共用同一份测试私钥，比不出差别。
    #[cfg(windows)]
    #[test]
    fn refresh_credential_follows_the_binding_across_an_activity_flip() {
        use crate::modules::trae::icube::test_support as fixtures;
        let env = crate::modules::trae::test_support::TempEnv::empty();
        let variant = TraeVariant::TraeWork;
        // 两个候选都注册了设备身份，各给不同 deviceId；首位活跃。
        let cells =
            fixtures::write_device_entries_grid(&env.appdata(), variant, &[(true, true), (true, false)]);
        let (bound_id, bound_name, bound_dir) = &cells[1];
        let (other_id, other_name, other_dir) = &cells[0];
        assert_ne!(bound_id, other_id, "前置：两台设备必须是不同的 id");

        let credential =
            resolve_refresh_credential(variant, Some(bound_id)).expect("绑定的设备必须能找到");
        assert_eq!(credential.device_id.as_str(), bound_id.as_str());
        assert_eq!(
            credential.source_app.as_str(),
            *bound_name,
            "必须取**绑定**那台设备的私钥"
        );

        // 翻转活跃度：把**绑定那台**（次位）钉旧、把首位钉新。
        let storage = |dir: &std::path::Path| dir.join("User").join("globalStorage").join("storage.json");
        fixtures::pin_activity(&storage(other_dir), 0);
        fixtures::pin_activity(&storage(bound_dir), 24);

        // 先证明翻转**真的生效**，否则下面那条断言是空的（「碰巧一致」= 假绿）。
        let fallback =
            resolve_refresh_credential(variant, None).expect("无绑定回落时也应能取到凭证");
        assert_eq!(
            fallback.source_app.as_str(),
            *other_name,
            "前置：活跃度翻转未生效，下一条断言就没有意义"
        );

        let still = resolve_refresh_credential(variant, Some(bound_id)).expect("仍应能找到");
        assert_eq!(
            still.source_app.as_str(),
            *bound_name,
            "活跃目录翻转后，绑定仍必须指向**来源设备**"
        );
        assert_ne!(still.source_app, fallback.source_app);
    }

    /// ★【T13-4 ③ / ⑤】无绑定（旧账号）⇒ 回落「当前目录的那一条」，**且必须留痕**。
    ///
    /// 留痕不是装饰：这条路径拿到的私钥可能与账号当初用的那台不一致，
    /// 出问题时（20403/20405）必须能从日志里看出「这次是猜的」。
    #[cfg(windows)]
    #[test]
    fn refresh_credential_falls_back_and_leaves_a_trace_when_binding_is_absent() {
        use crate::modules::trae::icube::test_support as fixtures;
        let env = crate::modules::trae::test_support::TempEnv::empty();
        let variant = TraeVariant::TraeWork;
        let cells =
            fixtures::write_device_entries_grid(&env.appdata(), variant, &[(true, true), (true, false)]);
        let (_, active_name, _) = &cells[0];

        let credential = resolve_refresh_credential(variant, None).expect("有设备身份，应能取到");
        assert_eq!(
            credential.source_app.as_str(),
            *active_name,
            "无绑定应回落**当前目录**那一条"
        );

        let log = std::fs::read_to_string(paths::checkin_log_file_for(variant))
            .expect("回落必须写一条留痕日志");
        assert!(log.contains("回落"), "日志必须写明「回落」：{log}");
    }

    /// ★【T13-4 ⑦ / I-2】删账号必须一并删绑定，且**另一 uid 的绑定不受影响**。
    #[test]
    fn delete_account_removes_device_binding() {
        with_temp_home(|| {
            let variant = TraeVariant::TraeWork;
            let mut file = load_accounts_for(variant);
            file.accounts.push(raw("u1", "一号"));
            file.accounts.push(raw("u2", "二号"));
            file.device_bindings.insert("u1".into(), "dev-1".into());
            file.device_bindings.insert("u2".into(), "dev-2".into());
            save_accounts_for(variant, &file).expect("造绑定");

            delete_for(variant, "u1", false).expect("删除应成功");

            let after = load_accounts_for(variant);
            assert!(
                !after.device_bindings.contains_key("u1"),
                "被删账号的绑定必须一并删除，否则 uid 被复用时拿旧设备的私钥去签名"
            );
            assert_eq!(
                after.device_bindings.get("u2").map(String::as_str),
                Some("dev-2"),
                "另一 uid 的绑定不得受影响"
            );
        });
    }

    /// ★【T13-4 ⑧ / I-3】换 JWT 导致 uid 变化时，旧 uid 的绑定被删除、新 uid **无绑定**；
    /// uid 未变时绑定保留。
    #[test]
    fn update_for_clears_binding_when_uid_changes() {
        with_temp_home(|| {
            let variant = TraeVariant::TraeWork;
            let mut file = load_accounts_for(variant);
            file.accounts.push(raw("old-uid", "旧号"));
            file.device_bindings.insert("old-uid".into(), "dev-old".into());
            save_accounts_for(variant, &file).expect("造绑定");

            // ① uid 未变（只改名）⇒ 绑定必须保留。
            update_for(variant, "old-uid", Some("改名".into()), None).expect("改名应成功");
            assert_eq!(
                bound_device_id(variant, "old-uid").as_deref(),
                Some("dev-old"),
                "uid 未变时绑定不得被清掉"
            );

            // ② 换 JWT ⇒ uid 变化 ⇒ 旧绑定删除、新 uid 不继承。
            update_for(variant, "old-uid", None, Some(make_jwt("new-uid"))).expect("换 JWT 应成功");
            assert_eq!(
                bound_device_id(variant, "old-uid"),
                None,
                "旧 uid 的绑定属于旧账号，必须删除"
            );
            assert_eq!(
                bound_device_id(variant, "new-uid"),
                None,
                "旧设备的私钥对新账号本就不适用，不得搬过去"
            );
            assert!(find_for(variant, "new-uid").is_some(), "账号应已改挂到新 uid");
        });
    }

    /// ★【T13-4 ⑨】同一 uid 在两条产品线各绑不同设备，**互不污染**。
    #[test]
    fn same_uid_binds_independently_per_variant() {
        with_temp_home(|| {
            let mut work = load_accounts_for(TraeVariant::TraeWork);
            work.device_bindings.insert("same-uid".into(), "dev-work".into());
            save_accounts_for(TraeVariant::TraeWork, &work).expect("写 TraeWork 绑定");

            let mut cn = load_accounts_for(TraeVariant::Global);
            cn.device_bindings.insert("same-uid".into(), "dev-cn".into());
            save_accounts_for(TraeVariant::Global, &cn).expect("写 Trae 绑定");

            assert_eq!(
                bound_device_id(TraeVariant::TraeWork, "same-uid").as_deref(),
                Some("dev-work")
            );
            assert_eq!(
                bound_device_id(TraeVariant::Global, "same-uid").as_deref(),
                Some("dev-cn")
            );

            // 只清一条线，另一条不得受影响。
            let mut work = load_accounts_for(TraeVariant::TraeWork);
            work.device_bindings.remove("same-uid");
            save_accounts_for(TraeVariant::TraeWork, &work).expect("清 TraeWork 绑定");

            assert_eq!(bound_device_id(TraeVariant::TraeWork, "same-uid"), None);
            assert_eq!(
                bound_device_id(TraeVariant::Global, "same-uid").as_deref(),
                Some("dev-cn"),
                "另一条产品线的绑定不得被污染"
            );
        });
    }
}
