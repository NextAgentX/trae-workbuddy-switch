//! 常量、路径与通用工具函数（对照 server.py 常量区与工具区）

use chrono::{Local, TimeZone};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

pub const WORKBUDDY_API_ENDPOINT: &str = "https://www.codebuddy.cn";
pub const WORKBUDDY_API_PREFIX: &str = "/v2/plugin";
pub const WORKBUDDY_PLATFORM: &str = "workbuddy";

pub const OAUTH_TIMEOUT_SECONDS: i64 = 600;

pub const CHECKIN_API_PREFIX: &str = "/v2/billing/meter";
pub const CHECKIN_LOG_KEEP_DAYS: i64 = 30;
pub const CHECKIN_LOG_MAX_RECORDS: usize = 500;

/// 派猫猫旅行接口前缀（成长中心，非 /v2/plugin 体系，直接挂在 API 域名下）。
pub const TRAVEL_API_PREFIX: &str = "/activity/growth/buddy/travel";

static CHECKIN_LOG_WRITE_LOCK: Mutex<()> = Mutex::new(());
static TRAVEL_CACHE_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Serialize travel-cache read-modify-write across depart and claim cycles.
pub fn with_travel_cache_lock<T>(f: impl FnOnce() -> T) -> T {
    let _guard = TRAVEL_CACHE_WRITE_LOCK.lock().unwrap();
    f()
}

pub const ROTATE_LOG_MAX_RECORDS: usize = 200;

/// 官网套餐页桌面 Chrome UA（plans-usage 捕获）。
pub const DEFAULT_HTTP_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36";

// ---------------------------------------------------------------------------
// 路径
// ---------------------------------------------------------------------------

/// 覆盖用户主目录的环境变量名（仅用于可移植部署 / 测试隔离）。
pub const BUDDY_SWITCH_HOME_ENV: &str = "BUDDY_SWITCH_HOME";

// 这里曾有两个「`wb-switch` 时期」的兼容项，已于 2026-09-23 删除：
//   - 环境变量 `WB_SWITCH_HOME`
//   - 数据目录回落 `~/.wb-switch`
//
// **不要重新加回**：`~/.wb-switch` 是另一个独立项目（`changexbc/workbuddy-switch`）的
// **固定数据目录**（对方 `wb-switch-core` 里硬编码 `home_dir().join(".wb-switch")`，
// 无环境变量、无回落）。两边有 12 个同名文件（`accounts.json`、`workbuddy_exe.json`、
// `credit_usage_snapshots.json`、`official_usage_cache.json`、`backups/` 等），而
// `workbuddy_exe.json` 的 schema 互不兼容（本项目的 `{"exe":...}` 对方读不出，反之亦然）。
// 一旦回落，本项目就会接管对方的活数据目录，并用自己的 schema 覆盖对方文件。
// 本项目与对方各自独立、不得相互替换，因此只认 [`store_dir`] 里的 `~/.buddy-switch`。

/// 用户主目录。
///
/// **默认行为（未设置环境变量）与改造前完全一致**：返回 `dirs::home_dir()`，
/// 解析失败时回退 `.`。若设置了**合法的** [`BUDDY_SWITCH_HOME_ENV`]
/// （`BUDDY_SWITCH_HOME`），则返回该路径——用于可移植部署，以及在集成测试中把
/// `~/.buddy-switch/` 与认证文件重定向到隔离的临时目录，避免触碰真实用户数据。
///
/// **护栏**：覆盖值必须是一个**已存在的绝对目录**，否则会被忽略并（本进程内
/// 一次性）打印 `stderr` 警告，最终回落真实 home。详见 [`home_dir_override`]。
///
/// 注意：环境变量是**进程级全局**状态，多个测试并行修改会相互竞争；凡是需要
/// 设置该变量的测试必须用互斥锁串行化，并在结束时恢复（见
/// `tests/switch_region_binding.rs`）。
pub fn home_dir() -> PathBuf {
    if let Some(overridden) = home_dir_override() {
        return overridden;
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// 覆盖值被拒绝的原因（用于一次性警告文案与单测断言）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverrideReject {
    /// 不是绝对路径。
    NotAbsolute,
    /// 不是已存在的目录（不存在，或是普通文件）。
    NotDir,
}

impl OverrideReject {
    /// 面向用户的简短原因描述。
    fn describe(self) -> &'static str {
        match self {
            OverrideReject::NotAbsolute => "必须是绝对路径",
            OverrideReject::NotDir => "必须是一个已存在的目录",
        }
    }
}

/// 校验一个**非空**的覆盖值，成功时返回可用的绝对目录。
///
/// 收紧护栏的原因：覆盖值一旦指向相对路径或尚未创建的目录，`~/.buddy-switch/`
/// 与认证文件就会落到进程当前工作目录、甚至一个「半初始化」的路径下，导致账号 /
/// 网关 Key 在预期之外的位置被读写。这里要求覆盖值**必须是已存在的绝对目录**。
fn validate_home_override(raw: &str) -> Result<PathBuf, OverrideReject> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(OverrideReject::NotAbsolute);
    }
    if !path.is_dir() {
        return Err(OverrideReject::NotDir);
    }
    Ok(path)
}

/// 保证同一进程内「[`BUDDY_SWITCH_HOME_ENV`] 被忽略」的警告**只打印一次**：否则每个
/// 落到 `home_dir()` 的调用都会重复刷屏。
static HOME_OVERRIDE_WARNED: Once = Once::new();

/// 读取 [`BUDDY_SWITCH_HOME_ENV`] 覆盖值；不满足护栏时忽略并（一次性）警告。
///
/// 返回语义：
/// - 未设置 / 仅含空白 → `None`（**与改造前完全一致**，且**不**产生任何副作用）；
/// - 非空且是「已存在的绝对目录」→ `Some(path)`；
/// - 其他（相对路径 / 不存在 / 是普通文件）→ `None`，并在本进程内**最多**打印
///   一次 `stderr` 警告后回落真实 home。
fn home_dir_override() -> Option<PathBuf> {
    let value = std::env::var(BUDDY_SWITCH_HOME_ENV).ok()?;
    if value.trim().is_empty() {
        // 未设置或空白：保持改造前的回落行为，绝不产生任何副作用。
        return None;
    }
    match validate_home_override(&value) {
        Ok(path) => Some(path),
        Err(reject) => {
            HOME_OVERRIDE_WARNED.call_once(|| {
                eprintln!(
                    "[buddy-switch] 环境变量 {BUDDY_SWITCH_HOME_ENV}={value:?} 已忽略：{}；\
回落到真实用户主目录。",
                    reject.describe()
                );
            });
            None
        }
    }
}

// ---------------------------------------------------------------------------
// 测试专用：进程级 home 覆盖的串行化
// ---------------------------------------------------------------------------

/// 单元测试共用的「进程级 home 覆盖」互斥锁。
///
/// ## 为什么必须有
///
/// [`BUDDY_SWITCH_HOME_ENV`] 是**进程级全局状态**，而 lib 单元测试**全在同一个进程里
/// 并行跑**（集成测试每个文件各自起进程，不受此影响 —— 那侧用 `tests/*.rs` 里各自的
/// `ENV_LOCK`）。任何一处 `set_var` 都可能被并发读取的测试观察到，症状是
/// **自相矛盾的断言**：例如 `paths.rs` 里
/// `assert_eq!(credits_history_file().parent(), Some(trae_dir().as_path()))` 会在
/// 两次调用之间被改掉 home，于是左边是临时目录、右边是真实目录，报出一个
/// 「同一个函数族内部不一致」的假失败。
///
/// 因此：**凡是要改它、或断言结果依赖它的测试，都必须先取这把锁**。
/// 改值请用 [`HomeOverrideGuard`]，它会在 drop 时恢复原值（含 panic 路径），
/// 避免把临时 home 泄漏给后续测试。
#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 把 home 覆盖指向某个目录，并在 drop 时**恢复原值**。
///
/// 只改不还原是这里最容易犯的错：`set_var` 到临时目录、结束时只删目录不删变量，
/// 于是变量继续指向一个**已不存在的目录**，后续所有测试都吃一次
/// 「已忽略：必须是一个已存在的目录」的警告并回落真实 home；
/// 若测试 panic 导致目录没删掉，后续测试还会**静默**地把数据写进临时目录。
#[cfg(test)]
pub(crate) struct HomeOverrideGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous_new: Option<std::ffi::OsString>,
}

