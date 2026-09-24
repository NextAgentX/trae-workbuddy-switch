//! WorkBuddy 官方请求用量投影。
//!
//! 这里负责请求最近 31 个自然日、处理分页、校验和归一化明细，并只向上层
//! 暴露统计字段与有限的请求摘要。投影会写入本地缓存，避免每次打开统计页
//! 都打官方用量接口；上游可能携带的 prompt/input 等字段永远不会被复制。

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde_json::{json, Map, Value};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::modules::account::{account_display_name, get_str};
use crate::modules::config::{atomic_write, store_dir};
use crate::modules::credits::authenticated_post_for;
use crate::modules::region::{official_usage_cache_file_for, region_spec, Region};

/// 官方请求用量端点路径（两版共用同一路径，**只有主机按 region 变**）。
const OFFICIAL_USAGE_PATH: &str = "/billing/meter/get-user-request-usage";

/// CN 官方请求用量端点（既有值，**零变化**）。
///
/// 主机是 `www.workbuddy.cn`（= CN 的用户中心 web 端点，与
/// `credits::new_resource_endpoint` 对 `workbuddy.cn` 域账号的选择一致），
/// 不是 `billing_base`（`www.codebuddy.cn`）。
pub const OFFICIAL_USAGE_URL: &str =
    "https://www.workbuddy.cn/billing/meter/get-user-request-usage";

/// 按 region 取官方请求用量端点。
///
/// **为什么必须分主机**：该端点是「用户中心」域的接口，网关按 realm 鉴权。
/// 实测证据（本机真实数据，2026-09-17）：
/// - global 账号的资源查询走 `https://www.workbuddy.ai/billing/meter/*` **成功**
///   （`credit_usage_snapshots.json` 里两个 global 账号各有 8 条真实余额记录）；
/// - 但同一批 global 账号打 `www.workbuddy.cn` 的用量端点时，
///   `official_usage_cache.json` 里留下的是 **APISIX 网关的 401**
///   （`401 Authorization Required / openresty / APISIX`）。
///
/// 即：请求头形态在 `www.workbuddy.ai` 与 `www.workbuddy.cn` 上**都**能过网关
/// （资源查询两边都成功），所以那个 401 不是缺头，而是**把 global 凭据打到了 CN 用户中心**。
/// 因此 Global 用该 region 的基址，CN 保持既有值。
///
/// ⚠️ 仍未直接观测到 Global 端点的 200（本机没有可用的 global 会话），此判断由上面
/// 两条观测 + 项目自身约定（`region_spec` / `new_resource_endpoint_for`）推出。
/// 万一路径在 .ai 域不存在，会得到 404 → `status: unavailable`，与改动前
/// （401 → unavailable）**用户可见行为相同**，故这是一个无下行风险的修正。
pub fn official_usage_url(region: Region) -> String {
    match region {
        Region::Cn => OFFICIAL_USAGE_URL.to_string(),
        Region::Global => format!(
            "{}{OFFICIAL_USAGE_PATH}",
            region_spec(Region::Global).billing_base
        ),
    }
}

/// 官方接口单页条数（分页抓取用）。
pub const OFFICIAL_USAGE_PAGE_SIZE: usize = 3_000;

/// 单账号向上层下发的**请求明细**条数上限（按请求时间取最近的 N 条）。
///
/// 这个值同时决定「官方用量缓存文件」与「统计接口单次响应」的载荷规模，所以必须有限：
/// 实测每行明细约 200 B，3_000 行 ≈ 600 KB/账号，属可接受区间。
///
/// **为什么从 100 放宽到 3_000**：100 条对重度用户只覆盖最近一两天，明细表里根本看不到
/// 「哪几笔请求最贵」；而聚合口径（`models` / `daily`）本来就是**全量**的，于是出现
/// 「合计用了全部 N 条请求，列表却只给 100 条」的口径不一致。放宽后与官方单页上限同值，
/// 覆盖绝大多数账号 31 天的全部请求；真的超过时仍以 `detailTruncated` 如实上报，
/// 前端的「仅展示最近 N 条」提示不会说谎。
pub const OFFICIAL_USAGE_DETAIL_LIMIT: usize = 3_000;

const OFFICIAL_USAGE_MAX_PAGES: usize = 100;

/// 官方用量采集结果的进程内记忆，**按 region 分家**。
///
/// 与 [`official_usage_cache_file_for`] 同理：混用一份记忆会让 CN 视图命中
/// Global 的采集结果（payload 自带 `accounts[]`，前端拿它当账号列表）。
fn official_usage_memory(region: Region) -> &'static Mutex<Option<Value>> {
    static CN: Mutex<Option<Value>> = Mutex::new(None);
    static GLOBAL: Mutex<Option<Value>> = Mutex::new(None);
    match region {
        Region::Cn => &CN,
        Region::Global => &GLOBAL,
    }
}

fn official_usage_fetch_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn take_object_fields(value: &Value, keys: &[&str]) -> Map<String, Value> {
    let mut out = Map::new();
    let Some(object) = value.as_object() else {
        return out;
    };
    for key in keys {
        if let Some(field) = object.get(*key) {
            out.insert((*key).to_string(), field.clone());
        }
    }
    out
}

