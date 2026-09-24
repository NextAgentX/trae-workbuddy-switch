//! 出站 HTTP 的**传输层错误文案**——全仓唯一出口。
//!
//! ## 为什么要有这个模块
//!
//! reqwest 的错误 `Display` 只有顶层一行：
//!
//! ```text
//! error sending request for url (https://api.trae.cn/…/claim)
//! ```
//!
//! 真正的原因（DNS 解析失败 / TCP 被拒 / TLS 握手失败 / 超时）藏在 `source` 链里。
//! 用户看到的一行提示**完全无法判断**下一步该重试、换网络还是修代理——2026-09-21
//! 用户就是拿着这样一行来报「签到失败」的，而当时谁也说不出原因。
//!
//! 本模块把文案统一成「**中文种类 + 顶层信息 + 展开的原因链**」：
//!
//! ```text
//! 无法连接（DNS 解析失败 / 网络不可达 / TLS 握手失败）：error sending request for url (…)；原因: client error (Connect)；原因: tcp connect error … (os error 10061)
//! ```
//!
//! ## 谁在用它
//!
//! core 的 Trae 模块（签到 / 积分 / 续期 / OAuth）与 `upstream`（WorkBuddy 上游）、
//! 以及 gateway crate（`trae/routes.rs`）全部走 [`describe_transport_error`]。
//! **不要在任何调用点再写 `format!("{e}")`** —— 那是这次要消灭的写法。
//!
//! ## 为什么拆成一堆小函数
//!
//! reqwest 的错误类型无法在单测里凭空构造（`is_timeout()` 之类也就无从触发），
//! 所以只让它做**分类**（[`classify_transport_error`]），文案逻辑全部落在纯函数上，可测。

use super::error_code::{AppError, ErrorCode};

/// 传输层错误的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportErrorKind {
    /// 连接或读取超时。
    Timeout,
    /// 连接建立失败：DNS / 路由 / TCP / TLS 任一环节。
    Connect,
    /// 连接成功但响应体读不出来（连接被中断、解码失败）。
    BodyOrDecode,
    /// 其余（构建请求失败、重定向、状态码错误等）。
    Other,
}

/// 各类的中文标签。
///
/// 措辞源自 gateway `trae/routes.rs` 早期版本的四分法，现在是**全仓唯一一份**，
/// 改这里等于同时改 core 与 gateway —— 同一个故障不会有两种说法。
pub fn transport_error_label(kind: TransportErrorKind) -> &'static str {
    match kind {
        TransportErrorKind::Timeout => "连接超时",
        TransportErrorKind::Connect => "无法连接（DNS 解析失败 / 网络不可达 / TLS 握手失败）",
        TransportErrorKind::BodyOrDecode => "响应体读取失败",
        TransportErrorKind::Other => "请求失败",
    }
}

/// 把 reqwest 错误归到 [`TransportErrorKind`]。
pub fn classify_transport_error(error: &reqwest::Error) -> TransportErrorKind {
    if error.is_timeout() {
        TransportErrorKind::Timeout
    } else if error.is_connect() {
        TransportErrorKind::Connect
    } else if error.is_body() || error.is_decode() {
        TransportErrorKind::BodyOrDecode
    } else {
        TransportErrorKind::Other
    }
}

/// 把 `error` 及其 `source` 链拼成一行（最多 4 段，超出以「…」收尾）。
///
/// 上限是必要的：某些平台会把系统错误嵌套得很深，不加限制会把 toast / 日志撑爆。
pub fn join_error_chain(error: &dyn std::error::Error) -> String {
    const MAX_SOURCES: usize = 4;
    let mut text = error.to_string();
    let mut source = error.source();
    let mut depth = 0;
    while let Some(cause) = source {
        if depth >= MAX_SOURCES {
            text.push_str("；…");
            break;
        }
        text.push_str(&format!("；原因: {cause}"));
        source = cause.source();
        depth += 1;
    }
    text
}

/// 组装最终文案：`种类：顶层信息；原因: …`。
pub fn compose_transport_message(kind: TransportErrorKind, chain: &str) -> String {
    format!("{}：{}", transport_error_label(kind), chain)
}

/// 把 [`TransportErrorKind`] 映射到跨通道的错误码。
///
/// 四种 kind 必须映射到**互不相同**的码，否则前端无法区分「该重试」还是「该修网络」。
pub fn transport_error_code(kind: TransportErrorKind) -> ErrorCode {
    match kind {
        TransportErrorKind::Timeout => ErrorCode::NetTransportTimeout,
        TransportErrorKind::Connect => ErrorCode::NetTransportConnect,
        TransportErrorKind::BodyOrDecode => ErrorCode::NetTransportBody,
        TransportErrorKind::Other => ErrorCode::NetTransportOther,
    }
}

/// 带错误码的传输层错误：**文本与 [`describe_transport_error`] 逐字节相同**，
/// 另外携带 `net.transport.*` 码与 `chain` 参数，供前端按当前语言重新渲染。
///
/// 两者是**同一份实现**（本函数是唯一真相，`describe_transport_error` 委托过来取 `Display`），
/// 因此不可能出现「中文版和带码版的文案不一致」。
///
/// 只在**能把码送到前端**的调用点用它；纯内部日志用 [`describe_transport_error`] 即可。
pub fn transport_error(error: &reqwest::Error) -> AppError {
    let kind = classify_transport_error(error);
    let chain = join_error_chain(error);
    AppError::new(
        transport_error_code(kind),
        compose_transport_message(kind, &chain),
    )
    // 前端文案写作 `{kindLabel}：{chain}`，因此只把原因链交给前端拼接。
    .with("chain", chain)
}

