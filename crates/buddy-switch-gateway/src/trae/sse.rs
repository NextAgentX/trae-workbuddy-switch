//! SOLO SSE → OpenAI SSE 转换。
//!
//! Trae 上游返回的是私有事件流（`event:output` / `event:token_usage` / `event:done` /
//! `event:error`，每行带 `id:N` 前缀），与 OpenAI 的 `data: {...}\n\n` 完全不同，
//! 因此**必须逐事件转换**，不能像 WorkBuddy 网关那样透传。
//!
//! ## 两条硬约束
//!
//! 1. **绝不悬挂连接**：无论上游是正常结束、中途断流、还是报错，都必须补发
//!    `data: [DONE]`。客户端（尤其 VS Code 系插件）会一直等到它为止。
//! 2. **`stream: false` 也要走这里**：上游只支持流式，非流式请求由 [`aggregate`]
//!    在本地把事件流拼成单个 `chat.completion`。

use std::io;

use axum::body::{Body, Bytes};
use futures_util::StreamExt;
use serde_json::{json, Map, Value};
use tokio::sync::mpsc;

/// 从上游 `token_usage` 事件提取的用量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt: u64,
    pub completion: u64,
    pub total: u64,
    pub reasoning: u64,
}

impl TokenUsage {
    /// 从事件 payload 解析；字段缺失按 0 处理，`total` 缺失时用 prompt + completion 补齐。
    pub fn from_value(value: &Value) -> Self {
        let read = |key: &str| value.get(key).and_then(Value::as_u64).unwrap_or(0);
        let prompt = read("prompt_tokens");
        let completion = read("completion_tokens");
        let total = match read("total_tokens") {
            0 => prompt + completion,
            value => value,
        };
        Self {
            prompt,
            completion,
            total,
            reasoning: read("reasoning_tokens"),
        }
    }

    /// 转成 OpenAI `usage` 对象。
    pub fn to_openai(self) -> Value {
        json!({
            "prompt_tokens": self.prompt,
            "completion_tokens": self.completion,
            "total_tokens": self.total,
        })
    }

    /// 是否拿到了有效数据（全 0 视为没拿到）。
    pub fn is_meaningful(self) -> bool {
        self.total > 0 || self.prompt > 0 || self.completion > 0
    }
}

/// 一个解析完成的 SOLO 事件。
#[derive(Debug, Clone, Default)]
pub struct SoloEvent {
    pub event: String,
    pub response: String,
    pub reasoning: String,
    pub tool_calls: Option<Value>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: String,
    pub error_code: Option<i64>,
    pub error_message: String,
}

/// 增量 SSE 解析器：跨 chunk 保留未完成的行与未结束的事件。
///
/// 缓冲**按字节**保存：网络分片可能把同一个多字节字符切成两半，先转成 `String`
/// 再拼接会得到替换字符（U+FFFD）并污染正文。
#[derive(Debug, Default)]
pub struct SoloParser {
    buffer: Vec<u8>,
    event: String,
    data: String,
}

impl SoloParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入一段字节，返回本次解析出的完整事件（可能为空）。
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SoloEvent> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        // 逐字节找换行，避免把不完整的行当作完整行解析。
        while let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=index).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]);
            if let Some(event) = self.scan_line(line.trim_end_matches('\r')) {
                events.push(event);
            }
        }
        events
    }

    /// 流结束后冲刷：把没有以空行收尾的最后一个事件也解析出来。
    pub fn finish(&mut self) -> Vec<SoloEvent> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let tail = std::mem::take(&mut self.buffer);
            let tail = String::from_utf8_lossy(&tail).to_string();
            for line in tail.lines() {
                if let Some(event) = self.scan_line(line.trim_end_matches('\r')) {
                    events.push(event);
                }
            }
        }
        if !self.event.is_empty() || !self.data.is_empty() {
            events.push(self.build_event());
        }
        events
    }

    /// 处理一行；返回 `Some` 表示一个事件已收尾（遇到空行）。
    fn scan_line(&mut self, line: &str) -> Option<SoloEvent> {
        if line.is_empty() {
            if self.event.is_empty() && self.data.is_empty() {
                return None;
            }
            return Some(self.build_event());
        }
        // `id:N` / `retry:` 等字段对本网关无意义，忽略即可。
        if let Some(rest) = line.strip_prefix("event:") {
            self.event = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            self.data.push_str(rest);
        }
        None
    }

    fn build_event(&mut self) -> SoloEvent {
        let event = std::mem::take(&mut self.event);
        let data = std::mem::take(&mut self.data);
        parse_event(&event, &data)
    }
}

