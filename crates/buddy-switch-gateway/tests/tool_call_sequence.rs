//! 工具调用序列自洽性（Anthropic 入站 → 出站改写后的上游形态）。
//!
//! 上游（WorkBuddy）对 `assistant.tool_calls` 与 `role:"tool"` 做**序列**校验：
//! 每条工具调用必须紧跟同 id 的工具结果，反之亦然。不自洽时整单被拒：
//! `400 {"code":11148,"extError":{"code":"tool_call_sequence_broken"}}`。
//!
//! 这两条路径分别由两个模块负责，缺陷也各自独立，故在**跨模块**层面钉住不变量：
//! - `protocol::anthropic::to_upstream_request`：同一条 user 消息内的文本块与工具结果
//!   的相对顺序；
//! - `outbound::prepare_outbound_body`：孤儿清理时「删除」与「放行」是否用了同一套 id。

use buddy_switch_core::modules::region::Region;
use buddy_switch_gateway::outbound::{prepare_outbound_body, OutboundMeta, OutboundOptions};
use buddy_switch_gateway::protocol::anthropic::to_upstream_request;
use serde_json::{json, Value};

/// 校验工具调用序列：每条 `assistant.tool_calls` 后面必须紧邻 id 一一对应的
/// `role:"tool"` 消息，且不存在无主（悬空）的 `role:"tool"` 消息。
fn check_tool_sequence(messages: &[Value]) -> Result<(), String> {
    let mut index = 0usize;
    while index < messages.len() {
        let message = &messages[index];
        match message.get("role").and_then(Value::as_str) {
            Some("tool") => {
                return Err(format!(
                    "位置 {index}：`role:\"tool\"` 消息没有紧跟在带 tool_calls 的 assistant 之后 \
                     —— 该消息是悬空的：{message}"
                ));
            }
            Some("assistant") => {
                let Some(calls) = message.get("tool_calls").and_then(Value::as_array) else {
                    index += 1;
                    continue;
                };
                let expected: Vec<&str> = calls
                    .iter()
                    .filter_map(|call| call.get("id").and_then(Value::as_str))
                    .collect();
                let mut actual: Vec<&str> = Vec::new();
                let mut cursor = index + 1;
                while cursor < messages.len()
                    && messages[cursor].get("role").and_then(Value::as_str) == Some("tool")
                {
                    actual.push(
                        messages[cursor]
                            .get("tool_call_id")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    );
                    cursor += 1;
                }
                if expected != actual {
                    return Err(format!(
                        "位置 {index}：assistant.tool_calls {expected:?} 与紧随的工具结果 \
                         {actual:?} 不匹配"
                    ));
                }
                index = cursor;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    Ok(())
}

/// 走完整出站管线：Anthropic 请求 → 上游（OpenAI 形态）→ 出站改写，取最终 `messages`。
fn outbound_messages(inbound: &Value) -> Vec<Value> {
    let upstream = to_upstream_request(inbound);
    let raw = serde_json::to_string(&upstream).expect("上游请求体可序列化");
    let prepared = prepare_outbound_body(
        Region::Cn,
        &raw,
        &OutboundOptions::default(),
        &OutboundMeta::default(),
        false,
    );
    serde_json::from_str::<Value>(&prepared)
        .expect("改写结果可解析")
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .expect("messages 是数组")
}

/// 同一条 user 消息里既有工具结果又有文本块时，文本必须排在结果**之后**。
///
/// Claude Code 会在「用户插话 / 中断 / 钩子反馈」时发出这种形状；若把文本块排到
/// 结果之前，工具结果就与发起调用的 assistant 消息隔开，上游判序列断裂。
#[test]
fn text_and_tool_result_in_one_turn_keeps_tool_adjacency() {
    let inbound = json!({
        "model": "claude-sonnet-4-5",
        "max_tokens": 1024,
        "messages": [
            {"role": "user", "content": "跑一下测试"},
            {"role": "assistant", "content": [
                {"type": "text", "text": "我来跑。"},
                {"type": "tool_use", "id": "toolu_01", "name": "Bash", "input": {"command": "cargo test"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_01", "content": "all green"},
                {"type": "text", "text": "[用户已中断本次请求]"}
            ]},
            {"role": "user", "content": "继续"}
        ]
    });

    let messages = outbound_messages(&inbound);
    check_tool_sequence(&messages).unwrap_or_else(|error| panic!("{error}"));

    let tool_at = messages
        .iter()
        .position(|message| message.get("role").and_then(Value::as_str) == Some("tool"))
        .expect("工具结果必须保留");
    assert_eq!(
        messages[tool_at - 1].get("role").and_then(Value::as_str),
        Some("assistant"),
        "工具结果必须紧邻发起调用的 assistant：{messages:?}"
    );
}

/// 并行批次 `[A, B]` 只有 `A` 有结果时，整批 `tool_calls` 会被删除——此时 `A` 的
/// 工具结果必须**一起**删除，否则悬空（历史被裁剪 / 某个工具被丢弃时会命中）。
#[test]
fn partial_tool_result_set_leaves_no_dangling_tool_message() {
    let inbound = json!({
        "model": "claude-sonnet-4-5",
        "max_tokens": 1024,
        "messages": [
            {"role": "user", "content": "并行跑两个工具"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "toolu_A", "name": "Bash", "input": {"command": "ls"}},
                {"type": "tool_use", "id": "toolu_B", "name": "Read", "input": {"file_path": "a.txt"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_A", "content": "ok"}
            ]}
        ]
    });

    let messages = outbound_messages(&inbound);
    check_tool_sequence(&messages).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        !messages
            .iter()
            .any(|message| message.get("role").and_then(Value::as_str) == Some("tool")),
        "批次被整批删除后不得残留工具结果：{messages:?}"
    );
}

/// 对照组：规范的并行工具轮次本来就自洽，修复不得把它改坏。
#[test]
fn well_formed_parallel_tool_turn_stays_intact() {
    let inbound = json!({
        "model": "claude-sonnet-4-5",
        "max_tokens": 1024,
        "messages": [
            {"role": "user", "content": "并行跑两个工具"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "toolu_A", "name": "Bash", "input": {"command": "ls"}},
                {"type": "tool_use", "id": "toolu_B", "name": "Read", "input": {"file_path": "a.txt"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_A", "content": "ok"},
                {"type": "tool_result", "tool_use_id": "toolu_B", "content": "ok"}
            ]}
        ]
    });

    let messages = outbound_messages(&inbound);
    check_tool_sequence(&messages).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.get("role").and_then(Value::as_str) == Some("tool"))
            .count(),
        2,
        "两条配对完整的工具结果都必须保留：{messages:?}"
    );
}
