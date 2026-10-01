//! Trae 网关的四个端点与 Bearer 鉴权中间件。
//!
//! ## 与 WorkBuddy 网关路由的关系
//!
//! 只有 `POST /v1/chat/completions` 与 WorkBuddy 侧**同名**，实现却毫无共同点：
//! WorkBuddy 是「出站改写 → 透传 OpenAI SSE」，Trae 是「出站改写 → 转换私有 SOLO SSE」。
//! 因此本文件不引用 `crate::routes` 的任何内容，只有**错误响应体形状**刻意保持一致
//! （`{"error":{"message","type","code"}}`），让同一个 OpenAI 客户端两处都能读懂。
//!
//! ## 换号语义
//!
//! | 路径 | 连接期失败（HTTP 非 2xx / 传输层错误） | 流内失败（`event:error`） |
//! |:---|:---|:---|
//! | 流式 | 冷却该账号 → **换号重试**（最多 `max_rotate` 次） | 冷却该账号 → **不换号** |
//! | 非流式 | 冷却该账号 → **换号重试** | 冷却该账号 → **换号重试** |
//!
//! 差别来自「响应头是否已经发出」：流式一旦把 `200 + text/event-stream` 发给客户端，
//! 就只能把错误塞进事件流里（补一条 `data: {...error...}` 再 `data: [DONE]`）；
//! 非流式在聚合完成前一个字节都还没发给客户端，因此可以整轮重来。
//!
//! 流内失败也写回冷却，是为了让**下一轮**请求自动避开这个账号——否则每次都要先撞墙。

use std::collections::HashSet;
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::extract::{Extension, RawQuery, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use buddy_switch_core::modules::trae::account;
use buddy_switch_core::modules::trae::region::TraeRegion;
use buddy_switch_core::modules::trae::variant::TraeVariant;

use crate::protocol::anthropic;
use crate::session_headers;
use crate::sticky;

use super::payload;
use super::pool::{classify_http, classify_solo, PickedTraeAccount, TraeErrKind, TraePool};
use super::sse::{self, TokenUsage};
use super::capture;
use super::{
    now_secs, TraeGatewayState, TRAE_APP_ID, TRAE_IDE_VERSION,
    TRAE_IDE_VERSION_CODE, TRAE_LLM_CHAT_PATH,
};

/// 请求扩展：承载 Bearer Key 的**归属标识**，供 handler 选择对应的账号池与 `function`。
///
/// ## ★ 键里存的是**程序位**（2026-09-30 起可带 TraeCode）
///
/// 键记录由前端「归属程序位」下拉创建，取值是 4 个程序位：
/// `trae_work`（国内 TraeWork）/ `trae_cn`（国内 TraeCode）/ `global`（国际 TraeWork）/
/// `global_trae_code`（国际 TraeCode）。旧值 `cn` / `global` 经 [`TraeVariant::parse`]
/// 仍落**区域主程序**（TraeWork），老键行为不变。
///
/// 这里拿到的变体同时决定两件事，**两者必须同源**：
/// 1. `/v1/models` 列哪份客户端清单（[`payload::models_response_for`]）；
/// 2. 请求体里的 `function`（[`super::function_for`]）。
///
/// 只改其一就会出现「列得出来、调不动」—— 这正是 issue #4 的形态：
/// TraeCode 的模型（`glm-5.3-flash`）曾被写死的 `solo_work_lite` 一律拒成
/// `4001 param is invalid`（2026-09-30 上游实测，见
/// [`super::function_for`] 的实测表与 `tests::probe_traecode_model_names`）。
///
/// 账号池是**区域级**的（`pool.rs` 用 `entries_for_region`），两条程序位共用一本库，
/// 因此程序位只影响清单与 `function`，不需要新池。
///
/// 用「请求扩展」而非给每个 handler 加参数：鉴权中间件解析出归属后写入，
/// `chat_completions` 读出，避免中间件 → handler 的签名穿透一堆函数。
#[derive(Clone, Copy, Debug)]
pub struct TraeKeyVariant(pub TraeVariant);

/// 日志里的端点名（与 WorkBuddy 网关同名字符串，便于统一聚合）。
///
/// ★ 两条入口各有一个常量，并且**同时**决定出站协议：`/v1/messages` 要额外做一次
/// Anthropic 转换。把「协议」与「日志端点」合成同一个参数，是为了让它们**不可能**
/// 对不上 —— 否则某天会出现「按 Anthropic 转换了、日志却记成 chat completions」
/// 这种统计静默错位。
const ENDPOINT_CHAT: &str = "/v1/chat/completions";
const ENDPOINT_MESSAGES: &str = "/v1/messages";

/// 该端点是否要 Anthropic 出站转换。
fn is_anthropic_endpoint(endpoint: &str) -> bool {
    endpoint == ENDPOINT_MESSAGES
}

/// 上游 UA。参考实现固定为 `TraeClient/TTNet`，改它没有好处。
const TRAE_USER_AGENT: &str = "TraeClient/TTNet";

// ---------------------------------------------------------------------------
// 处理器
// ---------------------------------------------------------------------------

/// `GET /health`：存活探针，**免鉴权**。
///
/// 附带账号池摘要：探活时顺手看一眼「还有没有可用账号」比再发一次 `/status` 省事。
/// 摘要取**默认变体**的池（探针无 Key、无归属，只能给一条产品线的概览）。
pub async fn health(State(state): State<TraeGatewayState>) -> Response {
    let variant = TraeVariant::default();
    let summary = {
        let mut pools = state.pools.lock().await;
        let pool = pools.entry(variant).or_insert_with(|| TraePool::for_variant(variant));
        pool.sync_for(variant);
        pool.summary(now_secs())
    };
    json_response(
        StatusCode::OK,
        json!({
            "status": "ok",
            "service": "trae-gateway",
            "running": true,
            "total_requests": state.total_requests.load(std::sync::atomic::Ordering::Relaxed),
            "pool": summary,
        }),
    )
}

/// Bearer 鉴权中间件：`/health` 免鉴权，其余端点校验 `Authorization: Bearer <key>`。
///
/// 与参考实现的一处**有意差异**：没有任何可用 Key 时**拒绝**而不是放行。参考实现
/// 「留空即不鉴权」意味着任何人只要猜到 7864 端口就能白嫖额度；本网关要求 Key 由
/// 用户显式创建（或经旧 `settings.apiKey` 兼容读得来），不存在「用户忘了配」的放行场景，
/// 所以宁严勿宽。
///
/// 鉴权通过后把 `record.variant`（**归属产品线**）写进请求扩展
/// （[`TraeKeyVariant`]），下游据此选择对应的账号池；同时 `touch` 刷新最近使用时间。
pub async fn bearer_auth(
    State(state): State<TraeGatewayState>,
    mut request: Request,
    next: Next,
) -> Response {
    if request.uri().path() == "/health" {
        return next.run(request).await;
    }

    // ★ 错误体形状按**端点**分派：Anthropic 客户端只认 `{"type":"error",...}`，
    // 给它 OpenAI 形状的话，它报的是「响应格式错误」而不是「凭据无效」——
    // 排查方向会被带偏。401 对应 Anthropic 的 `authentication_error`。
    let anthropic = request.uri().path() == "/v1/messages";
    let deny = move |message: &str| {
        if anthropic {
            anthropic_error(StatusCode::UNAUTHORIZED, "authentication_error", message)
        } else {
            openai_error(
                StatusCode::UNAUTHORIZED,
                "invalid_request_error",
                message,
            )
        }
    };

    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .map(str::trim)
        })
        .filter(|value| !value.is_empty());

    match presented {
        None => deny("缺少凭据：请在 Authorization 头携带 `Bearer <API Key>`"),
        Some(key) => match state.key_store.verify(key) {
            Some(record) => {
                // 归属产品线透传给 handler（决定用哪个账号池）。
                request
                    .extensions_mut()
                    .insert(TraeKeyVariant(record.variant));
                state.key_store.touch(&record.id);
                next.run(request).await
            }
            None => deny("API Key 无效：请到「API 服务」页创建或复制一把可用的 Key"),
        },
    }
}

