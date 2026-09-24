//! 出站请求体改写管线（对照参考实现 `internal/upstream/` 的 payload 管线）。
//!
//! 步骤顺序**不得调整**——每一步都依赖前一步的产物（如 `reasoning_effort`
//! 降级必须在 `thinking` 注入之后，否则注入的默认档位不会被按模型归一）：
//!
//! 0. 提示词体系（`custom` 恒替换 / `passthrough` 仅降级期替换）
//! 1. 基础规范化：强制 `stream:true`、`developer`→`system`、`tool_choice` 扁平化
//!    （复用 `buddy-switch-core` 的唯一实现，避免两处规则漂移）
//! 2. `stream_options.include_usage` 注入
//! 3. 孤儿 `tool_calls` 清理
//! 4. DeepSeek 思维链：`thinking` 注入 → `reasoning_effort` 归一 → `reasoning_content` 回填
//! 5. 指纹脱敏（`sanitize_fingerprints` 开启时）
//! 6. `prompt_cache_key` 注入
//! 7. 国际版前置最小 system（`ensure_console_system`）
//!
//! 设计约束：管线对**不可解析的请求体**一律原样返回，绝不让格式化成为新的失败点。

pub mod effort;
pub mod prompt;
pub mod sanitize;
pub mod thinking;

use std::collections::HashSet;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use buddy_switch_core::modules::region::Region;
use buddy_switch_core::modules::upstream::{prepare_chat_body, INTERNATIONAL_SYSTEM_PROMPT};

pub use effort::{adjust_effort, contains_effort, effort_rank, lookup_default_effort, EffortSpec};
pub use prompt::{
    rewrite_system_prompt, DegradeGate, PromptMode, PromptSettings, BUILTIN_SYSTEM_PROMPT,
    DEGRADED_SYSTEM_PROMPT,
};
pub use sanitize::{has_fingerprint, sanitize_messages, sanitize_text};
pub use thinking::{backfill_reasoning_content, inject_thinking, is_deepseek_model};

/// 出站改写选项（由网关配置派生）。
#[derive(Debug, Clone, Default)]
pub struct OutboundOptions {
    /// 是否启用指纹脱敏（配置 `features.sanitize_blacklist_fingerprints`，默认 true）。
    pub sanitize_fingerprints: bool,
    /// 提示词体系设置。
    pub prompt: PromptSettings,
}

/// 单次出站改写所需的会话上下文。
#[derive(Debug, Clone, Default)]
pub struct OutboundMeta {
    /// 账号 uid（`prompt_cache_key` 派生用）。
    pub uid: String,
    /// 会话 id 回落值（请求体 `metadata` 内没有时使用）。
    pub conversation_id: Option<String>,
}

/// 按参考实现顺序执行完整出站改写。
///
/// `degraded` 表示当前处于内容拦截降级期（决定 `passthrough` 模式是否改用降级提示词）。
pub fn prepare_outbound_body(
    region: Region,
    source: &str,
    options: &OutboundOptions,
    meta: &OutboundMeta,
    degraded: bool,
) -> String {
    // 步骤 0：提示词体系。
    let after_prompt = match options.prompt.active_text(degraded) {
        Some(text) => prompt::rewrite_system_prompt(source, text),
        None => source.to_string(),
    };

    // 步骤 1：基础规范化（core 唯一实现）。
    let base = prepare_chat_body(&after_prompt);
    let Ok(mut body) = serde_json::from_str::<Value>(&base) else {
        return base;
    };
    let Some(obj) = body.as_object_mut() else {
        return base;
    };

    // 步骤 2：usage 上报（上游按流式返回，非流式聚合依赖末帧 usage）。
    if !obj.contains_key("stream_options") {
        obj.insert(
            "stream_options".to_string(),
            json!({ "include_usage": true }),
        );
    }

    // 步骤 3：孤儿工具调用。
    cleanup_orphan_tool_calls(obj);

    // 步骤 4：DeepSeek 思维链（仅 DeepSeek 系模型）。
    let model = obj
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if is_deepseek_model(&model) {
        let default_effort = lookup_default_effort(region, &model);
        inject_thinking(obj, &default_effort);
        normalize_effort_in_place(region, &model, obj);
        backfill_reasoning_content(obj);
    }

    // 步骤 5：指纹脱敏。
    if options.sanitize_fingerprints {
        if let Some(Value::Array(messages)) = obj.get_mut("messages") {
            sanitize::sanitize_messages(messages);
        }
    }

    // 步骤 6：缓存键。
    inject_prompt_cache_key(obj, &meta.uid, meta.conversation_id.as_deref());

    // 步骤 7：国际版前置最小 system。
    if region == Region::Global {
        ensure_console_system(obj);
    }

    serde_json::to_string(&body).unwrap_or(base)
}

