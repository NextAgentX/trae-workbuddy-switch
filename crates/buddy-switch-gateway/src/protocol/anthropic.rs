//! Anthropic `/v1/messages` 协议：请求双向转换（含 tool use）+ 上游 SSE → Anthropic 事件流。

use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Bytes;
use futures_util::Stream;
use serde_json::{json, Map, Value};

use super::openai::{sse_event, CompletionAccumulator};

/// 把 Anthropic content（字符串或 content block 数组）中的文本拼出来。
fn content_to_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    block.get("text").and_then(Value::as_str).map(str::to_string)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// tool_result 的 content 可能是字符串或 text block 数组。
fn tool_result_to_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(value @ Value::Array(_)) => content_to_text(value),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
        None => String::new(),
    }
}

/// Anthropic 请求 → 上游（OpenAI 形态）请求体。
///
/// 关键映射（A-5.2）：
/// - 顶层 `system` → `messages[0] {role:"system"}`
/// - assistant `tool_use` block → `tool_calls`
/// - user `tool_result` block → 独立 `{role:"tool",tool_call_id,content}` 消息
/// - 同一条 user 消息里的文本块排在工具结果**之后**：`role:"tool"` 必须紧邻发起
///   调用的 assistant 消息，否则上游判 `tool_call_sequence_broken`
/// - `tools[{name,description,input_schema}]` → `tools[{type:"function",function:{...parameters}}]`
/// - `tool_choice`（auto/any/tool）→ 上游字符串形态
/// - `stop_sequences` → `stop`；强制 `stream:true`
pub fn to_upstream_request(body: &Value) -> Value {
    let mut out = Map::new();

    for key in ["model", "max_tokens", "temperature", "top_p"] {
        if let Some(value) = body.get(key) {
            out.insert(key.to_string(), value.clone());
        }
    }
    if let Some(stop) = body.get("stop_sequences") {
        out.insert("stop".to_string(), stop.clone());
    }
    // 上游不接受非流式（L4）：强制 stream。
    out.insert("stream".to_string(), json!(true));

    // tools：schema 包装转换。
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let converted: Vec<Value> = tools
            .iter()
            .map(|tool| {
                let name = tool.get("name").cloned().unwrap_or_else(|| json!(""));
                let description = tool.get("description").cloned().unwrap_or_else(|| json!(""));
                let parameters = tool
                    .get("input_schema")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
                json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": description,
                        "parameters": parameters,
                    }
                })
            })
            .collect();
        if !converted.is_empty() {
            out.insert("tools".to_string(), Value::Array(converted));
        }
    }

    // tool_choice 映射。
    if let Some(tool_choice) = body.get("tool_choice") {
        match tool_choice.get("type").and_then(Value::as_str) {
            Some("auto") => {
                out.insert("tool_choice".to_string(), json!("auto"));
            }
            Some("any") => {
                out.insert("tool_choice".to_string(), json!("required"));
            }
            Some("tool") => {
                if let Some(name) = tool_choice.get("name").and_then(Value::as_str) {
                    out.insert("tool_choice".to_string(), json!(name));
                }
            }
            Some("none") => {
                out.remove("tools");
            }
            _ => {}
        }
    }

    // messages：system + 逐条转换。
    let mut messages: Vec<Value> = Vec::new();
    if let Some(system) = body.get("system") {
        let text = content_to_text(system);
        if !text.is_empty() {
            messages.push(json!({"role": "system", "content": text}));
        }
    }

    if let Some(input) = body.get("messages").and_then(Value::as_array) {
        for message in input {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user")
                .to_string();
            match message.get("content") {
                Some(Value::String(text)) => {
                    messages.push(json!({"role": role, "content": text}));
                }
                Some(value @ Value::Array(blocks)) => {
                    let text = content_to_text(value);
                    let mut tool_calls: Vec<Value> = Vec::new();
                    let mut tool_results: Vec<Value> = Vec::new();
                    for block in blocks {
                        match block.get("type").and_then(Value::as_str) {
                            Some("tool_use") => {
                                let id = block.get("id").cloned().unwrap_or_else(|| json!(""));
                                let name = block.get("name").cloned().unwrap_or_else(|| json!(""));
                                let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                                let arguments =
                                    serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                                tool_calls.push(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": { "name": name, "arguments": arguments },
                                }));
                            }
                            Some("tool_result") => {
                                let tool_call_id =
                                    block.get("tool_use_id").cloned().unwrap_or_else(|| json!(""));
                                let content = tool_result_to_text(block.get("content"));
                                tool_results.push(json!({
                                    "role": "tool",
                                    "tool_call_id": tool_call_id,
                                    "content": content,
                                }));
                            }
                            _ => {}
                        }
                    }

                    if role == "assistant" {
                        let mut assistant = Map::new();
                        assistant.insert("role".to_string(), json!("assistant"));
                        if !text.is_empty() {
                            assistant.insert("content".to_string(), json!(text));
                        } else if !tool_calls.is_empty() {
                            assistant.insert("content".to_string(), Value::Null);
                        }
                        if !tool_calls.is_empty() {
                            assistant.insert("tool_calls".to_string(), Value::Array(tool_calls));
                        }
                        if assistant.len() > 1 {
                            messages.push(Value::Object(assistant));
                        }
                    } else {
                        // 顺序不可交换：`role:"tool"` 必须**紧跟**发起该调用的
                        // assistant 消息，中间插入任何消息都会被上游判
                        // `tool_call_sequence_broken`（400，tool calls and tool
                        // results do not match）。因此同一条 user 消息里的文本块
                        // （Claude Code 的插话 / 中断说明）排在工具结果**之后**，
                        // 作为本轮结果之后的新用户发言。
                        messages.extend(tool_results);
                        if !text.is_empty() {
                            messages.push(json!({"role": "user", "content": text}));
                        }
                    }
                }
                _ => {
                    messages.push(json!({"role": role}));
                }
            }
        }
    }

    out.insert("messages".to_string(), Value::Array(messages));
    Value::Object(out)
}