/// 把 `(event, data)` 解析为 [`SoloEvent`]。
fn parse_event(event: &str, data: &str) -> SoloEvent {
    let mut parsed = SoloEvent {
        event: event.trim().to_string(),
        ..SoloEvent::default()
    };
    if data.is_empty() {
        return parsed;
    }
    let Ok(raw) = serde_json::from_str::<Value>(data) else {
        return parsed;
    };
    let Some(object) = raw.as_object() else {
        return parsed;
    };

    match parsed.event.as_str() {
        // `llm_utils_chat` 用 `output`；`create_agent_task` 用 `thought`。两者都接，
        // 这样上游换端点时不必改代码。
        "output" | "thought" => {
            if let Some(text) = object.get("response").and_then(Value::as_str) {
                parsed.response = text.to_string();
            } else if let Some(text) = object.get("thought").and_then(Value::as_str) {
                parsed.response = text.to_string();
            }
            if let Some(text) = object.get("reasoning_content").and_then(Value::as_str) {
                parsed.reasoning = text.to_string();
            }
            if let Some(calls) = object.get("tool_calls") {
                if !calls.is_null() {
                    parsed.tool_calls = Some(calls.clone());
                }
            }
        }
        "token_usage" => {
            parsed.usage = Some(TokenUsage::from_value(&raw));
        }
        "done" => {
            parsed.finish_reason = object
                .get("finish_reason")
                .and_then(Value::as_str)
                .unwrap_or("stop")
                .to_string();
        }
        "turn_completion" => {
            parsed.finish_reason = "stop".to_string();
        }
        "error" => {
            parsed.error_code = object.get("code").and_then(Value::as_i64);
            parsed.error_message = object
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("上游返回了未描述的错误")
                .to_string();
        }
        _ => {}
    }
    parsed
}

/// SOLO 的 `tool_calls` 转 OpenAI 形态：`function_call` → `function`，并去掉上游私有字段。
fn convert_tool_calls(value: &Value) -> Vec<Value> {
    let Some(calls) = value.as_array() else {
        return Vec::new();
    };
    calls
        .iter()
        .map(|call| {
            let mut call = call.clone();
            let Some(object) = call.as_object_mut() else {
                return call;
            };
            if let Some(function) = object.remove("function_call") {
                object.insert("function".into(), function);
            }
            if let Some(function) = object.get("function").and_then(Value::as_object).cloned() {
                let mut clean = function;
                // 这两个字段是上游内部的流式增量，OpenAI 客户端不认识。
                clean.remove("namespace");
                clean.remove("partial_arguments");
                object.insert("function".into(), Value::Object(clean));
            }
            call
        })
        .collect()
}

/// 构造一个 OpenAI `chat.completion.chunk` 的 `data:` 行。
fn chunk_line(chat_id: &str, model: &str, delta: Value, finish_reason: &str, usage: Option<TokenUsage>) -> String {
    let mut choice = Map::new();
    choice.insert("index".into(), json!(0));
    choice.insert("delta".into(), delta);
    if !finish_reason.is_empty() {
        choice.insert("finish_reason".into(), json!(finish_reason));
    }
    let mut chunk = Map::new();
    chunk.insert("id".into(), json!(chat_id));
    chunk.insert("object".into(), json!("chat.completion.chunk"));
    chunk.insert("created".into(), json!(now_secs()));
    chunk.insert("model".into(), json!(model));
    chunk.insert("choices".into(), json!([Value::Object(choice)]));
    if let Some(usage) = usage.filter(|usage| usage.is_meaningful()) {
        chunk.insert("usage".into(), usage.to_openai());
    }
    format!("data: {}\n\n", Value::Object(chunk))
}

/// `data: [DONE]\n\n`。
fn done_line() -> &'static str {
    "data: [DONE]\n\n"
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// 一次流式转换的结果。
#[derive(Debug, Clone, Default)]
pub struct StreamOutcome {
    /// 上游报错时的 `(code, message)`。
    pub error: Option<(i64, String)>,
    /// 实测用量（用于账本与日志）。
    pub usage: TokenUsage,
    /// 是否见过结束标记。
    pub saw_done: bool,
}