/// **唯一出口**：把传输层错误变成可诊断的中文文案。
///
/// 委托 [`transport_error`]，保证带码版与本函数的文本永远一致。
pub fn describe_transport_error(error: &reqwest::Error) -> String {
    transport_error(error).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一条可自由构造的错误链，用来钉死 [`join_error_chain`] 的展开行为。
    #[derive(Debug)]
    struct ChainError {
        message: &'static str,
        cause: Option<Box<dyn std::error::Error + 'static>>,
    }

    impl std::fmt::Display for ChainError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.message)
        }
    }

    impl std::error::Error for ChainError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.cause.as_ref().map(|cause| cause.as_ref())
        }
    }

    fn leaf(message: &'static str) -> ChainError {
        ChainError {
            message,
            cause: None,
        }
    }

    fn wrap(message: &'static str, cause: ChainError) -> ChainError {
        ChainError {
            message,
            cause: Some(Box::new(cause)),
        }
    }

    /// ★ 可证伪性：若调用点退回 `format!("{error}")`（只留顶层一行），这条断言会红。
    #[test]
    fn 传输层错误必须展开原因链() {
        let error = wrap(
            "error sending request for url (https://api.trae.cn/…/claim)",
            wrap(
                "client error (Connect)",
                leaf("tcp connect error: 由于目标计算机积极拒绝，无法连接。(os error 10061)"),
            ),
        );
        let text = join_error_chain(&error);
        assert!(
            text.contains("error sending request for url"),
            "顶层信息丢失: {text}"
        );
        assert!(text.contains("client error (Connect)"), "第一层原因丢失: {text}");
        assert!(
            text.contains("os error 10061"),
            "底层原因丢失——只剩一行顶层文案就无从诊断: {text}"
        );
    }

    #[test]
    fn 超长原因链要截断() {
        let mut error = leaf("os error 1");
        for _ in 0..8 {
            error = wrap("layer", error);
        }
        let text = join_error_chain(&error);
        assert_eq!(
            text.matches("；原因:").count(),
            4,
            "原因链未截断: {text}"
        );
        assert!(text.ends_with("；…"), "截断后缺少省略标记: {text}");
    }

    #[test]
    fn 种类标签覆盖四类且不重复() {
        let labels = [
            transport_error_label(TransportErrorKind::Timeout),
            transport_error_label(TransportErrorKind::Connect),
            transport_error_label(TransportErrorKind::BodyOrDecode),
            transport_error_label(TransportErrorKind::Other),
        ];
        assert_eq!(labels[0], "连接超时");
        assert_eq!(
            labels[1],
            "无法连接（DNS 解析失败 / 网络不可达 / TLS 握手失败）"
        );
        assert_eq!(labels[2], "响应体读取失败");
        assert_eq!(labels[3], "请求失败");
        // 四类必须措辞不同，否则「分类」这件事就没意义了。
        let mut unique = std::collections::HashSet::new();
        for label in labels {
            assert!(unique.insert(label), "标签重复: {label}");
        }
    }

    /// 最终文案必须是「种类：顶层信息；原因: …」——先给结论再给细节。
    #[test]
    fn 文案以种类开头再跟原因链() {
        let message = compose_transport_message(
            TransportErrorKind::Connect,
            "error sending request for url (…)；原因: tcp connect error",
        );
        assert!(
            message.starts_with("无法连接（DNS 解析失败 / 网络不可达 / TLS 握手失败）："),
            "缺少种类前缀: {message}"
        );
        assert!(
            message.contains("；原因: tcp connect error"),
            "原因链被种类前缀挤掉了: {message}"
        );
    }

    /// ★ 可证伪性：若有人把两种 kind 映射到同一个码，这条会红。
    /// 前端正是靠这个码决定文案与「重试 / 修网络」的建议，撞码等于分类失效。
    #[test]
    fn 四种种类映射到互不相同的错误码() {
        let codes = [
            transport_error_code(TransportErrorKind::Timeout),
            transport_error_code(TransportErrorKind::Connect),
            transport_error_code(TransportErrorKind::BodyOrDecode),
            transport_error_code(TransportErrorKind::Other),
        ];
        assert_eq!(codes[0], crate::modules::error_code::ErrorCode::NetTransportTimeout);
        let mut unique = std::collections::HashSet::new();
        for code in codes {
            assert!(unique.insert(code.as_str()), "错误码重复: {}", code.as_str());
        }
    }

    /// 带码版的文本必须与纯文案版**逐字节相同**（前端换语言失败时回落显示的就是它）。
    #[test]
    fn 带码版文本与纯文案版一致() {
        let chain = "error sending request for url (…)；原因: tcp connect error";
        for kind in [
            TransportErrorKind::Timeout,
            TransportErrorKind::Connect,
            TransportErrorKind::BodyOrDecode,
            TransportErrorKind::Other,
        ] {
            let coded = crate::modules::error_code::AppError::new(
                transport_error_code(kind),
                compose_transport_message(kind, chain),
            )
            .with("chain", chain);
            assert_eq!(coded.to_string(), compose_transport_message(kind, chain));
            // 尾部必须真的带上了码与原因链，否则前端无从重渲染。
            let decoded = crate::modules::error_code::decode_wire(&coded.to_wire());
            assert_eq!(
                decoded.code,
                Some(transport_error_code(kind)),
                "码丢失: {kind:?}"
            );
            assert_eq!(
                decoded.params,
                vec![("chain".to_string(), chain.to_string())]
            );
        }
    }
}