/// `finish_reason` → Anthropic `stop_reason`（stop→end_turn、length→max_tokens、tool_calls→tool_use）。
pub fn map_stop_reason(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("stop") => "end_turn",
        Some("length") => "max_tokens",
        Some("tool_calls") | Some("function_call") => "tool_use",
        _ => "end_turn",
    }
}

/// 聚合上游 SSE 为 Anthropic `message` 对象（非流式请求）。
pub fn aggregate_message(sse_text: &str, fallback_model: &str) -> Value {
    let accumulator = CompletionAccumulator::from_sse(sse_text);
    let id = accumulator
        .id
        .clone()
        .map(|id| format!("msg_{id}"))
        .unwrap_or_else(|| format!("msg_{}", uuid::Uuid::new_v4().simple()));
    let model = accumulator
        .model
        .clone()
        .unwrap_or_else(|| fallback_model.to_string());

    let mut content: Vec<Value> = Vec::new();
    if !accumulator.content.is_empty() {
        content.push(json!({"type": "text", "text": accumulator.content}));
    }
    for tool_call in &accumulator.tool_calls {
        let input: Value = serde_json::from_str(&tool_call.arguments).unwrap_or_else(|_| json!({}));
        content.push(json!({
            "type": "tool_use",
            "id": tool_call.id,
            "name": tool_call.name,
            "input": input,
        }));
    }

    json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": map_stop_reason(accumulator.finish_reason.as_deref()),
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": accumulator.prompt_tokens.unwrap_or(0),
            "output_tokens": accumulator.completion_tokens.unwrap_or(0),
        },
    })
}

/// 上游 OpenAI SSE → Anthropic SSE 事件流的转换流。
///
/// 完整事件序列：`message_start` → `content_block_start` → `content_block_delta`
/// → `content_block_stop` → `message_delta` → `message_stop`。
pub struct AnthropicSseStream<S> {
    inner: S,
    buffer: Vec<u8>,
    model: String,
    message_id: String,
    started: bool,
    finished: bool,
    next_index: i64,
    text_index: Option<i64>,
    tool_index: HashMap<i64, i64>,
    open_tools: Vec<i64>,
    input_tokens: u64,
    output_tokens: u64,
    stop_reason: String,
    pending: VecDeque<Bytes>,
}

