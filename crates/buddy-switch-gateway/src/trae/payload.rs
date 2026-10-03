//! OpenAI 请求体 → Trae `llm_utils_chat` 请求体改写。
//!
//! 上游不是 OpenAI 兼容端点，它要求一组**固定字段**（`config_name` / `model_name` /
//! `function` / `workspace_id` …）。本模块负责把标准 OpenAI 请求「翻译」过去，
//! 并顺手修掉几个上游不接受、但 OpenAI 客户端普遍会发的形态。
//!
//! ## 三处必须改写的形态
//!
//! 1. `content` 只接受**数组**形态（`[{"type":"text","text":…}]`），字符串会被拒。
//! 2. `tools[].function.parameters` 只接受 **JSON 字符串**，不接受对象。
//! 3. `assistant.tool_calls[].function` 要改成 `function_call`（历史消息回传时）。
//!
//! ## 关于 `stream`
//!
//! 上游**只**支持流式。`stream: false` 的客户端请求由本网关在本地聚合
//! （见 [`super::sse::aggregate`]），而不是把 `stream` 关掉发给上游。

use serde_json::{json, Map, Value};

use buddy_switch_core::modules::trae::model_list::ClientModelList;
use buddy_switch_core::modules::trae::variant::TraeVariant;

use super::{function_for, TRAE_APP_ID, TRAE_DEFAULT_MODEL, TRAE_IDE_VERSION, TRAE_IDE_VERSION_CODE};

/// 模型名映射：`模型名 → (config_name, model_name)`。
///
/// ## 规则（2026-09-30 上游实测，issue #4）
///
/// - `config_name` = 客户端清单里的模型 `name`。本机实测 TraeWork 的 `solo_work_lite`
///   分组 **16/16 条** `config_name == name`（客户端 `state.vscdb` 的
///   `AI.agent.model.model_list_map` 里两个字段并存，可直接对照）。
/// - `model_name` = 该名字 **+ `__dev`**。实测 `glm-5.3-flash__dev` / `qwen3.8-flash__dev`
///   均通过；**去掉后缀直接 `code 4023 model is unknown`**。
///
/// ## 为什么默认是**派生**而不是回落默认模型
///
/// 改造前未知模型一律回落成 `DeepSeek-V4-Flash` —— 于是「传错名字也拿到 DeepSeek 的
/// 回答」，是本仓最难发现的一类错（既不报错、也不是用户要的模型）。派生之后：
/// 客户端清单里真实存在的模型**不必改代码就能用**；拼错的名字会收到上游明确的
/// `4023 / 4001`，而不是一个看起来正常的答案。
///
/// ## 这张表为什么还留着
///
/// 只留「名字与客户端清单不一致」的**历史别名 / 大小写归一**：老调用方仍在传
/// `deepseek-v4-flash`（客户端里早已改叫 `deepseek-v4.1-flash`），
/// `seed-code-pro-0430` 也是客户端早期用过的名字。删掉它们等于断老调用方。
/// **新增模型不需要改这里** —— 这正是本次要摆脱的维护方式。
pub fn model_config(model: &str) -> (String, String) {
    let name = model.trim();
    // 空名（配置里 `defaultModel` 被清空这类退化输入）用兜底默认模型派生，
    // 同样**不**回落成某个碰巧写死的模型。
    let name = if name.is_empty() { TRAE_DEFAULT_MODEL } else { name };

    let known = match name.to_lowercase().as_str() {
        "deepseek-v4-flash" => Some(("DeepSeek-V4-Flash", "deepseek_v4_flash__dev")),
        "deepseek-v4-flash-official" => {
            Some(("DeepSeek-V4-Flash-Official", "DeepSeek-V4-Flash-Official__dev"))
        }
        "deepseek-v4-pro" => Some(("DeepSeek-V4-Pro", "deepseek_v4_pro__dev")),
        "glm-5.2" => Some(("glm-5.2", "glm-5.2__dev")),
        "glm-5.3" => Some(("glm-5.3", "glm-5.3__dev")),
        "doubao-seed-2.1-pro" | "seed-code-pro-0430" => {
            Some(("Doubao-Seed-2.1-Pro", "Doubao-Seed-2.1-Pro__dev"))
        }
        "doubao-seed-2.1-turbo" => Some(("Doubao-Seed-2.1-Turbo", "Doubao-Seed-2.1-Turbo__dev")),
        "kimi-k2.7-code" => Some(("kimi-k2.7-code", "kimi-k2.7-code__dev")),
        "minimax-m3" => Some(("minimax-m3", "minimax-m3__dev")),
        _ => None,
    };

    match known {
        Some((config_name, model_name)) => (config_name.to_string(), model_name.to_string()),
        None => (name.to_string(), format!("{name}__dev")),
    }
}

