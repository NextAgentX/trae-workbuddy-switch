//! 签到：状态查询 / 执行签到 / 自动签到调度。
//!
//! 对照 server.py `get_checkin_status` / `perform_checkin` /
//! `checkin_account` / `run_checkin_cycle` / `_checkin_request` /
//! `_is_unauthorized`。
//!
//! **region 化**：新增 `*_for(region, …)` 变体；旧 CN 签名保留为薄包装。
//! 国际版签到按「该功能独立降级」处理（Q7）：返回结构化 unsupported，不影响同页
//! 其他功能。

use chrono::Local;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::modules::account::{
    account_display_name, build_auth_headers, envelope_token_error, load_accounts_for,
};
use crate::modules::config::{
    add_checkin_log, http_request, load_checkin_config, load_checkin_logs, now_ms, RunFlagGuard,
    CHECKIN_API_PREFIX,
};
use crate::modules::refresh::{ensure_fresh_token_for, refresh_account_token_for};
use crate::modules::region::{region_spec, Region};

static CHECKIN_RUNNING: AtomicBool = AtomicBool::new(false);
static CHECKIN_ACCOUNTS_RUNNING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Automatic recovery cadence shared by every host.
pub const CHECKIN_RECOVERY_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// 国际版签到不可用时的统一文案。
const UNSUPPORTED_MESSAGE: &str = "该版本暂不支持自动签到";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckinCycleMode {
    /// Always verify every account against the server after a host starts.
    StartupVerify,
    /// Re-verify every account against the server during background recovery.
    PeriodicRecovery,
}

#[derive(Debug, Eq, PartialEq)]
enum StatusDecision {
    Already,
    Submit,
    Error(String),
}

struct AccountRunGuard {
    key: String,
}

impl AccountRunGuard {
    fn try_acquire(account: &Value) -> Option<Self> {
        let key = account
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(String::from)
            .unwrap_or_else(|| account_display_name(account));
        let mut running = CHECKIN_ACCOUNTS_RUNNING
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .unwrap();
        if !running.insert(key.clone()) {
            return None;
        }
        Some(Self { key })
    }
}

impl Drop for AccountRunGuard {
    fn drop(&mut self) {
        if let Some(running) = CHECKIN_ACCOUNTS_RUNNING.get() {
            running.lock().unwrap().remove(&self.key);
        }
    }
}