/// `GET /status`：运行状态 + 账号明细 + 诊断。
///
/// 可选 query `variant`（`trae_work` / `trae_cn`）决定看哪个账号池；
/// 缺失或无法识别一律回落 [`TraeVariant::default`]（TraeWork），与旧行为一致。
pub async fn status(
    State(state): State<TraeGatewayState>,
    RawQuery(query): RawQuery,
) -> Response {
    let variant = parse_variant_query(query.as_deref());
    // 复用管理面那份组装逻辑：`/status` 与「API 服务」页显示的必须是同一份状态。
    let config = state.config_snapshot().await;
    let addr = Some(format!("{}:{}", config.bind_addr, config.port));
    let body =
        super::status_view(&state, true, addr, env!("CARGO_PKG_VERSION"), variant).await;
    json_response(StatusCode::OK, body)
}

/// 从 query 串解析变体参数（最简实现，与 server 的 `query_value` 同风格）。
fn parse_variant_query(query: Option<&str>) -> TraeVariant {
    query
        .unwrap_or("")
        .split('&')
        .find_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (key == "variant").then(|| TraeVariant::parse(value).unwrap_or_default())
        })
        .unwrap_or_default()
}

/// `GET /v1/models`：**随上游刷新**的模型清单（issue #4）。
///
/// 清单来源与「API 服务」页的模型卡**同源**：读该 Key **归属产品线**的客户端
/// `state.vscdb` 缓存（上游下发）。归属由鉴权中间件经请求扩展传入。
///
/// ## 两处刻意的取舍
///
/// 1. **归属用必需提取器**（不是 `Option<Extension<…>>`）：中间件对除 `/health`
///    外的所有路由都会注入它，**拿不到就说明鉴权链断了** —— 那属于配置错误，
///    应当 500 响亮失败，而不是静默回落默认变体（那会把「链断了」伪装成正常）。
/// 2. **读库放进阻塞池**：`models_response_for` 会同步读 SQLite（客户端可能在写，
///    最多等 core 侧设的 `busy_timeout` 那个窗口）。直接在 async handler 里做会占住 worker。
///
/// 客户端没启动过 / 没登录时读不到 ⇒ 回落静态清单（见 [`payload::models_response_for`]），
/// 保证 OpenAI 客户端**开箱即可列出模型**，而不是返回空表或报错。
pub async fn models(Extension(key_variant): Extension<TraeKeyVariant>) -> Response {
    let variant = key_variant.0;
    // `models_response_for` 自己**不失败**（读不到就回落静态清单），所以这里的
    // `JoinError` 只可能是运行时正在关闭 —— 用静态清单兜底，别让「列模型」挂掉。
    let body = tokio::task::spawn_blocking(move || payload::models_response_for(variant))
        .await
        .unwrap_or_else(|_| payload::models_response());
    json_response(StatusCode::OK, body)
}

/// `POST /v1/chat/completions`。
///
/// 归属产品线由鉴权中间件经请求扩展（[`TraeKeyVariant`]）传入，决定用哪个账号池。
pub async fn chat_completions(
    State(state): State<TraeGatewayState>,
    Extension(key_variant): Extension<TraeKeyVariant>,
    _headers: HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    let variant = key_variant.0;
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => {
            return openai_error(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                &format!("请求体不是合法 JSON：{error}"),
            )
        }
    };

    let config = state.config_snapshot().await;
    let model = payload::model_of(&parsed, &config.default_model);
    let stream = payload::wants_stream(&parsed);
    let max_rotate = config.max_rotate.max(1);
    let chat_id = format!("chatcmpl-{}", uuid_like());

    let sticky_key = sticky_key_of(&parsed, variant, &model);

    if stream {
        stream_chat(
            &state,
            &body,
            &config.default_model,
            &model,
            &chat_id,
            max_rotate,
            &config.preferred_uid,
            sticky_key.as_deref(),
            ENDPOINT_CHAT,
            started,
            variant,
        )
        .await
    } else {
        aggregate_chat(
            &state,
            &body,
            &config.default_model,
            &model,
            &chat_id,
            max_rotate,
            &config.preferred_uid,
            sticky_key.as_deref(),
            started,
            variant,
        )
        .await
    }
}

