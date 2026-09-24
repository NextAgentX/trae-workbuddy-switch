//! 用户可见错误的**机器可读码**——让前端按当前语言重新渲染同一条错误。
//!
//! ## 为什么需要它
//!
//! 两条通道的错误载体都是**字符串**：
//!
//! - 桌面端：Tauri command 统一签名 `Result<_, String>`（70 处），`invoke` reject 出字符串；
//! - webui：server 统一 `json_err(String)`，响应体 `{ok:false, error:"…"}`。
//!
//! core 里拼好的中文（如 [`super::net::describe_transport_error`] 的「无法连接（DNS 解析失败…）」）
//! 到前端时已经是成品句，前端**无从判断这是哪一类错误**，也就无法翻译。
//!
//! ## 契约：文本 + 结构尾
//!
//! 不新增字段、不改 JSON 形状、不动 70 个命令签名，而是让字符串**自带尾部编码**：
//!
//! ```text
//! 无法连接（DNS 解析失败 / 网络不可达 / TLS 握手失败）：error sending request…␟net.transport.connect␟{"chain":"…"}
//! ```
//!
//! 分隔符是 **`U+001F`（Unit Separator）** —— 它本来就是为「同一条记录内分隔字段」设计的，
//! 不会出现在任何正常的错误文案里（`Display` 出来的通常文本不含控制字符）。
//!
//! ## 三条硬约束
//!
//! 1. **`Display` 只给纯中文**（不含尾部）。因此 `format!("{e}")` / `to_string()` / 既有断言
//!    的可见行为**逐字节不变** —— 这是「CN 行为零变化」的落点。
//! 2. **只有显式调用 [`AppError::to_wire`] 才加尾**。所以「哪些错误带码」是**生产者逐个决定**的，
//!    而不是全仓一起变。
//! 3. **解码永不失败**：对没有尾部的普通字符串，[`decode_wire`] 原样返回文本、`code` 为
//!    `None`。旧版本前端 / 旧日志 / 手工写入的文本都不会因此报错。
//!
//! ## 新增一个码
//!
//! 在 [`ErrorCode`] 上加一个变体 + `as_str` 一行，并在前端 `src/lib/error-code.ts` 的
//! catalog 里补 zh/en 两条文案。**码一旦发布不得改名**（它是跨版本的协议）。

/// 字段分隔符。见模块文档「契约」一节。
pub const WIRE_SEP: char = '\u{1f}';

/// 用户可见错误的机器可读码。
///
/// 命名规则：`<域>.<子域>.<种类>`，全小写点分。**发布后不得改名**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// 连接或读取超时。
    NetTransportTimeout,
    /// 连接建立失败：DNS / 路由 / TCP / TLS 任一环节。
    NetTransportConnect,
    /// 连接成功但响应体读不出来（连接被中断、解码失败）。
    NetTransportBody,
    /// 其余传输层失败（构建请求失败、重定向、状态码错误等）。
    NetTransportOther,
    /// 缺少「完全磁盘访问 / App 管理」权限，写认证文件被系统拒绝。
    ///
    /// 前端据此弹出授权引导（轮询授权状态、给出三步说明）。**必须走码而不是匹配中文**：
    /// 界面语言可切换，按中文关键词判定会让引导在英文界面下静默失效。
    PermissionDenied,
}

impl ErrorCode {
    /// 协议标识（跨版本稳定）。
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::NetTransportTimeout => "net.transport.timeout",
            ErrorCode::NetTransportConnect => "net.transport.connect",
            ErrorCode::NetTransportBody => "net.transport.body",
            ErrorCode::NetTransportOther => "net.transport.other",
            ErrorCode::PermissionDenied => "permission.denied",
        }
    }

    /// 从协议标识还原。未知标识返回 `None`（**不报错**：新后端 + 旧前端必须能共存）。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "net.transport.timeout" => Some(ErrorCode::NetTransportTimeout),
            "net.transport.connect" => Some(ErrorCode::NetTransportConnect),
            "net.transport.body" => Some(ErrorCode::NetTransportBody),
            "net.transport.other" => Some(ErrorCode::NetTransportOther),
            "permission.denied" => Some(ErrorCode::PermissionDenied),
            _ => None,
        }
    }
}