impl<S> AnthropicSseStream<S> {
    /// 新建转换流。
    pub fn new(inner: S, model: String) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
            model,
            message_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            started: false,
            finished: false,
            next_index: 0,
            text_index: None,
            tool_index: HashMap::new(),
            open_tools: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            stop_reason: "end_turn".to_string(),
            pending: VecDeque::new(),
        }
    }

    fn emit(&mut self, event: &str, data: Value) {
        self.pending.push_back(sse_event(event, &data));
    }

    fn ensure_started(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        self.emit(
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": self.message_id,
                    "type": "message",
                    "role": "assistant",
                    "model": self.model,
                    "content": [],
                    "stop_reason": Value::Null,
                    "stop_sequence": Value::Null,
                    "usage": { "input_tokens": 0, "output_tokens": 0 },
                }
            }),
        );
    }

    fn ingest(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
        loop {
            let Some(position) = self.buffer.iter().position(|&byte| byte == b'\n') else {
                break;
            };
            let line: Vec<u8> = self.buffer.drain(..=position).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            let Some(payload) = line.strip_prefix("data:") else {
                continue;
            };
            let payload = payload.trim();
            if payload.is_empty() {
                continue;
            }
            if payload == "[DONE]" {
                self.finalize();
                return;
            }
            if let Ok(value) = serde_json::from_str::<Value>(payload) {
                self.handle_delta(&value);
            }
        }
    }

    fn handle_delta(&mut self, value: &Value) {
        self.ensure_started();
        if let Some(model) = value.get("model").and_then(Value::as_str) {
            if !model.is_empty() {
                self.model = model.to_string();
            }
        }
        if let Some(usage) = value.get("usage") {
            self.capture_usage(usage);
        }

        let Some(choice) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return;
        };

        if let Some(delta) = choice.get("delta") {
            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                if !text.is_empty() {
                    self.push_text(text);
                }
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for tool_call in tool_calls {
                    self.push_tool_call(tool_call);
                }
            }
        }

        // `finish_reason` 的**空值形态**不是终止信号：上游 OpenAI 兼容实现常在中间分片里
        // 写 `""`（而非标准的 `null`）。`as_str()` 对 `""` 会返回 `Some("")`，若直接据此
        // `finalize`，转换流会在第一个分片就收尾，客户端表现为「流式对话刚开始就断了」。
        if let Some(finish_reason) = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .filter(|reason| !reason.is_empty())
        {
            self.stop_reason = map_stop_reason(Some(finish_reason)).to_string();
            self.finalize();
        }
    }

    fn push_text(&mut self, text: &str) {
        let index = match self.text_index {
            Some(index) => index,
            None => {
                let index = self.next_index;
                self.next_index += 1;
                self.text_index = Some(index);
                self.emit(
                    "content_block_start",
                    json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": { "type": "text", "text": "" },
                    }),
                );
                index
            }
        };
        self.emit(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": { "type": "text_delta", "text": text },
            }),
        );
    }

    fn push_tool_call(&mut self, tool_call: &Value) {
        let upstream_index = tool_call
            .get("index")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let index = if let Some(&index) = self.tool_index.get(&upstream_index) {
            index
        } else {
            // 关闭已打开的文本块，开启新的 tool_use 块。
            if let Some(text_index) = self.text_index.take() {
                self.emit(
                    "content_block_stop",
                    json!({"type": "content_block_stop", "index": text_index}),
                );
            }
            let index = self.next_index;
            self.next_index += 1;
            self.tool_index.insert(upstream_index, index);
            self.open_tools.push(index);
            let id = tool_call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = tool_call
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            self.emit(
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": index,
                    "content_block": { "type": "tool_use", "id": id, "name": name, "input": {} },
                }),
            );
            index
        };

        if let Some(arguments) = tool_call
            .get("function")
            .and_then(|function| function.get("arguments"))
            .and_then(Value::as_str)
        {
            if !arguments.is_empty() {
                self.emit(
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": index,
                        "delta": { "type": "input_json_delta", "partial_json": arguments },
                    }),
                );
            }
        }
    }

    fn capture_usage(&mut self, usage: &Value) {
        if let Some(prompt) = usage.get("prompt_tokens").and_then(Value::as_u64) {
            self.input_tokens = prompt;
        }
        if let Some(completion) = usage.get("completion_tokens").and_then(Value::as_u64) {
            self.output_tokens = completion;
        }
    }

    fn finalize(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.ensure_started();
        if let Some(text_index) = self.text_index.take() {
            self.emit(
                "content_block_stop",
                json!({"type": "content_block_stop", "index": text_index}),
            );
        }
        // 先整体取出待关闭的工具块索引，释放对 self 的可变借用后再逐条 emit，
        // 避免 drain 持有跨迭代借用与 self.emit 的 &mut self 冲突。
        let open_tools = std::mem::take(&mut self.open_tools);
        for index in open_tools {
            self.emit(
                "content_block_stop",
                json!({"type": "content_block_stop", "index": index}),
            );
        }
        self.emit(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": { "stop_reason": self.stop_reason, "stop_sequence": Value::Null },
                "usage": { "output_tokens": self.output_tokens },
            }),
        );
        self.emit("message_stop", json!({"type": "message_stop"}));
    }
}