fn sanitize_models(value: Option<&Value>) -> Value {
    Value::Array(
        value
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|item| {
                Value::Object(take_object_fields(
                    item,
                    &["model", "requestCount", "credit"],
                ))
            })
            .collect(),
    )
}

fn sanitize_daily(value: Option<&Value>) -> Value {
    Value::Array(
        value
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|item| {
                let mut point = take_object_fields(item, &["date", "usage"]);
                point.insert("models".into(), sanitize_models(item.get("models")));
                Value::Object(point)
            })
            .collect(),
    )
}

fn sanitize_requests(value: Option<&Value>) -> Value {
    Value::Array(
        value
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|item| {
                Value::Object(take_object_fields(
                    item,
                    &[
                        "accountId",
                        "accountName",
                        "requestId",
                        "credit",
                        "model",
                        "client",
                        "requestTime",
                    ],
                ))
            })
            .collect(),
    )
}

fn sanitize_accounts(value: Option<&Value>) -> Value {
    Value::Array(
        value
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|item| {
                let mut account = take_object_fields(
                    item,
                    &[
                        "accountId",
                        "accountName",
                        "ok",
                        "requestCount",
                        "detailTruncated",
                        "usageToday",
                        "usage7Days",
                        "usageThisMonth",
                        "error",
                        "reportedTotal",
                        "fetchedCount",
                    ],
                );
                account.insert("models".into(), sanitize_models(item.get("models")));
                account.insert("daily".into(), sanitize_daily(item.get("daily")));
                Value::Object(account)
            })
            .collect(),
    )
}

fn sanitize_cached_payload(value: &Value) -> Option<Value> {
    let status = value.get("status")?.as_str()?;
    if !matches!(status, "complete" | "partial" | "unavailable") {
        return None;
    }
    let mut payload = take_object_fields(
        value,
        &[
            "status",
            "rangeStart",
            "rangeEnd",
            "summary",
            "detailLimitPerAccount",
            "collectedAt",
            "errors",
        ],
    );
    payload.insert("daily".into(), sanitize_daily(value.get("daily")));
    payload.insert("models".into(), sanitize_models(value.get("models")));
    payload.insert("accounts".into(), sanitize_accounts(value.get("accounts")));
    payload.insert("requests".into(), sanitize_requests(value.get("requests")));
    Some(Value::Object(payload))
}

fn parse_official_usage_cache(text: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(text).ok()?;
    let payload = value.get("payload").unwrap_or(&value);
    sanitize_cached_payload(payload)
}

fn load_official_usage_cache_from(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    parse_official_usage_cache(&text)
}

fn save_official_usage_cache_to(path: &Path, payload: &Value) {
    let body = json!({ "payload": payload });
    let content = serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string());
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(store_dir);
    let _ = std::fs::create_dir_all(parent).and_then(|_| atomic_write(path, &content));
}

fn remembered_official_usage_for(region: Region) -> Option<Value> {
    let memory = official_usage_memory(region);
    if let Ok(guard) = memory.lock() {
        if let Some(cached) = guard.as_ref() {
            return Some(cached.clone());
        }
    }
    let loaded = load_official_usage_cache_from(&official_usage_cache_file_for(region))?;
    if let Ok(mut guard) = memory.lock() {
        *guard = Some(loaded.clone());
    }
    Some(loaded)
}

fn remember_official_usage_for(region: Region, payload: &Value) {
    if let Ok(mut guard) = official_usage_memory(region).lock() {
        *guard = Some(payload.clone());
    }
    save_official_usage_cache_to(&official_usage_cache_file_for(region), payload);
}

/// 统计页默认读缓存；`refresh = true` 时才重新请求官方用量接口（CN 薄包装）。
pub async fn official_usage_for_statistics(accounts: &[Value], at_ms: i64, refresh: bool) -> Value {
    official_usage_for_statistics_for(Region::Cn, accounts, at_ms, refresh).await
}

/// 按 region 取官方用量投影（缓存 + 采集都按 region 隔离）。
///
/// **为什么必须带 region**：这条链路会（在 token 陈旧时）刷新账号并把结果写回
/// **该 region 的账号库**。旧实现固定走 CN 的 [`authenticated_post`]，
/// 于是 Global 视图会把 global 账号拿到 CN 的 billing 基址去换 token，
/// 失败后连 `needs_relogin` 标记一起写进 `accounts.json`（CN 账号库）——
/// global 账号因此出现在 CN 视图里，违反 PRD G1「两版互不污染」。
///
/// [`authenticated_post`]: crate::modules::credits::authenticated_post
pub async fn official_usage_for_statistics_for(
    region: Region,
    accounts: &[Value],
    at_ms: i64,
    refresh: bool,
) -> Value {
    if !refresh {
        if let Some(cached) = remembered_official_usage_for(region) {
            return cached;
        }
    }
    let _guard = official_usage_fetch_lock().lock().await;
    if !refresh {
        if let Some(cached) = remembered_official_usage_for(region) {
            return cached;
        }
    }
    let usage = collect_official_usage_for(region, accounts, at_ms).await;
    remember_official_usage_for(region, &usage);
    usage
}