/// 网关对外暴露的模型清单（**静态兜底**，`/v1/models` 读不到客户端清单时才用）。
///
/// ## 内容口径（2026-09-30 逐条实测修正，不再是拍脑袋的常量）
///
/// 这份兜底清单**必须与实时路径（客户端 `solo_work_lite` 分组）口径一致**，
/// 否则客户端读不到时会对外宣传一批**调不动**的名字 —— 那正是 issue #4
/// 「看得见、调不动」的另一半。现在的构成：
///
/// 1. **客户端 `solo_work_lite` 分组的内容**（15 条）：即实时路径会返回的那批，
///    减去一条**实测被上游拒绝**的第三方路由条目（见下面清单里的说明）。
/// 2. **历史对外名**（6 条：`deepseek-v4-flash` / `deepseek-v4-pro` / `glm-5-turbo` /
///    `glm-5` / `sagitta` / `aquila`）：不在客户端清单里，但**逐个实测上游仍然接受**
///    （见 `routes.rs::tests::probe_traecode_model_names` 的 X1–X3、W1），
///    删掉等于断老调用方；`deepseek-v4-flash` 还是默认模型。
/// 3. **已剔除**：`doubao-seed-2.0-code`（属于 `solo_coder`，在 `solo_work_lite` 下
///    实测 `4001`）。
///
/// ⚠️ 改这里的名字**要同时看 [`model_config`]** —— 只有「与客户端清单不一致」的名字
/// 才需要在那张表里登记（例如 `deepseek-v4-flash` → `DeepSeek-V4-Flash`）；
/// 其余名字按**派生规则**发给上游（`config_name = 名字`、`model_name = 名字 + "__dev"`），
/// 正确与否**只能靠实测上游认哪个名字**，不能照抄客户端清单的显示名。
///
pub const MODEL_NAMES: &[&str] = &[
    // ---- 客户端 TraeWork 的 `solo_work_lite` 分组（16 条，逐条实测）----
    // 顺序与客户端清单一致（保序），内容按 `probe_work_list_names` 的实测结果取舍：
    // 13 条明确被上游接受、2 条推理模型首 token 慢（45s 内未回 metadata，但**没有**回
    // `event:error` —— 被拒的一律在首块就回错误），**1 条被明确拒绝**（见下）。
    "Doubao-Seed-Evolving",
    "Doubao-Seed-2.1-Pro",
    "Doubao-Seed-2.1-Turbo",
    "step-5-preview",
    "glm-5.3",
    "glm-5.2",
    "deepseek-v4.1-flash",
    "DeepSeek-V4-Flash-Official",
    "DeepSeek-V4-Pro-Official",
    "kimi-k3",
    "kimi-k2.7-code",
    "kimi-k2.6",
    "minimax-m3",
    "qwen3.8-max",
    "qwen-3.7-plus",
    // ⚠️ 客户端那份里还有第 16 条 `openrouter//stealth/ox-alpha`（`provider = "openrouter"`）
    // —— **实测被上游 `4001 param is invalid` 拒**，故**不列入**对外清单
    // （`servable_entries` 也按同一条规则把这类第三方路由条目过滤掉，两处口径一致）。
    // ---- 历史对外名：不在客户端清单里，但**实测上游仍然接受**，删掉会断老调用方 ----
    // `deepseek-v4-flash` 同时是 `TRAE_DEFAULT_MODEL`（默认模型），必须在清单里。
    "deepseek-v4-flash",
    "deepseek-v4-pro",
    "glm-5-turbo",
    "glm-5",
    "sagitta",
    "aquila",
];
/// 静态兜底清单的上下文窗口（客户端清单里取不到时沿用，保持既有形状不变）。
const DEFAULT_CONTEXT_LENGTH: i64 = 131_072;

/// `/v1/models` 的响应体（**静态兜底清单**）。
///
/// 只在客户端清单读不到时才被用到，见 [`models_response_for`]。
pub fn models_response() -> Value {
    models_response_from(
        MODEL_NAMES
            .iter()
            .map(|name| (*name, None))
            .collect::<Vec<_>>(),
    )
}

/// `/v1/models` 的响应体 —— **按变体读客户端（上游下发）的清单**，读不到回落静态。
///
/// ## 为什么对外清单也要跟着客户端走（issue #4）
///
/// Trae 的模型清单由服务端下发，客户端拉取后落在 `state.vscdb`
/// （见 `buddy_switch_core::modules::trae::model_list`）。网关对外暴露的清单
/// **必须与客户端里那份一致** —— 否则用户照着 `/v1/models` 列出的名字去调用，
/// 很可能传的是上游根本不认的旧名（本仓实测：静态清单里的 `deepseek-v4-flash`
/// 在客户端里早已是 `deepseek-v4.1-flash`，而 `sagitta` / `aquila` **根本不存在**）。
///
/// ## 为什么读不到要**回落**而不是报错
///
/// 客户端没启动过 / 没登录时读不到清单。此时报错会让 OpenAI 客户端连「列模型」
/// 都失败（开箱即空）；回落静态清单至少保证**可用**，代价是名字可能过时。
/// 两者相权取回落 —— 与「网关不因管理面缺数据而拒绝服务」的既有取向一致。
///
/// ## ⚠️ 只列**本变体真能调**的模型（2026-09-30 实测修正）
///
/// 上游按 `function` 做白名单（见 [`super::function_for`]）。改造前列的是该产品线
/// **全部分组的并集** —— 本机实测 TraeWork 并集 27 条，而网关只会用 `solo_work_lite`
/// 发请求，其中 **11 条（来自 `solo_coder` 等分组）宣传了但一律 `4001` 调不动**。
/// 那是同一句「看得见、调不动」，只是发生在另一批模型上。
///
/// ⇒ 现在只取 `function_for(variant)` 对应的**那一个分组**。
pub fn models_response_for(variant: TraeVariant) -> Value {
    let list = buddy_switch_core::modules::trae::model_list::read_client_model_list(variant);
    if list.groups.is_empty() {
        return models_response();
    }

    models_response_from(servable_entries(&list, variant))
}

/// 取**本变体真能调**的模型：该程序位 `function` 对应的那个分组。
///
/// 分组缺失时回落**全分组并集**（改造前的口径）：客户端版本变动 / 上游给分组改名时，
/// 宁可多列（老行为），也不要因为一个分组名对不上就把整张清单清空 ——
/// 「列模型」是开箱能力，不能因为清单结构漂移而失效。
fn servable_entries(list: &ClientModelList, variant: TraeVariant) -> Vec<(&str, Option<i64>)> {
    let function = function_for(variant);
    let Some(group) = list.groups.iter().find(|group| group.function == function) else {
        return dedupe_entries(list);
    };

    // 分组内仍可能重名（上游偶有重复），同样保序去重。
    // 另外**剔除第三方 / 自定义路由条目**：网关发的是 `config_name` / `model_name`
    // 这套字段，对它们无效 —— 本机实测 `openrouter//stealth/ox-alpha`
    // （`provider = "openrouter"`，就在 `solo_work_lite` 分组里）被上游
    // `4001 param is invalid` 拒。列出来就是又一个「看得见、调不动」。
    let mut seen = std::collections::HashSet::new();
    group
        .models
        .iter()
        .filter(|model| !model.is_bypass)
        .filter(|model| seen.insert(model.name.as_str()))
        .map(|model| (model.name.as_str(), model.context_window))
        .collect()
}