/// 把上游响应体流式转换为 OpenAI SSE，逐块送入 `sender`。
///
/// **总是**以 `data: [DONE]` 收尾（除非发送端已被对端丢弃），保证客户端不会悬挂。
///
/// `capture_id` 为抓取编号（诊断旁路）：非空时上游原始流逐块追加落盘
/// `<编号>_upstream_resp.ndjson`；空串表示不抓取（单元测试）。
pub async fn stream_convert(
    response: reqwest::Response,
    sender: mpsc::Sender<Result<Bytes, io::Error>>,
    chat_id: &str,
    model: &str,
    capture_id: &str,
) -> StreamOutcome {
    let mut parser = SoloParser::new();
    let mut outcome = StreamOutcome::default();
    let mut pending_usage: Option<TokenUsage> = None;
    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = match item {
            Ok(chunk) => chunk,
            Err(error) => {
                // 上游中途断流：不是账号问题，但必须让客户端收到收尾。
                if outcome.error.is_none() {
                    outcome.error = Some((0, format!("上游连接中断：{error}")));
                }
                break;
            }
        };
        // 抓取（诊断旁路）：上游原始流逐块追加，先于解析——解析失败也有据可查。
        super::capture::append_capture(capture_id, "_upstream_resp.ndjson", &chunk);
        for event in parser.feed(&chunk) {
            if emit(&event, sender.clone(), chat_id, model, &mut pending_usage, &mut outcome)
                .await
                .is_err()
            {
                // 客户端已断开：停止转换，不再补发任何内容。
                return outcome;
            }
        }
    }

    for event in parser.finish() {
        if emit(&event, sender.clone(), chat_id, model, &mut pending_usage, &mut outcome)
            .await
            .is_err()
        {
            return outcome;
        }
    }

    if !outcome.saw_done {
        let _ = sender
            .send(Ok(Bytes::from(done_line().to_string())))
            .await;
        outcome.saw_done = true;
    }
    outcome
}

/// 把一个事件转成 0..N 个 OpenAI chunk 并发送。返回 `Err` 表示发送端已关闭。
async fn emit(
    event: &SoloEvent,
    sender: mpsc::Sender<Result<Bytes, io::Error>>,
    chat_id: &str,
    model: &str,
    pending_usage: &mut Option<TokenUsage>,
    outcome: &mut StreamOutcome,
) -> Result<(), ()> {
    match event.event.as_str() {
        "output" | "thought" => {
            let mut delta = Map::new();
            if !event.response.is_empty() {
                delta.insert("content".into(), json!(event.response));
            }
            if !event.reasoning.is_empty() {
                delta.insert("reasoning_content".into(), json!(event.reasoning));
            }
            if let Some(calls) = &event.tool_calls {
                let converted = convert_tool_calls(calls);
                if !converted.is_empty() {
                    delta.insert("tool_calls".into(), json!(converted));
                }
            }
            if delta.is_empty() {
                return Ok(());
            }
            // 正文 chunk 先不带 usage：usage 通常在本事件之后才到。
            let line = chunk_line(chat_id, model, Value::Object(delta), "", *pending_usage);
            sender.send(Ok(Bytes::from(line))).await.map_err(|_| ())
        }
        "token_usage" => {
            if let Some(usage) = event.usage {
                *pending_usage = Some(usage);
                outcome.usage = usage;
            }
            Ok(())
        }
        "done" | "turn_completion" => {
            let finish = if event.finish_reason.is_empty() {
                "stop"
            } else {
                event.finish_reason.as_str()
            };
            let line = chunk_line(chat_id, model, json!({}), finish, *pending_usage);
            sender.send(Ok(Bytes::from(line))).await.map_err(|_| ())?;
            sender
                .send(Ok(Bytes::from(done_line().to_string())))
                .await
                .map_err(|_| ())?;
            outcome.saw_done = true;
            Ok(())
        }
        "error" => {
            let code = event.error_code.unwrap_or(0);
            outcome.error = Some((code, event.error_message.clone()));
            let payload = json!({
                "error": {
                    "message": event.error_message,
                    "type": "api_error",
                    "code": code,
                }
            });
            sender
                .send(Ok(Bytes::from(format!("data: {payload}\n\n"))))
                .await
                .map_err(|_| ())?;
            sender
                .send(Ok(Bytes::from(done_line().to_string())))
                .await
                .map_err(|_| ())?;
            outcome.saw_done = true;
            Ok(())
        }
        // `metadata` / `history` / `timing_cost` 等事件对 OpenAI 客户端无意义，忽略。
        _ => Ok(()),
    }
}