/// 按模型支持档位归一 `reasoning_effort`（原地改写）。
///
/// 字段探测顺序 `reasoning_effort` → `reasoningEffort`；两者都无、值非字符串、
/// 或值不是已知档位 → 不改写。模型名精确匹配（非前缀）。
fn normalize_effort_in_place(region: Region, model: &str, obj: &mut Map<String, Value>) {
    let key = if obj.get("reasoning_effort").and_then(Value::as_str).is_some() {
        "reasoning_effort"
    } else if obj.get("reasoningEffort").and_then(Value::as_str).is_some() {
        "reasoningEffort"
    } else {
        return;
    };
    let requested = obj
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if effort::effort_rank(&requested).is_none() {
        return;
    }
    if let Some(adjustment) = effort::adjust_effort(region, model, &requested) {
        obj.insert(key.to_string(), json!(adjustment.effort));
    }
}

/// 清理孤儿工具调用。
///
/// 规则：前两条对齐参考实现 `cleanupOrphanToolCalls`；第 3 条**刻意收紧**（理由见下）：
/// - 保留集 = `assistant.tool_calls[].id` ∩ `role=="tool"` 的 `tool_call_id`；
/// - `assistant` 消息只要**批内任一** `id` 不在保留集 → 整批删除 `tool_calls`；
/// - `role=="tool"` 且 `tool_call_id` 不在**存活批次**内 → **整条消息删除**。
///
/// ★ 第 3 条与参考实现是**刻意分歧**（勿按「对齐参考实现」改回）：参考实现按「保留集」
/// 放行工具结果，在部分孤儿批次（`[A,B]` 只有 `A` 有结果）下会残留悬空的 `role:"tool"`，
/// 与它自己头注释「反之 `role:tool` 也必须有对应的前置 `tool_call`……缺任一侧上游都会以
/// HTTP 400 拒绝整个请求」相矛盾（其测试只断言 `tool_calls` 被删，从不断言那条 `tool`
/// 消息的去向）。本项目改为与批次裁决同源的「存活批次」集合：删除与放行以同一套 id 为准。
///
/// 「存活批次」而非「保留集」是必须的区分：批次 `[A,B]` 只有 `A` 有结果时，
/// `保留集 = {A}`，但该批次因 `B` 是孤儿而**整批**被删——此时若按保留集放行，
/// `A` 的 `role:"tool"` 结果就会悬空，上游照样判协议错误（400
/// `tool_call_sequence_broken`）。删除与放行必须以同一套 id 为准。
///
/// 无工具流量时不做任何改动。残留的孤儿对会被上游判协议错误（400），
/// 因此这一步是「历史被截断/被裁剪」场景下的必要自愈。
fn cleanup_orphan_tool_calls(obj: &mut Map<String, Value>) {
    let Some(Value::Array(messages)) = obj.get_mut("messages") else {
        return;
    };

    let mut call_ids: HashSet<String> = HashSet::new();
    let mut result_ids: HashSet<String> = HashSet::new();
    for message in messages.iter() {
        let Some(wrapped) = message.as_object() else {
            continue;
        };
        if wrapped.get("role").and_then(Value::as_str) == Some("tool") {
            if let Some(id) = wrapped.get("tool_call_id").and_then(Value::as_str) {
                result_ids.insert(id.to_string());
            }
        }
        if let Some(calls) = wrapped.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                if let Some(id) = call.get("id").and_then(Value::as_str) {
                    call_ids.insert(id.to_string());
                }
            }
        }
    }
    if call_ids.is_empty() && result_ids.is_empty() {
        return;
    }

    let keep: HashSet<String> = call_ids.intersection(&result_ids).cloned().collect();

    // 第一趟：裁决每个 assistant 批次，并记录真正存活下来的 tool_call id。
    let mut surviving: HashSet<String> = HashSet::new();
    for message in messages.iter_mut() {
        let Some(wrapped) = message.as_object_mut() else {
            continue;
        };
        let batch: Vec<String> = wrapped
            .get("tool_calls")
            .and_then(Value::as_array)
            .map(|calls| {
                calls
                    .iter()
                    .map(|call| {
                        call.get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    })
                    .collect()
            })
            .unwrap_or_default();
        if batch.is_empty() {
            continue;
        }
        if batch.iter().any(|id| !keep.contains(id)) {
            wrapped.remove("tool_calls");
        } else {
            surviving.extend(batch);
        }
    }

    // 第二趟：工具结果只认存活批次。
    messages.retain(|message| {
        let Some(wrapped) = message.as_object() else {
            return true;
        };
        if wrapped.get("role").and_then(Value::as_str) != Some("tool") {
            return true;
        }
        let id = wrapped
            .get("tool_call_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        surviving.contains(id)
    });
}