/// 一条带码的用户可见错误。
///
/// `text` 是**中文兜底**，也是 [`Display`] 的全部输出 —— 前端认不出该码时直接显示它，
/// 因此任何一端漏配文案都不会出现空白或英文乱入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    code: ErrorCode,
    params: Vec<(String, String)>,
    text: String,
}

impl AppError {
    pub fn new(code: ErrorCode, text: impl Into<String>) -> Self {
        Self {
            code,
            params: Vec::new(),
            text: text.into(),
        }
    }

    /// 追加一个插值参数（前端文案里写作 `{key}`）。同名键后者覆盖前者，便于链式调用。
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        let value = value.into();
        if let Some(slot) = self.params.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            self.params.push((key, value));
        }
        self
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// 中文兜底文案（**不含**尾部编码）。
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn params(&self) -> &[(String, String)] {
        &self.params
    }

    /// 编码为跨通道字符串：`文本␟码␟参数JSON`。
    ///
    /// 参数序列化失败（理论上不会：全是 `String`）时**退化为不带尾部**，
    /// 宁可少一个码，也不能因此丢掉用户看得见的文案。
    pub fn to_wire(&self) -> String {
        let params: serde_json::Map<String, serde_json::Value> = self
            .params
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        match serde_json::to_string(&params) {
            Ok(encoded) => format!(
                "{}{WIRE_SEP}{}{WIRE_SEP}{encoded}",
                self.text,
                self.code.as_str()
            ),
            Err(_) => self.text.clone(),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // ★ 只给纯文本：这是「CN 文案逐字节不变」的落点，勿改成 to_wire()。
        f.write_str(&self.text)
    }
}

impl std::error::Error for AppError {}

/// [`decode_wire`] 的结果：文本永远有值，码与参数可能缺席。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedWire {
    pub text: String,
    pub code: Option<ErrorCode>,
    pub params: Vec<(String, String)>,
    /// 尾部存在但**码无法识别**（新后端 → 旧前端）。此时 `text` 仍然可信。
    pub unrecognized_code: Option<String>,
}