/// 非流式聚合：读完整条事件流，拼成单个 `chat.completion`。
///
/// 返回 `(响应体, 上游错误, 用量)`。上游报错时响应体为 `None`。
///
/// `capture_id` 为抓取编号（诊断旁路）：非空时上游原始流逐块追加落盘
/// `<编号>_upstream_resp.ndjson`；空串表示不抓取（单元测试）。
pub async fn aggregate(
    response: reqwest::Response,
    chat_id: &str,
    model: &str,
    capture_id: &str,
) -> (Option<Value>, Option<(i64, String)>, TokenUsage) {
    let mut parser = SoloParser::new();
    let mut content = String::new();
    let mut reasoning = String::new();
    let mut finish_reason = "stop".to_string();
    let mut usage = TokenUsage::default();
    let mut error: Option<(i64, String)> = None;
    let mut tool_calls: Vec<Value> = Vec::new();
    let mut stream = response.bytes_stream();

    let consume = |event: SoloEvent,
                       content: &mut String,
                       reasoning: &mut String,
                       finish_reason: &mut String,
                       usage: &mut TokenUsage,
                       error: &mut Option<(i64, String)>,
                       tool_calls: &mut Vec<Value>| {
        match event.event.as_str() {
            "output" | "thought" => {
                content.push_str(&event.response);
                reasoning.push_str(&event.reasoning);
                if let Some(calls) = &event.tool_calls {
                    tool_calls.extend(convert_tool_calls(calls));
                }
            }
            "token_usage" => {
                if let Some(value) = event.usage {
                    *usage = value;
                }
            }
            "done" | "turn_completion" => {
                if !event.finish_reason.is_empty() {
                    *finish_reason = event.finish_reason;
                }
            }
            "error" => {
                *error = Some((event.error_code.unwrap_or(0), event.error_message));
            }
            _ => {}
        }
    };

    while let Some(item) = stream.next().await {
        let chunk = match item {
            Ok(chunk) => chunk,
            Err(failure) => {
                if error.is_none() {
                    error = Some((0, format!("上游连接中断：{failure}")));
                }
                break;
            }
        };
        // 抓取（诊断旁路）：上游原始流逐块追加，先于解析——解析失败也有据可查。
        super::capture::append_capture(capture_id, "_upstream_resp.ndjson", &chunk);
        for event in parser.feed(&chunk) {
            consume(
                event,
                &mut content,
                &mut reasoning,
                &mut finish_reason,
                &mut usage,
                &mut error,
                &mut tool_calls,
            );
        }
    }
    for event in parser.finish() {
        consume(
            event,
            &mut content,
            &mut reasoning,
            &mut finish_reason,
            &mut usage,
            &mut error,
            &mut tool_calls,
        );
    }

    if let Some((code, message)) = error {
        return (None, Some((code, message)), usage);
    }

    let mut message = Map::new();
    message.insert("role".into(), json!("assistant"));
    message.insert("content".into(), json!(content));
    if !reasoning.is_empty() {
        message.insert("reasoning_content".into(), json!(reasoning));
    }
    if !tool_calls.is_empty() {
        message.insert("tool_calls".into(), json!(tool_calls));
    }

    let mut response = Map::new();
    response.insert("id".into(), json!(chat_id));
    response.insert("object".into(), json!("chat.completion"));
    response.insert("created".into(), json!(now_secs()));
    response.insert("model".into(), json!(model));
    response.insert(
        "choices".into(),
        json!([{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": finish_reason,
        }]),
    );
    if usage.is_meaningful() {
        response.insert("usage".into(), usage.to_openai());
    }

    (Some(Value::Object(response)), None, usage)
}

/// 把接收端包装成**字节流**。
///
/// 与 [`body_from_receiver`] 是同一种构造 —— 抽出来是为了让 `/v1/messages` 的
/// Anthropic 转换流能包同一份东西（`AnthropicSseStream` 要的是 `Stream`，
/// 不是 axum 的 `Body`）。两处若各写一份 `unfold`，行为会随重构漂移。
pub fn stream_from_receiver(
    rx: mpsc::Receiver<Result<Bytes, io::Error>>,
) -> impl futures_util::Stream<Item = Result<Bytes, io::Error>> {
    futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}

/// 把接收端包装成 axum 响应体。
pub fn body_from_receiver(rx: mpsc::Receiver<Result<Bytes, io::Error>>) -> Body {
    Body::from_stream(stream_from_receiver(rx))
}

