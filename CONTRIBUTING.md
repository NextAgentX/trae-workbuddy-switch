# 贡献指南

**欢迎一切形式的贡献，也欢迎 PR。** 提 Issue、补文档、修 Bug、加功能都算。这个项目由一个人维护，而它要对付的是几个会随时改版的客户端 —— 社区反馈的问题和补丁，往往就是它能跟上变化的原因。

**简体中文** · [English](CONTRIBUTING.en.md)

---

## 你可以做什么

| 类型 | 说明 |
| --- | --- |
| 报告问题 | 到 [Issues](https://github.com/NextAgentX/trae-workbuddy-switch/issues) 反馈 Bug 或提建议。请附**复现步骤**、**客户端版本**和日志；粘贴前先抹掉 token、账号等敏感信息 |
| 修 Bug | 客户端升级后数据结构或接口变了、某个功能失效 —— 这类修复价值最高 |
| 适配新客户端版本 | 跟进新版客户端的数据结构与端点变化 |
| 文档与翻译 | 修正 README / `docs/` 里过时的描述，改进中英文表述 |
| 界面与体验 | 前端交互、可访问性、深色模式细节（见下方[前端组件](#前端组件shadcn-优先)） |
| 新功能 | 建议先开 Issue 对齐方向，避免做完才发现思路不一致 |

## 提 PR 之前

1. 大改动**先开 Issue** 讨论方向；小修复（错别字、文档、明确的 Bug）可以直接提 PR
2. Fork → 新建分支（`fix/…`、`feat/…`）→ 提交 → 向 `main` 开 PR
3. 本地至少跑一遍：

```bash
npm install
npm run build     # tsc 类型检查 + API 契约校验（check:api）+ 前端构建
cargo test        # Rust 单元测试
```

`npm run build` 已经包含 `npm run check:api`；只改了 Rust 侧或路由表时，单独跑 `npm run check:api` 就够。

4. 改了界面就附截图（浅色 / 深色各一张更好）

### PR 自检清单

- [ ] `npm run build` 与 `cargo test` 都通过
- [ ] 动过 API 命令名或路由 ⇒ `npm run check:api` 通过
- [ ] diff 里没有真实凭据、本地数据文件或日志
- [ ] 新增的界面元素在深色模式下可读
- [ ] 修 Bug 时说明了复现步骤与根因，而不只是「改了哪一行」

## 本仓库的几条约定

### 提交信息

用 Conventional Commit 前缀（`feat:` / `fix:` / `docs:` / `refactor:` …），**标题与正文用中文**。

发布说明由 `scripts/gen-release-notes.mjs` 按前缀分组、且**只取每个提交的首行**生成 ⇒ 首行要能独立读懂「改了什么」。

### 前端组件：shadcn 优先

取舍顺序：复用 `src/components/ui/` 里已有的组件 → 组合现有 shadcn 组件 → 缺组件就补一个 shadcn/Radix 实现并放进 `src/components/ui/` → 只有在 shadcn 及其组合 API 确实满足不了时才自写，并在代码里写明理由（**只是视觉偏好不算理由**）。

自写组件同样要复用项目主题 token（`primary` / `ring` / `border` / `popover` / `muted` / `destructive` / `brand`）与既有的密度、圆角、可访问性约定；菜单 / 弹层 / 对话框 / 下拉 / 提示一律用组件，不要用 `details` / `summary` 之类的原生写法代替。

完整规则见 [`.trellis/spec/guides/ui-component-guidelines.md`](.trellis/spec/guides/ui-component-guidelines.md)。

### 加一个 API 命令要同步的地方

双通道（桌面端走 Tauri invoke，webui 走本地 HTTP）意味着同一个命令名散落在多处**纯字符串**里，而且没有编译期保护。新增或改名一个命令时，这几处必须一起改：

| # | 位置 | 漏了的症状 |
| --- | --- | --- |
| 1 | `src/lib/api.ts` 的 `ROUTES` 表 | webui 抛「暂不支持该操作」 |
| 2 | `src/lib/api.ts` 的 `call("<cmd>")` | 命令没人调用 / 被报成死路由 |
| 3 | `crates/buddy-switch-server/src/api.rs` 的 `.route(...)` | webui 404 |
| 4 | `src-tauri/src/lib.rs` 的 `generate_handler![…]` | 桌面端 command not found |
| 5 | `src/lib/screenshot-demo.ts`（**只读**命令） | 演示模式运行时抛「缺少只读数据」 |

`npm run check:api` 会逐条校验这些对齐关系，改完跑一遍即可。

> **「单 Value 参数」命令有个坑**：若命令签名里除 Tauri 注入参数外**只剩一个 `Value` 参数**，前端必须把它作为唯一的顶层键传过去（`call("x", { options: { … } })`），**不能平铺**。Tauri 按**参数名**取值，平铺会报 `missing required key <参数名>`；而 webui 侧后端对两种形状都兼容 ⇒ 症状是「浏览器里好用、桌面端点不动」。契约校验的第 7 条专门盯这个。

### 展示字段必须是「字符串或 null」

从后端返回给界面的展示字段（账号名、昵称之类）必须归一成字符串或 `null`：Rust 侧走 `account::display_str`，前端走 `src/lib/display-text.ts` 的 `displayText`。脏值（对象、数字、加密信封）会让 React 卸载整棵树 ⇒ **白屏**。

### 两处刻意的同款实现

`src-tauri/src/commands.rs` 与 `crates/buddy-switch-server/src/api.rs` 里有若干**刻意不去重**的同款实现（两个宿主各自一份）。改一处必须改另一处，保留两边互相指认的「两处必须保持一致」注释，并**两边都补测试**。

### 不保留死代码

清理代码时留意「最小可提交单元」：如果 A 用到了 B 里**新增的符号**，B 必须同批提交，否则中间那次提交编译不过。

## 构建与调试提示

- 前端产物是**编译期**嵌进服务端二进制的（`rust-embed`）⇒ 改了前端必须重新编译宿主，否则界面不会变
- 重编前先停掉正在运行的 `buddy-switch`，否则 Windows 上会因二进制被占用而链接失败
- Windows 上若遇到 `LNK1104` 或 `拒绝访问 (os error 5)`，通常是并发编译的瞬时冲突：重试或改用 `cargo build -j 1` 即可，**不是代码错误**
- 环境要求、打包与发布流程见 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)

## 几条硬性要求

- **不要提交本地数据**：账号库、认证文件、API Key、token、日志一律不进仓库。提交前自查一遍 diff，确认没有真实凭据
- **不要提交来源不明的代码**：引用其他项目的片段必须注明出处，并确认许可兼容
- **本项目采用 [PolyForm Noncommercial License 1.0.0](./LICENSE)**：提交贡献即表示你同意该贡献以同一许可分发，并确认你有权提交这些代码。说明见 [docs/LICENSING.md](docs/LICENSING.md)
- 请勿在 Issue / PR 中粘贴完整的 token 或账号凭据，即使是自己的

## 有问题在哪问

- 用法问题、Bug、功能建议 → [Issues](https://github.com/NextAgentX/trae-workbuddy-switch/issues)
- 安全相关（凭据泄露、越权访问等）→ 请**不要**公开开 Issue，先私下联系作者说明情况

---

如果这个项目帮到了你，点个 Star ⭐ 就是最好的鼓励 —— 当然，有 PR 更好。