#[cfg(test)]
impl HomeOverrideGuard {
    /// 取锁并把 [`BUDDY_SWITCH_HOME_ENV`] 指向 `dir`（调用方需保证 `dir` 已存在，
    /// 否则 [`validate_home_override`] 会忽略它）。
    pub(crate) fn set(dir: &std::path::Path) -> Self {
        let lock = env_lock();
        let previous_new = std::env::var_os(BUDDY_SWITCH_HOME_ENV);
        std::env::set_var(BUDDY_SWITCH_HOME_ENV, dir);
        Self {
            _lock: lock,
            previous_new,
        }
    }
}

#[cfg(test)]
impl Drop for HomeOverrideGuard {
    fn drop(&mut self) {
        match self.previous_new.take() {
            Some(value) => std::env::set_var(BUDDY_SWITCH_HOME_ENV, value),
            None => std::env::remove_var(BUDDY_SWITCH_HOME_ENV),
        }
    }
}

/// 本项目的数据目录：**恒定** `~/.buddy-switch`。
///
/// 这里**刻意不做任何回落**（包括曾经的 `~/.wb-switch`）：那个目录属于另一个独立项目
/// `changexbc/workbuddy-switch`，对方在其 `wb-switch-core` 里硬编码
/// `home_dir().join(".wb-switch")`。两边有 12 个同名文件，且 `workbuddy_exe.json`
/// 的 schema 互不兼容 —— 一旦回落，本项目会接管对方的活数据目录并覆盖对方文件。
/// 本项目与对方各自独立、不得相互替换，故只认 `~/.buddy-switch`。
pub fn store_dir() -> PathBuf {
    home_dir().join(".buddy-switch")
}

pub fn accounts_file() -> PathBuf {
    store_dir().join("accounts.json")
}

// ---------------------------------------------------------------------------
// region 化路径派生（对照设计 A-3.1）
//
// 复用 `region` 模块的实现，避免两处路径逻辑漂移。CN 的 `accounts_file_for`
// 与上面的 `accounts_file()` 返回同一路径（`accounts.json`）。
// ---------------------------------------------------------------------------

pub use crate::modules::region::{
    accounts_file_for, catalog_cache_file, gateway_config_file, gateway_keys_file,
};

/// 目标 region 的 billing / 官网基址（CN = `https://www.codebuddy.cn`，
/// Global = `https://www.workbuddy.ai`）。
pub fn api_endpoint_for(region: crate::modules::region::Region) -> &'static str {
    crate::modules::region::region_spec(region).billing_base
}


pub fn backup_dir() -> PathBuf {
    store_dir().join("backups")
}

pub fn checkin_config_file() -> PathBuf {
    store_dir().join("auto_checkin_config.json")
}

pub fn checkin_logs_file() -> PathBuf {
    store_dir().join("auto_checkin_logs.json")
}

pub fn travel_config_file() -> PathBuf {
    store_dir().join("auto_travel_config.json")
}

pub fn travel_cache_file() -> PathBuf {
    store_dir().join("travel_cache.json")
}

/// 领取类写操作的「当日已处理」闸（活跃地图的礼包 / 补偿 / 兑换 / 抽奖）。
///
/// 落盘而非仅进程内标记：进程内标记重启即清零，会导致**重启后重跑当日领取**，
/// 对上游产生多余的往返与日志噪声（上游虽有幂等兜底，但不该依赖它兜底）。
pub fn reward_gate_file() -> PathBuf {
    store_dir().join("reward_gate.json")
}

pub fn credit_usage_snapshots_file() -> PathBuf {
    store_dir().join("credit_usage_snapshots.json")
}

pub fn official_usage_cache_file() -> PathBuf {
    store_dir().join("official_usage_cache.json")
}

pub fn auto_rotate_config_file() -> PathBuf {
    store_dir().join("auto_rotate_config.json")
}

pub fn auto_rotate_logs_file() -> PathBuf {
    store_dir().join("auto_rotate_logs.json")
}

/// 账号切换与账号列表展示的配置（全局单份，无需 region）。
///
/// 两项都服务于**频繁切号**这一个场景，因此共用一个文件、一套命令：
/// 拆成两份就要多一套命令与六处登记点，收益只是形式上的整齐。
pub fn switch_config_file() -> PathBuf {
    store_dir().join("switch_config.json")
}

pub fn workbuddy_exe_cache_file() -> PathBuf {
    store_dir().join("workbuddy_exe.json")
}

fn parse_workbuddy_exe_cache_json(text: &str) -> Option<PathBuf> {
    let v: Value = serde_json::from_str(text).ok()?;
    let exe = v.get("exe")?.as_str()?.trim();
    if exe.is_empty() {
        None
    } else {
        Some(PathBuf::from(exe))
    }
}

/// 读取上次成功解析到的 WorkBuddy.exe；损坏或空文件视为无缓存。
pub fn load_workbuddy_exe_cache() -> Option<PathBuf> {
    let f = workbuddy_exe_cache_file();
    if !f.exists() {
        return None;
    }
    let text = std::fs::read_to_string(&f).ok()?;
    parse_workbuddy_exe_cache_json(&text)
}

/// 记住已存在的 WorkBuddy.exe，供下次未运行时启动。
pub fn save_workbuddy_exe_cache(exe: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(store_dir())?;
    let content =
        serde_json::to_string_pretty(&json!({ "exe": exe.to_string_lossy() })).unwrap_or_default();
    atomic_write(&workbuddy_exe_cache_file(), &content)
}

pub fn clear_workbuddy_exe_cache() {
    let _ = std::fs::remove_file(workbuddy_exe_cache_file());
}

/// region 化的 App/可执行文件路径缓存文件。
///
/// CN 沿用 `workbuddy_exe.json`（**不变**），Global 用 `workbuddy_exe.global.json`：
/// 两版 App 路径互不污染（否则 CN 探测结果会覆盖国际版启动路径）。
pub fn workbuddy_exe_cache_file_for(region: crate::modules::region::Region) -> PathBuf {
    match region {
        crate::modules::region::Region::Cn => workbuddy_exe_cache_file(),
        crate::modules::region::Region::Global => store_dir().join("workbuddy_exe.global.json"),
    }
}

/// 读取该 region 上次成功解析到的 App 路径；损坏或空文件视为无缓存。
pub fn load_workbuddy_exe_cache_for(region: crate::modules::region::Region) -> Option<PathBuf> {
    let f = workbuddy_exe_cache_file_for(region);
    if !f.exists() {
        return None;
    }
    let text = std::fs::read_to_string(&f).ok()?;
    parse_workbuddy_exe_cache_json(&text)
}

/// 记住该 region 已存在的 App 路径，供下次未运行时启动。
pub fn save_workbuddy_exe_cache_for(
    region: crate::modules::region::Region,
    exe: &Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(store_dir())?;
    let content =
        serde_json::to_string_pretty(&json!({ "exe": exe.to_string_lossy() })).unwrap_or_default();
    atomic_write(&workbuddy_exe_cache_file_for(region), &content)
}

/// 丢弃该 region 的 App 路径缓存。
pub fn clear_workbuddy_exe_cache_for(region: crate::modules::region::Region) {
    let _ = std::fs::remove_file(workbuddy_exe_cache_file_for(region));
}

pub fn codebuddy_cn_app_cache_file() -> PathBuf {
    store_dir().join("codebuddy_cn_app.json")
}

fn parse_codebuddy_cn_app_cache_json(text: &str) -> Option<PathBuf> {
    parse_workbuddy_exe_cache_json(text)
}