#[derive(Clone, Debug)]
struct RequestRow {
    request_id: String,
    credit: f64,
    model: String,
    client: String,
    request_time: String,
    request_ts: i64,
    date: NaiveDate,
}

#[derive(Clone, Debug)]
struct OfficialPage {
    total: usize,
    raw_len: usize,
    rows: Vec<RequestRow>,
}

#[derive(Clone, Debug)]
struct AccountFetch {
    rows: Vec<RequestRow>,
    reported_total: usize,
    fetched_raw: usize,
}

fn non_empty(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
}

fn parse_number(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(value)) => value.as_f64(),
        Some(Value::String(value)) => value.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn response_code(response: &Value) -> Option<i64> {
    response.get("code").and_then(|value| {
        value.as_i64().or_else(|| {
            value
                .as_str()
                .and_then(|text| text.trim().parse::<i64>().ok())
        })
    })
}

fn error_message(response: &Value) -> String {
    let upstream_message = response
        .get("message")
        .or_else(|| response.get("msg"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(|message| message.chars().take(160).collect::<String>());
    match response_code(response) {
        Some(code) if code == -1 => "官方请求失败（网络或服务不可达）".to_string(),
        Some(code) => upstream_message
            .map(|message| format!("官方请求失败（code={code}）：{message}"))
            .unwrap_or_else(|| format!("官方请求失败（code={code}）")),
        None => "官方响应格式无效".to_string(),
    }
}

fn local_datetime_to_parts(value: NaiveDateTime) -> Option<(NaiveDate, i64)> {
    Local
        .from_local_datetime(&value)
        .single()
        .map(|date| (date.date_naive(), date.timestamp_millis()))
}

fn timestamp_to_parts(ts: i64) -> Option<(NaiveDate, i64)> {
    Local
        .timestamp_millis_opt(ts)
        .single()
        .map(|date| (date.date_naive(), ts))
}

fn parse_request_time(value: Option<&Value>) -> Option<(NaiveDate, i64, String)> {
    let value = value?;
    if let Some(number) = parse_number(Some(value)) {
        if !number.is_finite() {
            return None;
        }
        let ts = if number.abs() < 10_000_000_000.0 {
            (number * 1000.0).round() as i64
        } else {
            number.round() as i64
        };
        let (date, ts) = timestamp_to_parts(ts)?;
        return Some((date, ts, ts.to_string()));
    }

    let text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(text) {
        let ts = parsed.timestamp_millis();
        let date = Local.timestamp_millis_opt(ts).single()?.date_naive();
        return Some((date, ts, text.to_string()));
    }
    for pattern in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(text, pattern) {
            let (date, ts) = local_datetime_to_parts(parsed)?;
            return Some((date, ts, text.to_string()));
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        let parsed = date.and_hms_opt(12, 0, 0)?;
        let (date, ts) = local_datetime_to_parts(parsed)?;
        return Some((date, ts, text.to_string()));
    }
    None
}

fn compact_string(value: Option<&Value>, fallback: &str) -> String {
    let text = non_empty(value).unwrap_or_else(|| fallback.to_string());
    text.chars().take(160).collect()
}

fn normalize_row(value: &Value) -> Option<RequestRow> {
    let credit = parse_number(value.get("credit"))?;
    if !credit.is_finite() || credit < 0.0 {
        return None;
    }
    let (date, request_ts, request_time) = parse_request_time(
        value
            .get("requestTime")
            .or_else(|| value.get("request_time")),
    )?;
    Some(RequestRow {
        request_id: compact_string(
            value.get("requestId").or_else(|| value.get("request_id")),
            "unknown",
        ),
        credit,
        model: compact_string(value.get("model"), "—"),
        client: compact_string(value.get("client"), "—"),
        request_time,
        request_ts,
        date,
    })
}

fn parse_page(response: &Value) -> Result<OfficialPage, String> {
    if response_code(response) != Some(0) && response_code(response) != Some(200) {
        return Err(error_message(response));
    }
    let Some(data) = response.get("data").and_then(Value::as_object) else {
        return Err("官方响应格式无效".to_string());
    };
    let Some(raw_rows) = data.get("data").and_then(Value::as_array) else {
        return Err("官方响应格式无效".to_string());
    };
    let total = data
        .get("total")
        .and_then(|value| {
            value.as_u64().or_else(|| {
                value
                    .as_str()
                    .and_then(|text| text.trim().parse::<u64>().ok())
            })
        })
        .unwrap_or(raw_rows.len() as u64)
        .min(usize::MAX as u64) as usize;
    Ok(OfficialPage {
        total,
        raw_len: raw_rows.len(),
        rows: raw_rows.iter().filter_map(normalize_row).collect(),
    })
}

fn should_fetch_next_page(
    page_number: usize,
    total: usize,
    fetched_raw: usize,
    page_len: usize,
) -> bool {
    page_len > 0 && fetched_raw < total && page_number < OFFICIAL_USAGE_MAX_PAGES
}

fn local_date_at(ts: i64) -> NaiveDate {
    Local
        .timestamp_millis_opt(ts)
        .single()
        .map(|date| date.date_naive())
        .unwrap_or_else(|| Local::now().date_naive())
}

async fn fetch_account_usage_for(
    region: Region,
    account: &Value,
    range_start: NaiveDate,
    range_end: NaiveDate,
) -> Result<AccountFetch, String> {
    let start_time = format!("{range_start} 00:00:00");
    let end_time = format!("{range_end} 23:59:59");
    let mut page_number = 1;
    let mut fetched_raw = 0;
    let mut reported_total = 0;
    let mut rows = Vec::new();
    let mut seen_request_ids = HashSet::new();

    loop {
        let response = authenticated_post_for(
            region,
            account,
            &official_usage_url(region),
            json!({
                "startTime": start_time,
                "endTime": end_time,
                "pageNum": page_number,
                "pageSize": OFFICIAL_USAGE_PAGE_SIZE,
            }),
        )
        .await;
        let page = parse_page(&response)?;
        reported_total = reported_total.max(page.total);
        fetched_raw += page.raw_len;
        for row in page
            .rows
            .into_iter()
            .filter(|row| row.date >= range_start && row.date <= range_end)
        {
            if seen_request_ids.insert((row.request_id.clone(), row.request_ts)) {
                rows.push(row);
            }
        }

        if !should_fetch_next_page(page_number, reported_total, fetched_raw, page.raw_len) {
            if page.raw_len == 0 && fetched_raw < reported_total {
                return Err("官方用量分页数据不完整".to_string());
            }
            if page_number >= OFFICIAL_USAGE_MAX_PAGES && fetched_raw < reported_total {
                return Err("官方用量分页超过安全上限".to_string());
            }
            break;
        }
        page_number += 1;
    }

    Ok(AccountFetch {
        rows,
        reported_total,
        fetched_raw,
    })
}

fn aggregate_rows(
    rows: &[RequestRow],
    today: NaiveDate,
) -> (f64, f64, f64, HashMap<NaiveDate, f64>) {
    let mut today_usage = 0.0;
    let mut week_usage = 0.0;
    let mut month_usage = 0.0;
    let mut daily = HashMap::new();
    for row in rows {
        let distance = (today - row.date).num_days();
        if distance == 0 {
            today_usage += row.credit;
        }
        if (0..7).contains(&distance) {
            week_usage += row.credit;
        }
        if row.date.year() == today.year() && row.date.month() == today.month() {
            month_usage += row.credit;
        }
        *daily.entry(row.date).or_insert(0.0) += row.credit;
    }
    (today_usage, week_usage, month_usage, daily)
}

fn model_name(model: &str) -> String {
    let model = model.trim();
    if model.is_empty() || model == "—" {
        "未知模型".to_string()
    } else {
        model.to_string()
    }
}

fn add_model_usage(usage: &mut HashMap<String, (usize, f64)>, row: &RequestRow) {
    let entry = usage.entry(model_name(&row.model)).or_insert((0, 0.0));
    entry.0 += 1;
    entry.1 += row.credit;
}

fn model_usage_values(usage: HashMap<String, (usize, f64)>) -> Vec<Value> {
    let mut models: Vec<(String, (usize, f64))> = usage.into_iter().collect();
    models.sort_by(
        |(left_name, (left_count, left_credit)), (right_name, (right_count, right_credit))| {
            right_credit
                .partial_cmp(left_credit)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right_count.cmp(left_count))
                .then_with(|| left_name.cmp(right_name))
        },
    );
    models
        .into_iter()
        .map(|(model, (request_count, credit))| {
            json!({
                "model": model,
                "requestCount": request_count,
                "credit": credit,
            })
        })
        .collect()
}

fn aggregate_models(rows: &[RequestRow]) -> Vec<Value> {
    let mut usage = HashMap::new();
    for row in rows {
        add_model_usage(&mut usage, row);
    }
    model_usage_values(usage)
}

/// 生成 range_start..=range_end 的逐日序列（无数据的天补 0，模型聚合缺省为空）。
fn daily_series(
    totals: &HashMap<NaiveDate, f64>,
    models: &HashMap<NaiveDate, HashMap<String, (usize, f64)>>,
    range_start: NaiveDate,
    range_end: NaiveDate,
) -> Vec<Value> {
    let mut daily = Vec::new();
    let mut date = range_start;
    while date <= range_end {
        daily.push(json!({
            "date": date.format("%Y-%m-%d").to_string(),
            "usage": totals.get(&date).copied().unwrap_or(0.0),
            "models": models
                .get(&date)
                .map(|models| model_usage_values(models.clone()))
                .unwrap_or_default(),
        }));
        date += Duration::days(1);
    }
    daily
}

fn request_value(account_id: &str, account_name: &str, row: &RequestRow) -> Value {
    json!({
        "accountId": account_id,
        "accountName": account_name,
        "requestId": row.request_id,
        "credit": row.credit,
        "model": row.model,
        "client": row.client,
        "requestTime": row.request_time,
    })
}

/// 明细行按**请求时间倒序**取最近 `limit` 条。
///
/// 只有**明细**受 `limit` 约束；`models` / `daily` 聚合走全量 `rows`。
/// 两处口径不同，正是「合计是全部 N 条、列表却只给 100 条」的成因，故此处单列出来，
/// 让「上限足够时必须一条不丢」这个边界可以被单测钉住。
fn recent_detail_rows(rows: &[RequestRow], limit: usize) -> Vec<RequestRow> {
    let mut sorted = rows.to_vec();
    sorted.sort_by_key(|row| Reverse(row.request_ts));
    sorted.truncate(limit);
    sorted
}

/// 查询全部当前账号并生成官方请求用量投影（CN 薄包装）。
pub async fn collect_official_usage(accounts: &[Value], at_ms: i64) -> Value {
    collect_official_usage_for(Region::Cn, accounts, at_ms).await
}

/// 按 region 查询传入账号并生成官方请求用量投影。
///
/// 逐账号请求走 [`authenticated_post_for`]（带 region）——这一点是本模块
/// 「两版互不污染」的关键：该函数内部会在 token 陈旧时按 region 刷新，
/// 并把新账号写回**该 region 的账号库**。
pub async fn collect_official_usage_for(region: Region, accounts: &[Value], at_ms: i64) -> Value {
    let today = local_date_at(at_ms);
    let range_start = today - Duration::days(30);
    let range_end = today;
    let range_start_text = range_start.format("%Y-%m-%d").to_string();
    let range_end_text = range_end.format("%Y-%m-%d").to_string();

    let mut successful_accounts = 0usize;
    let mut account_rows = Vec::new();
    let mut errors = Vec::new();
    let mut daily_totals: HashMap<NaiveDate, f64> = HashMap::new();
    let mut daily_models: HashMap<NaiveDate, HashMap<String, (usize, f64)>> = HashMap::new();
    let mut account_daily_totals: HashMap<String, HashMap<NaiveDate, f64>> = HashMap::new();
    let mut account_daily_models: HashMap<
        String,
        HashMap<NaiveDate, HashMap<String, (usize, f64)>>,
    > = HashMap::new();
    let mut total_today = 0.0;
    let mut total_week = 0.0;
    let mut total_month = 0.0;
    let mut recent_requests: Vec<(i64, Value)> = Vec::new();
    let mut model_totals: HashMap<String, (usize, f64)> = HashMap::new();

    for (index, account) in accounts.iter().enumerate() {
        let account_id = get_str(account, "id").unwrap_or_else(|| format!("unknown-{index}"));
        let account_name = account_display_name(account);
        match fetch_account_usage_for(region, account, range_start, range_end).await {
            Ok(result) => {
                successful_accounts += 1;
                let (usage_today, usage_week, usage_month, daily) =
                    aggregate_rows(&result.rows, today);
                total_today += usage_today;
                total_week += usage_week;
                total_month += usage_month;
                for row in &result.rows {
                    add_model_usage(&mut model_totals, row);
                    // 每日按模型聚合（全量，不受 100 条明细限制）
                    add_model_usage(daily_models.entry(row.date).or_default(), row);
                    // 单账号每日按模型聚合（同样不受明细条数限制）
                    add_model_usage(
                        account_daily_models
                            .entry(account_id.clone())
                            .or_default()
                            .entry(row.date)
                            .or_default(),
                        row,
                    );
                }
                for (date, amount) in daily {
                    *daily_totals.entry(date).or_insert(0.0) += amount;
                    *account_daily_totals
                        .entry(account_id.clone())
                        .or_default()
                        .entry(date)
                        .or_insert(0.0) += amount;
                }

                for row in recent_detail_rows(&result.rows, OFFICIAL_USAGE_DETAIL_LIMIT) {
                    recent_requests.push((
                        row.request_ts,
                        request_value(&account_id, &account_name, &row),
                    ));
                }
                account_rows.push(json!({
                    "accountId": account_id,
                    "accountName": account_name,
                    "ok": true,
                    "requestCount": result.reported_total,
                    "detailTruncated": result.reported_total > OFFICIAL_USAGE_DETAIL_LIMIT,
                    "usageToday": usage_today,
                    "usage7Days": usage_week,
                    "usageThisMonth": usage_month,
                    "error": Value::Null,
                    "reportedTotal": result.reported_total,
                    "fetchedCount": result.fetched_raw,
                    "models": aggregate_models(&result.rows),
                    "daily": daily_series(
                        account_daily_totals
                            .get(&account_id)
                            .unwrap_or(&HashMap::new()),
                        account_daily_models
                            .get(&account_id)
                            .unwrap_or(&HashMap::new()),
                        range_start,
                        range_end,
                    ),
                }));
            }
            Err(error) => {
                errors.push(json!({
                    "accountId": account_id,
                    "accountName": account_name,
                    "error": error,
                }));
                account_rows.push(json!({
                    "accountId": account_id,
                    "accountName": account_name,
                    "ok": false,
                    "requestCount": 0,
                    "detailTruncated": false,
                    "usageToday": Value::Null,
                    "usage7Days": Value::Null,
                    "usageThisMonth": Value::Null,
                    "error": errors.last().and_then(|item| item.get("error")).cloned().unwrap_or(Value::Null),
                    "reportedTotal": Value::Null,
                    "fetchedCount": 0,
                    "models": [],
                    "daily": [],
                }));
            }
        }
    }

    recent_requests.sort_by_key(|(ts, _)| Reverse(*ts));
    let requests: Vec<Value> = recent_requests
        .into_iter()
        .map(|(_, value)| value)
        .collect();

    let daily = daily_series(&daily_totals, &daily_models, range_start, range_end);

    let status = if accounts.is_empty() {
        "unavailable"
    } else if successful_accounts == accounts.len() {
        "complete"
    } else if successful_accounts > 0 {
        "partial"
    } else {
        "unavailable"
    };

    json!({
        "status": status,
        "rangeStart": range_start_text,
        "rangeEnd": range_end_text,
        "collectedAt": at_ms,
        "summary": {
            "usageToday": total_today,
            "usage7Days": total_week,
            "usageThisMonth": total_month,
        },
        "daily": daily,
        "accounts": account_rows,
        "requests": requests,
        "models": model_usage_values(model_totals),
        "detailLimitPerAccount": OFFICIAL_USAGE_DETAIL_LIMIT,
        "errors": errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_date(days_ago: i64, hour: u32) -> (NaiveDate, i64) {
        let date = Local::now().date_naive() - Duration::days(days_ago);
        let timestamp = Local
            .with_ymd_and_hms(date.year(), date.month(), date.day(), hour, 0, 0)
            .single()
            .expect("valid local test date")
            .timestamp_millis();
        (date, timestamp)
    }

    fn row(date: NaiveDate, ts: i64, credit: f64) -> RequestRow {
        RequestRow {
            request_id: "request-1".to_string(),
            credit,
            model: "model-a".to_string(),
            client: "client-a".to_string(),
            request_time: format!("{date} 12:00:00"),
            request_ts: ts,
            date,
        }
    }

    #[test]
    fn parses_success_page_and_does_not_copy_prompt_fields() {
        let page = parse_page(&json!({
            "code": 0,
            "data": {
                "total": "1",
                "data": [{
                    "requestId": "req-1",
                    "credit": "1.25",
                    "model": "model-a",
                    "client": "cli",
                    "requestTime": "2026-08-24 12:34:56",
                    "input": "do not expose this prompt",
                    "inputTrunc": "also secret"
                }]
            }
        }))
        .expect("valid official page");

        assert_eq!(page.total, 1);
        assert_eq!(page.raw_len, 1);
        assert_eq!(page.rows[0].credit, 1.25);
        let output = request_value("account-1", "one@example.com", &page.rows[0]);
        let text = output.to_string();
        assert!(!text.contains("do not expose"));
        assert!(!text.contains("inputTrunc"));
        assert_eq!(output["requestId"], "req-1");
    }

    #[test]
    fn malformed_rows_are_ignored_without_turning_into_zero_usage() {
        let page = parse_page(&json!({
            "code": 200,
            "data": {
                "total": 3,
                "data": [
                    {"requestId": "valid", "credit": 2, "requestTime": "2026-08-24 12:00:00"},
                    {"requestId": "negative", "credit": -1, "requestTime": "2026-08-24 12:00:00"},
                    {"requestId": "bad-time", "credit": 4, "requestTime": "not-a-date"}
                ]
            }
        }))
        .expect("page shape is valid");

        assert_eq!(page.raw_len, 3);
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].credit, 2.0);
    }

    #[test]
    fn pagination_stops_on_empty_short_total_and_page_limit() {
        assert!(!should_fetch_next_page(1, 10, 0, 0));
        assert!(should_fetch_next_page(1, 10, 2, 2));
        assert!(!should_fetch_next_page(
            1,
            2_000,
            2_000,
            OFFICIAL_USAGE_PAGE_SIZE
        ));
        assert!(should_fetch_next_page(
            1,
            6_000,
            OFFICIAL_USAGE_PAGE_SIZE,
            OFFICIAL_USAGE_PAGE_SIZE
        ));
        assert!(!should_fetch_next_page(
            OFFICIAL_USAGE_MAX_PAGES,
            usize::MAX,
            OFFICIAL_USAGE_PAGE_SIZE * OFFICIAL_USAGE_MAX_PAGES,
            OFFICIAL_USAGE_PAGE_SIZE,
        ));
    }

    #[test]
    fn aggregation_uses_positive_credits_and_local_today_windows() {
        let (today, today_ts) = local_date(0, 12);
        let (yesterday, yesterday_ts) = local_date(1, 12);
        let (last_month, last_month_ts) = local_date(40, 12);
        let rows = vec![
            row(today, today_ts, 1.5),
            row(yesterday, yesterday_ts, 2.0),
            row(last_month, last_month_ts, 5.0),
        ];

        let (today_usage, week_usage, month_usage, daily) = aggregate_rows(&rows, today);
        assert_eq!(today_usage, 1.5);
        assert_eq!(week_usage, 3.5);
        // 月初时“昨天”可能属于上月（甚至跨年），此时本月仅包含今天这条。
        let expected_month_usage = if yesterday.year() == today.year()
            && yesterday.month() == today.month()
        {
            3.5
        } else {
            1.5
        };
        assert_eq!(month_usage, expected_month_usage);
        assert_eq!(daily[&today], 1.5);
        assert_eq!(daily[&yesterday], 2.0);
    }

    #[test]
    fn model_aggregation_sorts_by_credit_and_groups_requests() {
        let (today, today_ts) = local_date(0, 12);
        let mut first = row(today, today_ts, 1.0);
        first.model = "model-a".to_string();
        let mut second = row(today, today_ts + 1, 4.0);
        second.model = "model-b".to_string();
        let mut third = row(today, today_ts + 2, 2.0);
        third.model = "model-a".to_string();

        let models = aggregate_models(&[first, second, third]);
        assert_eq!(models[0]["model"], "model-b");
        assert_eq!(models[0]["requestCount"], 1);
        assert_eq!(models[0]["credit"], 4.0);
        assert_eq!(models[1]["model"], "model-a");
        assert_eq!(models[1]["requestCount"], 2);
        assert_eq!(models[1]["credit"], 3.0);
    }

    /// 明细上限的**边界行为**：上限足够时一条不丢，上限不足时只留最近的 N 条。
    ///
    /// 旧实现是内联 `take(100)`：重度用户 31 天有几千条请求，明细表里只看得见最近一两天，
    /// 而 `models` / `daily` 聚合是全量的 —— 口径不一致正是本次修复的动因。
    #[test]
    fn recent_detail_rows_keep_the_newest_up_to_the_limit() {
        let (today, today_ts) = local_date(0, 12);
        let rows: Vec<RequestRow> = (0..150)
            .map(|index| row(today, today_ts + index as i64, index as f64))
            .collect();

        // 上限足够：一条不丢，且最近的在最前。
        let all = recent_detail_rows(&rows, 150);
        assert_eq!(all.len(), 150, "上限足够时必须返回全部明细");
        assert_eq!(all[0].request_ts, today_ts + 149, "必须按请求时间倒序");
        assert_eq!(all[149].request_ts, today_ts);

        // 上限不足：只保留最近的 N 条。
        let capped = recent_detail_rows(&rows, 3);
        assert_eq!(capped.len(), 3);
        assert_eq!(capped[0].request_ts, today_ts + 149);
        assert_eq!(capped[2].request_ts, today_ts + 147);
    }

    /// 明细上限不得小于官方单页上限。
    ///
    /// 改回 100 本用例即红：「一页都装不下」对重度用户必然截断，
    /// 与全量的聚合口径再次不一致（即用户报障的原状）。
    #[test]
    fn detail_limit_covers_a_full_official_page() {
        assert!(
            OFFICIAL_USAGE_DETAIL_LIMIT >= OFFICIAL_USAGE_PAGE_SIZE,
            "明细上限（{OFFICIAL_USAGE_DETAIL_LIMIT}）不得小于官方单页上限（{OFFICIAL_USAGE_PAGE_SIZE}）"
        );
    }

    #[test]
    fn rejects_error_and_missing_data_shapes() {
        assert!(matches!(
            parse_page(&json!({"code": 500, "msg": "bad"})),
            Err(error) if error == "官方请求失败（code=500）：bad"
        ));
        assert!(matches!(
            parse_page(&json!({"code": 0, "data": {"total": 0}})),
            Err(error) if error == "官方响应格式无效"
        ));
        assert_eq!(
            error_message(&json!({"code": -1, "message": "secret token"})),
            "官方请求失败（网络或服务不可达）"
        );
    }

    #[test]
    fn cache_round_trip_keeps_projection_and_strips_prompt_fields() {
        let payload = json!({
            "status": "complete",
            "rangeStart": "2026-07-26",
            "rangeEnd": "2026-08-25",
            "collectedAt": 1,
            "summary": { "usageToday": 1.5, "usage7Days": 3.0, "usageThisMonth": 3.0 },
            "daily": [{ "date": "2026-08-25", "usage": 1.5, "models": [{ "model": "model-a", "requestCount": 1, "credit": 1.5 }] }],
            "models": [{ "model": "model-a", "requestCount": 1, "credit": 1.5 }],
            "accounts": [{
                "accountId": "account-1",
                "accountName": "one@example.com",
                "ok": true,
                "requestCount": 1,
                "detailTruncated": false,
                "usageToday": 1.5,
                "usage7Days": 1.5,
                "usageThisMonth": 1.5,
                "error": null,
                "reportedTotal": 1,
                "fetchedCount": 1,
                "models": [{ "model": "model-a", "requestCount": 1, "credit": 1.5 }],
                "daily": []
            }],
            "requests": [{
                "accountId": "account-1",
                "accountName": "one@example.com",
                "requestId": "req-1",
                "credit": 1.5,
                "model": "model-a",
                "client": "cli",
                "requestTime": "2026-08-25 12:00:00",
                "input": "do not persist this prompt"
            }],
            "detailLimitPerAccount": 100,
            "errors": [],
            "secret": "drop-me"
        });
        let path = std::env::temp_dir().join(format!(
            "buddy-switch-official-usage-cache-{}.json",
            uuid::Uuid::new_v4().simple()
        ));
        save_official_usage_cache_to(&path, &payload);
        let loaded = load_official_usage_cache_from(&path).expect("valid cache");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded["status"], "complete");
        assert_eq!(loaded["summary"]["usageToday"], 1.5);
        assert_eq!(loaded["requests"][0]["requestId"], "req-1");
        assert!(loaded.get("secret").is_none());
        let text = loaded.to_string();
        assert!(!text.contains("do not persist"));
        assert!(!text.contains("drop-me"));
    }

    #[test]
    fn cache_parser_rejects_corrupt_and_unknown_status() {
        assert!(parse_official_usage_cache("not-json").is_none());
        assert!(parse_official_usage_cache("{}").is_none());
        assert!(parse_official_usage_cache(r#"{"payload":{"status":"nope"}}"#).is_none());
    }

    /// 官方用量端点必须按 region 分主机，且 CN 逐字节保持既有值。
    ///
    /// 若把 `official_usage_url` 改成「两版都返回 `OFFICIAL_USAGE_URL`」，
    /// global 凭据会被打到 CN 用户中心并被网关按 realm 拒掉（实测 APISIX 401），
    /// 本用例会红。
    #[test]
    fn official_usage_url_is_region_scoped() {
        assert_eq!(
            official_usage_url(Region::Cn),
            "https://www.workbuddy.cn/billing/meter/get-user-request-usage",
            "CN 端点不得改动（零回归）"
        );
        assert_eq!(
            official_usage_url(Region::Global),
            "https://www.workbuddy.ai/billing/meter/get-user-request-usage"
        );
        assert_ne!(official_usage_url(Region::Cn), official_usage_url(Region::Global));
        // 两版只有主机不同，路径必须一致（否则是两份不同的契约，需另行取证）。
        let path_of = |url: &str| {
            url.split_once("://")
                .and_then(|(_, rest)| rest.find('/').map(|index| rest[index..].to_string()))
                .expect("url has path")
        };
        assert_eq!(
            path_of(&official_usage_url(Region::Cn)),
            path_of(&official_usage_url(Region::Global))
        );
    }

    /// 进程内记忆与落盘缓存都必须按 region 分家。
    ///
    /// 采集结果的 payload 自带 `accounts[]`（本次采集用的账号集合），而前端把
    /// `officialUsage.accounts` 直接当账号列表（`CreditStatsPage` 的
    /// `filterAccounts = official ? official.accounts : stats.accounts`）。
    /// 两版共用一份缓存时，先看 Global 再看 CN，CN 视图会**命中 Global 的采集结果**
    /// 并因此列出 Global 账号——PRD G1「两版互不污染」被破坏。
    ///
    /// 若把 `official_usage_memory` 改成单个 static、或把
    /// `official_usage_cache_file_for` 的 Global 分支改回 CN 路径，本用例必红。
    #[test]
    fn official_usage_cache_is_region_scoped() {
        // ① 进程内记忆：写入 CN 不得被 Global 读到，反之亦然。
        official_usage_memory(Region::Cn)
            .lock()
            .expect("cn memory")
            .replace(json!({ "marker": "cn" }));
        official_usage_memory(Region::Global)
            .lock()
            .expect("global memory")
            .replace(json!({ "marker": "global" }));
        assert_eq!(
            official_usage_memory(Region::Cn)
                .lock()
                .expect("cn memory")
                .as_ref()
                .expect("cn cached")["marker"],
            "cn"
        );
        assert_eq!(
            official_usage_memory(Region::Global)
                .lock()
                .expect("global memory")
                .as_ref()
                .expect("global cached")["marker"],
            "global"
        );
        // 同进程其他用例不应受本用例影响（这两个 static 只在本模块测试里被写入）。
        official_usage_memory(Region::Cn)
            .lock()
            .expect("cn memory")
            .take();
        official_usage_memory(Region::Global)
            .lock()
            .expect("global memory")
            .take();

        // ② 落盘缓存：两版不同文件；CN 必须沿用既有文件名（老缓存零迁移）。
        let cn = official_usage_cache_file_for(Region::Cn);
        let global = official_usage_cache_file_for(Region::Global);
        assert_ne!(cn, global, "两版官方用量缓存不得共用文件");
        assert_eq!(
            cn.file_name().and_then(|name| name.to_str()),
            Some("official_usage_cache.json"),
            "CN 缓存文件名不得改动，否则老用户缓存失效（白跑一次采集）"
        );
        assert_eq!(
            global.file_name().and_then(|name| name.to_str()),
            Some("official_usage_cache.global.json")
        );
    }
}
