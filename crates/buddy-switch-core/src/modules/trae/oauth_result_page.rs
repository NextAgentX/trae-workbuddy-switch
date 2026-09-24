//! OAuth 回调结果页：授权完成后**浏览器里那一页**。
//!
//! ## 为什么要有它（而不是像原先那样回一句纯文本）
//!
//! 回调监听（[`super::oauth`]）原先一律以 `Content-Type: text/plain` 回一句中文，
//! 浏览器把 `text/plain` 当**纯文本**渲染：左上角一行小字、没有居中、没有视觉层级，
//! 用户看不出「这是登录流程的一部分」还是「页面出错了」。
//!
//! 本模块把它换成一张居中的结果卡片（成功 ✓ 绿色 / 失败 ✕ 红色 / 取消 — 灰色），
//! 与上游项目 `TraeWorkAssistant` 的
//! `src-tauri/src/commands/oauth_loopback.rs::html_response` **同构**：
//! 同样的 `.card` 布局、同样的圆底图标 + 标题 + 两段正文。
//!
//! ## 只有「用户看得见的页」用 HTML，探测响应**必须**仍是纯文本
//!
//! 授权页在「认证中」阶段会跨源探 `127.0.0.1:17388`，它只关心状态码；
//! 那一类响应继续走 `text/plain`（理由见 [`super::oauth`] 的 `respond` 文档）。
//! 本模块**只负责渲染回调结果**，不参与探测应答。
//!
//! ## 安全：两件事不可省
//!
//! 1. **每个插值都转义**。账号昵称来自上游 `userInfo`、失败原因是上游报错原文，
//!    两者都是外部输入；回调里的 `error=` 参数更是**浏览器可控**。
//!    不转义就是一个自造的 XSS（参考实现把它记为 P0 缺陷 6）。
//! 2. **不回显回调 URL**。它的查询串里带 `authCodeInfo` / `refreshToken`，
//!    而本链路的既有不变式是「回调响应体不含凭据」（见 `oauth.rs` 模块头）。
//!    参考实现在失败页里回显了 URL，**我们刻意不跟**：
//!    那一页的价值是「告诉用户发生了什么」，不需要把凭据再写一遍到页面上
//!    （用户要看地址栏里本来就有）。

/// 结果页语义：决定图标与主色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    /// 成功：绿底 ✓
    Success,
    /// 失败：红底 ✕
    Failure,
    /// 取消 / 主动放弃：灰底 —
    Cancelled,
}

impl PageKind {
    /// `(图标字符, 主色)`。
    ///
    /// 成功/失败的取值与参考实现逐字一致（`#16a34a` / `#dc2626`），
    /// 取消是参考实现没有的第三态，取中性灰。
    fn icon_and_tone(self) -> (&'static str, &'static str) {
        match self {
            PageKind::Success => ("✓", "#16a34a"),
            PageKind::Failure => ("✕", "#dc2626"),
            PageKind::Cancelled => ("—", "#6b7280"),
        }
    }
}

/// HTML 实体转义：`&`、`<`、`>`、`"`、`'`（文本与属性上下文都覆盖）。
///
/// `&` **必须第一个替换**，否则会把后面替换出来的实体再转义一遍（`<` → `&amp;lt;`）。
pub fn escape_html(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// 结果页模板。占位符用 `%NAME%` 形式（CSS 里的 `{}` 因此不需要转义，
/// 比把整段 CSS 塞进 `format!` 再逐个大括号写成 `{{}}` 可读得多）。
const PAGE_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Trae 账号登录</title>
<style>
  :root {
    color-scheme: light dark;
    --bg: #f6f7f9;
    --card: #ffffff;
    --card-shadow: 0 4px 16px rgba(0, 0, 0, .08);
    --title: #111827;
    --text: #555555;
  }
  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #101114;
      --card: #1b1d21;
      --card-shadow: 0 4px 16px rgba(0, 0, 0, .45);
      --title: #f3f4f6;
      --text: #a1a6b0;
    }
  }
  body {
    font-family: system-ui, -apple-system, 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB',
                 'Microsoft YaHei', sans-serif;
    display: flex; align-items: center; justify-content: center;
    min-height: 100vh; margin: 0; background: var(--bg); color: var(--text);
  }
  .card {
    background: var(--card); border-radius: 12px; padding: 40px 48px;
    max-width: 560px; box-shadow: var(--card-shadow); text-align: center;
  }
  .icon {
    width: 56px; height: 56px; border-radius: 50%; color: #fff;
    font-size: 28px; line-height: 56px; margin: 0 auto 16px;
    background: %TONE%;
  }
  h1 { font-size: 20px; margin: 0 0 12px; color: var(--title); }
  p { color: var(--text); line-height: 1.7; margin: 6px 0; }
  @media (max-width: 600px) { .card { padding: 28px 20px; margin: 16px; } }
</style></head>
<body><div class="card">
  <div class="icon">%ICON%</div>
  <h1>%TITLE%</h1>
  %BODY%
</div></body></html>
"#;