/// 读取上次成功解析到的 CodeBuddy CN 应用路径；损坏或空文件视为无缓存。
pub fn load_codebuddy_cn_app_cache() -> Option<PathBuf> {
    let f = codebuddy_cn_app_cache_file();
    if !f.exists() {
        return None;
    }
    let text = std::fs::read_to_string(&f).ok()?;
    parse_codebuddy_cn_app_cache_json(&text)
}

pub fn save_codebuddy_cn_app_cache(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(store_dir())?;
    let content =
        serde_json::to_string_pretty(&json!({ "exe": path.to_string_lossy() })).unwrap_or_default();
    atomic_write(&codebuddy_cn_app_cache_file(), &content)
}

pub fn clear_codebuddy_cn_app_cache() {
    let _ = std::fs::remove_file(codebuddy_cn_app_cache_file());
}

// ---------------------------------------------------------------------------
// 签到配置 / 日志（对照 server.py load/save_checkin_config / load/save/add_checkin_log）
// ---------------------------------------------------------------------------

/// 默认签到配置。
///
/// 调度时点一律由 `schedule_config.json` 的小时表决定（`modules::scheduler`），本配置只管
/// 「是否启用 / 保活天数 / 懒刷新间隔」。**旧的时间窗口字段 `start_hour` / `end_hour` 已删除**：
/// 它们没有任何读取方（纯死字段），留着会让「改它就能改签到时间」的错觉一直存在；老配置文件
/// 里残留的这两个键会在下次保存时被自然丢弃（合并从默认值出发，只透传白名单内的键）。
pub fn default_checkin_config() -> Value {
    json!({
        "enabled": true,
        "keepalive_days": 0,
        "lazy_refresh_hours": 24,
    })
}

fn merge_checkin_config(input: &Value) -> Value {
    let mut merged = default_checkin_config();
    let Some(map) = input.as_object() else {
        return merged;
    };
    if let Some(enabled) = map.get("enabled").and_then(Value::as_bool) {
        merged["enabled"] = json!(enabled);
    }
    for key in ["keepalive_days", "lazy_refresh_hours"] {
        if let Some(value) = map.get(key).and_then(Value::as_i64) {
            merged[key] = json!(value);
        }
    }
    merged
}

/// 读取签到配置（缺失/损坏时合并默认值）。
pub fn load_checkin_config() -> Value {
    let f = checkin_config_file();
    if f.exists() {
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                return merge_checkin_config(&value);
            }
        }
    }
    default_checkin_config()
}

/// 保存签到配置（只保留已知字段）。
pub fn save_checkin_config(cfg: &Value) -> std::io::Result<()> {
    let merged = merge_checkin_config(cfg);
    std::fs::create_dir_all(store_dir())?;
    let content = serde_json::to_string_pretty(&merged).unwrap_or_default();
    atomic_write(&checkin_config_file(), &content)
}

/// 读取签到日志。
pub fn load_checkin_logs() -> Vec<Value> {
    let f = checkin_logs_file();
    if f.exists() {
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&text) {
                return arr;
            }
        }
    }
    vec![]
}

fn save_checkin_logs_unlocked(logs: &[Value]) -> std::io::Result<()> {
    let kept = normalize_checkin_logs(logs, now_ms());
    std::fs::create_dir_all(store_dir())?;
    let content = serde_json::to_string_pretty(&kept).unwrap_or_default();
    atomic_write(&checkin_logs_file(), &content)
}

/// 保存签到日志（30 天过滤 + 保留最近 500 条，保持插入顺序）。
pub fn save_checkin_logs(logs: &[Value]) -> std::io::Result<()> {
    let _guard = CHECKIN_LOG_WRITE_LOCK.lock().unwrap();
    save_checkin_logs_unlocked(logs)
}

fn checkin_log_local_date(ts_ms: i64) -> Option<String> {
    Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .map(|date| date.format("%Y-%m-%d").to_string())
}

fn legacy_checkin_identity(entry: &Value) -> Option<String> {
    if let Some(account_id) = entry
        .get("accountId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(format!("account:{account_id}"));
    }

    // Old log rows predate accountId and only carried the display identity in
    // `email`. Keep this fallback namespaced so it can never merge with a
    // stable local account ID that happens to have the same text.
    entry
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|email| format!("legacy:{email}"))
}

/// Apply the persisted check-in log contract without changing the source file.
///
/// `success` and `error` entries retain their full multiplicity. Only repeated
/// legacy `already` rows are reduced to the latest timestamp for one account
/// and local calendar date.
fn normalize_checkin_logs(logs: &[Value], at_ms: i64) -> Vec<Value> {
    let cutoff = at_ms.saturating_sub(CHECKIN_LOG_KEEP_DAYS * 24 * 3600 * 1000);
    let retained: Vec<(usize, i64, &Value)> = logs
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let ts = norm_ts(entry.get("ts"))?;
            (ts >= cutoff).then_some((index, ts, entry))
        })
        .collect();

    let mut dedupable_already_indices = HashSet::new();
    let mut latest_already = HashMap::<(String, String), (i64, usize)>::new();
    for (index, ts, entry) in &retained {
        if entry.get("result").and_then(Value::as_str) != Some("already") {
            continue;
        }
        let Some(identity) = legacy_checkin_identity(entry) else {
            continue;
        };
        let Some(date) = checkin_log_local_date(*ts) else {
            continue;
        };
        dedupable_already_indices.insert(*index);
        let candidate = (*ts, *index);
        latest_already
            .entry((identity, date))
            .and_modify(|current| {
                if candidate >= *current {
                    *current = candidate;
                }
            })
            .or_insert(candidate);
    }

    let winning_already_indices: HashSet<usize> = latest_already
        .into_values()
        .map(|(_, index)| index)
        .collect();
    let mut normalized: Vec<Value> = retained
        .into_iter()
        .filter(|(index, _, entry)| {
            entry.get("result").and_then(Value::as_str) != Some("already")
                || !dedupable_already_indices.contains(index)
                || winning_already_indices.contains(index)
        })
        .map(|(_, _, entry)| entry.clone())
        .collect();

    if normalized.len() > CHECKIN_LOG_MAX_RECORDS {
        normalized.drain(..normalized.len() - CHECKIN_LOG_MAX_RECORDS);
    }
    normalized
}

fn compact_checkin_logs_at(path: &Path, at_ms: i64) -> std::io::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(path)?;
    let Ok(Value::Array(logs)) = serde_json::from_str::<Value>(&text) else {
        // Preserve unreadable user data rather than replacing it with an empty
        // file. Normal log loading keeps its existing tolerant behavior.
        return Ok(false);
    };
    let normalized = normalize_checkin_logs(&logs, at_ms);
    if normalized == logs {
        return Ok(false);
    }
    let content = serde_json::to_string_pretty(&normalized).unwrap_or_default();
    atomic_write(path, &content)?;
    Ok(true)
}

/// Compact legacy persisted check-in logs once during host startup.
///
/// Returns `true` only when the file was rewritten. Loading logs remains a
/// read-only operation; both hosts invoke this explicit migration before their
/// first automatic verification cycle.
pub fn compact_checkin_logs() -> std::io::Result<bool> {
    let _guard = CHECKIN_LOG_WRITE_LOCK.lock().unwrap();
    compact_checkin_logs_at(&checkin_logs_file(), now_ms())
}

/// 追加一条签到日志。
pub fn add_checkin_log(entry: &Value) {
    // Account-scoped check-in coordination permits unrelated accounts to run
    // concurrently. Serialize the file read-modify-write so neither entry is lost.
    let _guard = CHECKIN_LOG_WRITE_LOCK.lock().unwrap();
    let mut logs = load_checkin_logs();
    logs.push(entry.clone());
    let _ = save_checkin_logs_unlocked(&logs);
}

// ---------------------------------------------------------------------------
// 派猫猫旅行配置 / 缓存
// ---------------------------------------------------------------------------

/// 默认自动旅行配置。
pub fn default_travel_config() -> Value {
    json!({ "enabled": true })
}