/// 把客户端清单展平成 `(模型名, 上下文窗口)`，并**按出现顺序去重**。
///
/// 去重是必需的：同一个模型会在多个 function 分组里重复出现（本机实测
/// `solo_work_lite` 与 `solo_work_remote` 内容完全相同）。
/// **保序**（而不是排序）是为了让 `/v1/models` 的顺序与客户端里看到的一致。
fn dedupe_entries(list: &ClientModelList) -> Vec<(&str, Option<i64>)> {
    let mut seen = std::collections::HashSet::new();
    let mut entries: Vec<(&str, Option<i64>)> = Vec::new();
    for group in &list.groups {
        for model in &group.models {
            if seen.insert(model.name.as_str()) {
                entries.push((model.name.as_str(), model.context_window));
            }
        }
    }
    entries
}

/// 把 `(模型名, 上下文窗口)` 组装成 OpenAI `/v1/models` 形状。
///
/// 两条来源（客户端清单 / 静态兜底）共用本函数 —— 形状只有一处，不会漂移。
fn models_response_from(entries: Vec<(&str, Option<i64>)>) -> Value {
    json!({
        "object": "list",
        "data": entries
            .into_iter()
            .map(|(name, context_window)| json!({
                "id": name,
                "object": "model",
                "created": 1_753_600_000,
                "owned_by": "trae",
                "context_length": context_window.unwrap_or(DEFAULT_CONTEXT_LENGTH),
            }))
            .collect::<Vec<_>>(),
    })
}

/// 生成一个 UUID 形态的随机串（上游要求 `conversation_id` 等字段形如 UUID）。
fn uuid_like() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 从 OpenAI 请求体提取"对话稳定标识原文"（移植自本机 Python 转发器
/// `ZhuanFaFuWuQi.py` 的 `Han_QuHuiHuaBiaoShi`）。
///
/// 优先级：
/// 1. `prompt_cache_key` —— 客户端自带的提示词缓存键，同对话内天然稳定；
/// 2. `metadata.session_id` —— 部分客户端把自身会话号放在元数据里；
/// 3. `user` —— 客户端传入的稳定用户标识；
/// 4. 兜底内容指纹：`system` 提示 + 首条 `user` 消息一起拼串。
///    OpenAI 协议下多轮对话的历史在 `messages` 头部只增不改，
///    所以同一对话的多次请求哈希原文完全一致 —— 这就是"对话复用"的根基。
///
/// 返回 `None` 表示连指纹都拼不出（无消息/空消息），调用方退回随机 UUID。
fn conversation_identity_source(object: &serde_json::Map<String, Value>) -> Option<String> {
    // 1-3：客户端自带标识，加类型前缀防止不同来源的值意外撞车。
    for (prefix, field) in [
        ("cache_key:", "prompt_cache_key"),
        ("metadata:", "metadata"),
        ("user:", "user"),
    ] {
        let raw = object.get(field);
        let raw = match field {
            // metadata 是对象，取其 session_id 子字段
            "metadata" => raw
                .and_then(|metadata| metadata.get("session_id"))
                .and_then(Value::as_str),
            _ => raw.and_then(Value::as_str),
        };
        if let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) {
            return Some(format!("{prefix}{value}"));
        }
    }

    // 4：内容指纹 —— 扫描 messages，取首个 system 与首个 user 的文本。
    let messages = object.get("messages")?.as_array()?;
    let mut system_text: Option<String> = None;
    let mut user_text: Option<String> = None;
    for message in messages {
        let (Some(role), Some(content)) = (
            message.get("role").and_then(Value::as_str),
            message.get("content"),
        ) else {
            continue;
        };
        // 字符串原样；分段数组等复杂结构整体序列化（同客户端同请求形态稳定）。
        let text = match content {
            Value::String(text) => Some(text.clone()),
            Value::Null => None,
            other => serde_json::to_string(other).ok(),
        };
        match (role, text) {
            ("system", Some(text)) if system_text.is_none() => system_text = Some(text),
            ("user", Some(text)) if user_text.is_none() => user_text = Some(text),
            _ => {}
        }
        if system_text.is_some() && user_text.is_some() {
            break; // 两段都取到，无需继续扫描长对话
        }
    }
    let mut parts = Vec::new();
    if let Some(text) = system_text {
        parts.push(format!("system:{text}"));
    }
    if let Some(text) = user_text {
        parts.push(format!("user:{text}"));
    }
    (!parts.is_empty()).then(|| parts.join("|"))
}

/// 由标识原文派生**稳定**的 UUID 形态会话号（移植 Python 转发器的
/// `Han_ShengChengHuiHuaHao`：SHA256 摘要前 16 字节 → UUID v4 位形）。
///
/// `salt` 区分字段用途（conv/sess/proj 各一把盐）：同一对话派生出
/// 三个互不相同、又各自稳定的 ID。同 Python 实现一样手工置版本/变体位，
/// 保证上游看到的仍是标准 v4 UUID 形态。
fn stable_conversation_id(source: &str, salt: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b"|");
    hasher.update(source.as_bytes());
    let digest = hasher.finalize();
    let mut bytes: [u8; 16] = digest[..16].try_into().expect("摘要必有 16 字节");
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // 版本位 → v4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // 变体位 → RFC 4122
    uuid::Uuid::from_bytes(bytes).to_string()
}

/// 从 OpenAI 请求体读取模型名；缺失或空白时用 `default_model`。
pub fn model_of(body: &Value, default_model: &str) -> String {
    body.get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default_model)
        .to_string()
}

/// 客户端是否要求流式响应。
pub fn wants_stream(body: &Value) -> bool {
    body.get("stream").and_then(Value::as_bool).unwrap_or(false)
}