/// 渲染一张结果页。
///
/// - `kind` 决定图标与主色；
/// - `title` 是卡片标题（如「登录成功」）；
/// - `paragraphs` 是正文段落，逐段包 `<p>`，**每段都会被 HTML 转义**。
///
/// 传进来的内容一律当**不可信文本**处理：调用方不需要（也不应该）自己转义。
pub fn result_page(kind: PageKind, title: &str, paragraphs: &[String]) -> String {
    let (icon, tone) = kind.icon_and_tone();
    let body = paragraphs
        .iter()
        .map(|line| format!("<p>{}</p>", escape_html(line)))
        .collect::<String>();

    // ⚠️ `%BODY%` 必须**最后**替换：正文来自外部输入，先插进去就会被后续的
    // `.replace()` 当成模板再处理一遍（顺序反了 = 外部输入能改写页面结构）。
    // 同理，`%TITLE%` 之后的替换只剩 `%BODY%`，而标题恒为调用点写死的字面量。
    PAGE_TEMPLATE
        .replace("%TONE%", tone)
        .replace("%ICON%", icon)
        .replace("%TITLE%", &escape_html(title))
        .replace("%BODY%", &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_text(html: &str) -> String {
        // 只做「断言用」的粗剥标签：模板本身固定，测试关心的是插值有没有漏转义。
        html.replace("<p>", "\n").replace("</p>", "\n")
    }

    #[test]
    fn success_page_carries_account_name_and_region() {
        // 夹具与 `oauth::success_page` 的实际产物保持一致（应用 → 产品 → 区域）。
        let html = result_page(
            PageKind::Success,
            "登录成功",
            &[
                "账号 [Jackey] 登录成功".to_string(),
                "账号已添加到 Buddy Switch 的 Trae（国内版）账号库，可关闭此页面返回应用。"
                    .to_string(),
            ],
        );

        assert!(html.starts_with("<!DOCTYPE html>"), "必须是完整 HTML 文档");
        assert!(html.contains("</html>"), "{html}");
        assert!(html.contains("登录成功"), "{html}");
        assert!(html.contains("账号 [Jackey] 登录成功"), "{html}");
        assert!(html.contains("国内版"), "结果页必须点明账号落在哪个区域库: {html}");
        // 成功页用绿底 ✓，与参考实现同色。
        assert!(html.contains("#16a34a"), "{html}");
        assert!(html.contains("✓"), "{html}");
    }

    /// ★ 插值来自上游/浏览器，**必须**转义，否则是自造 XSS。
    ///
    /// 用 `<script>` 当载荷：如果转义漏了，页面上会出现一个真实可执行的标签。
    #[test]
    fn interpolated_values_are_html_escaped() {
        let payload = r#"<script>alert('x')</script>&"#;
        let html = result_page(
            PageKind::Failure,
            payload,
            &[format!("账号 [{payload}] 登录失败")],
        );

        assert!(
            !html.contains("<script>"),
            "未转义的插值 = 自造 XSS（参考实现记为 P0 缺陷 6）: {html}"
        );
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("&amp;"), "& 必须先于其他实体被替换: {html}");
        assert!(!html.contains("&amp;lt;"), "& 替换顺序错了，实体被二次转义: {html}");
        // 引号也要转义（属性上下文）
        assert!(!html.contains("alert('x')"), "{html}");
    }

    /// 三种语义各自的主色与图标不能串（成功页显示红 ✕ 是最典型的串味症状）。
    #[test]
    fn page_kinds_have_distinct_icon_and_tone() {
        let ok = result_page(PageKind::Success, "登录成功", &[]);
        let fail = result_page(PageKind::Failure, "登录失败", &[]);
        let cancel = result_page(PageKind::Cancelled, "已取消", &[]);

        assert!(ok.contains("#16a34a") && ok.contains("✓"), "{ok}");
        assert!(fail.contains("#dc2626") && fail.contains("✕"), "{fail}");
        assert!(cancel.contains("#6b7280"), "{cancel}");

        assert_ne!(ok, fail);
        assert_ne!(fail, cancel);
    }

    /// 无正文段落时也要是结构完整的页面（不能漏出占位符）。
    ///
    /// ⚠️ 断言的是**具体占位符**而不是「页面里没有 `%`」：模板的 CSS 里本来就有
    /// `border-radius: 50%` / `100vh` 这类合法的百分号。
    #[test]
    fn empty_paragraph_list_leaves_no_placeholder() {
        let html = result_page(PageKind::Cancelled, "已取消", &[]);
        for placeholder in ["%TITLE%", "%BODY%", "%ICON%", "%TONE%"] {
            assert!(
                !html.contains(placeholder),
                "模板占位符 {placeholder} 没被替换: {html}"
            );
        }
    }

    /// ★ 结果页**不得**回显回调 URL 里的凭据（本链路的既有不变式）。
    #[test]
    fn page_never_contains_credential_markers() {
        // 调用方不会把凭据传进来；这里把「万一传进来」也钉住：转义不等于脱敏，
        // 所以断言的是「页面里不出现凭据形态的字面量」。
        let html = result_page(
            PageKind::Success,
            "登录成功",
            &["账号 [Jackey] 登录成功".to_string()],
        );
        for marker in ["authCodeInfo", "refreshToken", "AuthCode", "access_token"] {
            assert!(
                !html.contains(marker),
                "结果页出现了凭据标记 {marker}（本链路不变式：回调响应体不含凭据）: {html}"
            );
        }
    }

    #[test]
    fn paragraphs_are_split_into_separate_p_tags() {
        let html = result_page(
            PageKind::Success,
            "登录成功",
            &["第一段".to_string(), "第二段".to_string()],
        );
        let text = page_text(&html);
        assert!(text.contains("第一段") && text.contains("第二段"), "{html}");
        assert_eq!(html.matches("<p>").count(), 2, "{html}");
    }
}