/// 读取自动旅行配置（缺失/损坏时合并默认值）。
pub fn load_travel_config() -> Value {
    let mut cfg = default_travel_config();
    let f = travel_config_file();
    if f.exists() {
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) {
                if let Some(enabled) = map.get("enabled").and_then(Value::as_bool) {
                    cfg["enabled"] = json!(enabled);
                }
            }
        }
    }
    cfg
}

/// 保存自动旅行配置（只保留已知字段）。
pub fn save_travel_config(cfg: &Value) -> std::io::Result<()> {
    let mut merged = default_travel_config();
    if let Some(enabled) = cfg.get("enabled").and_then(Value::as_bool) {
        merged["enabled"] = json!(enabled);
    }
    std::fs::create_dir_all(store_dir())?;
    let content = serde_json::to_string_pretty(&merged).unwrap_or_default();
    atomic_write(&travel_config_file(), &content)
}

/// 读取旅行缓存（`{ date, completed, results: { accountId: {...} } }`）。
pub fn load_travel_cache() -> Value {
    let f = travel_cache_file();
    if f.exists() {
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) {
                return Value::Object(map);
            }
        }
    }
    json!({})
}

/// 保存旅行缓存。
pub fn save_travel_cache(cache: &Value) -> std::io::Result<()> {
    std::fs::create_dir_all(store_dir())?;
    let content = serde_json::to_string_pretty(cache).unwrap_or_default();
    atomic_write(&travel_cache_file(), &content)
}

// ---------------------------------------------------------------------------
// 自动轮换配置 / 日志（CodeBuddy CLI 账号轮换）
// ---------------------------------------------------------------------------

/// 默认自动轮换配置。
pub fn default_auto_rotate_config() -> Value {
    json!({
        "enabled": false,
        "check_interval_minutes": 5,
        "cooldown_minutes": 120,
        "min_gap_hours": 24,
        "min_urgency_hours": 72,
        "active_guard_minutes": 30,
        "min_remaining_credits": 0,
    })
}

/// 读取自动轮换配置（缺失/损坏时合并默认值）。
pub fn load_auto_rotate_config() -> Value {
    let mut cfg = default_auto_rotate_config();
    let f = auto_rotate_config_file();
    if f.exists() {
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) {
                for (k, v) in map {
                    cfg[k] = v;
                }
            }
        }
    }
    cfg
}

/// 保存自动轮换配置（只保留已知字段）。
pub fn save_auto_rotate_config(cfg: &Value) -> std::io::Result<()> {
    let mut merged = default_auto_rotate_config();
    let allowed: Vec<&str> = vec![
        "enabled",
        "check_interval_minutes",
        "cooldown_minutes",
        "min_gap_hours",
        "min_urgency_hours",
        "active_guard_minutes",
        "min_remaining_credits",
    ];
    for k in allowed {
        if let Some(v) = cfg.get(k) {
            merged[k] = v.clone();
        }
    }
    std::fs::create_dir_all(store_dir())?;
    let content = serde_json::to_string_pretty(&merged).unwrap_or_default();
    atomic_write(&auto_rotate_config_file(), &content)
}

// ---------------------------------------------------------------------------
// 账号切换 / 账号列表展示（全局单份，无需 region）
// ---------------------------------------------------------------------------

/// 账号切换与账号列表展示的默认配置。
///
/// - `copy_sessions_by_default` **默认关**：切换时复制会话是「顺带搬一个数据副本」，
///   沉默地改变切换语义会让人以为切错了号，必须由用户显式打开。
/// - `pin_current_account` **默认开**：它只改展示顺序、不动任何数据，
///   而频繁切号的用户最需要「我正在用哪个」一眼可见。
pub fn default_switch_config() -> Value {
    json!({
        "copy_sessions_by_default": false,
        "pin_current_account": true,
    })
}

/// 合并默认值与已知字段；未知键一律丢弃（与 `save_auto_rotate_config` 同口径）。
///
/// 缺失的键**回落到默认值**而不是原样透传：前端老版本不带新字段时，
/// 行为必须等于「刚装上」，否则新设置项会表现为随机取值。
fn normalize_switch_config(input: &Value) -> Value {
    let mut merged = default_switch_config();
    for key in ["copy_sessions_by_default", "pin_current_account"] {
        if let Some(value) = input.get(key).and_then(Value::as_bool) {
            merged[key] = json!(value);
        }
    }
    merged
}

/// 读取账号切换配置（缺失/损坏时回落默认值）。
pub fn load_switch_config() -> Value {
    let f = switch_config_file();
    if f.exists() {
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                return normalize_switch_config(&value);
            }
        }
    }
    default_switch_config()
}

/// 保存账号切换配置（只保留已知字段）。
pub fn save_switch_config(cfg: &Value) -> std::io::Result<()> {
    let merged = normalize_switch_config(cfg);
    std::fs::create_dir_all(store_dir())?;
    let content = serde_json::to_string_pretty(&merged).unwrap_or_default();
    atomic_write(&switch_config_file(), &content)
}

/// 读取自动轮换日志。
pub fn load_rotate_logs() -> Vec<Value> {
    let f = auto_rotate_logs_file();
    if f.exists() {
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&text) {
                return arr;
            }
        }
    }
    vec![]
}

/// 保存自动轮换日志（保留最近 N 条，保持插入顺序）。
pub fn save_rotate_logs(logs: &[Value]) -> std::io::Result<()> {
    let mut kept: Vec<Value> = logs.to_vec();
    if kept.len() > ROTATE_LOG_MAX_RECORDS {
        kept.drain(..kept.len() - ROTATE_LOG_MAX_RECORDS);
    }
    std::fs::create_dir_all(store_dir())?;
    let content = serde_json::to_string_pretty(&kept).unwrap_or_default();
    atomic_write(&auto_rotate_logs_file(), &content)
}

/// 追加一条自动轮换日志。
pub fn add_rotate_log(entry: &Value) {
    let mut logs = load_rotate_logs();
    logs.push(entry.clone());
    let _ = save_rotate_logs(&logs);
}

// ---------------------------------------------------------------------------
// 并发运行标志（替代 Python threading.Lock，Send 安全可跨 await）
// ---------------------------------------------------------------------------

/// RAII 运行标志：进入临界区置 true，Drop 时复位。
pub struct RunFlagGuard<'a> {
    flag: &'a AtomicBool,
}

impl<'a> RunFlagGuard<'a> {
    /// 尝试获取标志；已被占用返回 None。
    pub fn try_acquire(flag: &'a AtomicBool) -> Option<Self> {
        if flag.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(Self { flag })
        }
    }
}

impl Drop for RunFlagGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// 时间
// ---------------------------------------------------------------------------

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn utc_iso() -> String {
    // 对照 Python utc_iso：%Y-%m-%dT%H-%M-%S + "Z"
    format!("{}Z", chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S"))
}

// ---------------------------------------------------------------------------
// 文件
// ---------------------------------------------------------------------------

/// 原子写文件（临时文件 + rename），对照 Python atomic_write。
pub fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{file_name}.tmp-{}", uuid::Uuid::new_v4().simple()));
    if let Err(e) = std::fs::write(&tmp, content) {
        eprintln!("[atomic] write tmp FAILED: {e}");
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        eprintln!("[atomic] rename FAILED: {e}");
        return Err(e);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 时间戳归一化
// ---------------------------------------------------------------------------

/// 把秒/毫秒/字符串时间戳统一为毫秒；无效返回 None。对照 server.py `_norm_ts`。
pub fn norm_ts(v: Option<&Value>) -> Option<i64> {
    let mut ts: i64 = match v {
        Some(Value::String(s)) => s.trim().parse::<f64>().ok()? as i64,
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64))?,
        _ => return None,
    };
    if ts < 10_000_000_000 {
        ts *= 1000; // 秒 → 毫秒
    }
    Some(ts)
}