/// 改写为 `llm_utils_chat` 请求体。
///
/// `variant` 决定 `function`（**按程序位分家**，见 [`super::function_for`]）——
/// 上游按 `function` 做白名单，写死一个会让另一条产品线的模型全部
/// `4001 param is invalid`（issue #4 的实测现场）。
///
/// `src` 不是合法 JSON 对象时原样返回（交给上游报错，而不是在这里吞掉）。
pub fn prepare_llm_chat_body(
    src: &[u8],
    variant: TraeVariant,
    default_model: &str,
    uid: &str,
    device_id: &str,
    machine_id: &str,
) -> Vec<u8> {
    let mut body: Value = match serde_json::from_slice(src) {
        Ok(value) => value,
        Err(_) => return src.to_vec(),
    };
    let Some(object) = body.as_object_mut() else {
        return src.to_vec();
    };

    normalize_messages(object);
    normalize_tool_choice(object);
    normalize_tools(object);

    let model = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default_model)
        .to_string();
    let (config_name, model_name) = model_config(&model);

    // 固定字段：覆盖客户端可能传来的同名值（上游只认这一套）。
    object.insert("config_name".into(), json!(config_name));
    object.insert("model_name".into(), json!(model_name));
    // 上游只支持流式；`stream:false` 由本网关本地聚合。
    object.insert("stream".into(), json!(true));
    object.insert("function".into(), json!(function_for(variant)));
    // 会话三 ID：按「对话内容 + 账号」稳定派生（对话复用），取不到指纹才随机。
    // 同一对话的多次请求（含多轮追加历史）派生出相同的 ID，
    // 上游侧得以做路由亲和与提示词缓存；不同对话互不串号。
    // `uid` 必须混入哈希：两个账号发起内容相同的对话时，若只按内容派生
    // 会得到完全相同的 ID —— 跨账号同会话号是典型的风控特征。
    // 实测上游对任意 UUID 形态的值都接受（随机 v4 也一直通过）。
    let identity_source = conversation_identity_source(object);
    let derive_id = |salt: &str| {
        identity_source
            .as_deref()
            .map(|source| stable_conversation_id(&format!("{source}|uid:{uid}"), salt))
            .unwrap_or_else(uuid_like)
    };
    object.insert("conversation_id".into(), json!(derive_id("conv:")));
    object.insert("user_id".into(), json!(uid));
    object.insert("session_id".into(), json!(derive_id("sess:")));
    object.insert("device_id".into(), json!(device_id));
    object.insert("machine_id".into(), json!(machine_id));
    object.insert("project_id".into(), json!(derive_id("proj:")));
    object.insert("workspace_id".into(), json!("e04cdd"));
    object.insert("prompt_max_tokens".into(), json!(168_000));
    object.insert("mode".into(), json!("FunctionCall"));
    object.insert("ide_version".into(), json!(TRAE_IDE_VERSION));
    object.insert("ide_version_code".into(), json!(TRAE_IDE_VERSION_CODE));
    object.insert("app_id".into(), json!(TRAE_APP_ID));
    object.insert("package_type".into(), json!("stable_cn"));

    serde_json::to_vec(&body).unwrap_or_else(|_| src.to_vec())
}

/// 消息数组的三处改写：`content` 转数组、`tool_calls` 转 `function_call`、
/// 丢掉没有函数名的空 tool_call。
fn normalize_messages(object: &mut Map<String, Value>) {
    let Some(messages) = object.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages.iter_mut() {
        let Some(message) = message.as_object_mut() else {
            continue;
        };

        if message.get("role").and_then(Value::as_str) == Some("assistant") {
            if let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
                let kept: Vec<Value> = calls
                    .iter_mut()
                    .filter_map(|call| {
                        let call = call.as_object_mut()?;
                        if let Some(function) = call.remove("function") {
                            call.insert("function_call".into(), function);
                        }
                        // 没有函数名的 tool_call 是上游会拒绝的脏数据，直接丢弃。
                        let named = call
                            .get("function_call")
                            .and_then(|function| function.get("name"))
                            .and_then(Value::as_str)
                            .map(|name| !name.trim().is_empty())
                            .unwrap_or(false);
                        named.then(|| Value::Object(call.clone()))
                    })
                    .collect();
                if kept.is_empty() {
                    message.remove("tool_calls");
                } else {
                    *calls = kept;
                }
            }
        }

        // `content` 字符串 → 数组形态。
        if let Some(text) = message.get("content").and_then(Value::as_str) {
            message.insert("content".into(), json!([{ "type": "text", "text": text }]));
        }
    }
}