/// OpenAI 形态的错误响应体。
pub fn error_body(code: &str, message: &str) -> Value {
    json!({
        "error": {
            "message": message,
            "type": "api_error",
            "code": code,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_handles_chunk_split_across_event_boundaries() {
        let mut parser = SoloParser::new();
        let first = parser.feed(b"id:1\nevent:output\ndata:{\"response\":\"he");
        assert!(first.is_empty(), "半截事件不该产出");
        let second = parser.feed(b"llo\"}\n\n");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].event, "output");
        assert_eq!(second[0].response, "hello");
    }

    #[test]
    fn parser_keeps_multibyte_characters_intact_across_chunks() {
        let mut parser = SoloParser::new();
        let payload = "{\"response\":\"你好\"}";
        let bytes = payload.as_bytes();
        // 在「你」的 UTF-8 中间切开。注意只能切 `bytes`：直接切 `&str` 会 panic
        // （那正是本用例要验证的场景——真实网络分片不会顾及字符边界）。
        let split = payload.find('你').unwrap() + 1;
        let mut head = b"event:output\ndata:".to_vec();
        head.extend_from_slice(&bytes[..split]);
        assert!(parser.feed(&head).is_empty(), "半截事件不该产出");
        let events = parser.feed(&[&bytes[split..], b"\n\n"].concat());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].response, "你好", "不得出现替换字符");
    }

    #[test]
    fn parser_finish_flushes_an_unterminated_event() {
        let mut parser = SoloParser::new();
        parser.feed(b"event:output\ndata:{\"response\":\"tail\"}");
        let flushed = parser.finish();
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].response, "tail");
    }

    #[test]
    fn parse_event_reads_every_documented_solo_event() {
        let output = parse_event("output", r#"{"response":"a","reasoning_content":"r","tool_calls":[{"function_call":{"name":"f"}}]}"#);
        assert_eq!(output.response, "a");
        assert_eq!(output.reasoning, "r");
        assert!(output.tool_calls.is_some());

        let usage = parse_event("token_usage", r#"{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12,"reasoning_tokens":1}"#);
        assert_eq!(
            usage.usage,
            Some(TokenUsage { prompt: 10, completion: 2, total: 12, reasoning: 1 })
        );

        let done = parse_event("done", r#"{"finish_reason":"length"}"#);
        assert_eq!(done.finish_reason, "length");

        let turn = parse_event("turn_completion", "{}");
        assert_eq!(turn.finish_reason, "stop");

        let error = parse_event("error", r#"{"code":1005,"message":"plan limit"}"#);
        assert_eq!(error.error_code, Some(1005));
        assert_eq!(error.error_message, "plan limit");
    }

    #[test]
    fn parse_event_tolerates_malformed_payloads() {
        assert_eq!(parse_event("output", "not json").response, "");
        assert_eq!(parse_event("output", "[1,2]").response, "");
        assert_eq!(parse_event("", "").event, "");
        // thought 事件的正文在 `thought` 字段。
        assert_eq!(parse_event("thought", r#"{"thought":"t"}"#).response, "t");
    }

    #[test]
    fn token_usage_fills_total_and_reports_meaningfulness() {
        let partial = TokenUsage::from_value(&json!({"prompt_tokens": 3, "completion_tokens": 4}));
        assert_eq!(partial.total, 7);
        assert!(partial.is_meaningful());
        assert!(!TokenUsage::default().is_meaningful());

        let openai = TokenUsage { prompt: 1, completion: 2, total: 3, reasoning: 0 }.to_openai();
        assert_eq!(openai["prompt_tokens"], 1);
        assert_eq!(openai["total_tokens"], 3);
        // reasoning_tokens 不是 OpenAI 标准字段，不外发。
        assert!(openai.get("reasoning_tokens").is_none());
    }

    #[test]
    fn tool_call_conversion_strips_private_fields() {
        let converted = convert_tool_calls(&json!([
            {"id": "1", "function_call": {"name": "f", "arguments": "{}", "namespace": "x", "partial_arguments": "y"}}
        ]));
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["function"]["name"], "f");
        assert!(converted[0]["function"].get("namespace").is_none());
        assert!(converted[0]["function"].get("partial_arguments").is_none());
        assert!(converted[0].get("function_call").is_none());
    }

    #[test]
    fn chunk_line_omits_empty_finish_reason_and_useless_usage() {
        let plain = chunk_line("id", "m", json!({"content": "x"}), "", None);
        assert!(plain.starts_with("data: {"));
        assert!(plain.ends_with("\n\n"));
        let parsed: Value = serde_json::from_str(plain.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(parsed["object"], "chat.completion.chunk");
        assert_eq!(parsed["model"], "m");
        assert!(parsed["choices"][0].get("finish_reason").is_none());
        assert!(parsed.get("usage").is_none());

        // 全 0 的 usage 不应外发（否则客户端会显示 0 token）。
        let with_zero = chunk_line("id", "m", json!({}), "stop", Some(TokenUsage::default()));
        let parsed: Value = serde_json::from_str(with_zero.trim_start_matches("data: ").trim()).unwrap();
        assert!(parsed.get("usage").is_none());
        assert_eq!(parsed["choices"][0]["finish_reason"], "stop");

        let with_usage = chunk_line(
            "id",
            "m",
            json!({}),
            "stop",
            Some(TokenUsage { prompt: 1, completion: 2, total: 3, reasoning: 0 }),
        );
        let parsed: Value = serde_json::from_str(with_usage.trim_start_matches("data: ").trim()).unwrap();
        assert_eq!(parsed["usage"]["total_tokens"], 3);
    }

    #[test]
    fn error_body_is_openai_shaped() {
        let body = error_body("no_healthy_account", "无可用账号");
        assert_eq!(body["error"]["type"], "api_error");
        assert_eq!(body["error"]["code"], "no_healthy_account");
        assert_eq!(body["error"]["message"], "无可用账号");
    }

    /// 端到端：喂入一段完整的 SOLO 流，验证 OpenAI chunk 序列。
    #[tokio::test]
    async fn stream_convert_emits_chunks_then_done() {
        let (tx, mut rx) = mpsc::channel(64);
        let mut parser = SoloParser::new();
        let mut outcome = StreamOutcome::default();
        let mut pending: Option<TokenUsage> = None;

        let raw = concat!(
            "id:1\nevent:output\ndata:{\"response\":\"he\"}\n\n",
            "id:2\nevent:output\ndata:{\"response\":\"llo\"}\n\n",
            "id:3\nevent:token_usage\ndata:{\"prompt_tokens\":5,\"completion_tokens\":2,\"total_tokens\":7}\n\n",
            "id:4\nevent:done\ndata:{\"finish_reason\":\"stop\"}\n\n",
        );
        for event in parser.feed(raw.as_bytes()) {
            emit(&event, tx.clone(), "chatcmpl-1", "glm-5.3", &mut pending, &mut outcome)
                .await
                .unwrap();
        }
        drop(tx);

        let mut lines = Vec::new();
        while let Some(item) = rx.recv().await {
            lines.push(String::from_utf8(item.unwrap().to_vec()).unwrap());
        }

        assert_eq!(lines.len(), 4, "两个正文 chunk + 结束 chunk + [DONE]");
        assert!(lines[0].contains("\"content\":\"he\""));
        assert!(lines[1].contains("\"content\":\"llo\""));
        assert!(lines[2].contains("\"finish_reason\":\"stop\""));
        // usage 应挂在最后一个 chunk 上。
        assert!(lines[2].contains("\"total_tokens\":7"));
        assert_eq!(lines[3], "data: [DONE]\n\n");
        assert!(outcome.saw_done);
        assert_eq!(outcome.usage.total, 7);
        assert!(outcome.error.is_none());
    }

    /// 上游在流内报错时，必须先发 error chunk 再发 `[DONE]`，且记录错误类别。
    #[tokio::test]
    async fn stream_convert_forwards_error_then_closes() {
        let (tx, mut rx) = mpsc::channel(64);
        let mut parser = SoloParser::new();
        let mut outcome = StreamOutcome::default();
        let mut pending: Option<TokenUsage> = None;

        let raw = "id:1\nevent:error\ndata:{\"code\":1005,\"message\":\"plan limit\"}\n\n";
        for event in parser.feed(raw.as_bytes()) {
            emit(&event, tx.clone(), "chatcmpl-2", "m", &mut pending, &mut outcome)
                .await
                .unwrap();
        }
        drop(tx);

        let mut lines = Vec::new();
        while let Some(item) = rx.recv().await {
            lines.push(String::from_utf8(item.unwrap().to_vec()).unwrap());
        }
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"code\":1005"));
        assert_eq!(lines[1], "data: [DONE]\n\n");
        assert_eq!(outcome.error, Some((1005, "plan limit".to_string())));
        assert!(outcome.saw_done);
    }
}