// ---------------------------------------------------------------------------
// HTTP 客户端（对照 Python http_request）
// ---------------------------------------------------------------------------

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(DEFAULT_HTTP_USER_AGENT)
}

fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        http_client_builder()
            .build()
            .expect("failed to build reqwest client")
    })
}

/// 通用 HTTP 请求，返回解析后的 JSON。
///
/// 行为对齐 Python 版：
/// - 2xx：解析 body 为 JSON；
/// - HTTP 错误：body 可解析则返回其 JSON，否则 `{"code": <status>, "message": <body 前 500 字符>}`；
/// - 网络错误：`{"code": -1, "message": <原因>}`。
pub async fn http_request(
    url: &str,
    method: &str,
    body: Option<Value>,
    headers: Option<&HashMap<String, String>>,
) -> Value {
    http_request_with_proxy(url, method, body, headers, None).await
}

/// 通用 HTTP 请求，可为单次请求显式指定 HTTP/HTTPS 代理。
pub async fn http_request_with_proxy(
    url: &str,
    method: &str,
    body: Option<Value>,
    headers: Option<&HashMap<String, String>>,
    proxy: Option<&str>,
) -> Value {
    let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let client = match proxy.map(str::trim).filter(|value| !value.is_empty()) {
        Some(proxy) => match http_client_builder()
            .proxy(match reqwest::Proxy::all(proxy) {
                Ok(proxy) => proxy,
                Err(e) => return json!({"code": -1, "message": format!("代理地址无效: {e}")}),
            })
            .build()
        {
            Ok(client) => client,
            Err(e) => return json!({"code": -1, "message": format!("代理客户端创建失败: {e}")}),
        },
        None => http_client().clone(),
    };
    let mut req = client.request(method, url);
    req = req.header("Content-Type", "application/json");
    if let Some(h) = headers {
        for (k, v) in h {
            req = req.header(k, v);
        }
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if status.is_success() {
                serde_json::from_str(&text).unwrap_or(Value::Null)
            } else {
                serde_json::from_str(&text).unwrap_or_else(|_| {
                    json!({
                        "code": status.as_u16(),
                        "message": normalize_error_body(&text),
                    })
                })
            }
        }
        Err(e) => json!({"code": -1, "message": e.to_string()}),
    }
}

/// 非 JSON 错误响应体归一化：网关（openresty / APISIX 等）的 401 / 5xx 常返回
/// **整页 HTML**，原样截断会把 `<html>…` 整段塞进通知与界面卡片。
///
/// - HTML ⇒ 提取 `<title>` 作为可读信息（如 `401 Authorization Required`）；
/// - 其余 ⇒ 保持原有的 500 字符截断。
fn normalize_error_body(text: &str) -> String {
    if text.trim_start().starts_with('<') {
        let title = text
            .split_once("<title>")
            .and_then(|(_, rest)| rest.split_once("</title>"))
            .map(|(title, _)| title.trim())
            .unwrap_or_default();
        return if title.is_empty() {
            "服务端返回 HTML 错误页（无标题）".to_string()
        } else {
            format!("服务端返回 HTML 错误页：{title}")
        };
    }
    text.chars().take(500).collect::<String>()
}

/// 通用 HTTP 请求，返回原始响应（状态码 + 响应头 + 响应体），可选是否跟随重定向。
///
/// 供需要读取响应头（如 302 的 `Location`）或自行处理非 JSON 响应的场景使用；
/// 其余场景优先用 [`http_request_with_proxy`]。失败（网络错误 / 代理配置错误）
/// 返回 `(0, HashMap::new(), 错误信息)`，由调用方根据 status 判断。
pub async fn http_request_raw(
    url: &str,
    method: &str,
    body: Option<Value>,
    headers: Option<&HashMap<String, String>>,
    proxy: Option<&str>,
    follow_redirects: bool,
) -> (u16, HashMap<String, String>, String) {
    let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let client = match proxy.map(str::trim).filter(|value| !value.is_empty()) {
        Some(proxy) => {
            let mut builder = http_client_builder().proxy(match reqwest::Proxy::all(proxy) {
                Ok(proxy) => proxy,
                Err(e) => return (0, HashMap::new(), format!("代理地址无效: {e}")),
            });
            if !follow_redirects {
                builder = builder.redirect(reqwest::redirect::Policy::none());
            }
            match builder.build() {
                Ok(client) => client,
                Err(e) => return (0, HashMap::new(), format!("代理客户端创建失败: {e}")),
            }
        }
        None => {
            if follow_redirects {
                http_client().clone()
            } else {
                match http_client_builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                {
                    Ok(client) => client,
                    Err(e) => return (0, HashMap::new(), format!("客户端创建失败: {e}")),
                }
            }
        }
    };
    let mut req = client.request(method, url);
    req = req.header("Content-Type", "application/json");
    if let Some(h) = headers {
        for (k, v) in h {
            req = req.header(k, v);
        }
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    match req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let mut resp_headers = HashMap::new();
            for (k, v) in resp.headers() {
                if let Ok(vs) = v.to_str() {
                    resp_headers.insert(k.as_str().to_string(), vs.to_string());
                }
            }
            let text = resp.text().await.unwrap_or_default();
            (status, resp_headers, text)
        }
        Err(e) => (0, HashMap::new(), e.to_string()),
    }
}

/// 把响应体文本解析为 JSON；非 JSON（HTML / 空体）时包成 `{"raw": <前 500 字符>}`。
fn parse_json_body(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| {
        json!({ "raw": text.chars().take(500).collect::<String>() })
    })
}