/// `POST /v1/messages`：Anthropic 协议入口。
///
/// ## 为什么能与 `/v1/chat/completions` 共用整条链路
///
/// 上游只认一种东西（Trae 私有的 `llm_utils_chat`），所以两条入口的差别**只在协议层**：
/// 入站 Anthropic → OpenAI 形态（[`anthropic::to_upstream_request`]），
/// 出站 OpenAI → Anthropic（[`anthropic::AnthropicSseStream`] /
/// [`anthropic::message_from_response`]）。中间的中继（选号 / 换号 / 冷却 / 粘性 /
/// 日志）**一行都不用改** —— 这正是把这两个转换放进共用 `protocol` 层
/// （而不是 Trae 模块内）的价值。
///
/// ## 上游请求体是**重新序列化**的
///
/// `to_upstream_request` 产出 `Value`，要再序列化成 `Bytes` 才能喂给中继 ——
/// 不能透传原始 `Bytes`：Anthropic 的 `system` / `max_tokens` 与 OpenAI 的
/// `messages` / `max_completion_tokens` 不同形。
pub async fn messages(
    State(state): State<TraeGatewayState>,
    Extension(key_variant): Extension<TraeKeyVariant>,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    let variant = key_variant.0;

    let config = state.config_snapshot().await;
    let Ok(parsed) = serde_json::from_slice::<Value>(&body) else {
        return anthropic_error(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "请求体不是合法 JSON",
        );
    };

    let upstream = anthropic::to_upstream_request(&parsed);
    let Ok(encoded) = serde_json::to_vec(&upstream) else {
        return anthropic_error(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "请求体无法序列化",
        );
    };
    let upstream_body = Bytes::from(encoded);

    // 模型取**转换后**的请求体（那是真正发给上游的东西）；
    // 流式开关见 [`anthropic_wants_stream`]（**必须**读原始请求体）。
    let model = payload::model_of(&upstream, &config.default_model);
    let stream = anthropic_wants_stream(&parsed);
    let max_rotate = config.max_rotate.max(1);
    // 上游响应 id 用 `chatcmpl-` 前缀（与 `/v1/chat/completions` 一致）。
    //
    // ★ 不要用 `msg_`：`protocol::anthropic::message_from_accumulator` 会在上游 id 上
    // **再包一层** `msg_`，用 `msg_` 当上游前缀会得到 `msg_msg_xxx`（真实请求实测到）。
    let chat_id = format!("chatcmpl-{}", uuid_like());

    // 会话 id 取**原始** Anthropic 请求体：`to_upstream_request` 不保证把
    // `metadata` 原样带过去，而客户端塞会话 id 的习惯位置就是那里
    // （与 WorkBuddy 侧 `messages.rs` 同源）。拿不到 ⇒ 返回 `None` ⇒ 整段跳过粘性。
    let sticky_key = sticky_key_of(&parsed, variant, &model);

    if stream {
        stream_chat(
            &state,
            &upstream_body,
            &config.default_model,
            &model,
            &chat_id,
            max_rotate,
            &config.preferred_uid,
            sticky_key.as_deref(),
            ENDPOINT_MESSAGES,
            started,
            variant,
        )
        .await
    } else {
        match aggregate_payload(
            &state,
            &upstream_body,
            &config.default_model,
            &model,
            &chat_id,
            max_rotate,
            &config.preferred_uid,
            sticky_key.as_deref(),
            ENDPOINT_MESSAGES,
            started,
            variant,
        )
        .await
        {
            Ok(payload) => json_response(
                StatusCode::OK,
                anthropic::message_from_response(&payload, &model),
            ),
            Err(failure) => anthropic_error(
                StatusCode::from_u16(failure.status).unwrap_or(StatusCode::BAD_GATEWAY),
                &failure.code,
                &failure.message,
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// 流式
// ---------------------------------------------------------------------------

/// 流式路径：先换号直到拿到 2xx，再把响应体交给转换任务。
#[allow(clippy::too_many_arguments)]
#[allow(non_snake_case)] // 抓取编号绑定沿用拼音命名规范
async fn stream_chat(
    state: &TraeGatewayState,
    body: &Bytes,
    default_model: &str,
    model: &str,
    chat_id: &str,
    max_rotate: usize,
    preferred_uid: &str,
    sticky_key: Option<&str>,
    // 入口端点：同时决定**出站协议**（`/v1/messages` 要额外做 Anthropic 转换）
    // 与**日志端点名** —— 合成一个参数，两者就不可能对不上。
    //
    // 上游链路完全相同（SOLO SSE → OpenAI SSE），差别只在最后一层封装 ——
    // 所以用参数而不是另写一个函数：另写一份会让换号 / 冷却 / 粘性 / 日志
    // 全部再实现一遍，两处迟早分家。
    endpoint: &'static str,
    started: Instant,
    variant: TraeVariant,
) -> Response {
    let mut tried: HashSet<String> = HashSet::new();
    let mut last: Option<UpstreamFailure> = None;

    for _ in 0..max_rotate {
        match attempt_once(
            state,
            body,
            default_model,
            &mut tried,
            preferred_uid,
            sticky_key,
            variant,
        )
        .await
        {
            None => break,
            Some(AttemptResult::Failed { failure, .. }) => last = Some(failure),
            Some(AttemptResult::Ok { account, response, capture_id }) => {
                let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(64);
                let task_state = state.clone();
                let task_model = model.to_string();
                let task_chat_id = chat_id.to_string();
                let task_capture_id = capture_id;
                tokio::spawn(async move {
                    let outcome = sse::stream_convert(
                        response,
                        tx,
                        &task_chat_id,
                        &task_model,
                        &task_capture_id,
                    )
                    .await;
                    settle(
                        &task_state,
                        &account,
                        &task_model,
                        true,
                        started,
                        outcome.error,
                        outcome.usage,
                        endpoint,
                        variant,
                    )
                    .await;
                });
                return if is_anthropic_endpoint(endpoint) {
                    anthropic_sse_response(rx, model)
                } else {
                    sse_response(rx)
                };
            }
        }
    }

    // 没拿到任何可用上游：这里**还没发过响应头**，可以正常返回 JSON 错误。
    let failure = last.unwrap_or_else(|| no_account_failure_sync(state, variant));
    settle_failure(state, model, true, started, &failure, endpoint, variant).await;
    openai_error(
        StatusCode::from_u16(failure.status).unwrap_or(StatusCode::BAD_GATEWAY),
        &failure.code,
        &failure.message,
    )
}

// ---------------------------------------------------------------------------
// 非流式
// ---------------------------------------------------------------------------

/// 非流式路径：本地聚合。**流内错误也换号**——聚合完成前客户端一个字节都没收到。
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
#[allow(non_snake_case)] // 抓取编号绑定沿用拼音命名规范
async fn aggregate_payload(
    state: &TraeGatewayState,
    body: &Bytes,
    default_model: &str,
    model: &str,
    chat_id: &str,
    max_rotate: usize,
    preferred_uid: &str,
    sticky_key: Option<&str>,
    endpoint: &'static str,
    started: Instant,
    variant: TraeVariant,
) -> Result<Value, UpstreamFailure> {
    let mut tried: HashSet<String> = HashSet::new();
    let mut last: Option<UpstreamFailure> = None;

    for _ in 0..max_rotate {
        let (account, response, capture_id) = match attempt_once(
            state,
            body,
            default_model,
            &mut tried,
            preferred_uid,
            sticky_key,
            variant,
        )
        .await
        {
                None => break,
                Some(AttemptResult::Failed { failure, .. }) => {
                    last = Some(failure);
                    continue;
                }
                Some(AttemptResult::Ok { account, response, capture_id }) => {
                    (account, response, capture_id)
                }
            };

        let (payload, error, usage) =
            sse::aggregate(response, chat_id, model, &capture_id).await;

        match (payload, error) {
            (Some(payload), None) => {
                settle(state, &account, model, false, started, None, usage, endpoint, variant).await;
                return Ok(payload);
            }
            (_, Some((code, message))) => {
                // 流内错误：冷却该账号 → 换下一个账号整轮重来（响应头还没发）。
                last = Some(
                    record_stream_error(state, &account, code, &message, variant).await,
                );
            }
            _ => {
                // 既没有 payload 也没有错误：上游给了个空流。按 5xx 处理并换号。
                last = Some(
                    record_stream_error(state, &account, 0, "上游返回空事件流", variant).await,
                );
            }
        }
    }

    let failure = last.unwrap_or_else(|| no_account_failure_sync(state, variant));
    settle_failure(state, model, false, started, &failure, endpoint, variant).await;
    Err(failure)
}

/// 非流式 OpenAI 端点：把 [`aggregate_payload`] 的结果包成 OpenAI 形状。
#[allow(clippy::too_many_arguments)]
async fn aggregate_chat(
    state: &TraeGatewayState,
    body: &Bytes,
    default_model: &str,
    model: &str,
    chat_id: &str,
    max_rotate: usize,
    preferred_uid: &str,
    sticky_key: Option<&str>,
    started: Instant,
    variant: TraeVariant,
) -> Response {
    match aggregate_payload(
        state,
        body,
        default_model,
        model,
        chat_id,
        max_rotate,
        preferred_uid,
        sticky_key,
        ENDPOINT_CHAT,
        started,
        variant,
    )
    .await
    {
        Ok(payload) => json_response(StatusCode::OK, payload),
        Err(failure) => openai_error(
            StatusCode::from_u16(failure.status).unwrap_or(StatusCode::BAD_GATEWAY),
            &failure.code,
            &failure.message,
        ),
    }
}

// ---------------------------------------------------------------------------
// 选号与出站
// ---------------------------------------------------------------------------

/// Anthropic 请求是否要流式响应。
///
/// ★ 必须读**原始** Anthropic 请求体，不能读 [`anthropic::to_upstream_request`] 的产物：
/// 后者会把 `stream` **强制为 `true`**（上游只接受流式）⇒ 拿它判断的话，
/// 非流式客户端会被当成流式，收到一条事件流而不是 JSON。
fn anthropic_wants_stream(parsed: &Value) -> bool {
    parsed
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// 由请求体、产品线与模型派生**会话粘性键**；客户端未给会话 id 时返回 `None`。
///
/// ★ 返回 `None` 时调用方必须**整段跳过**粘性 —— 不得退化成轮级键：那会造出一个
/// 永不命中的绑定，让「粘性会话数」看起来非 0 却毫无作用
/// （见 [`sticky::sticky_key`] 的文档）。
///
/// 变体进键是必须的：两条产品线的账号库**分家**，粘到一起会指向另一条线的账号。
fn sticky_key_of(parsed: &Value, variant: TraeVariant, model: &str) -> Option<String> {
    session_headers::conversation_id_of(parsed)
        // ★ 必须 trim：`conversation_id_of` 只排除**空串**，纯空白（`"   "`）会漏过来。
        // 直接拿它当键的话，所有「填了空白」的客户端会**共用同一个绑定**
        // （表现为不同对话互相串号），而且不报错。WorkBuddy 侧的 `sticky_key_of` 同样 trim。
        .map(|conversation| conversation.trim().to_string())
        .filter(|conversation| !conversation.is_empty())
        .map(|conversation| sticky::sticky_key(variant.as_str(), model, &conversation))
}

/// 合并两个偏好来源，得到本次选号的偏好 uid。
///
/// 优先级：**显式指定 > 会话粘性**。
///
/// 为什么显式指定优先：粘性记录的是「上次成功的账号」，是一个**陈旧**的观察；
/// 而 `preferred_uid` 是用户此刻的明确要求。若让粘性压过它，用户改完设置后会看到
/// 「下一轮仍走旧账号」—— 表现为设置不生效。
///
/// ★ 本函数只决定「优先试谁」，**不保证一定用谁**：两者都不可用时由
/// [`crate::pool::pick_with_preference`] 回落到自动择优。粘性是优化而非约束。
fn resolve_preference<'a>(explicit_uid: &'a str, sticky_uid: Option<&'a str>) -> Option<&'a str> {
    if explicit_uid.is_empty() {
        sticky_uid
    } else {
        Some(explicit_uid)
    }
}

/// 一次出站尝试的结果。
enum AttemptResult {
    /// 上游返回 2xx（响应体尚未读取）。
    Ok {
        account: PickedTraeAccount,
        response: reqwest::Response,
        /// 抓取编号：本次尝试落盘文件的文件名前缀（诊断旁路，见 [`super::capture`]）；
        /// 空串表示抓取开关关闭（落盘函数遇空编号一律跳过）。
        capture_id: String,
    },
    /// 出站失败：已分类、已写回冷却文件。
    Failed {
        #[allow(dead_code)]
        account: PickedTraeAccount,
        failure: UpstreamFailure,
    },
}

/// 选号 + 出站一次。`tried` 在内部累加，调用方反复调用即可自动换号。
///
/// 返回 `None` 表示**已经挑不出新账号**（不是失败，是没得试了）。
///
/// 出站前一刻抓取请求对（客户端原文体 + 改写后上游体，诊断旁路）：
/// 换号重试时每次尝试各生成一个新编号，`AttemptResult::Ok` 把编号带给
/// 响应消费方，供上游流继续追加落盘。
#[allow(clippy::too_many_arguments)]
#[allow(non_snake_case)] // 抓取编号局部变量沿用拼音命名规范
async fn attempt_once(
    state: &TraeGatewayState,
    body: &Bytes,
    default_model: &str,
    tried: &mut HashSet<String>,
    preferred_uid: &str,
    sticky_key: Option<&str>,
    variant: TraeVariant,
) -> Option<AttemptResult> {
    // 先读粘性（**不持 pools 锁**）：两把锁不嵌套 —— 既不延长池的持锁时间，
    // 也避免日后有人以相反顺序取锁造成死锁。
    let sticky_uid = state.sticky_uid(sticky_key).await;

    let picked = {
        let mut pools = state.pools.lock().await;
        let pool = pools.entry(variant).or_insert_with(|| TraePool::for_variant(variant));
        // 每次选号前重新同步：另一个入口（签到页 / 桌面端）可能刚写了冷却或刷新了积分。
        pool.sync_for(variant);
        // 偏好优先级：**显式指定 > 会话粘性 > 自动择优**（见 [`resolve_preference`]）。
        // 两者都不可用时 `pick_with_preference` 会自行回落到自动择优，绝不拒绝服务。
        let preferred = resolve_preference(preferred_uid, sticky_uid.as_deref());
        pool.pick(now_secs(), tried, preferred)
    }?;
    tried.insert(picked.uid.clone());

    let converted = payload::prepare_llm_chat_body(
        body,
        variant,
        default_model,
        &picked.uid,
        &picked.device_id,
        &picked.machine_id,
    );

    // 抓取（诊断旁路）：客户端原文体 + 改写后上游体各落一份，编号同时带给响应侧。
    // 开关由 api_gateway.json 的 capture_enabled 控制（默认关）：关时不生成编号，
    // 三个落盘函数遇空编号一律静默跳过（空编号即"未启用"的既有哨兵语义）。
    let capture_id = if state.config_snapshot().await.capture_enabled {
        let id = capture::new_capture_id();
        capture::capture_request_pair(&id, body, &converted);
        id
    } else {
        String::new()
    };

    match send_llm_chat(state, &picked, &converted, &capture_id).await {
        Ok(response) => {
            // 会话粘性：**只在成功之后**绑定，让下一轮优先复用这个账号。
            state.bind_sticky(sticky_key, &picked.uid).await;
            Some(AttemptResult::Ok {
                account: picked,
                response,
                capture_id,
            })
        }
        Err((status, detail)) => {
            let kind = classify_http(status);
            let failure = UpstreamFailure {
                // 传输层错误没有 HTTP 状态码，对客户端统一报 502（网关上游不可达）。
                status: if status == 0 { 502 } else { status },
                code: if status == 0 {
                    "upstream_unreachable".to_string()
                } else {
                    kind.as_str().to_string()
                },
                message: format!(
                    "账号「{}」上游失败（HTTP {}）：{}",
                    picked.name,
                    if status == 0 { "—".into() } else { status.to_string() },
                    detail
                ),
            };
            {
                let mut pools = state.pools.lock().await;
                pools
                    .entry(variant)
                    .or_insert_with(|| TraePool::for_variant(variant))
                    .apply_error(&picked.uid, kind, &failure.message);
            }
            *state.last_error.write().await = Some(failure.message.clone());
            // 会话粘性：这个账号刚失败 ⇒ 立刻**解绑**，下一轮换号重新绑。
            // 粘性是优化而非约束，绝不能因为粘性而反复撞同一堵墙。
            state.unbind_sticky(sticky_key).await;
            Some(AttemptResult::Failed {
                account: picked,
                failure,
            })
        }
    }
}

/// 发一次 `llm_utils_chat`：拿到响应头即返回，不读 body。
///
/// 头部逐字对齐参考实现（`x-ide-token` 用裸 JWT，**不带** `Cloud-IDE-JWT ` 前缀，
/// 前缀只在 `Authorization` 场景使用）。`accept-encoding` 显式写 `identity`：
/// 本 crate 的 reqwest 未开 gzip/br/zstd 特性，若上游压缩返回，读出来就是乱码。
async fn send_llm_chat(
    state: &TraeGatewayState,
    account: &PickedTraeAccount,
    body: &[u8],
    capture_id: &str,
) -> Result<reqwest::Response, (u16, String)> {
    let url = format!("{}{TRAE_LLM_CHAT_PATH}", state.upstream);
    let trace_id = trace_id();

    let response = state
        .http
        .post(&url)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "*/*")
        .header(header::ACCEPT_ENCODING, "identity")
        .header(header::USER_AGENT, TRAE_USER_AGENT)
        .header(header::REFERER, &url)
        .header("x-ide-token", &account.jwt)
        .header("x-app-id", TRAE_APP_ID)
        .header("x-app-version", "default")
        .header("x-app-version-code", TRAE_IDE_VERSION_CODE)
        .header("x-ide-version", TRAE_IDE_VERSION)
        .header("x-ide-version-code", TRAE_IDE_VERSION_CODE)
        .header("x-ide-version-type", "stable")
        .header("x-device-type", "windows")
        .header("x-device-brand", "CREFG-XX")
        .header("x-device-cpu", "Intel")
        .header("x-device-id", &account.device_id)
        .header("x-machine-id", &account.machine_id)
        .header("x-os-version", "Windows 11 Home China")
        .header("request-traffic-type", "prod")
        .header("package-type", "stable_cn")
        .header("x-lgw-req-sdk-type", "3")
        .header("x-lscbd-aid", "787976")
        .header("x-lscbd-platform", "windows")
        .header("x-ss-dp", "787976")
        .header("app-version", TRAE_IDE_VERSION)
        .header("x-custom-trace-id", &trace_id[..16])
        .header(
            "x-flow-traceparent",
            format!("04-{}-{}-01", &trace_id[3..35], uuid_like()),
        )
        .header("x-tt-trace-id", &trace_id)
        .header("x-request-id", format!("req_{}", uuid_like()))
        .body(body.to_vec())
        .send()
        .await
        .map_err(|error| (0u16, describe_transport_error(&error)))?;

    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(response);
    }

    // 错误体可能很长（含堆栈），截断后再回传与落日志。
    let text = response.text().await.unwrap_or_default();
    // 抓取（诊断旁路）：上游非 2xx 的完整错误响应体。
    capture::write_capture(capture_id, "_upstream_http_error.txt", text.as_bytes());
    Err((status, preview(&text, 300)))
}

// ---------------------------------------------------------------------------
// 收尾：写回池 + 落日志
// ---------------------------------------------------------------------------

/// 一次请求的收尾（流式与非流式共用）。
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
async fn settle(
    state: &TraeGatewayState,
    account: &PickedTraeAccount,
    model: &str,
    stream: bool,
    started: Instant,
    error: Option<(i64, String)>,
    usage: TokenUsage,
    endpoint: &'static str,
    variant: TraeVariant,
) {
    let latency_ms = started.elapsed().as_millis() as i64;

    let message = match &error {
        Some((code, detail)) => Some(format!(
            "账号「{}」流内错误（code={code}）：{}",
            account.name,
            preview(detail, 200)
        )),
        None => None,
    };

    if let Some((code, detail)) = &error {
        let kind = classify_solo(*code, detail);
        if kind != TraeErrKind::None {
            let reason = message.clone().unwrap_or_default();
            state
                .pools
                .lock()
                .await
                .entry(variant)
                .or_insert_with(|| TraePool::for_variant(variant))
                .apply_error(&account.uid, kind, &reason);
        }
    } else {
        state
            .pools
            .lock()
            .await
            .entry(variant)
            .or_insert_with(|| TraePool::for_variant(variant))
            .note_success(&account.uid);
    }

    // 状态码恒为 200：SSE 已经以 200 开头发出，错误只能体现在事件里。
    state
        .record_request(
            endpoint,
            model,
            200,
            &account.uid,
            latency_ms,
            stream,
            usage.prompt,
            usage.completion,
            message,
            variant,
        )
        .await;
}

/// 流内错误 → 写回冷却 + 构造可重试的失败。
async fn record_stream_error(
    state: &TraeGatewayState,
    account: &PickedTraeAccount,
    code: i64,
    detail: &str,
    variant: TraeVariant,
) -> UpstreamFailure {
    let kind = classify_solo(code, detail);
    let message = format!(
        "账号「{}」流内错误（code={code}）：{}",
        account.name,
        preview(detail, 200)
    );
    if kind != TraeErrKind::None {
        state
            .pools
            .lock()
            .await
            .entry(variant)
            .or_insert_with(|| TraePool::for_variant(variant))
            .apply_error(&account.uid, kind, &message);
    }
    *state.last_error.write().await = Some(message.clone());
    UpstreamFailure {
        status: 502,
        code: kind.as_str().to_string(),
        message,
    }
}

/// 落一条失败请求日志（用于「网关页能看到最近为什么全失败」）。
async fn settle_failure(
    state: &TraeGatewayState,
    model: &str,
    stream: bool,
    started: Instant,
    failure: &UpstreamFailure,
    endpoint: &'static str,
    variant: TraeVariant,
) {
    state
        .record_request(
            endpoint,
            model,
            failure.status,
            "",
            started.elapsed().as_millis() as i64,
            stream,
            0,
            0,
            Some(failure.message.clone()),
            variant,
        )
        .await;
}

/// 池里挑不出账号时的失败：带上**逐账号原因**，且**文案必须点名用户当前所在的区域**。
///
/// 只回一句「没有可用账号」等于把排查成本转嫁给用户；把 `diagnose()` 的结果拼进去，
/// 用户能立刻看出是「全部冷却中」还是「积分都过期了」。
///
/// 按区域取名的原因：Key 的**归属就是区域**（账号库、冷却、积分都按区域分家），
/// 账号管理页的切换器也是「国内版 / 国际版」两个 Tab。文案若按程序位取名
/// （`Trae Work` / `Trae`），用户会被指到界面上**根本不存在**的入口去加账号。
fn no_account_failure_sync(state: &TraeGatewayState, variant: TraeVariant) -> UpstreamFailure {
    // 这里不能 await（调用点在 `unwrap_or_else` 里），用阻塞锁读一次内存视图即可：
    // 池状态在 `attempt_once` 里刚同步过，读到的不会比磁盘旧。
    let diagnose = match state.pools.try_lock() {
        Ok(pools) => pools
            .get(&variant)
            .map(|pool| pool.diagnose(now_secs()))
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    // 选中的池为空时，顺手看一眼**另一个区域**是否有账号：若另一个有而这个没有，
    // 用户多半是「账号加在了另一个区域、Key 却归属这个区域」——直接点出该怎么改。
    //
    // ⚠️ 必须遍历 [`TraeRegion::all`]，**不能**用 `TraeVariant::all()`：后者只有两条
    // **国内**程序位，于是「国内 Key + 国内库为空 + 国际版库有账号」这一情形
    // 永远得不到提示 —— 真正可能有账号的那条线根本不在候选里。提示要双向覆盖，
    // 否则国际版接入后，只在「国际版 Key 配国内账号」这一个方向上给指引。
    let other_region_with_accounts = if diagnose.is_empty() {
        TraeRegion::all()
            .into_iter()
            .filter(|candidate| *candidate != variant.region())
            .find(|candidate| !account::entries_for_region(*candidate).is_empty())
    } else {
        None
    };
    UpstreamFailure {
        status: 503,
        code: "no_healthy_account".to_string(),
        message: no_account_message(variant, &diagnose, other_region_with_accounts),
    }
}

/// 空池文案的唯一构造点（纯函数，便于单测钉住「区域名必须出现」）。
///
/// `other_region_with_accounts` 为「另一个区域有账号」时的提示对象；`None` 表示不提。
///
/// 取名一律走 [`TraeVariant::region`] + [`TraeRegion::display_name`]：
/// `TraeWork` 与 `Trae` **同属国内区域**，因此两者产出的文案**逐字相同**。
fn no_account_message(
    variant: TraeVariant,
    diagnose: &[String],
    other_region_with_accounts: Option<TraeRegion>,
) -> String {
    let name = variant.region().display_name();
    let base = if diagnose.is_empty() {
        format!(
            "「{name}」没有可用账号：请先在「账号管理」切到 {name} 添加账号，\
             或在「一键签到」页查看冷却原因"
        )
    } else {
        format!("「{name}」没有可用账号：{}", diagnose.join("、"))
    };
    match other_region_with_accounts {
        Some(other) => {
            let other_name = other.display_name();
            format!("{base}（检测到 {other_name} 有账号：请改用归属 {other_name} 的 API Key）")
        }
        None => base,
    }
}

/// 出站失败的统一形状。
struct UpstreamFailure {
    status: u16,
    code: String,
    message: String,
}

// ---------------------------------------------------------------------------
// 响应与工具
// ---------------------------------------------------------------------------

/// JSON 响应。
fn json_response(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

/// OpenAI 端点错误响应。
fn openai_error(status: StatusCode, code: &str, message: &str) -> Response {
    json_response(status, sse::error_body(code, message))
}

/// Anthropic 端点错误响应：`{"type":"error","error":{"type","message"}}`。
///
/// 形状与 WorkBuddy 网关的 `GatewayError::anthropic_body` **刻意保持一致** ——
/// 同一个 Anthropic 客户端连两条网关都要能读懂。Trae 模块不复用 `crate::routes`
/// 的类型（见模块文档），所以按同一形状自己构造。
fn anthropic_error(status: StatusCode, kind: &str, message: &str) -> Response {
    json_response(
        status,
        json!({
            "type": "error",
            "error": { "type": kind, "message": message },
        }),
    )
}

/// SSE 响应。
fn sse_response(rx: mpsc::Receiver<Result<Bytes, std::io::Error>>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(sse::body_from_receiver(rx))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Anthropic SSE 响应：把同一条 OpenAI SSE 逐事件转成 Anthropic 事件流。
///
/// `AnthropicSseStream` 要求内层 `S: Unpin`，而 `stream_from_receiver` 产出的是
/// `unfold`（含 async block，**不是** `Unpin`）⇒ 必须 `Box::pin` 包一层。
/// 少了这一步会得到一句与运行时毫无关系的 `Unpin` 约束报错。
fn anthropic_sse_response(
    rx: mpsc::Receiver<Result<Bytes, std::io::Error>>,
    model: &str,
) -> Response {
    let inner = Box::pin(sse::stream_from_receiver(rx));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(anthropic::AnthropicSseStream::new(
            inner,
            model.to_string(),
        )))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// 32 位 hex，用作 trace / request id 的原料。
fn uuid_like() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// W3C traceparent 形态的上游追踪号：`00-<32hex>-<32hex>-01`（71 字符）。
///
/// 长度是硬约束：`x-custom-trace-id` 取 `[..16]`、`x-flow-traceparent` 取 `[3..35]`，
/// 越界会 panic。所以这里不做「短一点更省」的优化。
fn trace_id() -> String {
    format!("00-{}-{}-01", uuid_like(), uuid_like())
}

/// 截断到 `max` 个**字符**（不是字节），避免把多字节字符切一半。
fn preview(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(max).collect();
    format!("{head}…（共 {} 字符）", trimmed.chars().count())
}

/// 传输层失败的**可读**描述。
///
/// 用户该看到「连接超时」而不是 `error sending request for url (...)`，
/// 但原始错误也不能丢——排查时它是唯一线索。
///
/// 实现在 core 的 `modules::net`，**本 crate 不再自己维护一份**：
/// 两个 crate 各写一遍必然出现「同一个故障两种说法」，而且 core 那份还会
/// 额外展开 `source` 链（DNS / TCP / TLS / 超时的具体原因），排查时才够用。
fn describe_transport_error(error: &reqwest::Error) -> String {
    buddy_switch_core::modules::net::describe_transport_error(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trae::TraeGatewayConfig;

    /// ★ 粘性表的接线往返：绑定后能查到、解绑后查不到、`None` 键**整段跳过**。
    ///
    /// 这一层是 `StickyTable` 的**薄封装**，但封装自己也会错 —— 最典型的是
    /// 「`None` 键被当成空串键写进表里」，那会让所有没有会话 id 的客户端
    /// **共用同一个绑定**（表现为「不同对话互相串号」），而且不报错。
    ///
    /// （构造 `TraeGatewayState` 无磁盘副作用：`RequestLog::new` 只读、Key store 只存路径。）
    #[tokio::test]
    async fn sticky_bind_unbind_round_trip_and_none_key_is_inert() {
        let state = TraeGatewayState::new(TraeGatewayConfig::default());
        let key = Some("trae_work|deepseek-v4-flash|conv-1");

        assert_eq!(state.sticky_uid(key).await, None, "未绑定时应查不到");

        state.bind_sticky(key, "uid-a").await;
        assert_eq!(state.sticky_uid(key).await.as_deref(), Some("uid-a"));

        state.unbind_sticky(key).await;
        assert_eq!(state.sticky_uid(key).await, None, "解绑后必须查不到");

        // `None` 键（客户端没给会话 id）⇒ 不绑、不查、不报错。
        assert_eq!(state.sticky_uid(None).await, None);
        state.bind_sticky(None, "uid-a").await;
        assert_eq!(state.sticky_uid(None).await, None, "None 键不得产生任何绑定");
    }

    /// ★ 会话粘性键必须带**产品线**与**模型**。
    ///
    /// - 产品线：两条线的账号库分家，同名会话粘到一起会指向另一条线的账号；
    /// - 模型：同一会话切模型时应允许落到各自最合适的账号，否则「模型级限流只封锁该模型」
    ///   的优势会被粘性抵消。
    #[test]
    fn sticky_key_carries_variant_and_model() {
        let body = json!({"metadata": {"conversation_id": "conv-1"}});
        let work = sticky_key_of(&body, TraeVariant::TraeWork, "deepseek-v4-flash").unwrap();
        assert!(work.contains("conv-1"), "键里要带会话 id：{work}");

        assert_ne!(
            work,
            sticky_key_of(&body, TraeVariant::Trae, "deepseek-v4-flash").unwrap(),
            "不同产品线不得共享绑定"
        );
        assert_ne!(
            work,
            sticky_key_of(&body, TraeVariant::TraeWork, "glm-5.3").unwrap(),
            "不同模型不得共享绑定"
        );
        // 同一输入必须稳定（否则绑定永远命中不了）。
        assert_eq!(
            work,
            sticky_key_of(&body, TraeVariant::TraeWork, "deepseek-v4-flash").unwrap()
        );
    }

    /// ★ 客户端没给会话 id ⇒ 必须返回 `None`，调用方据此**整段跳过**粘性。
    ///
    /// **不得**退化成轮级键 —— 那会造出一个永不命中的绑定，
    /// 让「粘性会话数」看起来非 0 却毫无作用。
    #[test]
    fn sticky_key_is_none_without_a_conversation_id() {
        assert_eq!(sticky_key_of(&json!({}), TraeVariant::TraeWork, "m"), None);
        assert_eq!(
            sticky_key_of(
                &json!({"metadata": {"conversation_id": "   "}}),
                TraeVariant::TraeWork,
                "m"
            ),
            None,
            "空白串不算会话 id"
        );
        assert_eq!(
            sticky_key_of(&json!({"metadata": {}}), TraeVariant::TraeWork, "m"),
            None
        );
    }

    /// ★ 流式开关必须来自**原始** Anthropic 请求体。
    ///
    /// 回归背景（本用例写下之前刚修掉）：`to_upstream_request` 会把 `stream` 强制为
    /// `true`（上游只接受流式），若拿**转换后**的体判断，非流式客户端会收到事件流
    /// 而不是 JSON —— 而且不报错。
    ///
    /// 末段把「两个体在这一字段上必然不同」这个**前提**也钉住：否则哪天上游支持了
    /// 非流式，本用例会静默失去意义（仍绿，但已测不到东西）。
    #[test]
    fn anthropic_stream_flag_comes_from_the_raw_request() {
        assert!(!anthropic_wants_stream(&json!({})), "缺 stream ⇒ 非流式");
        assert!(!anthropic_wants_stream(&json!({"stream": false})));
        assert!(anthropic_wants_stream(&json!({"stream": true})));
        assert!(
            !anthropic_wants_stream(&json!({"stream": "yes"})),
            "非布尔值按非流式处理（不猜）"
        );

        let upstream = anthropic::to_upstream_request(&json!({"model": "m", "max_tokens": 1}));
        assert_eq!(
            upstream.get("stream"),
            Some(&json!(true)),
            "前提：上游请求体的 stream 被强制为 true —— 这正是不能拿它判断的原因"
        );
    }

    /// ★ 偏好优先级：**显式指定压过粘性**。    ///
    /// 粘性记录的是「上次成功的账号」，是一个陈旧观察；`preferred_uid` 是用户此刻的
    /// 明确要求。若让粘性压过它，用户改完设置后会看到「下一轮仍走旧账号」—— 设置不生效。
    #[test]
    fn explicit_preference_wins_over_sticky() {
        assert_eq!(resolve_preference("", Some("sticky")), Some("sticky"));
        assert_eq!(resolve_preference("", None), None, "两个来源都空 ⇒ 走自动择优");
        assert_eq!(
            resolve_preference("explicit", Some("sticky")),
            Some("explicit"),
            "显式指定必须压过粘性"
        );
        assert_eq!(resolve_preference("explicit", None), Some("explicit"));
    }

    #[test]
    fn trace_id_has_the_length_the_header_slices_require() {
        let trace = trace_id();
        // 71 = "00-" + 32 + "-" + 32 + "-01"
        assert_eq!(trace.len(), 71, "trace id 长度是切片安全的前提");
        assert!(trace.starts_with("00-"));
        assert!(trace.ends_with("-01"));
        // 这两处切片一旦越界就是 panic，必须在这里钉住。
        assert_eq!(&trace[..16].len(), &16);
        assert_eq!(&trace[3..35].len(), &32);
        assert_ne!(trace, trace_id());
    }

    #[test]
    fn uuid_like_is_32_hex_chars() {
        let value = uuid_like();
        assert_eq!(value.len(), 32);
        assert!(value.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn preview_truncates_by_char_not_by_byte() {
        assert_eq!(preview("abc", 10), "abc");
        assert_eq!(preview("  abc  ", 10), "abc");
        // 中文按字符截断：3 个字符不该被腰斩成乱码。
        let text = "一二三四五";
        let cut = preview(text, 3);
        assert!(cut.starts_with("一二三"));
        assert!(cut.contains("共 5 字符"));
        assert!(!cut.contains('\u{FFFD}'));
    }

    #[test]
    fn error_body_is_openai_shaped() {
        let response = openai_error(StatusCode::UNAUTHORIZED, "invalid_request_error", "缺少凭据");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = sse::error_body("no_healthy_account", "没有可用账号");
        assert_eq!(body["error"]["code"], "no_healthy_account");
        assert_eq!(body["error"]["type"], "api_error");
        assert_eq!(body["error"]["message"], "没有可用账号");
    }

    #[test]
    fn no_account_failure_message_is_actionable_and_region_aware() {
        // 这条文案是用户唯一能看到的排障线索：必须包含「下一步做什么」且**点名区域**。
        let cn = no_account_message(TraeVariant::TraeWork, &[], None);
        assert!(cn.contains("账号管理"), "{cn}");
        assert!(cn.contains("一键签到"), "{cn}");
        assert!(cn.contains("国内版"), "文案必须点名选中的区域: {cn}");

        // ⚠️ 契约在 2026-09-21 变了：网关的号池按**区域**分区（`pool.rs::sync_for`
        // 走 `entries_for_region`），Key 的归属也是区域，因此文案按区域取名。
        // `TraeWork` 与 `Trae` 是**程序位**、同属国内区域 ⇒ 两段文案必须**逐字相同**。
        // 若哪天又按程序位取名（用户会在账号管理页找不到那个入口），这条 `assert_eq!` 会红。
        let trae = no_account_message(TraeVariant::Trae, &[], None);
        assert_eq!(cn, trae, "国内两个程序位共用一本账号库，文案不得按程序位分叉");

        // 国际版必须是另一个名字（否则用户分不清该去哪一版加账号）。
        let global = no_account_message(TraeVariant::Global, &[], None);
        assert!(global.contains("国际版"), "{global}");
        assert_ne!(cn, global, "两个区域的文案必须可区分: {cn} / {global}");

        // 有逐账号原因时，原因必须出现在文案里。
        let with_reason = no_account_message(
            TraeVariant::Trae,
            &["主号(7481920:冷却中,积分=0)".to_string()],
            None,
        );
        assert!(with_reason.contains("国内版"), "{with_reason}");
        assert!(with_reason.contains("冷却中"), "{with_reason}");

        // 「另一个区域有账号」的提示必须**双向**都给。
        // 方向①：国际版 Key + 只有国内账号（R8①的原始场景）。
        let hinted_global = no_account_message(TraeVariant::Global, &[], Some(TraeRegion::Cn));
        assert!(
            hinted_global.contains("国际版"),
            "选中的区域必须出现: {hinted_global}"
        );
        assert!(
            hinted_global.contains("国内版"),
            "另一个有账号的区域必须出现（否则用户不知道该往哪加 Key）: {hinted_global}"
        );
        // 方向②：国内版 Key + 只有国际版账号。**这条在修复前做不到** ——
        // 旧实现遍历的是 `TraeVariant::all()`（两条国内程序位），候选里根本没有国际版。
        let hinted_cn = no_account_message(TraeVariant::TraeWork, &[], Some(TraeRegion::Global));
        assert!(hinted_cn.contains("国内版"), "{hinted_cn}");
        assert!(
            hinted_cn.contains("国际版"),
            "另一个有账号的区域必须出现: {hinted_cn}"
        );
    }

    #[test]
    fn status_query_variant_parsing_falls_back_to_default() {
        assert_eq!(parse_variant_query(Some("variant=trae_cn")), TraeVariant::Trae);
        assert_eq!(parse_variant_query(Some("variant=trae_work")), TraeVariant::TraeWork);
        // 缺失 / 未知 → 默认（TraeWork），老客户端行为不变。
        assert_eq!(parse_variant_query(None), TraeVariant::default());
        assert_eq!(parse_variant_query(Some("")), TraeVariant::default());
        assert_eq!(parse_variant_query(Some("variant=unknown")), TraeVariant::default());
        assert_eq!(parse_variant_query(Some("days=7")), TraeVariant::default());
    }

    /// **实测探针**：上游认不认 TraeCode 的 `function` 与模型名（issue #4「看得见、调不动」）。
    ///
    /// # 为什么必须真发请求
    ///
    /// 网关发上游的 `config_name` / `model_name` **无法从客户端本地推导**：
    /// TraeWork 的清单每条都带 `config_name`，而 **TraeCode 的清单里根本没有这个字段**
    /// （本机 2026-09-30 逐条核对）。所以「`glm-5.3-flash` 该配哪个 `function`、
    /// 哪个上游模型名」只能问上游。本仓的既有裁定也是这一条：映射对不对**只能实测**。
    ///
    /// # 会真实调用上游（消耗少量积分）
    ///
    /// 6 次极短请求（prompt = `ping`）。**不写任何状态**：直接调 [`send_llm_chat`]，
    /// 不经过 [`attempt`] ⇒ 不写冷却、不落错误、不动粘性表。账号库只读。
    ///
    /// 运行：`cargo test -p buddy-switch-gateway --lib -- --ignored --nocapture probe_traecode`
    ///
    /// # 判读方式（★ 有对照，不是裸测）
    ///
    /// - `C1` 是**转录对照**：`solo_work_lite` + `glm-5.3` 是网关**当前已在用**的组合，
    ///   它必须成功 —— 若它也失败，说明探针本身（请求体 / 头 / 凭据）有问题，
    ///   后面几行**一律不可采信**（假阴性）。
    /// - `C2` 是**function 对照**：`glm-5.3` 也在 TraeCode 的 `chat_v3` 清单里。
    ///   若 C2 成功而 `V2` 失败 ⇒ 差别只在模型名；若 C2 也失败 ⇒ `chat_v3` 这个
    ///   function 名不对（或该账号没有 TraeCode 权益）。
    #[tokio::test]
    #[ignore = "实测：会真实调用上游并消耗少量积分；需本机有可用 Trae 账号"]
    async fn probe_traecode_model_names() {
        let state = TraeGatewayState::new(TraeGatewayConfig::default());

        // 真实 home 的 CN 池：只读选号（`sync_for` 只读账号库 / 冷却 / 积分）。
        let picked = {
            let mut pool = TraePool::for_variant(TraeVariant::TraeWork);
            pool.sync_for(TraeVariant::TraeWork);
            pool.pick(now_secs(), &HashSet::new(), None)
        };
        let Some(picked) = picked else {
            panic!("本机 CN 账号池没有可用账号，探针无法进行");
        };
        println!("[probe] 账号 {}（uid {}）", picked.name, picked.uid);

        // (标签, function, config_name, model_name)
        let cases: [(&str, &str, &str, &str); 13] = [
            ("C1 对照·已知可用", "solo_work_lite", "glm-5.3", "glm-5.3__dev"),
            ("C2 function 对照", "chat_v3", "glm-5.3", "glm-5.3__dev"),
            ("V1 错配 function", "solo_work_lite", "glm-5.3-flash", "glm-5.3-flash__dev"),
            ("V2 正配·猜 __dev", "chat_v3", "glm-5.3-flash", "glm-5.3-flash__dev"),
            ("V3 无 __dev 后缀", "chat_v3", "glm-5.3-flash", "glm-5.3-flash"),
            ("V4 换 solo_agent", "solo_agent", "glm-5.3-flash", "glm-5.3-flash__dev"),
            // ★ 决定「`/v1/models` 是否过度宣传」：`Doubao-Seed-Code` 只出现在 TraeWork 的
            //   `solo_coder` 分组里，**不在** `solo_work_lite`。若它也 4001 ⇒ 当前对外清单
            //   里那些「不属于本 function 的模型」同样调不动（与 glm-5.3-flash 同一类缺陷）。
            ("V5 分组外的模型", "solo_work_lite", "Doubao-Seed-Code", "Doubao-Seed-Code__dev"),
            // ★ 确认 `config_name = name` / `model_name = name + "__dev"` 这条规则能推广。
            ("V6 规则推广", "chat_v3", "qwen3.8-flash", "qwen3.8-flash__dev"),
            // ★★ 第三轮：旧对外清单里那三个「既不在客户端 `solo_work_lite` 分组里、
            //   也没被实测过」的名字 —— 决定它们是**保留**还是**从清单里删掉**。
            ("X1 旧名 deepseek-v4-pro", "solo_work_lite", "DeepSeek-V4-Pro", "deepseek_v4_pro__dev"),
            ("X2 旧名 glm-5-turbo", "solo_work_lite", "glm-5-turbo", "glm-5-turbo__dev"),
            ("X3 旧名 glm-5", "solo_work_lite", "glm-5", "glm-5__dev"),
            // ★★ 第四轮：候选**新默认模型**（旧默认 `deepseek-v4-flash` 是客户端已不再
            //   提供的旧名，虽仍可用；换默认前必须先量新名字能不能过）。
            ("Y1 新默认候选", "solo_work_lite", "deepseek-v4.1-flash", "deepseek-v4.1-flash__dev"),
            ("Y2 正式版候选", "solo_work_lite", "DeepSeek-V4-Flash-Official", "DeepSeek-V4-Flash-Official__dev"),
        ];

        for (label, function, config_name, model_name) in cases {
            let src = format!(
                r#"{{"model":"{config_name}","messages":[{{"role":"user","content":"ping"}}]}}"#
            );
            let converted = payload::prepare_llm_chat_body(
                src.as_bytes(),
                TraeVariant::TraeWork,
                config_name,
                &picked.uid,
                &picked.device_id,
                &picked.machine_id,
            );
            // 只改这三个字段 —— 其余字段与真实流量逐字相同（复用同一个构造器）。
            let mut body: Value =
                serde_json::from_slice(&converted).expect("构造器产出必然是合法 JSON");
            body["function"] = serde_json::json!(function);
            body["config_name"] = serde_json::json!(config_name);
            body["model_name"] = serde_json::json!(model_name);
            let encoded = serde_json::to_vec(&body).expect("序列化");

            match send_llm_chat(&state, &picked, &encoded, "").await {
                Ok(mut response) => {
                    // 累积若干块，直到看见 `event:metadata`（上游认了）或 `event:error`（上游拒了）。
                    // ⚠️ 只看**首块**会把 `event:progress_notice`（排队中）误判成结论 —— 踩过。
                    let mut buffer = String::new();
                    let mut verdict = "<45s 内没等到 metadata/error>".to_string();
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
                    while std::time::Instant::now() < deadline {
                        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                        match tokio::time::timeout(remaining, response.chunk()).await {
                            Ok(Ok(Some(bytes))) => {
                                buffer.push_str(&String::from_utf8_lossy(&bytes));
                                if let Some(index) = buffer.find("event:error") {
                                    verdict = buffer[index..]
                                        .chars()
                                        .take(160)
                                        .collect::<String>()
                                        .replace('\n', "\\n");
                                    break;
                                }
                                if buffer.contains("event:metadata") {
                                    verdict = "event:metadata（上游接受）".to_string();
                                    break;
                                }
                            }
                            Ok(Ok(None)) => {
                                verdict = "<响应体结束，未出现 metadata/error>".to_string();
                                break;
                            }
                            Ok(Err(error)) => {
                                verdict = format!("<读 body 失败：{error}>");
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                    println!(
                        "[{tag}] {label}  function={function} config_name={config_name} model_name={model_name}\n        HTTP {} | {verdict}",
                        response.status(),
                        tag = if verdict.starts_with("event:metadata") { "OK  " } else { "FAIL" },
                    );
                }
                Err((status, detail)) => println!(
                    "[FAIL] {label}  function={function} config_name={config_name} model_name={model_name}\n        HTTP {status} | {detail}"
                ),
            }
        }
    }

    /// **第二轮实测**：枚举本机 TraeWork `solo_work_lite` 分组里的**每一个**模型，
    /// 逐个问上游「认不认」—— 结论直接决定静态兜底清单 `MODEL_NAMES` 该收哪些名字。
    ///
    /// 与上一条的分工：那条钉**规则**（function 白名单 / 名字派生），这条钉**清单内容**。
    /// 两者都 `#[ignore]`、都真实调用上游、都不写任何状态。
    ///
    /// 运行：`cargo test -p buddy-switch-gateway --lib -- --ignored --nocapture probe_work_list`
    #[tokio::test]
    #[ignore = "实测：会真实调用上游并消耗少量积分；需本机有可用 Trae 账号"]
    async fn probe_work_list_names() {
        use buddy_switch_core::modules::trae::model_list::read_client_model_list;

        let state = TraeGatewayState::new(TraeGatewayConfig::default());
        let picked = {
            let mut pool = TraePool::for_variant(TraeVariant::TraeWork);
            pool.sync_for(TraeVariant::TraeWork);
            pool.pick(now_secs(), &HashSet::new(), None)
        };
        let Some(picked) = picked else {
            panic!("本机 CN 账号池没有可用账号");
        };

        let function = crate::trae::function_for(TraeVariant::TraeWork);
        let list = read_client_model_list(TraeVariant::TraeWork);
        let group = list.groups.iter().find(|group| group.function == function);
        let Some(group) = group else {
            panic!("本机客户端清单里没有 `{function}` 分组，无法枚举");
        };
        println!(
            "[probe] 账号 {} | 分组 `{function}` 共 {} 条 | 客户端数据目录 {:?}",
            picked.name,
            group.models.len(),
            list.data_dir
        );

        // 负对照：客户端清单里**不存在**的名字，必须被拒（否则说明探针没有鉴别力）。
        let mut names: Vec<String> = vec!["sagitta".to_string(), "aquila".to_string()];
        names.extend(group.models.iter().map(|model| model.name.clone()));

        for name in names {
            let (config_name, model_name) = payload::model_config(&name);
            let src = format!(r#"{{"model":"{name}","messages":[{{"role":"user","content":"ping"}}]}}"#);
            let converted = payload::prepare_llm_chat_body(
                src.as_bytes(),
                TraeVariant::TraeWork,
                &name,
                &picked.uid,
                &picked.device_id,
                &picked.machine_id,
            );
            let mut body: Value = serde_json::from_slice(&converted).expect("构造器产出合法 JSON");
            body["function"] = serde_json::json!(function);
            body["config_name"] = serde_json::json!(config_name);
            body["model_name"] = serde_json::json!(model_name);
            let encoded = serde_json::to_vec(&body).expect("序列化");

            match send_llm_chat(&state, &picked, &encoded, "").await {
                Ok(mut response) => {
                    let mut buffer = String::new();
                    let mut verdict = "超时".to_string();
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
                    while std::time::Instant::now() < deadline {
                        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                        match tokio::time::timeout(remaining, response.chunk()).await {
                            Ok(Ok(Some(bytes))) => {
                                buffer.push_str(&String::from_utf8_lossy(&bytes));
                                if let Some(index) = buffer.find("event:error") {
                                    verdict = buffer[index..].chars().take(90).collect();
                                    break;
                                }
                                if buffer.contains("event:metadata") {
                                    verdict = "OK".to_string();
                                    break;
                                }
                            }
                            Ok(Ok(None)) => break,
                            Ok(Err(error)) => {
                                verdict = format!("读 body 失败：{error}");
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                    println!("[{:4}] {name:34} config_name={config_name:34} {verdict}", if verdict == "OK" { "OK" } else { "FAIL" });
                }
                Err((status, detail)) => println!("[FAIL] {name:34} HTTP {status} | {detail}"),
            }
        }
    }
}