/// 判断是否因 token 失效被拒（用于触发刷新重试）。
fn is_unauthorized(resp: &Value) -> bool {
    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code == 401 || code == 403 {
        return true;
    }
    let msg = resp
        .get("message")
        .or_else(|| resp.get("msg"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    ["unauthorized", "401", "登录", "失效", "过期", "token"]
        .iter()
        .any(|k| msg.contains(k))
}

/// 按 region 发签到相关请求；遇到未授权且存在 refresh token 时刷新一次并重试。
async fn checkin_request_for(region: Region, path: &str, account: &Value) -> Value {
    // 加密信封凭据短路：不发空 Bearer，直接给出可读错误（上游 PR #95 的同款处理）。
    if let Some(err) = envelope_token_error(account) {
        return json!({"code": -2, "message": err});
    }
    let url = format!("{}{path}", region_spec(region).billing_base);
    let headers = build_auth_headers(account);
    let mut resp = http_request(&url, "POST", Some(json!({})), Some(&headers)).await;
    if is_unauthorized(&resp)
        && !account
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
    {
        let refreshed = refresh_account_token_for(region, account.clone()).await;
        let headers = build_auth_headers(&refreshed);
        resp = http_request(&url, "POST", Some(json!({})), Some(&headers)).await;
    }
    resp
}

/// 查询签到状态（CN）。
pub async fn get_checkin_status(account: &Value) -> Value {
    get_checkin_status_for(Region::Cn, account).await
}

/// 按 region 查询签到状态：新接口 checkin-activity-status，失败回退 checkin-status。
pub async fn get_checkin_status_for(region: Region, account: &Value) -> Value {
    if region == Region::Global {
        return json!({"ok": false, "error": UNSUPPORTED_MESSAGE});
    }
    let resp = checkin_request_for(
        region,
        &format!("{CHECKIN_API_PREFIX}/checkin-activity-status"),
        account,
    )
    .await;
    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code == 0 || code == 200 {
        let data = resp.get("data").cloned().unwrap_or_else(|| json!({}));
        return json!({
            "ok": true,
            "todayCheckedIn": data.get("today_checked_in")
                .or_else(|| data.get("todayCheckedIn"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            "raw": data,
        });
    }
    let resp2 = checkin_request_for(
        region,
        &format!("{CHECKIN_API_PREFIX}/checkin-status"),
        account,
    )
    .await;
    let code2 = resp2.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code2 == 0 || code2 == 200 {
        let data = resp2.get("data").cloned().unwrap_or_else(|| json!({}));
        return json!({
            "ok": true,
            "todayCheckedIn": data.get("today_checked_in")
                .or_else(|| data.get("todayCheckedIn"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            "raw": data,
        });
    }
    json!({
        "ok": false,
        "error": resp2.get("message")
            .or_else(|| resp2.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or(&format!("code={code2}"))
            .to_string(),
    })
}

/// 执行签到（CN）。
pub async fn perform_checkin(account: &Value) -> Value {
    perform_checkin_for(Region::Cn, account).await
}

/// 按 region 执行签到（POST daily-checkin）；服务端返回已签到提示按成功处理。
pub async fn perform_checkin_for(region: Region, account: &Value) -> Value {
    if region == Region::Global {
        return json!({"ok": false, "error": UNSUPPORTED_MESSAGE});
    }
    let resp =
        checkin_request_for(region, &format!("{CHECKIN_API_PREFIX}/daily-checkin"), account).await;
    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code == 0 || code == 200 {
        return json!({"ok": true, "raw": resp.get("data").cloned().unwrap_or_else(|| json!({}))});
    }
    let msg = resp
        .get("message")
        .or_else(|| resp.get("msg"))
        .and_then(|v| v.as_str())
        .unwrap_or(&format!("code={code}"))
        .to_string();
    if msg.contains("已签到") || msg.to_lowercase().contains("repeat") {
        return json!({"ok": true, "already": true, "message": msg});
    }
    json!({"ok": false, "error": msg})
}

fn decide_from_status(status: &Value) -> StatusDecision {
    if status.get("ok").and_then(Value::as_bool) != Some(true) {
        return StatusDecision::Error(
            status
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("查询签到状态失败")
                .to_string(),
        );
    }
    if status.get("todayCheckedIn").and_then(Value::as_bool) == Some(true) {
        StatusDecision::Already
    } else {
        StatusDecision::Submit
    }
}

/// 对单个账号执行完整签到流程（CN）。
pub async fn checkin_account(account: &Value) -> Value {
    checkin_account_for(Region::Cn, account).await
}

/// 按 region 对单个账号执行完整签到流程：惰性刷新 → 查状态 → 未签到时提交 → 写提交日志。
pub async fn checkin_account_for(region: Region, account: &Value) -> Value {
    if region == Region::Global {
        return json!({"result": "unsupported", "error": UNSUPPORTED_MESSAGE});
    }
    let Some(_account_guard) = AccountRunGuard::try_acquire(account) else {
        return json!({"result": "error", "error": "该账号正在签到，请稍后再试"});
    };
    let cfg = load_checkin_config();
    let acc = ensure_fresh_token_for(region, account.clone(), &cfg).await;
    let status = get_checkin_status_for(region, &acc).await;
    match decide_from_status(&status) {
        StatusDecision::Already => return json!({"result": "already"}),
        StatusDecision::Error(error) => {
            return json!({"result": "error", "error": error});
        }
        StatusDecision::Submit => {}
    }

    // Only this branch submits daily-checkin, so only its outcome is eligible
    // for the sign-in log.
    let entry = json!({
        "ts": now_ms(),
        "accountId": acc.get("id").cloned().unwrap_or(Value::Null),
        "email": account_display_name(&acc),
    });
    let res = perform_checkin_for(region, &acc).await;
    let result = if res.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        if res.get("already").and_then(|v| v.as_bool()) == Some(true) {
            "already"
        } else {
            "success"
        }
    } else {
        "error"
    };
    let error = if result == "error" {
        res.get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };
    let mut entry_map = json!({
        "result": result,
        "ts": entry["ts"],
        "accountId": entry["accountId"],
        "email": entry["email"],
    });
    if let Some(e) = error.clone() {
        entry_map["error"] = json!(e);
    }
    add_checkin_log(&entry_map);
    json!({"result": result, "error": error})
}

pub fn date_str(ts_ms: Option<i64>) -> String {
    let dt = Local::now();
    if let Some(ms) = ts_ms {
        let secs = ms / 1000;
        chrono::DateTime::from_timestamp(secs, 0)
            .map(|d| d.with_timezone(&Local).format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| dt.format("%Y-%m-%d").to_string())
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

/// 执行一轮自动签到（CN）。
pub async fn run_checkin_cycle(mode: CheckinCycleMode) -> Value {
    run_checkin_cycle_for(Region::Cn, mode).await
}

/// 按 region 执行一轮自动签到。启动与周期轮次均逐账号查询服务端状态。
///
/// 并发锁防止与手动签到/上一轮重复运行。
pub async fn run_checkin_cycle_for(region: Region, _mode: CheckinCycleMode) -> Value {
    // Global 不支持签到是**静态事实**，必须先判 region：否则 CN 周期持有全局运行标志时，
    // Global 请求会在到达 region 判断前被误报为 skipped/already_running（违反 PRD G1
    // 「两版互不污染」）。Global 不参与「同一时刻只跑一个签到周期」的竞争。
    if region == Region::Global {
        return json!({"status": "unsupported", "reason": UNSUPPORTED_MESSAGE});
    }
    let Some(_guard) = RunFlagGuard::try_acquire(&CHECKIN_RUNNING) else {
        return json!({"status": "skipped", "reason": "already_running"});
    };
    let cfg = load_checkin_config();
    if cfg.get("enabled").and_then(|v| v.as_bool()) != Some(true) {
        return json!({"status": "disabled"});
    }
    let accounts = load_accounts_for(region);
    if accounts.is_empty() {
        return json!({"status": "no_accounts"});
    }
    let mut summary = json!({"status": "ok", "accounts": []});
    for acc in accounts {
        let result = checkin_account_for(region, &acc).await;
        summary["accounts"].as_array_mut().unwrap().push(json!({
            "email": account_display_name(&acc),
            "result": result.get("result").cloned().unwrap_or(Value::Null),
            "error": result.get("error").cloned().unwrap_or(Value::Null),
        }));
    }
    summary
}

/// True when every stored account has a today's log of `success` or `already`（CN）。
pub fn all_accounts_checked_in_today() -> bool {
    all_accounts_checked_in_today_for(Region::Cn)
}

/// 按 region 判断是否全部账号今日已签到。
///
/// Empty account list is false so the tray keeps offering 一键签到.
pub fn all_accounts_checked_in_today_for(region: Region) -> bool {
    accounts_checked_in_today(&load_accounts_for(region), &load_checkin_logs(), &date_str(None))
}

pub fn accounts_checked_in_today(accounts: &[Value], logs: &[Value], today: &str) -> bool {
    if accounts.is_empty() {
        return false;
    }
    accounts.iter().all(|account| {
        let Some(id) = account.get("id").and_then(Value::as_str) else {
            return false;
        };
        latest_today_result(logs, id, today)
            .map(|result| result == "success" || result == "already")
            .unwrap_or(false)
    })
}

fn latest_today_result<'a>(logs: &'a [Value], account_id: &str, today: &str) -> Option<&'a str> {
    logs.iter()
        .rev()
        .find(|entry| {
            entry.get("accountId").and_then(Value::as_str) == Some(account_id)
                && date_str(entry.get("ts").and_then(Value::as_i64)) == today
        })
        .and_then(|entry| entry.get("result").and_then(Value::as_str))
}

/// 对全部账号立即签到（CN，前端一键签到）。
pub async fn run_checkin_all() -> Value {
    run_checkin_all_for(Region::Cn).await
}

/// 按 region 对全部账号立即签到（前端一键签到）。
pub async fn run_checkin_all_for(region: Region) -> Value {
    // 同 run_checkin_cycle_for：Global 先返回 unsupported，不参与全局运行标志竞争，
    // 保证 CN 正在签到时不把 Global 误报成 skipped/already_running（PRD G1）。
    if region == Region::Global {
        return json!({"accounts": [], "status": "unsupported", "reason": UNSUPPORTED_MESSAGE});
    }
    let Some(_guard) = RunFlagGuard::try_acquire(&CHECKIN_RUNNING) else {
        return json!({"accounts": [], "status": "skipped", "reason": "already_running"});
    };
    let accounts = load_accounts_for(region);
    let mut results: Vec<Value> = Vec::new();
    for acc in accounts {
        let r = checkin_account_for(region, &acc).await;
        results.push(json!({
            "accountId": acc.get("id").cloned().unwrap_or(Value::Null),
            "email": account_display_name(&acc),
            "result": r.get("result").cloned().unwrap_or(Value::Null),
            "error": r.get("error").cloned().unwrap_or(Value::Null),
        }));
    }
    json!({"accounts": results})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归上游 issue #94：信封凭据的签到请求应在入口短路并返回可读错误，
    /// 不发出空 `Bearer`（此前会被网关 401 后把整页 HTML 回显到界面）。
    ///
    /// 可证伪：删掉 `checkin_request_for` 开头那段短路，本用例会真的去请求
    /// `www.codebuddy.cn/whatever`（或直接超时/报错），`code` 不再是 `-2`、
    /// `message` 里也不会有「加密信封」⇒ 两条断言都红。
    #[tokio::test]
    async fn envelope_credentials_short_circuit_before_request() {
        let account = json!({
            "id": "envelope-only",
            "variant": "cn",
            "access_token": {"$wbEncrypted": 1, "envelope": "…"},
            "refresh_token": {"$wbEncrypted": 1, "envelope": "…"},
        });
        let resp = checkin_request_for(Region::Cn, "/whatever", &account).await;
        assert_eq!(resp["code"], -2, "应在发请求之前短路：{resp}");
        let message = resp["message"].as_str().expect("message 应为字符串");
        assert!(message.contains("加密信封"), "文案应可读：{message}");

        // 阳性对照：明文凭据不得被这道护栏判定。直接问**同一个**判定函数，
        // 避免在单测里真的发网络请求（断言的是同一个谓词，不是另写一份判据）。
        assert!(
            envelope_token_error(&json!({"id": "plain", "access_token": "AT"})).is_none(),
            "明文凭据不得被信封护栏拦下"
        );
    }

    /// 签到模块的运行标志（`CHECKIN_RUNNING` / `CHECKIN_ACCOUNTS_RUNNING`）是**进程级
    /// 全局**的，cargo test 默认多线程并发跑用例时会互相污染：
    /// `manual_all_reports_busy_when_cycle_is_running` 持有 `CHECKIN_RUNNING` 期间，
    /// 另一个用例调用 `run_checkin_all_for` 会先撞上这把锁而返回 `skipped`，
    /// 而非期望的 `unsupported`——`--test-threads=1` 全绿、默认并发时随机红。
    ///
    /// 因此触及全局签到状态的用例必须先取这把测试专用锁串行执行。
    /// 用 `into_inner()` 吞掉中毒：单个用例 panic 不应连带其它用例全部失败。
    static CHECKIN_GLOBAL_STATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_checkin_global_state() -> std::sync::MutexGuard<'static, ()> {
        CHECKIN_GLOBAL_STATE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn checked_in_status_returns_already_without_submission() {
        assert_eq!(
            decide_from_status(&json!({"ok": true, "todayCheckedIn": true})),
            StatusDecision::Already
        );
    }

    #[test]
    fn failed_status_returns_error_without_submission() {
        assert_eq!(
            decide_from_status(&json!({"ok": false, "error": "offline"})),
            StatusDecision::Error("offline".to_string())
        );
    }

    #[test]
    fn unchecked_status_is_the_only_path_to_submission() {
        assert_eq!(
            decide_from_status(&json!({"ok": true, "todayCheckedIn": false})),
            StatusDecision::Submit
        );
    }

    #[test]
    fn same_account_cannot_acquire_two_operation_guards() {
        let account = json!({"id": "checkin-guard-test-account"});
        let first = AccountRunGuard::try_acquire(&account).expect("first operation acquires guard");
        assert!(AccountRunGuard::try_acquire(&account).is_none());
        drop(first);
        assert!(AccountRunGuard::try_acquire(&account).is_some());
    }

    #[tokio::test]
    async fn manual_all_reports_busy_when_cycle_is_running() {
        let _state = lock_checkin_global_state();
        let _cycle_guard =
            RunFlagGuard::try_acquire(&CHECKIN_RUNNING).expect("test acquires cycle guard");
        let result = run_checkin_all().await;

        assert_eq!(result["accounts"], json!([]));
        assert_eq!(result["status"], "skipped");
        assert_eq!(result["reason"], "already_running");
    }

    #[test]
    fn is_unauthorized_detects_code() {
        assert!(is_unauthorized(&json!({"code": 401})));
        assert!(is_unauthorized(&json!({"code": 403})));
        assert!(!is_unauthorized(&json!({"code": 0})));
    }

    #[test]
    fn checked_in_today_requires_every_account() {
        let accounts = vec![json!({"id": "a"}), json!({"id": "b"})];
        let logs = vec![
            json!({"accountId": "a", "result": "success", "ts": 1_700_000_000_000_i64}),
            json!({"accountId": "b", "result": "already", "ts": 1_700_000_100_000_i64}),
        ];
        let today = date_str(Some(1_700_000_000_000));
        assert!(accounts_checked_in_today(&accounts, &logs, &today));
    }

    #[test]
    fn checked_in_today_false_when_one_failed_last() {
        let accounts = vec![json!({"id": "a"})];
        let logs = vec![
            json!({"accountId": "a", "result": "success", "ts": 1_700_000_000_000_i64}),
            json!({"accountId": "a", "result": "error", "ts": 1_700_000_200_000_i64}),
        ];
        let today = date_str(Some(1_700_000_200_000));
        assert!(!accounts_checked_in_today(&accounts, &logs, &today));
    }

    #[test]
    fn checked_in_today_false_when_empty_or_missing() {
        assert!(!accounts_checked_in_today(&[], &[], "2026-08-19"));
        let accounts = vec![json!({"id": "a"})];
        assert!(!accounts_checked_in_today(&accounts, &[], "2026-08-19"));
    }

    #[tokio::test]
    async fn global_checkin_is_independently_degraded() {
        let _state = lock_checkin_global_state();
        // 模拟「CN 签到进行中」：全局运行标志已被 CN 周期持有。Global 必须在触碰
        // 该标志**之前**短路，否则会被误报成 skipped/already_running（违反 PRD G1）。
        // 这里显式持有标志而非依赖并发时序，使该交错确定复现——一旦产品代码把标志
        // 检查挪到 region 判断之前，本用例必然变红；若只靠并发去撞，则会被上面的
        // 串行锁排除掉，测试照样绿。
        let _cycle_guard =
            RunFlagGuard::try_acquire(&CHECKIN_RUNNING).expect("test holds cycle guard");
        let account = json!({"id": "g-1", "access_token": "t"});
        assert_eq!(
            checkin_account_for(Region::Global, &account).await["result"],
            json!("unsupported")
        );
        assert_eq!(
            run_checkin_all_for(Region::Global).await["status"],
            json!("unsupported")
        );
        assert_eq!(
            run_checkin_cycle_for(Region::Global, CheckinCycleMode::PeriodicRecovery).await["status"],
            json!("unsupported")
        );
        assert_eq!(
            get_checkin_status_for(Region::Global, &account).await["ok"],
            json!(false)
        );
    }
}
