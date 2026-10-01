# 变更记录 2026-10-01

## Trae 网关新增对话转发抓取（诊断旁路）

### 变更点

- 新增 `crates/buddy-switch-gateway/src/trae/zhuaqu.rs`：转发抓取模块。
  每次出站尝试落盘一组文件到 `~/.buddy-switch/trae/capture/`：
  - `<编号>_client.json`：客户端发来的原始请求体（OpenAI/Anthropic 原样）
  - `<编号>_upstream.json`：改写后发往上游的 `llm_utils_chat` 请求体
  - `<编号>_upstream_resp.ndjson`：上游返回的原始 SSE 流（逐块追加）
  - `<编号>_upstream_http_error.txt`：上游非 2xx 的错误响应体（如有）
- `routes.rs`：`attempt_once` 出站前抓取请求对并生成编号；
  `AttemptResult::Ok` 携带编号传给响应消费方；`send_llm_chat` 非 2xx
  时落盘错误响应体。
- `sse.rs`：`stream_convert` / `aggregate` 增加抓取编号参数，
  上游原始流逐块追加落盘；空编号表示不抓取（单元测试传 `""`）。
- 落盘失败一律静默：抓取是诊断旁路，绝不影响转发主链路。

### 影响范围

- 仅 Trae 网关（7864 端口）的对话转发链路；WorkBuddy 网关不受影响。
- 转发内容零改动：抓取只读不写，请求/响应字节流原样。
- 新增配置项：无。抓取常开，文件按请求累积，排查后可整目录删除。

### 遗留事项

- 本机未安装 Rust 工具链，未执行 `cargo check` / `cargo test`；
  下次构建时请留意编译诊断（改动点已静态复查）。