/// 带 token 刷新重试的**已授权**请求，返回 `(HTTP 状态码, 解析后的 JSON)`。
///
/// 与 [`http_request`] 的差异：**保留 HTTP 状态码**。growth / school / cat 这类接口需要
/// 区分「409 已领取」「403 天数不足」「400 无抽奖次数」等**正常态**——这些态要靠真实
/// HTTP 状态判定，而 `http_request` 在失败时会把状态码折叠进 `code` 字段，无法可靠区分。
///
/// 刷新重试只在 **401**（token 失效）且账号带 refresh token 时触发一次；**不看 403**——
/// growth 域的 403 是「连续登录天数不足」这类正常业务态，不应被误判为需刷新。
/// `extra_headers` 会在刷新前后都覆盖到基础鉴权头上（如 Origin/Referer/x-client-platform）。
pub async fn authed_json_request_for(
    region: crate::modules::region::Region,
    url: &str,
    method: &str,
    body: Option<Value>,
    account: &Value,
    extra_headers: &HashMap<String, String>,
) -> (u16, Value) {
    // 加密信封凭据短路：`build_auth_headers` 会把信封折成**空 Bearer**，
    // 发出去只会换回上游 401（以及整页 HTML）。这里直接给可读错误。
    // 401 是刻意的：调用方（growth / school / cat）都按「非 2xx = 失败」处理，
    // 并把 `message` 透出给用户；信封态**不是**可刷新状态，不该走下面的 401 刷新分支。
    if let Some(err) = crate::modules::account::envelope_token_error(account) {
        return (401, json!({"code": -2, "message": err}));
    }
    let mut headers = crate::modules::account::build_auth_headers(account);
    for (k, v) in extra_headers {
        headers.insert(k.clone(), v.clone());
    }
    let (status, _resp_headers, text) =
        http_request_raw(url, method, body.clone(), Some(&headers), None, true).await;
    let parsed = parse_json_body(&text);

    let has_refresh = !account
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .is_empty();
    if status == 401 && has_refresh {
        let refreshed =
            crate::modules::refresh::refresh_account_token_for(region, account.clone()).await;
        let mut headers = crate::modules::account::build_auth_headers(&refreshed);
        for (k, v) in extra_headers {
            headers.insert(k.clone(), v.clone());
        }
        let (status, _resp_headers, text) =
            http_request_raw(url, method, body, Some(&headers), None, true).await;
        return (status, parse_json_body(&text));
    }
    (status, parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::region::Region;

    /// 回归上游 issue #94：网关 401 返回的**整页 HTML** 要归一化为可读信息，
    /// 不能把 `<html>…` 原样塞进通知与界面卡片。
    #[test]
    fn normalize_error_body_extracts_html_title() {
        let html = "<html>\n<head><title>401 Authorization Required</title></head>\n\
                    <body>\n<center><h1>401 Authorization Required</h1></center>\n\
                    <hr><center>openresty</center>\n</body>\n</html>\n";
        assert_eq!(
            normalize_error_body(html),
            "服务端返回 HTML 错误页：401 Authorization Required"
        );

        // 前导空白不得让 HTML 判定失效（网关常带 BOM / 换行）。
        assert_eq!(
            normalize_error_body("\n  <!DOCTYPE html><html><body>boom</body></html>"),
            "服务端返回 HTML 错误页（无标题）"
        );

        // 非 HTML 错误体保持原有的 500 字符截断行为。
        let plain = "plain gateway error";
        assert_eq!(normalize_error_body(plain), plain);
        let long = "x".repeat(600);
        assert_eq!(normalize_error_body(&long).chars().count(), 500);
        assert_eq!(normalize_error_body(&long), "x".repeat(500));
    }

    /// ★ **接线**断言：`http_request_with_proxy` 收到「非 2xx + 整页 HTML」时，
    /// 必须把错误体归一化后再交给调用方，而不是把 `<html>…` 原样回显。
    ///
    /// 与上一条的分工：上一条测**纯函数**，挡不住「调用点又写回 `take(500)`」
    /// 这类回归（纯函数照样绿 —— 实测确认过）。这条喂一个本地桩服务、走真实的
    /// reqwest 调用链，才钉得住「调用点确实用了归一化」。
    ///
    /// ⚠️ 桩服务**必须先读完请求再回响应**（见下面 `spawn` 里的注释）。
    /// 回环地址不受 `HTTP_PROXY` 影响，故带代理的环境里同样可跑（已实测）。
    #[tokio::test]
    async fn non_json_error_body_is_normalized_end_to_end() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("绑定本地桩服务");
        let addr = listener.local_addr().expect("取回环地址");
        tokio::spawn(async move {
            let Ok((mut stream, _peer)) = listener.accept().await else {
                return;
            };
            // ★ 必须**先读完请求**再回响应：反过来（先写响应再关连接）在 Windows 上会让
            //   客户端收到 RST，reqwest 报 "error sending request for url"（本用例初版
            //   实测踩到，且当时 stderr 里看不出原因 —— 别往归一化逻辑上找）。
            let mut buf: Vec<u8> = Vec::new();
            let mut chunk = [0u8; 1024];
            while !String::from_utf8_lossy(&buf).contains("\r\n\r\n") {
                match stream.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(read) => buf.extend_from_slice(&chunk[..read]),
                }
            }

            let body = "<html><head><title>401 Authorization Required</title></head>\
                        <body><center><h1>401 Authorization Required</h1></center></body></html>";
            let response = format!(
                "HTTP/1.1 401 Unauthorized\r\n\
                 Content-Type: text/html\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
        });

        let value =
            http_request_with_proxy(&format!("http://{addr}/blocked"), "GET", None, None, None)
                .await;

        assert_eq!(value["code"], 401, "非 2xx 的 code 必须是真实状态码：{value}");
        let message = value["message"].as_str().expect("message 应为字符串");
        assert_eq!(
            message,
            "服务端返回 HTML 错误页：401 Authorization Required"
        );
        assert!(
            !message.contains("<html>"),
            "不得把整页 HTML 原样回显给用户：{message}"
        );
    }

    fn local_timestamp_ms(year: i32, month: u32, day: u32, hour: u32) -> i64 {
        Local
            .with_ymd_and_hms(year, month, day, hour, 0, 0)
            .single()
            .expect("test timestamp must be unambiguous")
            .timestamp_millis()
    }

    /// 签到配置**不得**再含 `start_hour` / `end_hour`：它们没有任何读取方（调度只认
    /// `schedule_config` 的小时表），留着等于给用户一个「改它就能改签到时间」的假控件。
    ///
    /// 可证伪性：把任一个键加回 [`default_checkin_config`] 或 [`merge_checkin_config`]
    /// 的白名单，本用例即变红。
    #[test]
    fn auto_checkin_config_carries_no_dead_time_window_fields() {
        let cfg = default_checkin_config();
        assert_eq!(cfg.get("enabled").and_then(Value::as_bool), Some(true));
        assert!(
            cfg.get("start_hour").is_none(),
            "start_hour 是死字段，不应再出现在默认签到配置里"
        );
        assert!(
            cfg.get("end_hour").is_none(),
            "end_hour 是死字段，不应再出现在默认签到配置里"
        );
        // 老配置文件里残留的这两个键也不得被透传回存盘结果（否则死字段复活）。
        let merged = merge_checkin_config(&json!({ "start_hour": 3, "end_hour": 4 }));
        assert!(merged.get("start_hour").is_none(), "start_hour 不得再被透传");
        assert!(merged.get("end_hour").is_none(), "end_hour 不得再被透传");
        assert_eq!(merged.get("enabled").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn auto_checkin_explicit_false_wins_and_invalid_value_uses_default() {
        let disabled = merge_checkin_config(&json!({"enabled": false, "keepalive_days": 7}));
        assert_eq!(
            disabled.get("enabled").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            disabled.get("keepalive_days").and_then(Value::as_i64),
            Some(7)
        );

        let corrupt = merge_checkin_config(&json!({"enabled": "no", "lazy_refresh_hours": null}));
        assert_eq!(corrupt.get("enabled").and_then(Value::as_bool), Some(true));
        assert_eq!(
            corrupt.get("lazy_refresh_hours").and_then(Value::as_i64),
            Some(24)
        );
    }

    #[test]
    fn checkin_log_normalization_keeps_latest_already_per_identity_and_local_date() {
        let day = local_timestamp_ms(2026, 8, 20, 12);
        let logs = vec![
            json!({"accountId": "a", "email": "same", "result": "already", "ts": day + 1, "marker": "a-old"}),
            json!({"accountId": "b", "email": "same", "result": "already", "ts": day + 2, "marker": "b"}),
            json!({"accountId": "a", "email": "same", "result": "already", "ts": day + 3, "marker": "a-new"}),
            json!({"email": "legacy@example.com", "result": "already", "ts": day + 4, "marker": "legacy-old"}),
            json!({"email": "legacy@example.com", "result": "already", "ts": day + 5, "marker": "legacy-new"}),
            json!({"result": "already", "ts": day + 6, "marker": "no-identity"}),
        ];

        let normalized = normalize_checkin_logs(&logs, day + 10);
        let markers: Vec<&str> = normalized
            .iter()
            .filter_map(|entry| entry.get("marker").and_then(Value::as_str))
            .collect();

        assert_eq!(markers, vec!["b", "a-new", "legacy-new", "no-identity"]);
    }

    #[test]
    fn checkin_log_identity_namespaces_stable_ids_and_legacy_email_fallbacks() {
        let day = local_timestamp_ms(2026, 8, 20, 12);
        let logs = vec![
            json!({"accountId": "same@example.com", "email": "display", "result": "already", "ts": day + 1, "marker": "stable-old"}),
            json!({"email": "same@example.com", "result": "already", "ts": day + 2, "marker": "legacy-old"}),
            json!({"accountId": "same@example.com", "email": "display", "result": "already", "ts": day + 3, "marker": "stable-new"}),
            json!({"email": "same@example.com", "result": "already", "ts": day + 4, "marker": "legacy-new"}),
        ];

        let normalized = normalize_checkin_logs(&logs, day + 10);
        let markers: Vec<&str> = normalized
            .iter()
            .filter_map(|entry| entry.get("marker").and_then(Value::as_str))
            .collect();

        assert_eq!(markers, vec!["stable-new", "legacy-new"]);
    }

    #[test]
    fn checkin_log_normalization_keeps_already_for_separate_dates() {
        let first_day = local_timestamp_ms(2026, 8, 19, 12);
        let second_day = local_timestamp_ms(2026, 8, 20, 12);
        let logs = vec![
            json!({"accountId": "a", "result": "already", "ts": first_day}),
            json!({"accountId": "a", "result": "already", "ts": second_day}),
        ];

        assert_eq!(normalize_checkin_logs(&logs, second_day).len(), 2);
    }

    #[test]
    fn checkin_log_normalization_preserves_success_and_error_multiplicity() {
        let day = local_timestamp_ms(2026, 8, 20, 12);
        let logs = vec![
            json!({"accountId": "a", "result": "success", "ts": day + 1}),
            json!({"accountId": "a", "result": "success", "ts": day + 2}),
            json!({"accountId": "a", "result": "error", "ts": day + 3}),
            json!({"accountId": "a", "result": "error", "ts": day + 4}),
        ];

        assert_eq!(normalize_checkin_logs(&logs, day + 10), logs);
    }

    #[test]
    fn checkin_log_normalization_applies_retention_and_record_cap() {
        let now = local_timestamp_ms(2026, 8, 20, 12);
        let cutoff = now - CHECKIN_LOG_KEEP_DAYS * 24 * 3600 * 1000;
        let mut logs = vec![json!({
            "accountId": "old",
            "result": "success",
            "ts": cutoff - 1,
            "marker": -1,
        })];
        logs.extend((0..505).map(|marker| {
            json!({
                "accountId": "a",
                "result": "success",
                "ts": now,
                "marker": marker,
            })
        }));

        let normalized = normalize_checkin_logs(&logs, now);
        assert_eq!(normalized.len(), CHECKIN_LOG_MAX_RECORDS);
        assert_eq!(normalized[0]["marker"], 5);
        assert_eq!(normalized.last().unwrap()["marker"], 504);
    }

    #[test]
    fn checkin_log_normalization_deduplicates_before_taking_final_500() {
        let now = local_timestamp_ms(2026, 8, 20, 12);
        let mut logs = vec![
            json!({"accountId": "duplicate", "result": "already", "ts": now - 2, "marker": "duplicate-old"}),
            json!({"accountId": "duplicate", "result": "already", "ts": now - 1, "marker": "duplicate-new"}),
        ];
        logs.extend((0..500).map(|marker| {
            json!({
                "accountId": "a",
                "result": "success",
                "ts": now,
                "marker": marker,
            })
        }));

        let normalized = normalize_checkin_logs(&logs, now);
        assert_eq!(normalized.len(), CHECKIN_LOG_MAX_RECORDS);
        assert_eq!(normalized[0]["marker"], 0);
        assert_eq!(normalized.last().unwrap()["marker"], 499);
        assert!(normalized
            .iter()
            .all(|entry| entry["marker"] != "duplicate-old"));
    }

    #[test]
    fn checkin_log_normalization_is_idempotent() {
        let day = local_timestamp_ms(2026, 8, 20, 12);
        let logs = vec![
            json!({"accountId": "a", "result": "already", "ts": day + 1}),
            json!({"accountId": "a", "result": "already", "ts": day + 2}),
            json!({"accountId": "a", "result": "success", "ts": day + 3}),
        ];

        let once = normalize_checkin_logs(&logs, day + 10);
        assert_eq!(normalize_checkin_logs(&once, day + 10), once);
    }

    #[test]
    fn persisted_checkin_log_compaction_writes_only_when_changed() {
        let day = local_timestamp_ms(2026, 8, 20, 12);
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-checkin-log-compaction-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logs.json");
        let logs = json!([
            {"accountId": "a", "result": "already", "ts": day + 1},
            {"accountId": "a", "result": "already", "ts": day + 2}
        ]);
        std::fs::write(&path, serde_json::to_string_pretty(&logs).unwrap()).unwrap();

        assert!(compact_checkin_logs_at(&path, day + 10).unwrap());
        let after_first = std::fs::read_to_string(&path).unwrap();
        assert!(!compact_checkin_logs_at(&path, day + 10).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), after_first);

        let missing = dir.join("missing.json");
        assert!(!compact_checkin_logs_at(&missing, day + 10).unwrap());
        assert!(!missing.exists());

        let corrupt = dir.join("corrupt.json");
        std::fs::write(&corrupt, "not-json").unwrap();
        assert!(!compact_checkin_logs_at(&corrupt, day + 10).unwrap());
        assert_eq!(std::fs::read_to_string(&corrupt).unwrap(), "not-json");

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parse_workbuddy_exe_cache_json_reads_exe() {
        let path = parse_workbuddy_exe_cache_json(
            r#"{ "exe": "D:\\Users\\Zhou\\AppData\\Local\\Programs\\WorkBuddy\\WorkBuddy.exe" }"#,
        )
        .expect("valid cache");
        assert_eq!(
            path.to_string_lossy(),
            r"D:\Users\Zhou\AppData\Local\Programs\WorkBuddy\WorkBuddy.exe"
        );
    }

    #[test]
    fn parse_workbuddy_exe_cache_json_ignores_corrupt_and_empty() {
        assert!(parse_workbuddy_exe_cache_json("not-json").is_none());
        assert!(parse_workbuddy_exe_cache_json(r#"{ "exe": "  " }"#).is_none());
        assert!(parse_workbuddy_exe_cache_json("{}").is_none());
    }

    #[test]
    fn parse_codebuddy_cn_app_cache_json_reads_exe() {
        let path = parse_codebuddy_cn_app_cache_json(
            r#"{ "exe": "/Applications/CodeBuddy CN.app" }"#,
        )
        .expect("valid cache");
        assert_eq!(path.to_string_lossy(), "/Applications/CodeBuddy CN.app");
    }

    #[test]
    fn parse_codebuddy_cn_app_cache_json_ignores_corrupt_and_empty() {
        assert!(parse_codebuddy_cn_app_cache_json("not-json").is_none());
        assert!(parse_codebuddy_cn_app_cache_json(r#"{ "exe": "  " }"#).is_none());
        assert!(parse_codebuddy_cn_app_cache_json("{}").is_none());
    }

    #[test]
    fn codebuddy_cn_app_cache_file_is_not_workbuddy_exe_cache() {
        assert_ne!(
            codebuddy_cn_app_cache_file(),
            workbuddy_exe_cache_file()
        );
        assert!(codebuddy_cn_app_cache_file()
            .file_name()
            .is_some_and(|n| n == "codebuddy_cn_app.json"));
    }

    /// App / 可执行文件路径缓存必须按 region 隔离。
    ///
    /// CN 沿用 `workbuddy_exe.json`（保持兼容），Global 用 `workbuddy_exe.global.json`；
    /// 若两版共用一份，CN 的探测结果会覆盖国际版启动路径。
    ///
    /// 本用例断言的是**绝对路径**（`store_dir().join(..)`），而 `store_dir()` 依赖进程级
    /// 的 `BUDDY_SWITCH_HOME`。若并发测试在中途改掉该变量，两次调用会读到**不同的 home**，
    /// 报出「同一个函数族内部不一致」的假失败 —— 所以必须先取 env 锁
    /// （同 `trae::paths::trae_files_do_not_collide_with_workbuddy_files`）。
    #[test]
    fn workbuddy_exe_cache_file_for_is_region_scoped() {
        let _lock = env_lock();

        let cn = workbuddy_exe_cache_file_for(Region::Cn);
        let global = workbuddy_exe_cache_file_for(Region::Global);

        assert_eq!(
            cn.file_name().and_then(|n| n.to_str()),
            Some("workbuddy_exe.json"),
            "CN App 路径缓存文件名"
        );
        assert_eq!(
            global.file_name().and_then(|n| n.to_str()),
            Some("workbuddy_exe.global.json"),
            "Global App 路径缓存文件名"
        );
        assert_ne!(cn, global, "CN / Global App 路径缓存必须隔离");
        // 同一 store 目录，仅文件名不同。
        assert_eq!(cn.parent(), global.parent());
        // CN 必须与既有兼容路径一致，避免两处逻辑漂移。
        assert_eq!(cn, workbuddy_exe_cache_file());
        assert_eq!(global, store_dir().join("workbuddy_exe.global.json"));
    }

    /// billing / 官网基址必须按 region 选择。
    #[test]
    fn api_endpoint_for_is_region_scoped() {
        assert_eq!(api_endpoint_for(Region::Cn), "https://www.codebuddy.cn");
        assert_eq!(api_endpoint_for(Region::Global), "https://www.workbuddy.ai");
        assert_ne!(
            api_endpoint_for(Region::Cn),
            api_endpoint_for(Region::Global),
            "两版 billing 基址不得相同"
        );
        // CN 与既有常量保持一致。
        assert_eq!(api_endpoint_for(Region::Cn), WORKBUDDY_API_ENDPOINT);
    }

    #[test]
    fn default_http_user_agent_matches_official_chrome_desktop() {
        assert_eq!(
            DEFAULT_HTTP_USER_AGENT,
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36"
        );
        let _ = http_client_builder();
    }

    /// 唯一临时路径（不创建），用于「不存在」用例。
    fn unique_missing_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "buddy-switch-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    /// F2：护栏的纯逻辑——覆盖值必须是**已存在的绝对目录**。
    #[test]
    fn validate_home_override_accepts_only_existing_absolute_dir() {
        // 已存在的绝对目录 → 接受，并原样返回。
        let dir = std::env::temp_dir();
        assert!(dir.is_absolute(), "precondition: temp dir must be absolute");
        assert_eq!(
            validate_home_override(dir.to_str().expect("utf8 temp dir")),
            Ok(dir.clone())
        );

        // 相对路径 → 拒绝（即使该名称恰好存在也是相对路径）。
        assert_eq!(
            validate_home_override("relative-buddy-switch-home"),
            Err(OverrideReject::NotAbsolute)
        );
        assert_eq!(validate_home_override("."), Err(OverrideReject::NotAbsolute));

        // 绝对但**不存在** → 拒绝，且不得顺手创建它。
        let missing = unique_missing_path("validate-missing");
        assert!(!missing.exists(), "precondition: path must not exist");
        assert_eq!(
            validate_home_override(missing.to_str().expect("utf8 missing path")),
            Err(OverrideReject::NotDir)
        );
        assert!(!missing.exists(), "the guard must never create the rejected path");

        // 绝对但是**普通文件** → 拒绝。
        let file = unique_missing_path("validate-file");
        std::fs::write(&file, b"not a directory").expect("write temp file");
        assert_eq!(
            validate_home_override(file.to_str().expect("utf8 file path")),
            Err(OverrideReject::NotDir)
        );
        let _ = std::fs::remove_file(&file);
    }

    /// F2：每种拒绝原因都有可读文案（面向用户的一次性警告）。
    #[test]
    fn override_reject_descriptions_are_distinct_and_non_empty() {
        let not_absolute = OverrideReject::NotAbsolute.describe();
        let not_dir = OverrideReject::NotDir.describe();
        assert!(!not_absolute.is_empty());
        assert!(!not_dir.is_empty());
        assert_ne!(not_absolute, not_dir);
        assert_ne!(OverrideReject::NotAbsolute, OverrideReject::NotDir);
    }

    /// `store_dir()` **恒定**返回 `~/.buddy-switch`：即使 `~/.wb-switch` 存在
    /// （那是另一个独立项目 `changexbc/workbuddy-switch` 的固定数据目录），也**绝不**被接管。
    ///
    /// 这条护栏防止两项目相互替换：双方有 12 个同名文件（`accounts.json`、
    /// `workbuddy_exe.json`、`credit_usage_snapshots.json`、`backups/` 等），
    /// 而 `workbuddy_exe.json` 的 schema 互不兼容 —— 一旦接管就会互相覆盖。
    ///
    /// 用隔离的临时 home 验证，绝不触碰真实 `~/.buddy-switch` / `~/.wb-switch`。
    ///
    /// 注意：**不要**在这里再手动 `env_lock()`。`HomeOverrideGuard::set()` 内部已经
    /// 取锁并把持到 drop，而 `env_lock()` 返回的是**非可重入**的 `MutexGuard`——
    /// 重复取用会**自死锁**，且因为测试线程互相等待，会连带把其他同样需要该锁的
    /// 测试（`trae::paths`、`trae::logs`）一起拖住，表现为整个测试进程挂死。
    #[test]
    fn store_dir_never_adopts_foreign_wb_switch_dir() {
        let home = std::env::temp_dir().join(format!(
            "buddy-switch-store-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let new_dir = home.join(".buddy-switch");
        let foreign_dir = home.join(".wb-switch");
        std::fs::create_dir_all(&home).expect("create isolated home");

        // 取锁 + 指向隔离 home，均由本 guard 负责（持锁至 drop）。
        let _guard = HomeOverrideGuard::set(&home);

        // 1) 两个目录都不存在 → 新目录（全新安装）。
        assert_eq!(store_dir(), new_dir, "全新安装应使用 ~/.buddy-switch");

        // 2) 只有「对方」目录存在 → 仍然用新目录，绝不接管。
        //    这正是最危险的场景：用户已装对方、首次运行本项目。
        std::fs::create_dir_all(&foreign_dir).expect("create foreign dir");
        assert_eq!(
            store_dir(),
            new_dir,
            "~/.wb-switch 属于另一个独立项目，绝不可被接管"
        );

        // 3) 两目录并存 → 同样只用新目录。
        std::fs::create_dir_all(&new_dir).expect("create new dir");
        assert_eq!(store_dir(), new_dir, "两目录并存时仍只用 ~/.buddy-switch");

        std::fs::remove_dir_all(&home).expect("cleanup isolated home");
    }

    /// 旧环境变量 `WB_SWITCH_HOME` 已**不再被识别**。
    ///
    /// 它曾用于指向旧数据目录 `~/.wb-switch`（另一个独立项目的数据目录）；
    /// 继续认它等于让对方的数据目录被本项目接管。只有 `BUDDY_SWITCH_HOME` 生效。
    #[test]
    fn legacy_wb_switch_home_env_is_ignored() {
        let _lock = env_lock();
        let base = std::env::temp_dir().join(format!(
            "buddy-switch-home-ignored-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let new_home = base.join("new-home");
        let legacy_home = base.join("legacy-home");
        std::fs::create_dir_all(&new_home).expect("create new home");
        let foreign_store = legacy_home.join(".wb-switch");
        std::fs::create_dir_all(&foreign_store).expect("create foreign store");

        let previous_new = std::env::var_os(BUDDY_SWITCH_HOME_ENV);
        let previous_legacy = std::env::var_os("WB_SWITCH_HOME");

        // 只设旧变量、且该目录下真的存在 `.wb-switch` → 仍必须被忽略。
        std::env::remove_var(BUDDY_SWITCH_HOME_ENV);
        std::env::set_var("WB_SWITCH_HOME", legacy_home.as_os_str());
        assert_ne!(
            store_dir(),
            foreign_store,
            "旧变量 WB_SWITCH_HOME 不得把本项目指向 ~/.wb-switch"
        );

        // 新变量生效。
        std::env::set_var(BUDDY_SWITCH_HOME_ENV, new_home.as_os_str());
        assert_eq!(home_dir(), new_home, "BUDDY_SWITCH_HOME 必须生效");
        assert_eq!(store_dir(), new_home.join(".buddy-switch"));

        // 还原（含 panic 时靠测试进程结束兜底，但不依赖它）。
        match previous_new {
            Some(value) => std::env::set_var(BUDDY_SWITCH_HOME_ENV, value),
            None => std::env::remove_var(BUDDY_SWITCH_HOME_ENV),
        }
        match previous_legacy {
            Some(value) => std::env::set_var("WB_SWITCH_HOME", value),
            None => std::env::remove_var("WB_SWITCH_HOME"),
        }

        std::fs::remove_dir_all(&base).expect("cleanup");
    }
}
