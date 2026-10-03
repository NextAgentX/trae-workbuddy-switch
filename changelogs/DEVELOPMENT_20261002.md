# 2026-10-02 开发记录

## 对话复用：会话三 ID 从"每请求随机"改为"按对话内容+账号稳定派生"

### 背景
网关此前对 `conversation_id` / `session_id` / `project_id` 每请求生成随机 UUID v4（`uuid_like()`），
上游无法做路由亲和与提示词缓存。参考本机 Python 转发器
`D:\AAAcodeyuanma\python\API\ZhuanFaFuWuQi.py` 的 `Han_ShengChengHuiHuaHao` / `Han_QuHuiHuaBiaoShi`
实现移植（Rust 版位于 `crates/buddy-switch-gateway/src/trae/payload.rs`）。

### 改动
- 新增 `hui_hua_biao_shi_yuan_wen()`：提取对话稳定标识原文，优先级
  `prompt_cache_key` > `metadata.session_id` > `user` > 内容指纹
  （system 提示 + 首条 user 消息拼串；OpenAI 协议多轮对话头部只增不改，指纹天然稳定）。
- 新增 `wen_ding_hui_hua_hao()`：`SHA256(盐 | 原文 | uid)` 前 16 字节手工置版本/变体位 → UUID v4 形态。
  盐前缀 `conv:` / `sess:` / `proj:` 保证三 ID 互不相同且各自稳定。
- `prepare_llm_chat_body()`：三个 ID 字段改为稳定派生；取不到指纹（无消息/空消息）才退回随机 UUID。
- **风控防呆（用户指正）**：`uid` 必须混入哈希 —— 两个账号发相同对话时若只按内容派生
  会得到相同 ID，跨账号同会话号是典型风控特征。

### 验证
- `cargo test -p buddy-switch-gateway`：lib 375 通过（含新增 4 个护栏测试：
  多轮对话同值、三 ID 互异、cache_key 优先、跨账号隔离、空消息随机兜底）+ e2e 全过。
- 重编译 `buddy-switch-server` 并重启，7864 在线，`/health` 正常。

### 注意
- 真实 IDE 对话请发一次请求，抓取文件（`~/.buddy-switch/trae/capture/`）可验证上游对新派生 ID 的接受度。
- 工作区改动未提交 Git（备份提交 72d9367 之后的功能改动），待确认后正式提交。