/// `tool_choice` 归一化：上游只认 `auto` / `required` / 具体函数名 / 缺省。
fn normalize_tool_choice(object: &mut Map<String, Value>) {
    let Some(choice) = object.remove("tool_choice") else {
        return;
    };
    let suppress = |object: &mut Map<String, Value>| {
        object.remove("tools");
        object.remove("functions");
    };
    match choice {
        Value::String(text) => {
            if text.trim().eq_ignore_ascii_case("none") {
                suppress(object);
            } else {
                object.insert("tool_choice".into(), Value::String(text));
            }
        }
        Value::Object(map) => {
            let kind = map
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_lowercase)
                .unwrap_or_default();
            match kind.as_str() {
                "none" => suppress(object),
                "auto" | "required" => {
                    object.insert("tool_choice".into(), Value::String(kind));
                }
                "function" => {
                    let name = map
                        .get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                        .or_else(|| map.get("name").and_then(Value::as_str))
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("auto")
                        .to_string();
                    object.insert("tool_choice".into(), Value::String(name));
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// `tools` 归一化：`parameters` 对象 → JSON 字符串；无函数名的条目丢弃。
fn normalize_tools(object: &mut Map<String, Value>) {
    let Some(raw) = object.get_mut("tools") else {
        return;
    };
    let Some(list) = raw.as_array_mut() else {
        return;
    };
    if list.is_empty() {
        object.remove("tools");
        return;
    }
    let mut kept = Vec::with_capacity(list.len());
    for item in list.iter_mut() {
        let Some(tool) = item.as_object_mut() else {
            continue;
        };
        let Some(function) = tool.get_mut("function").and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(parameters) = function.get("parameters") {
            if parameters.is_object() {
                if let Ok(text) = serde_json::to_string(parameters) {
                    function.insert("parameters".into(), Value::String(text));
                }
            }
        }
        kept.push(item.clone());
    }
    if kept.is_empty() {
        object.remove("tools");
    } else {
        *raw = Value::Array(kept);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // 两个 `function` 常量只被断言用到（lib 侧经 `function_for` 取），
    // 故**只在测试模块导入** —— 放顶层会得到 `unused_imports` 警告。
    use crate::trae::{TRAE_FUNCTION_CHAT_V3, TRAE_FUNCTION_SOLO_WORK};
    use buddy_switch_core::modules::trae::model_list::{ClientModel, ClientModelGroup};

    fn prepared(body: Value) -> Value {
        let bytes = serde_json::to_vec(&body).unwrap();
        let out = prepare_llm_chat_body(
            &bytes,
            TraeVariant::TraeWork,
            "deepseek-v4-flash",
            "u1",
            "dev1",
            "mach1",
        );
        serde_json::from_slice(&out).expect("output must stay valid JSON")
    }

    /// 对话复用护栏：同一对话（system + 首条 user 不变，仅追加历史）
    /// 的多次请求必须派生出**相同**的三个会话 ID；不同对话互不相同；
    /// 三个 ID 彼此也不同（盐前缀隔离）。
    #[test]
    fn conversation_ids_are_stable_per_conversation() {
        let first_turn = json!({
            "model": "glm-5.3-flash",
            "messages": [
                {"role": "system", "content": "你是助手"},
                {"role": "user", "content": "第一轮提问"}
            ]
        });
        // 模拟多轮对话：历史头部不动，末尾追加 assistant 与新 user。
        let second_turn = {
            let mut body = first_turn.clone();
            body["messages"].as_array_mut().unwrap().push(
                json!({"role": "assistant", "content": "第一轮回答"}),
            );
            body["messages"].as_array_mut().unwrap().push(
                json!({"role": "user", "content": "第二轮提问"}),
            );
            body
        };
        let first = prepared(first_turn);
        let second = prepared(second_turn);
        for field in ["conversation_id", "session_id", "project_id"] {
            let a = first[field].as_str().unwrap();
            let b = second[field].as_str().unwrap();
            assert_eq!(a, b, "{field} 同一对话多轮必须复用同值");
            assert!(a.len() == 36 && a.chars().filter(|c| *c == '-').count() == 4,
                    "{field} 必须保持 UUID 形态：{a}");
        }
        // 三个 ID 互不相同（不同盐派生）。
        assert_ne!(first["conversation_id"], first["session_id"]);
        assert_ne!(first["session_id"], first["project_id"]);
        // 不同对话（不同首条 user）不得串号。
        let other = prepared(json!({
            "messages": [
                {"role": "system", "content": "你是助手"},
                {"role": "user", "content": "另一段对话"}
            ]
        }));
        for field in ["conversation_id", "session_id", "project_id"] {
            assert_ne!(first[field], other[field], "{field} 不同对话必须不同");
        }
    }

    /// 客户端自带标识优先于内容指纹：prompt_cache_key 存在时以其为准，
    /// 即使消息内容完全不同也复用同一套 ID（与 Python 转发器语义一致）。
    #[test]
    fn client_cache_key_wins_over_content_fingerprint() {
        let plain = prepared(json!({
            "prompt_cache_key": "dialog-42",
            "messages": [{"role": "user", "content": "甲"}]
        }));
        let padded = prepared(json!({
            "prompt_cache_key": " dialog-42 ",
            "messages": [{"role": "user", "content": "乙"}]
        }));
        for field in ["conversation_id", "session_id", "project_id"] {
            assert_eq!(plain[field], padded[field],
                "{field} 相同 cache_key（含空白差异）必须同值");
        }
    }

    /// 无任何标识且无消息指纹（空 messages）时退回随机 UUID ——
    /// 每请求不同，但仍是合法 v4 形态（上游只认 UUID）。
    #[test]
    fn empty_messages_fall_back_to_random_uuid() {
        let first = prepared(json!({"messages": []}));
        let second = prepared(json!({"messages": []}));
        for field in ["conversation_id", "session_id", "project_id"] {
            let value = first[field].as_str().unwrap();
            assert_ne!(first[field], second[field], "{field} 无指纹时应每请求随机");
            assert!(value.len() == 36, "{field} 随机兜底也须 UUID 形态：{value}");
        }
    }

    /// 跨账号隔离护栏：**相同对话内容 + 不同账号**必须派生出不同 ID。
    ///
    /// 反例（改坏会红）：派生只混对话内容不混 `uid` ⇒ 两个账号发同一段
    /// "你好"会得到完全相同的 session_id —— 跨账号同会话号是典型的
    /// 风控特征，且号池切换账号时同一 ID 被反复提交。
    #[test]
    fn same_conversation_different_uid_must_differ() {
        let make_body = || {
            json!({
                "messages": [
                    {"role": "system", "content": "你是助手"},
                    {"role": "user", "content": "你好"}
                ]
            })
        };
        let bytes = serde_json::to_vec(&make_body()).unwrap();
        let prepare_for = |uid: &str| {
            serde_json::from_slice::<Value>(&prepare_llm_chat_body(
                &bytes,
                TraeVariant::TraeWork,
                "deepseek-v4-flash",
                uid,
                "dev1",
                "mach1",
            ))
            .unwrap()
        };
        let account_a = prepare_for("1958692729393946");
        let account_b = prepare_for("2958692729393947");
        for field in ["conversation_id", "session_id", "project_id"] {
            assert_ne!(account_a[field], account_b[field],
                "{field} 相同对话不同账号必须不同");
        }
    }

    #[test]
    fn model_config_is_case_insensitive_and_falls_back() {
        // 表里登记的：大小写归一 + 历史别名，行为与改造前**逐字一致**。
        assert_eq!(
            model_config("DeepSeek-V4-Flash"),
            ("DeepSeek-V4-Flash".to_string(), "deepseek_v4_flash__dev".to_string())
        );
        assert_eq!(model_config("  GLM-5.3 "), ("glm-5.3".to_string(), "glm-5.3__dev".to_string()));
        assert_eq!(
            model_config("seed-code-pro-0430"),
            ("Doubao-Seed-2.1-Pro".to_string(), "Doubao-Seed-2.1-Pro__dev".to_string())
        );
    }

    /// ★ 护栏：**表里没有的模型名按规则派生**，而不是静默回落成 DeepSeek。
    ///
    /// 反例（改坏会红）：把 `_ =>` 写回 `("DeepSeek-V4-Flash", "deepseek_v4_flash__dev")`
    /// ⇒ 客户端清单里真实存在的 `glm-5.3-flash` 会被当成 DeepSeek 发出去，
    /// 用户拿到的是**另一个模型的回答**且没有任何报错。
    ///
    /// 规则来自上游实测（见 `model_config` 的文档）：`config_name = 模型名`、
    /// `model_name = 模型名 + "__dev"`。
    #[test]
    fn unknown_model_is_derived_not_silently_swapped() {
        assert_eq!(
            model_config("glm-5.3-flash"),
            ("glm-5.3-flash".to_string(), "glm-5.3-flash__dev".to_string())
        );
        assert_eq!(
            model_config("  qwen3.8-flash  "),
            ("qwen3.8-flash".to_string(), "qwen3.8-flash__dev".to_string()),
            "派生必须用 trim 之后的名字"
        );
        assert_eq!(
            model_config("step-5-preview"),
            ("step-5-preview".to_string(), "step-5-preview__dev".to_string())
        );
        // 退化输入（配置里 `defaultModel` 被清空）：退回**内置默认模型**再正常解析，
        // 与「未指定模型」的既有语义一致（`prepare_llm_chat_body` 也走这条）。
        assert_eq!(
            model_config(""),
            model_config(TRAE_DEFAULT_MODEL),
            "空名应等同于内置默认模型"
        );
        // ★ 反向断言：**非空**的未知模型名一律不得被换成那个「碰巧写死的」模型。
        for name in ["glm-5.3-flash", "no-such-model", "qwen3.8-flash"] {
            assert_ne!(
                model_config(name).0,
                "DeepSeek-V4-Flash",
                "未知模型不许被换成 DeepSeek：{name}"
            );
            assert_eq!(
                model_config(name).1,
                format!("{name}__dev"),
                "派生规则：model_name = 名字 + __dev"
            );
        }
    }

    #[test]
    fn string_content_is_converted_to_text_array() {
        let out = prepared(json!({
            "model": "glm-5.3",
            "messages": [{"role": "user", "content": "你好"}],
        }));
        assert_eq!(
            out["messages"][0]["content"],
            json!([{"type": "text", "text": "你好"}])
        );
    }

    #[test]
    fn assistant_tool_calls_become_function_call_and_unnamed_are_dropped() {
        let out = prepared(json!({
            "model": "glm-5.3",
            "messages": [{
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"id": "1", "function": {"name": "read_file", "arguments": "{}"}},
                    {"id": "2", "function": {"name": "  ", "arguments": "{}"}}
                ],
            }],
        }));
        let calls = out["messages"][0]["tool_calls"].as_array().unwrap();
        assert_eq!(calls.len(), 1, "无名 tool_call 必须被丢弃");
        assert_eq!(calls[0]["function_call"]["name"], "read_file");
        assert!(calls[0].get("function").is_none(), "不得同时保留 function");
    }

    #[test]
    fn tool_parameters_object_is_stringified() {
        let out = prepared(json!({
            "model": "glm-5.3",
            "messages": [],
            "tools": [{
                "type": "function",
                "function": {"name": "f", "parameters": {"type": "object", "properties": {}}}
            }],
        }));
        let parameters = &out["tools"][0]["function"]["parameters"];
        assert!(parameters.is_string(), "parameters 必须是 JSON 字符串");
        let parsed: Value = serde_json::from_str(parameters.as_str().unwrap()).unwrap();
        assert_eq!(parsed["type"], "object");
    }

    #[test]
    fn tool_choice_none_suppresses_tools() {
        let out = prepared(json!({
            "model": "glm-5.3",
            "messages": [],
            "tool_choice": "none",
            "tools": [{"type": "function", "function": {"name": "f"}}],
        }));
        assert!(out.get("tools").is_none());
        assert!(out.get("tool_choice").is_none());
    }

    #[test]
    fn tool_choice_function_maps_to_a_plain_name() {
        let out = prepared(json!({
            "model": "glm-5.3",
            "messages": [],
            "tool_choice": {"type": "function", "function": {"name": "read_file"}},
        }));
        assert_eq!(out["tool_choice"], "read_file");
    }

    #[test]
    fn required_fields_are_always_injected_and_stream_is_forced() {
        let out = prepared(json!({
            "model": "glm-5.3",
            "messages": [],
            "stream": false,
            "max_tokens": 1,
        }));
        // 上游只支持流式：客户端说 false，这里仍必须发 true。
        assert_eq!(out["stream"], true);
        assert_eq!(out["function"], function_for(TraeVariant::TraeWork));
        assert_eq!(out["app_id"], TRAE_APP_ID);
        assert_eq!(out["ide_version"], TRAE_IDE_VERSION);
        assert_eq!(out["ide_version_code"], TRAE_IDE_VERSION_CODE);
        assert_eq!(out["workspace_id"], "e04cdd");
        assert_eq!(out["mode"], "FunctionCall");
        assert_eq!(out["user_id"], "u1");
        assert_eq!(out["device_id"], "dev1");
        assert_eq!(out["machine_id"], "mach1");
        assert_eq!(out["config_name"], "glm-5.3");
        assert_eq!(out["model_name"], "glm-5.3__dev");
        // 生成型字段必须存在且形如 UUID。
        for key in ["conversation_id", "session_id", "project_id"] {
            let value = out[key].as_str().expect(key);
            assert!(uuid::Uuid::parse_str(value).is_ok(), "{key}={value}");
        }
    }

    #[test]
    fn missing_model_uses_the_default() {
        let out = prepared(json!({"messages": []}));
        assert_eq!(out["config_name"], "DeepSeek-V4-Flash");
        assert_eq!(out["model_name"], "deepseek_v4_flash__dev");
    }

    #[test]
    fn invalid_json_is_passed_through_untouched() {
        let raw = b"not json at all";
        assert_eq!(
            prepare_llm_chat_body(raw, TraeVariant::TraeWork, "m", "u", "d", "m"),
            raw.to_vec()
        );
        // 合法 JSON 但不是对象，同样原样返回。
        let array = b"[1,2,3]";
        assert_eq!(
            prepare_llm_chat_body(array, TraeVariant::TraeWork, "m", "u", "d", "m"),
            array.to_vec()
        );
    }

    #[test]
    fn model_and_stream_readers_are_defensive() {
        assert_eq!(model_of(&json!({"model": "x"}), "d"), "x");
        assert_eq!(model_of(&json!({"model": "  "}), "d"), "d");
        assert_eq!(model_of(&json!({}), "d"), "d");
        assert!(!wants_stream(&json!({})));
        assert!(wants_stream(&json!({"stream": true})));
    }

    /// ★★ 护栏：静态兜底清单**内容**必须与「实测可调」一致（issue #4 后续报障的正题）。
    ///
    /// 反例（改坏会红）：
    /// - 把 `openrouter//stealth/ox-alpha` 加回去 ⇒ 它在 `solo_work_lite` 下**实测被
    ///   上游 `4001` 拒**，客户端读不到时就会对外宣传一个调不动的模型；
    /// - 把 `doubao-seed-2.0-code` 加回去 ⇒ 它属于 `solo_coder`，在 `solo_work_lite`
    ///   下同样 `4001`（它就是改造前「27 条里 11 条调不动」的一员）；
    /// - 把默认模型从清单里删掉 ⇒ 卡片会显示一个「默认 X」但 X 不在清单里。
    #[test]
    fn static_fallback_list_matches_measured_servable_names() {
        // 实测被上游拒绝的条目**不得**出现在对外清单里。
        assert!(
            !MODEL_NAMES.contains(&"openrouter//stealth/ox-alpha"),
            "第三方路由条目实测 4001，不得宣传"
        );
        assert!(
            !MODEL_NAMES.contains(&"doubao-seed-2.0-code"),
            "属于 solo_coder，在 solo_work_lite 下实测 4001"
        );

        // 默认模型必须在清单里（否则「默认 X」指向一个列不出来的名字）。
        assert!(
            MODEL_NAMES.contains(&TRAE_DEFAULT_MODEL),
            "默认模型 {TRAE_DEFAULT_MODEL} 必须在兜底清单里"
        );

        // 客户端 `solo_work_lite` 那批（实测接受）必须在清单里。
        for name in [
            "Doubao-Seed-Evolving",
            "step-5-preview",
            "deepseek-v4.1-flash",
            "qwen3.8-max",
            "DeepSeek-V4-Flash-Official",
        ] {
            assert!(MODEL_NAMES.contains(&name), "实测可调的 {name} 不该被删掉");
        }

        // 每个名字都必须能解析出非空的 config_name / model_name（派生规则生效）。
        for name in MODEL_NAMES {
            let (config_name, model_name) = model_config(name);
            assert!(!config_name.is_empty() && !model_name.is_empty(), "{name} 解析为空");
            assert!(model_name.ends_with("__dev"), "{name} 的 model_name 应为 __dev 形态");
        }
    }

    #[test]
    fn models_response_lists_every_name_as_openai_objects() {
        let response = models_response();
        let data = response["data"].as_array().unwrap();
        assert_eq!(data.len(), MODEL_NAMES.len());
        assert_eq!(data[0]["object"], "model");
        assert_eq!(response["object"], "list");
    }

    /// 客户端清单里的**真实**上下文窗口优先；取不到时回落既有常量（形状不退化）。
    #[test]
    fn models_response_from_uses_real_context_window_and_falls_back() {
        let response = models_response_from(vec![("a", Some(256_000)), ("b", None)]);
        let data = response["data"].as_array().unwrap();

        assert_eq!(data[0]["id"], "a");
        assert_eq!(data[0]["context_length"], 256_000);
        assert_eq!(data[1]["id"], "b");
        assert_eq!(
            data[1]["context_length"], DEFAULT_CONTEXT_LENGTH,
            "取不到窗口必须回落成数值，不能是 null —— 老客户端会直接读这个字段"
        );
    }

    /// ★ 去重必须**保序**（不是排序）：同一模型在多个 function 分组里重复出现，
    /// 但 `/v1/models` 不该列出重复项，且顺序要与客户端里看到的一致。
    #[test]
    fn dedupe_entries_keeps_first_occurrence_order() {
        let list = ClientModelList {
            variant: "trae_work".into(),
            variant_label: "Trae Work".into(),
            source: "client-cache".into(),
            read_at: 0,
            data_dir: None,
            uid: None,
            groups: vec![
                ClientModelGroup {
                    function: "a".into(),
                    models: vec![client_model("m1", Some(1)), client_model("m2", None)],
                },
                ClientModelGroup {
                    function: "b".into(),
                    models: vec![client_model("m2", Some(2)), client_model("m3", None)],
                },
            ],
            note: None,
        };

        let entries = dedupe_entries(&list);
        assert_eq!(
            entries.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            vec!["m1", "m2", "m3"],
            "重复的 m2 只能出现一次，且位置保持在首次出现处"
        );
        assert_eq!(
            entries[1].1, None,
            "保留**首次**出现那份的上下文窗口（m2 首次出现时没有窗口）"
        );
    }

    /// ★★ 护栏：`function` **按程序位分家，不按区域**（issue #4 的核心修复）。
    ///
    /// 反例（改坏会红）：写死一个常量 ⇒ TraeCode 的模型全部 `4001 param is invalid`
    /// （上游按 function 做白名单，2026-09-30 实测）。
    #[test]
    fn function_is_per_program_not_per_region() {
        // TraeWork 家族（国内 + 国际）用 chat 分组 `solo_work_lite`。
        assert_eq!(function_for(TraeVariant::TraeWork), TRAE_FUNCTION_SOLO_WORK);
        assert_eq!(function_for(TraeVariant::Global), TRAE_FUNCTION_SOLO_WORK);
        // TraeCode 家族用 `chat_v3`（实测能调 glm-5.3-flash）。
        assert_eq!(function_for(TraeVariant::Trae), TRAE_FUNCTION_CHAT_V3);
        assert_eq!(function_for(TraeVariant::GlobalTraeCode), TRAE_FUNCTION_CHAT_V3);
        // ★ 两个家族必须**不同** —— 否则「分家」等于没分。
        assert_ne!(
            function_for(TraeVariant::TraeWork),
            function_for(TraeVariant::Trae)
        );
        // 同一区域内的两条程序位也必须不同（判据是程序，不是区域）。
        assert_ne!(
            function_for(TraeVariant::TraeWork),
            function_for(TraeVariant::Trae),
            "国内版两条程序位的 function 不能相同"
        );
    }

    /// ★ 端到端（构造层）：TraeCode 变体发出的请求体里 `function` 必须是 `chat_v3`。
    ///
    /// 上面那条只钉住映射函数；这条钉住**真的写进了 body**（防止有人改了常量却漏接线）。
    #[test]
    fn trae_code_body_carries_chat_v3() {
        let bytes = serde_json::to_vec(&json!({
            "model": "glm-5.3-flash",
            "messages": [{"role": "user", "content": "ping"}],
        }))
        .unwrap();

        let out: Value = serde_json::from_slice(&prepare_llm_chat_body(
            &bytes,
            TraeVariant::Trae,
            "glm-5.3-flash",
            "u1",
            "dev1",
            "mach1",
        ))
        .unwrap();

        assert_eq!(out["function"], TRAE_FUNCTION_CHAT_V3);
        // 客户端清单里没有 `config_name` 字段 ⇒ 只能派生，派生结果必须原样是模型名。
        assert_eq!(out["config_name"], "glm-5.3-flash");
        assert_eq!(out["model_name"], "glm-5.3-flash__dev");
    }

    /// ★★ 护栏：`/v1/models` **只列本 function 分组里的模型**。
    ///
    /// 反例（改坏会红）：回到 `dedupe_entries`（全分组并集）⇒ 本机实测 TraeWork 会列出
    /// 27 条，其中 11 条（`solo_coder` 等分组）**宣传了但 4001 调不动** ——
    /// 正是 issue #4 那句「看得见、调不动」，只是换了一批模型。
    #[test]
    fn servable_entries_lists_only_the_function_group() {
        let list = |groups: Vec<ClientModelGroup>| ClientModelList {
            variant: "trae_work".into(),
            variant_label: "Trae Work".into(),
            source: "client-cache".into(),
            read_at: 0,
            data_dir: None,
            uid: None,
            groups,
            note: None,
        };
        let group = |function: &str, names: &[&str]| ClientModelGroup {
            function: function.into(),
            models: names.iter().map(|name| client_model(name, None)).collect(),
        };

        // TraeWork：只取 `solo_work_lite`，`solo_coder` 里的那条**不得**出现。
        let work = list(vec![
            group("solo_work_lite", &["glm-5.3", "deepseek-v4.1-flash"]),
            group("solo_coder", &["Doubao-Seed-Code"]),
        ]);
        assert_eq!(
            servable_entries(&work, TraeVariant::TraeWork)
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            vec!["glm-5.3", "deepseek-v4.1-flash"]
        );

        // TraeCode：只取 `chat_v3`，`refactor` 那批内部模型**不得**出现。
        let code = list(vec![
            group("chat_v3", &["glm-5.3-flash", "qwen3.8-flash"]),
            group("refactor", &["refactor_scoper"]),
            group("code_reviewer", &["code-review-judge"]),
        ]);
        assert_eq!(
            servable_entries(&code, TraeVariant::Trae)
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            vec!["glm-5.3-flash", "qwen3.8-flash"]
        );

        // ★ 回落：分组缺失（客户端改名 / 版本变动）⇒ 退回全分组并集，**绝不清空**。
        let renamed = list(vec![
            group("solo_work_lite_v2", &["glm-5.3"]),
            group("solo_coder", &["Doubao-Seed-Code"]),
        ]);
        assert_eq!(
            servable_entries(&renamed, TraeVariant::TraeWork)
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            vec!["glm-5.3", "Doubao-Seed-Code"],
            "分组名对不上时宁可多列（老行为），也不能把清单清空"
        );

        // ★ 第三方路由条目（`provider` 非空 / `custom_model_id` 有值）**必须被剔除**：
        //   实测 `openrouter//stealth/ox-alpha` 就在 `solo_work_lite` 分组里，
        //   但被上游 `4001` 拒 —— 列出来就是又一个「看得见、调不动」。
        let bypass = list(vec![ClientModelGroup {
            function: "solo_work_lite".into(),
            models: vec![
                client_model("glm-5.3", None),
                client_model_with("openrouter//stealth/ox-alpha", None, true),
            ],
        }]);
        assert_eq!(
            servable_entries(&bypass, TraeVariant::TraeWork)
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            vec!["glm-5.3"],
            "第三方路由条目不得进对外清单"
        );

        // 分组内重名同样保序去重。
        let dup = list(vec![group("chat_v3", &["a", "b", "a"])]);
        assert_eq!(
            servable_entries(&dup, TraeVariant::Trae)
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    fn client_model(name: &str, context_window: Option<i64>) -> ClientModel {
        client_model_with(name, context_window, false)
    }

    /// 同上，但可指定 `is_bypass`（第三方路由条目）。
    fn client_model_with(name: &str, context_window: Option<i64>, is_bypass: bool) -> ClientModel {
        ClientModel {
            name: name.to_string(),
            display_name: name.to_string(),
            model_type: String::new(),
            multimodal: false,
            is_default: false,
            is_preset: true,
            is_new: false,
            is_beta: false,
            is_bypass,
            context_window,
            prompt_max_tokens: None,
        }
    }
}