impl<S, E> Stream for AnthropicSseStream<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Debug,
{
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(event) = this.pending.pop_front() {
                return Poll::Ready(Some(Ok(event)));
            }
            if this.finished {
                return Poll::Ready(None);
            }
            // 直接 Pin::new（S: Unpin 已由 impl 的 where 子句保证），
            // 不依赖 StreamExt::as_mut，避免额外 trait 导入。
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => this.finalize(),
                Poll::Ready(Some(Err(error))) => {
                    eprintln!("[gateway] Anthropic 上游流中途异常: {error:?}");
                    this.finalize();
                }
                Poll::Ready(Some(Ok(chunk))) => this.ingest(&chunk),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    fn collect_text(events: &[Bytes]) -> String {
        events
            .iter()
            .map(|chunk| String::from_utf8_lossy(chunk).to_string())
            .collect()
    }

    #[test]
    fn request_maps_system_tools_and_tool_choice() {
        let anthropic = json!({
            "model": "m",
            "max_tokens": 100,
            "system": "be nice",
            "stop_sequences": ["END"],
            "tools": [{"name": "get_weather", "description": "d", "input_schema": {"type": "object", "properties": {"city": {"type": "string"}}}}],
            "tool_choice": {"type": "any"},
            "messages": [{"role": "user", "content": "hi"}],
        });
        let upstream = to_upstream_request(&anthropic);

        assert_eq!(upstream["stream"], true);
        assert_eq!(upstream["stop"], json!(["END"]));
        assert_eq!(upstream["messages"][0]["role"], "system");
        assert_eq!(upstream["messages"][0]["content"], "be nice");
        assert_eq!(upstream["messages"][1]["role"], "user");
        assert_eq!(upstream["tools"][0]["type"], "function");
        assert_eq!(upstream["tools"][0]["function"]["name"], "get_weather");
        assert_eq!(
            upstream["tools"][0]["function"]["parameters"]["properties"]["city"]["type"],
            "string"
        );
        assert_eq!(upstream["tool_choice"], "required");
    }