/// 解析跨通道字符串。**永不失败**——见模块文档约束 3。
pub fn decode_wire(raw: &str) -> DecodedWire {
    let plain = |text: &str| DecodedWire {
        text: text.to_string(),
        code: None,
        params: Vec::new(),
        unrecognized_code: None,
    };

    let mut parts = raw.rsplitn(3, WIRE_SEP);
    let params_json = match parts.next() {
        Some(value) => value,
        None => return plain(raw),
    };
    let code_text = match parts.next() {
        Some(value) => value,
        None => return plain(raw),
    };
    let text = match parts.next() {
        Some(value) => value,
        None => return plain(raw),
    };

    // 码缺失/为空 ⇒ 不是本契约的字符串（例如文案里恰好含分隔符），原样返回。
    if code_text.is_empty() {
        return plain(raw);
    }

    let params = match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(params_json)
    {
        Ok(map) => map
            .into_iter()
            .map(|(k, v)| {
                let value = match v {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                (k, value)
            })
            .collect(),
        // 参数坏了不影响文本与码：错误信息比参数重要。
        Err(_) => Vec::new(),
    };

    match ErrorCode::parse(code_text) {
        Some(code) => DecodedWire {
            text: text.to_string(),
            code: Some(code),
            params,
            unrecognized_code: None,
        },
        None => DecodedWire {
            text: text.to_string(),
            code: None,
            params,
            unrecognized_code: Some(code_text.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AppError {
        AppError::new(ErrorCode::NetTransportConnect, "无法连接：握手失败")
            .with("chain", "client error (Connect)")
    }

    /// ★ 可证伪性：若有人把 `Display` 改成 `to_wire()`，这条会红。
    /// 它钉的是「CN 用户看到的文案不因引入错误码而多出任何尾巴」。
    #[test]
    fn display_只给纯文本不带尾部() {
        let error = sample();
        assert_eq!(error.to_string(), "无法连接：握手失败");
        assert!(
            !error.to_string().contains(WIRE_SEP),
            "Display 混入了结构尾: {:?}",
            error.to_string()
        );
        assert!(
            !error.to_string().contains("net.transport"),
            "Display 混入了错误码: {:?}",
            error.to_string()
        );
    }

    #[test]
    fn 往返解码拿到码与参数() {
        let decoded = decode_wire(&sample().to_wire());
        assert_eq!(decoded.text, "无法连接：握手失败");
        assert_eq!(decoded.code, Some(ErrorCode::NetTransportConnect));
        assert_eq!(
            decoded.params,
            vec![("chain".to_string(), "client error (Connect)".to_string())]
        );
        assert_eq!(decoded.unrecognized_code, None);
    }

    #[test]
    fn 无尾部的普通字符串原样返回() {
        for raw in [
            "账号不存在",
            "error sending request for url (https://api.trae.cn/)",
            "",
        ] {
            let decoded = decode_wire(raw);
            assert_eq!(decoded.text, raw, "文本被改动: {raw}");
            assert_eq!(decoded.code, None);
            assert!(decoded.params.is_empty());
            assert_eq!(decoded.unrecognized_code, None);
        }
    }

    /// 新后端 + 旧前端：码认不出时**文本与参数仍要能用**，不能把整条错误吞掉。
    #[test]
    fn 未知码不吞文本() {
        let raw = format!("磁盘已满{WIRE_SEP}future.code{WIRE_SEP}{{\"p\":\"1\"}}");
        let decoded = decode_wire(&raw);
        assert_eq!(decoded.text, "磁盘已满");
        assert_eq!(decoded.code, None);
        assert_eq!(decoded.unrecognized_code.as_deref(), Some("future.code"));
        assert_eq!(decoded.params, vec![("p".to_string(), "1".to_string())]);
    }

    /// 参数 JSON 损坏时只丢参数，文本与码保住。
    #[test]
    fn 参数损坏仍保留文本与码() {
        let raw = format!("连接超时{WIRE_SEP}net.transport.timeout{WIRE_SEP}{{不是JSON");
        let decoded = decode_wire(&raw);
        assert_eq!(decoded.text, "连接超时");
        assert_eq!(decoded.code, Some(ErrorCode::NetTransportTimeout));
        assert!(decoded.params.is_empty());
    }

    #[test]
    fn 码标识可往返且互不相同() {
        let all = [
            ErrorCode::NetTransportTimeout,
            ErrorCode::NetTransportConnect,
            ErrorCode::NetTransportBody,
            ErrorCode::NetTransportOther,
        ];
        let mut seen = Vec::new();
        for code in all {
            let id = code.as_str();
            assert_eq!(ErrorCode::parse(id), Some(code), "标识不可往返: {id}");
            assert!(!seen.contains(&id), "标识重复: {id}");
            seen.push(id);
        }
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn 同名参数后者覆盖前者() {
        let error = sample().with("chain", "覆盖后的值");
        assert_eq!(error.params().len(), 1);
        assert_eq!(error.params()[0].1, "覆盖后的值");
    }

    /// 文本里本来就含分隔符时不能把内容切碎（宁可整条当普通文本）。
    #[test]
    fn 文本自带分隔符时不被误切() {
        let raw = format!("坏文本{WIRE_SEP}还有个分隔符");
        let decoded = decode_wire(&raw);
        assert_eq!(decoded.text, raw);
        assert_eq!(decoded.code, None);
    }
}