/// 注入 `prompt_cache_key`（请求体已有非空值时不动）。
///
/// 键形态：`wb2a-<uid 前 8 位>-<sha256(uid|conversation)[..16] hex>`。
/// 会话 id 取 `metadata.conversation_id` → `metadata.conversationId` → 调用方回落值。
fn inject_prompt_cache_key(
    obj: &mut Map<String, Value>,
    uid: &str,
    fallback_conversation: Option<&str>,
) {
    let already_set = obj
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    if already_set {
        return;
    }

    let conversation = obj
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|metadata| {
            metadata
                .get("conversation_id")
                .or_else(|| metadata.get("conversationId"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            fallback_conversation
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    let Some(conversation) = conversation else {
        return;
    };

    obj.insert(
        "prompt_cache_key".to_string(),
        json!(prompt_cache_key(uid, &conversation)),
    );
}

/// 派生缓存键：`wb2a-<uid8>-<hash16>`。空 uid 的 uid8 段为 `-`。
pub fn prompt_cache_key(uid: &str, conversation_id: &str) -> String {
    let uid8: String = uid.chars().take(8).collect();
    let uid8 = if uid8.is_empty() { "-".to_string() } else { uid8 };

    let mut hasher = Sha256::new();
    hasher.update(uid.as_bytes());
    hasher.update(b"|");
    hasher.update(conversation_id.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect();

    format!("wb2a-{uid8}-{hex}")
}

/// 国际版前置最小 system：首条消息已是 `system` 则不动，否则**头部插入**（不合并、不改序）。
fn ensure_console_system(obj: &mut Map<String, Value>) {
    let Some(Value::Array(messages)) = obj.get_mut("messages") else {
        return;
    };
    let first_is_system = messages
        .first()
        .and_then(Value::as_object)
        .and_then(|message| message.get("role"))
        .and_then(Value::as_str)
        == Some("system");
    if first_is_system {
        return;
    }
    messages.insert(
        0,
        json!({ "role": "system", "content": INTERNATIONAL_SYSTEM_PROMPT }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(sanitize: bool, prompt: PromptSettings) -> OutboundOptions {
        OutboundOptions {
            sanitize_fingerprints: sanitize,
            prompt,
        }
    }

    fn meta() -> OutboundMeta {
        OutboundMeta {
            uid: "u-1234567890".to_string(),
            conversation_id: Some("conv-fallback".to_string()),
        }
    }

    fn cn(source: &str) -> Value {
        let out = prepare_outbound_body(
            Region::Cn,
            source,
            &options(false, PromptSettings::default()),
            &meta(),
            false,
        );
        serde_json::from_str(&out).expect("产出必须是合法 JSON")
    }

    #[test]
    fn cn_pipeline_forces_stream_and_stream_options() {
        let value = cn(r#"{"model":"glm-5.2","messages":[{"role":"user","content":"hi"}]}"#);
        assert_eq!(value["stream"], json!(true));
        assert_eq!(value["stream_options"]["include_usage"], json!(true));
    }

    #[test]
    fn existing_stream_options_are_not_overwritten() {
        let value = cn(
            r#"{"model":"glm-5.2","stream_options":{"include_usage":false},"messages":[]}"#,
        );
        assert_eq!(
            value["stream_options"]["include_usage"],
            json!(false),
            "客户端显式配置必须保留"
        );
    }

    #[test]
    fn deepseek_gets_thinking_effort_and_reasoning_backfill() {
        let value = cn(
            r#"{"model":"deepseek-v4.1-flash","messages":[
                {"role":"assistant","reasoning":"think","content":"a"},
                {"role":"user","content":"b"}
            ]}"#,
        );
        assert_eq!(value["thinking"], json!({"type": "enabled"}));
        assert_eq!(value["reasoning_effort"], json!("high"));
        assert_eq!(value["messages"][0]["reasoning_content"], json!("think"));
    }

    #[test]
    fn non_deepseek_model_never_gets_thinking_fields() {
        let value = cn(r#"{"model":"glm-5.2","messages":[{"role":"user","content":"hi"}]}"#);
        assert!(value.get("thinking").is_none(), "非 DeepSeek 不得注入 thinking");
        assert!(value.get("reasoning_effort").is_none());
    }

    #[test]
    fn effort_is_downgraded_for_deepseek_pro() {
        // deepseek-v4-pro: low/high/xhigh；请求 max → xhigh
        let value = cn(
            r#"{"model":"deepseek-v4-pro","reasoning_effort":"max","messages":[]}"#,
        );
        assert_eq!(value["reasoning_effort"], json!("xhigh"));
    }

    #[test]
    fn global_uses_its_own_effort_table() {
        let out = prepare_outbound_body(
            Region::Global,
            r#"{"model":"deepseek-v4.1-flash","reasoning_effort":"low","messages":[]}"#,
            &options(false, PromptSettings::default()),
            &meta(),
            false,
        );
        let value: Value = serde_json::from_str(&out).unwrap();
        // Global 表该模型仅 high → 请求 low 被 floored 到 high
        assert_eq!(value["reasoning_effort"], json!("high"));
    }

    #[test]
    fn global_prepends_console_system_but_cn_does_not() {
        let source = r#"{"model":"glm-5.2","messages":[{"role":"user","content":"hi"}]}"#;
        let cn_value = cn(source);
        assert_eq!(
            cn_value["messages"].as_array().unwrap().len(),
            1,
            "CN 不得前置 system"
        );

        let out = prepare_outbound_body(
            Region::Global,
            source,
            &options(false, PromptSettings::default()),
            &meta(),
            false,
        );
        let global_value: Value = serde_json::from_str(&out).unwrap();
        let messages = global_value["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], json!(INTERNATIONAL_SYSTEM_PROMPT));
        assert_eq!(messages[1]["content"], json!("hi"));
    }

    #[test]
    fn global_keeps_existing_leading_system() {
        let out = prepare_outbound_body(
            Region::Global,
            r#"{"model":"glm-5.2","messages":[{"role":"system","content":"keep"}]}"#,
            &options(false, PromptSettings::default()),
            &meta(),
            false,
        );
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["messages"].as_array().unwrap().len(), 1);
        assert_eq!(value["messages"][0]["content"], json!("keep"));
    }

    #[test]
    fn sanitize_is_off_by_default_and_on_when_enabled() {
        let source = r#"{"model":"glm-5.2","messages":[{"role":"user","content":"code 11128"}]}"#;
        assert_eq!(cn(source)["messages"][0]["content"], json!("code 11128"));

        let out = prepare_outbound_body(
            Region::Cn,
            source,
            &options(true, PromptSettings::default()),
            &meta(),
            false,
        );
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["messages"][0]["content"], json!("code 11-128"));
    }

    #[test]
    fn orphan_tool_calls_are_cleaned() {
        let value = cn(
            r#"{"model":"glm-5.2","messages":[
                {"role":"assistant","content":null,"tool_calls":[{"id":"ok","type":"function","function":{"name":"a","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"ok","content":"result"},
                {"role":"assistant","content":null,"tool_calls":[{"id":"orphan","type":"function","function":{"name":"b","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"ghost","content":"dangling"}
            ]}"#,
        );
        let messages = value["messages"].as_array().expect("数组");
        assert_eq!(
            messages.len(),
            3,
            "孤儿的 role=tool 消息被删除，assistant 消息保留但清空 tool_calls"
        );
        assert!(
            messages[0].get("tool_calls").is_some(),
            "配对完整的 tool_calls 必须保留"
        );
        assert_eq!(messages[1]["role"], json!("tool"), "配对成功的 tool 消息保留");
        assert_eq!(messages[2]["role"], json!("assistant"));
        assert!(
            messages[2].get("tool_calls").is_none(),
            "含孤儿 id 的 assistant 批次应整批删除 tool_calls"
        );
        assert!(
            !messages
                .iter()
                .any(|message| message["tool_call_id"] == json!("ghost")),
            "悬空 tool_call_id 的消息必须整条移除"
        );
    }

    #[test]
    fn orphan_cleanup_is_noop_without_tool_traffic() {
        let value = cn(r#"{"model":"glm-5.2","messages":[{"role":"user","content":"hi"}]}"#);
        assert_eq!(value["messages"].as_array().unwrap().len(), 1);
    }

    /// 部分孤儿批次：`[A,B]` 只有 `A` 有结果 → 批次整批删除，`A` 的结果必须一起删。
    ///
    /// 若按「保留集」（`{A}`）放行工具结果，就会留下悬空的 `role:"tool"` 消息，
    /// 上游判 `tool_call_sequence_broken`（400）。
    #[test]
    fn partially_orphan_batch_does_not_leave_dangling_tool_result() {
        let value = cn(
            r#"{"model":"glm-5.2","messages":[
                {"role":"user","content":"go"},
                {"role":"assistant","content":null,"tool_calls":[
                    {"id":"a","type":"function","function":{"name":"x","arguments":"{}"}},
                    {"id":"b","type":"function","function":{"name":"y","arguments":"{}"}}
                ]},
                {"role":"tool","tool_call_id":"a","content":"result"}
            ]}"#,
        );
        let messages = value["messages"].as_array().expect("数组");
        assert_eq!(
            messages.len(),
            2,
            "批次被整批删除后，其工具结果不得残留：{messages:?}"
        );
        assert!(
            messages[1].get("tool_calls").is_none(),
            "含孤儿 id 的 assistant 批次应整批删除 tool_calls"
        );
        assert!(
            !messages
                .iter()
                .any(|message| message["role"] == json!("tool")),
            "不得留下无主的工具结果：{messages:?}"
        );
    }

    #[test]
    fn prompt_cache_key_prefers_body_metadata() {
        let value = cn(
            r#"{"model":"glm-5.2","metadata":{"conversation_id":"body-conv"},"messages":[]}"#,
        );
        let key = value["prompt_cache_key"].as_str().expect("字符串");
        assert!(key.starts_with("wb2a-u-123456-"), "uid 只取前 8 位: {key}");
        assert_eq!(key, prompt_cache_key("u-1234567890", "body-conv"));
    }

    #[test]
    fn prompt_cache_key_falls_back_and_skips_when_absent() {
        let value = cn(r#"{"model":"glm-5.2","messages":[]}"#);
        assert_eq!(
            value["prompt_cache_key"],
            json!(prompt_cache_key("u-1234567890", "conv-fallback"))
        );

        let out = prepare_outbound_body(
            Region::Cn,
            r#"{"model":"glm-5.2","messages":[]}"#,
            &options(false, PromptSettings::default()),
            &OutboundMeta::default(),
            false,
        );
        let bare: Value = serde_json::from_str(&out).unwrap();
        assert!(
            bare.get("prompt_cache_key").is_none(),
            "无会话上下文时不得凭空造键"
        );
    }

    #[test]
    fn existing_prompt_cache_key_is_respected() {
        let value = cn(
            r#"{"model":"glm-5.2","prompt_cache_key":"mine","metadata":{"conversation_id":"c"},"messages":[]}"#,
        );
        assert_eq!(value["prompt_cache_key"], json!("mine"));
    }

    #[test]
    fn custom_prompt_replaces_client_system_end_to_end() {
        let settings = PromptSettings::load(PromptMode::Custom, None).unwrap();
        let value = cn_with(
            r#"{"model":"glm-5.2","messages":[{"role":"system","content":"client system"},{"role":"user","content":"hi"}]}"#,
            options(false, settings),
            false,
        );
        let messages = value["messages"].as_array().expect("数组");
        assert_eq!(messages[0]["content"], json!(BUILTIN_SYSTEM_PROMPT));
        assert_eq!(messages[1]["content"], json!("hi"));
    }

    #[test]
    fn passthrough_keeps_client_system_unless_degraded() {
        let source =
            r#"{"model":"glm-5.2","messages":[{"role":"system","content":"client system"},{"role":"user","content":"hi"}]}"#;
        let kept = cn_with(source, options(false, PromptSettings::default()), false);
        assert_eq!(
            kept["messages"][0]["content"],
            json!("client system"),
            "passthrough 非降级期必须透传"
        );

        let degraded = cn_with(source, options(false, PromptSettings::default()), true);
        assert_eq!(
            degraded["messages"][0]["content"],
            json!(DEGRADED_SYSTEM_PROMPT),
            "降级期必须换成中性提示词"
        );
    }

    fn cn_with(source: &str, options: OutboundOptions, degraded: bool) -> Value {
        let out = prepare_outbound_body(Region::Cn, source, &options, &meta(), degraded);
        serde_json::from_str(&out).expect("产出必须是合法 JSON")
    }

    #[test]
    fn unparseable_body_is_returned_verbatim() {
        let out = prepare_outbound_body(
            Region::Cn,
            "not json",
            &options(false, PromptSettings::default()),
            &meta(),
            false,
        );
        assert_eq!(out, "not json");
    }

    #[test]
    fn pipeline_is_idempotent() {
        let source = r#"{"model":"deepseek-v4.1-flash","messages":[{"role":"assistant","reasoning":"t","content":"a"},{"role":"user","content":"b"}]}"#;
        let once = prepare_outbound_body(
            Region::Cn,
            source,
            &options(true, PromptSettings::default()),
            &meta(),
            false,
        );
        let twice = prepare_outbound_body(
            Region::Cn,
            &once,
            &options(true, PromptSettings::default()),
            &meta(),
            false,
        );
        assert_eq!(once, twice, "管线重复执行必须稳定");
    }

    #[test]
    fn prompt_cache_key_shape_is_stable() {
        let key = prompt_cache_key("", "c");
        assert!(key.starts_with("wb2a--"), "空 uid 的 uid8 段应为 -: {key}");
        let key = prompt_cache_key("abcdefghij", "c");
        assert!(key.starts_with("wb2a-abcdefgh-"), "超长 uid 截断到 8: {key}");
    }
}