    #[test]
    fn request_maps_tool_use_to_tool_calls_and_tool_result_to_tool_role() {
        let anthropic = json!({
            "model": "m",
            "max_tokens": 10,
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "weather?"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "let me check"},
                    {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"city": "SF"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_1", "content": "sunny"}
                ]}
            ]
        });
        let upstream = to_upstream_request(&anthropic);
        let messages = upstream["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "weather?");

        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"], "let me check");
        assert_eq!(messages[1]["tool_calls"][0]["id"], "toolu_1");
        assert_eq!(messages[1]["tool_calls"][0]["type"], "function");
        assert_eq!(messages[1]["tool_calls"][0]["function"]["name"], "get_weather");
        assert_eq!(
            messages[1]["tool_calls"][0]["function"]["arguments"],
            "{\"city\":\"SF\"}"
        );

        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "toolu_1");
        assert_eq!(messages[2]["content"], "sunny");
    }

    /// 同一条 user 消息里既有工具结果又有文本块时，文本必须排在结果**之后**。
    ///
    /// `role:"tool"` 必须紧邻发起调用的 assistant 消息；把文本块排到前面会让工具结果
    /// 与 assistant 隔开，上游判 `tool_call_sequence_broken`（400）。
    #[test]
    fn tool_result_precedes_sibling_text_block() {
        let anthropic = json!({
            "model": "m",
            "max_tokens": 10,
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_1", "content": "sunny"},
                    {"type": "text", "text": "顺带提一句"}
                ]}
            ]
        });
        let upstream = to_upstream_request(&anthropic);
        let messages = upstream["messages"].as_array().unwrap();

        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["tool_call_id"], "toolu_1");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"], "顺带提一句");
    }

    #[test]
    fn tool_choice_tool_maps_to_function_name_and_none_drops_tools() {
        let tool = json!({"model": "m", "tool_choice": {"type": "tool", "name": "foo"}, "messages": []});
        assert_eq!(to_upstream_request(&tool)["tool_choice"], "foo");

        let none = json!({"model": "m", "tool_choice": {"type": "none"}, "tools": [{"name": "x", "input_schema": {}}], "messages": []});
        assert!(to_upstream_request(&none).get("tools").is_none());
    }

    #[test]
    fn stop_reason_mapping_is_three_state() {
        assert_eq!(map_stop_reason(Some("stop")), "end_turn");
        assert_eq!(map_stop_reason(Some("length")), "max_tokens");
        assert_eq!(map_stop_reason(Some("tool_calls")), "tool_use");
        assert_eq!(map_stop_reason(None), "end_turn");
    }

    #[test]
    fn aggregate_message_builds_tool_use() {
        let sse = concat!(
            "data: {\"id\":\"c1\",\"model\":\"m\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"f\",\"arguments\":\"{\\\"a\\\":1}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
        );
        let message = aggregate_message(sse, "fallback");
        assert_eq!(message["type"], "message");
        assert_eq!(message["stop_reason"], "tool_use");
        assert_eq!(message["content"][0]["type"], "tool_use");
        assert_eq!(message["content"][0]["name"], "f");
        assert_eq!(message["content"][0]["input"]["a"], 1);
    }

    #[tokio::test]
    async fn stream_emits_full_anthropic_event_sequence_for_text() {
        let chunks = vec![Ok::<Bytes, std::io::Error>(Bytes::from_static(
            concat!(
                "data: {\"model\":\"m\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\" there\"},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n",
            )
            .as_bytes(),
        ))];
        let stream = AnthropicSseStream::new(futures_util::stream::iter(chunks), "m".to_string());
        let events: Vec<Bytes> = stream.map(|item| item.unwrap()).collect().await;
        let text = collect_text(&events);

        let order = [
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop",
        ];
        let mut cursor = 0usize;
        for name in order {
            let found = text[cursor..].find(name).map(|offset| cursor + offset);
            assert!(found.is_some(), "缺少事件 {name}：{text}");
            cursor = found.unwrap() + name.len();
        }
        assert!(text.contains("Hi"));
        assert!(text.contains(" there"));
        assert!(text.contains("\"stop_reason\":\"end_turn\""));
    }

    #[tokio::test]
    async fn stream_reassembles_sse_line_split_across_chunks() {
        let first = b"data: {\"choices\":[{\"delta\":{\"content\":\"split\"}}]}\n";
        let second = b"\ndata: [DONE]\n\n";
        let split_at = first.len() / 2;
        let chunks = vec![
            Ok::<Bytes, std::io::Error>(Bytes::copy_from_slice(&first[..split_at])),
            Ok::<Bytes, std::io::Error>(Bytes::copy_from_slice(&first[split_at..])),
            Ok::<Bytes, std::io::Error>(Bytes::from_static(second)),
        ];
        let stream = AnthropicSseStream::new(futures_util::stream::iter(chunks), "m".to_string());
        let events: Vec<Bytes> = stream.map(|item| item.unwrap()).collect().await;
        let text = collect_text(&events);
        assert!(text.contains("split"));
        assert!(text.contains("message_start"));
        assert!(text.contains("message_stop"));
    }

    #[tokio::test]
    async fn stream_emits_tool_use_block_and_input_json_delta() {
        let chunks = vec![Ok::<Bytes, std::io::Error>(Bytes::from_static(
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"f\",\"arguments\":\"{\\\"a\\\"\"}}]}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":1}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n",
            )
            .as_bytes(),
        ))];
        let stream = AnthropicSseStream::new(futures_util::stream::iter(chunks), "m".to_string());
        let events: Vec<Bytes> = stream.map(|item| item.unwrap()).collect().await;
        let text = collect_text(&events);

        assert!(text.contains("\"type\":\"tool_use\""));
        assert!(text.contains("\"name\":\"f\""));
        assert!(text.contains("\"type\":\"input_json_delta\""));
        assert!(text.contains("{\\\"a\\\""));
        assert!(text.contains(":1}"));
        assert!(text.contains("\"stop_reason\":\"tool_use\""));
        assert!(text.contains("message_stop"));
    }

    #[tokio::test]
    async fn stream_captures_usage_and_closes_text_before_tool_block() {
        let chunks = vec![Ok::<Bytes, std::io::Error>(Bytes::from_static(
            concat!(
                "data: {\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3},\"choices\":[{\"delta\":{\"content\":\"before\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n",
            )
            .as_bytes(),
        ))];
        let stream = AnthropicSseStream::new(futures_util::stream::iter(chunks), "m".to_string());
        let events: Vec<Bytes> = stream.map(|item| item.unwrap()).collect().await;
        let text = collect_text(&events);
        let text_stop = text.find("content_block_stop").unwrap();
        let tool_start = text.find("\"type\":\"tool_use\"").unwrap();
        assert!(text_stop < tool_start, "文本块必须在工具块前关闭");
        assert!(text.contains("\"output_tokens\":3"));
    }

    #[tokio::test]
    async fn stream_supports_two_parallel_tool_blocks() {
        let chunks = vec![Ok::<Bytes, std::io::Error>(Bytes::from_static(
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"one\",\"arguments\":\"{}\"}},{\"index\":1,\"id\":\"b\",\"function\":{\"name\":\"two\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                "data: [DONE]\n\n",
            )
            .as_bytes(),
        ))];
        let stream = AnthropicSseStream::new(futures_util::stream::iter(chunks), "m".to_string());
        let events: Vec<Bytes> = stream.map(|item| item.unwrap()).collect().await;
        let text = collect_text(&events);
        assert_eq!(text.matches("\"type\":\"tool_use\"").count(), 2);
        assert_eq!(text.matches("event: content_block_stop").count(), 2);
    }

    #[tokio::test]
    async fn stream_still_emits_start_and_stop_on_empty_upstream() {
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![];
        let stream = AnthropicSseStream::new(futures_util::stream::iter(chunks), "m".to_string());
        let events: Vec<Bytes> = stream.map(|item| item.unwrap()).collect().await;
        let text = collect_text(&events);
        assert!(text.contains("message_start"));
        assert!(text.contains("message_stop"));
    }

    /// 空值形态的 `finish_reason`（`""` 或 `null`）**不是**终止信号。
    ///
    /// 上游（OpenAI 兼容实现）在中间分片里把 `finish_reason` 写成空串而非 `null` 是常见
    /// 现象。若把「键存在且能读成字符串」当成终止，转换流会在**第一个**分片就 `finalize`，
    /// 客户端（Claude Code）看到的现象是「流式对话刚开始就断了」。
    ///
    /// 注意每个 SSE 帧必须是**独立的 chunk**：真实网络下逐帧到达。若把全部帧塞进同一个
    /// chunk，`ingest` 的循环会无视 `finished` 继续处理后续帧，从而掩盖提前终止的缺陷。
    #[tokio::test]
    async fn stream_does_not_finalize_on_empty_finish_reason() {
        let chunks = vec![
            Ok::<Bytes, std::io::Error>(Bytes::from_static(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":\"\"}]}\n\n",
            )),
            Ok::<Bytes, std::io::Error>(Bytes::from_static(
                b"data: {\"choices\":[{\"delta\":{\"content\":\" there\"},\"finish_reason\":null}]}\n\n",
            )),
            Ok::<Bytes, std::io::Error>(Bytes::from_static(
                b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            )),
            Ok::<Bytes, std::io::Error>(Bytes::from_static(b"data: [DONE]\n\n")),
        ];
        let stream = AnthropicSseStream::new(futures_util::stream::iter(chunks), "m".to_string());
        let events: Vec<Bytes> = stream.map(|item| item.unwrap()).collect().await;
        let text = collect_text(&events);

        assert!(text.contains("Hi"), "首个分片的内容必须送达：{text}");
        assert!(
            text.contains(" there"),
            "空值 finish_reason 不得提前结束流，后续分片必须继续送达：{text}"
        );
        assert_eq!(
            text.matches("\"stop_reason\":\"end_turn\"").count(),
            1,
            "只允许一个 message_delta 终止事件：{text}"
        );
        assert!(text.contains("message_stop"), "{text}");
    }
}
