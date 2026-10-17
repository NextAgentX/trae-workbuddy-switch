<p align="center">
  <img src="public/icon-transparent.png" alt="Buddy Switch 图标" width="120" />
</p>

<h1 align="center">Buddy Switch</h1>

<p align="center">
  <b>WorkBuddy · CodeBuddy · TraeWork · Qoder 多账号工作台</b><br />
  把「现在登的是哪个号」变成一次点击
</p>

<p align="center">
  <img alt="平台" src="https://img.shields.io/badge/%E5%B9%B3%E5%8F%B0-macOS%20%7C%20Windows%20%7C%20Linux-2ea043" />
  <a href="https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest"><img alt="最新版本" src="https://img.shields.io/github/v/release/NextAgentX/trae-workbuddy-switch?label=%E6%9C%80%E6%96%B0%E7%89%88%E6%9C%AC" /></a>
  <a href="./LICENSE"><img alt="许可" src="https://img.shields.io/badge/%E8%AE%B8%E5%8F%AF-PolyForm%20Noncommercial-4c8bf5" /></a>
  <a href="https://github.com/NextAgentX/trae-workbuddy-switch/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/NextAgentX/trae-workbuddy-switch?style=social" /></a>
</p>

<p align="center">
  <strong>简体中文</strong> ·
  <a href="README.en.md">English</a> ·
  <a href="README.ja.md">日本語</a> ·
  <a href="README.ko.md">한국어</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.fr.md">Français</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.pt.md">Português</a>
</p>

一个**本地运行**的多账号工作台：把 **WorkBuddy / CodeBuddy / TraeWork / Qoder** 四条产品线、**6 个作用域**的账号收在一处——扫码登录、一键切号、签到与积分、用量统计、会话副本同步，并把模型额度以 OpenAI / Anthropic 兼容接口开放给本机其它 AI 工具。所有数据只放在你自己的机器上。

|  |  |
| --- | --- |
| 🪪 **6 个作用域一屏管完** | 三条产品线 × 国内 / 国际，各有独立账号库；切号带实时进度，写失败自动回滚 |
| ✅ **该做的事自己冒出来** | 概览把「要重新登录 / 今日未签到 / 积分将过期 / 客户端没开」列成待办，点一下处理完 |
| 📊 **用量与限流看得见** | Token、积分、账号 × 模型限流、会话副本谁落后谁分叉，全在一屏 |
| 🔌 **额度还能喂给别的工具** | 一键暴露成 OpenAI / Anthropic 兼容接口，直接配进 Cursor、Claude Code、OpenWebUI |

<p align="center">
  <a href="https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest"><img alt="下载安装包" src="https://img.shields.io/badge/%E2%AC%87%20%E4%B8%8B%E8%BD%BD%E5%AE%89%E8%A3%85%E5%8C%85-Releases-2ea043?style=for-the-badge" /></a>
  <a href="https://github.com/NextAgentX/trae-workbuddy-switch/stargazers"><img alt="点个 Star" src="https://img.shields.io/badge/%E2%AD%90%20%E7%82%B9%E4%B8%AA%20Star-%E8%AE%A9%E4%BD%9C%E8%80%85%E7%9F%A5%E9%81%93%E6%9C%89%E4%BA%BA%E7%94%A8-f5c542?style=for-the-badge" /></a>
  <a href="CONTRIBUTING.md"><img alt="参与贡献" src="https://img.shields.io/badge/%F0%9F%9B%A0%20%E5%8F%82%E4%B8%8E%E8%B4%A1%E7%8C%AE-PR%20%E6%AC%A2%E8%BF%8E-4c8bf5?style=for-the-badge" /></a>
</p>

> ⚠️ **非官方第三方工具**（source-available，PolyForm Noncommercial 许可）。与上述产品及其权利人不存在隶属、授权或背书关系。它会读写本机客户端的认证数据、按你的配置自动发起请求，并可通过本地端口转发额度。使用前请读完 **[免责声明](docs/policies/DISCLAIMER.md)**。

## 界面预览


<table>
  <thead>
    <tr><th>概览（浅色）</th><th>概览（深色）</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/overview-light.png" alt="概览页（浅色）" /></td>
      <td><img src="docs/images/overview-dark.png" alt="概览页（深色）" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>WorkBuddy 账号（浅色）</th><th>WorkBuddy 账号（深色）</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/workbuddy-accounts-light.png" alt="WorkBuddy 账号页（浅色）" /></td>
      <td><img src="docs/images/workbuddy-accounts-dark.png" alt="WorkBuddy 账号页（深色）" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>用量 · 趋势（浅色）</th><th>用量 · 趋势（深色）</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/usage-trend-light.png" alt="用量趋势（浅色）" /></td>
      <td><img src="docs/images/usage-trend-dark.png" alt="用量趋势（深色）" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>网关（浅色）</th><th>网关（深色）</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/api-service-light.png" alt="网关页（浅色）" /></td>
      <td><img src="docs/images/api-service-dark.png" alt="网关页（深色）" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Qoder 网关（浅色）</th><th>Qoder 网关（深色）</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/qoder-gateway-light.png" alt="Qoder 网关（浅色）" /></td>
      <td><img src="docs/images/qoder-gateway-dark.png" alt="Qoder 网关（深色）" /></td>
    </tr>
  </tbody>
</table>

<details>
<summary><b>更多截图（会话同步 / 设置 / TraeWork 账号 / Qoder 账号）</b></summary>

<table>
  <thead>
    <tr><th>会话副本（浅色）</th><th>会话副本（深色）</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/sessions-light.png" alt="会话页（浅色）" /></td>
      <td><img src="docs/images/sessions-dark.png" alt="会话页（深色）" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>设置（浅色）</th><th>设置（深色）</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/settings-light.png" alt="设置页（浅色）" /></td>
      <td><img src="docs/images/settings-dark.png" alt="设置页（深色）" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>TraeWork 账号（浅色）</th><th>TraeWork 账号（深色）</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/trae-accounts-light.png" alt="TraeWork 账号页（浅色）" /></td>
      <td><img src="docs/images/trae-accounts-dark.png" alt="TraeWork 账号页（深色）" /></td>
    </tr>
  </tbody>
</table>

<table>
  <thead>
    <tr><th>Qoder 账号（浅色）</th><th>Qoder 账号（深色）</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/qoder-accounts-light.png" alt="Qoder 账号页（浅色）" /></td>
      <td><img src="docs/images/qoder-accounts-dark.png" alt="Qoder 账号页（深色）" /></td>
    </tr>
  </tbody>
</table>

</details>

> 图由 `scripts/gen-readme-shots.mjs` 从演示构建（`VITE_DEMO_MODE=1`）自动生成，账号、积分与请求记录均为虚构数据；界面改版后重跑一次即可同步。

## 全景

```mermaid
flowchart TB
    subgraph SRC["3 条产品线 · 6 个作用域"]
        direction LR
        WB["WorkBuddy<br/>国内版 / 国际版"]
        TR["TraeWork<br/>国内版 / 国际版"]
        QD["Qoder<br/>国内版 / 国际版"]
    end
    subgraph CORE["本机核心 · ~/.buddy-switch/"]
        direction LR
        AC["账号库<br/>凭据 · 回收站"]
        AU["自动化<br/>签到 · 保活 · 轮换"]
        US["用量<br/>Token / 积分"]
        SE["会话副本<br/>内容指纹 · 加法同步"]
        GW["额度网关<br/>OpenAI / Anthropic 兼容"]
    end
    WB --> AC
    TR --> AC
    QD --> AC
    AC --- AU --- US --- SE
    US --> GW
    GW --> OUT["本机其它 AI 工具<br/>Cursor · Claude Code · OpenWebUI"]
```

**三条硬纪律**，贯穿全部功能：数据只落本机、凭据只写客户端登录态（不代登录、不改上游协议）、任何写入前先备份。

## 三种形态

| 形态 | 获取方式 | 说明 |
| --- | --- | --- |
| **桌面 App** | [GitHub Releases](https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest) 下载安装包 | Tauri 打包，推荐日常使用 |
| **webui** | 从源码构建（见下） | 与桌面 App 同一份前端产物，走本地 HTTP 通道 |
| **在线演示** | [GitHub Pages](https://nextagentx.github.io/qoder-trae-workbuddy-switch/) | 只读演示，账号与用量均为虚构数据 |

> npm 包尚未发布，`npm i -g` 暂时装不到；webui 请按下面的源码构建。

## 快速开始

**桌面 App**：从 Releases 下载对应平台的安装包。

| 平台 | 安装包 | 安装 |
| --- | --- | --- |
| macOS Apple Silicon（arm64） | `BuddySwitch_<版本>_aarch64.dmg` | 打开 DMG，把 `BuddySwitch.app` 拖进「应用程序」 |
| macOS Intel（x86_64） | `BuddySwitch_<版本>_x86_64.dmg` | 同上 |
| Windows x64 | `BuddySwitch_<版本>_x64-setup.exe` | 运行安装程序 |
| Linux x64 | `BuddySwitch_<版本>_amd64.deb` / `.AppImage` | 装 `.deb`，或给 AppImage 加执行权限后直接运行 |

**webui（源码构建）**：前端产物在**编译期**被服务端嵌入（`rust-embed`），所以顺序不能反。

```bash
npm install
npm run build                                    # 1) 前端 → dist/
cargo build --release -p buddy-switch-server     # 2) 编译服务端（把 dist/ 嵌进去）
./target/release/buddy-switch                    # 3) 起服务并打开浏览器（默认 127.0.0.1:57890）
```

```bash
buddy-switch              # 起服务 + 打开浏览器
buddy-switch serve        # 只起服务（--port 指定端口）
buddy-switch status       # 终端查看当前账号
buddy-switch version      # 版本号
```

<details>
<summary><b>macOS 提示「无法验证开发者」／切换时提示无权限</b></summary>

- 无法验证开发者：Finder 里按住 Control 点击应用 → 「打开」，或到「系统设置 → 隐私与安全性」→「仍要打开」。
- 切换账号需要写入客户端认证文件：在「系统设置 → 隐私与安全性」的 **App 管理**（或 **完全磁盘访问**）中授权 BuddySwitch，然后重启应用。设置页的「权限检测」可随时验证。
- webui 形态的权限跟随启动服务的终端进程。

</details>

## 6 个作用域（平台 × 区域）

顶栏左上角的作用域选择器决定「现在管哪一套账号」。作用域写进 URL（`/s/wb-cn/accounts`），刷新、分享、前进后退都留在原地；`⌘K` / `Ctrl+K` 跨作用域搜索，`⌘1`–`⌘6` 直接切换。

| 作用域 | 平台 | 区域 | 管理的客户端 |
| --- | --- | --- | --- |
| `wb-cn` | WorkBuddy | 国内版 | WorkBuddy 桌面端、CodeBuddy CN IDE / CLI、VS Code / JetBrains 的 CodeBuddy 插件 |
| `wb-global` | WorkBuddy | 国际版 | 同上（国际版客户端） |
| `trae-cn` | TraeWork | 国内版 | Trae CN |
| `trae-global` | TraeWork | 国际版 | Trae |
| `qoder-cn` | Qoder | 国内版 | Qoder CN 桌面版 / CLI |
| `qoder-global` | Qoder | 国际版 | Qoder 桌面版 / CLI |

六个作用域**彼此独立**：独立账号库、独立客户端、独立配置、独立网关 Key。Trae 的两条老程序线（TraeWork / TraeCode）已被官方合并成同一个客户端，旧标识仅作兼容。

## 侧栏 6 个目的地

三个平台**逐条同构**：**概览 · 账号 · 用量 · 会话 · 网关 · 设置**。入口按**能力门控**（不是平台分支）：缺哪个能力就不画哪个入口，URL 直达时给结构化空态。

| 目的地 | 一句话 |
| --- | --- |
| **概览** | 待处理清单（需重新登录 / 今日未签到 / 积分将过期 / 客户端未运行）+ 统计卡 + 近 14 天 Token 柱状图 + 其他作用域速览 |
| **账号** | 卡片 / 表格双视图、程序位切换、扫码或设备码登录、本机导入、签到、回收站（30 天）、客户端限流台账（WorkBuddy 专有） |
| **用量** | 一个工作台四个视图：趋势 · 模型 · 会话 · 积分；参数条只有一条（范围 + 数据来源） |
| **会话** | 同一份会话在各账号的副本：谁落后、谁分叉、同步给谁（**仅 WorkBuddy**） |
| **网关** | 独立网关（每个作用域一套）与统一网关（跨作用域一个 provider） |
| **设置** | 九个分区：通用 · 账号与登录 · 签到与积分 · 网关 · 计划任务 · 客户端 · 权限与诊断 · 日志 · 备份与恢复 |

### 能力矩阵

| 能力 | WorkBuddy | TraeWork | Qoder |
| --- | :---: | :---: | :---: |
| 账号库 / 登录 / 本机导入 | ✅ 扫码 + 导入 | ✅ 网页登录 + JWT | ✅ 设备码 + PAT |
| 一键切程序位 | ✅ 5 个（4 可切换 + 1 只读探测） | ✅ 1 个（合并客户端） | ✅ 2 个（桌面版 / CLI） |
| 签到 / 积分 | ✅ 含自动旅行、自动轮换 | ✅ | ✅ 每日签到 + 套餐 / 额度 |
| 用量统计 | ✅ Token + 积分 | ✅ Token + 积分 | ⏳ 数据源待接入 |
| 会话副本同步 | ✅ 内容指纹 + 加法同步 | — | — |
| 模型额度网关 | ✅ `:57891` | ✅ `:57894` | ✅ `:57895` |

## 切换账号是怎么做的

```mermaid
flowchart LR
    A["选中账号"] --> B["备份当前登录态"] --> C["关闭客户端"] --> D["写入目标账号"] --> E["校验并重启"]
    D -->|"可选"| F["附带：会话复制<br/>记忆与连接器合并"]
```

- **先备份后写入**：认证文件、Trae 登录态快照、会话覆盖前，都会先留一份可回滚的副本。
- **写后校验**：写入失败自动还原；客户端运行中直接拒写，避免退出时把内存里的旧登录态写回覆盖。
- **删除是软的**：账号先进回收站保留 30 天，可随时恢复；永久删除才连带清凭据与快照。

## 模型额度网关

把当前账号的额度以**兼容接口**暴露给本机工具：OpenAI 兼容（`/v1/models`、`/v1/chat/completions`、`/v1/responses`）+ Anthropic 兼容（`/v1/messages`），都支持 SSE 流式。默认只监听 `127.0.0.1`。

```mermaid
flowchart LR
    C["本机 AI 工具"] -->|"sk-wb-…"| G1[":57891<br/>WorkBuddy 网关"]
    C -->|"sk-trae-…"| G2[":57894<br/>Trae 网关"]
    C -->|"sk-qoder-…"| G3[":57895<br/>Qoder 网关"]
    C -->|"sk-uni-…"| G4[":57892<br/>统一网关"]
    G1 --> P1["wb-cn / wb-global 账号池"]
    G2 --> P2["trae-cn / trae-global 账号池"]
    G3 --> P3["qoder-cn / qoder-global 账号池"]
    G4 -->|"模型名带命名空间前缀"| P1
    G4 --> P2
    G4 --> P3
```

- **四条监听彼此独立**：端口按 **`5789x` 一段**排开——`57890` 界面 · `57891` WorkBuddy · `57892` 统一 · `57893` 会话 hook · `57894` Trae · `57895` Qoder；关掉统一网关不影响任何既有客户端。路由 `/v1/*` 逐字重叠，所以**不能**合成一个 Router。
- **Key 前缀同族**：`sk-wb-` · `sk-trae-` · **`sk-qoder-`** · `sk-uni-`，看一眼前缀就知道该配哪个端口。
- **Key 三态**：吊销 · 恢复 · 删除（仅已吊销可删）。明文只在创建时返回一次，库里只存 sha256 + 前 4 / 后 4 位。
- **账号池**：按余额与限流状态挑账号，可排除不可用账号、可做「账号 × 模型」限流探测，恢复后一键复活。

**统一网关**解决「同名模型在不同平台互相覆盖」：模型名写成 `<命名空间>:<模型id>`，命名空间由作用域注册表派生（当前 6 条：`wb-cn` / `wb-global` / `trae-cn` / `trae-global` / `qoder-cn` / `qoder-global`）。

| 规则 | 行为 |
| --- | --- |
| 裸模型名（不带前缀） | **一律 400**，并列出可用前缀（不猜默认值，避免静默走错账号池） |
| 命名空间解析 | **只做整段精确匹配**（`trae` / `traework` / `traecode` 互为前缀） |
| 一把 Key 跨平台 | 同一个 `sk-uni-` Key 可同时授权多个平台 |
| `trae-*` 的 `/v1/responses` | 返回 **501**，不静默降级到 chat |
| 默认状态 | **默认关**，需在网关页「统一网关」分区手动启用 |

> 架构与决策：[UNIFIED-GATEWAY-ARCH.md](docs/specs/UNIFIED-GATEWAY-ARCH.md) · 接入片段：[guides/GATEWAY-CLIENT-INTEGRATION.md](docs/guides/GATEWAY-CLIENT-INTEGRATION.md)

## ❓ 常见问题 / Troubleshooting

### 网关返回 `400 {"code":11128,"msg":"request illegal"}`（"请求被拦截 / 非官方客户端"）

**现象**：本机 AI 工具（Cursor / Claude Code / OpenWebUI / DeepSeek Harness 等）经网关（如 `:57891` WorkBuddy）调用**国际版**（`wb-global`，如 lilyco-42 账号）时，请求返回：

```json
{"code":11128,"msg":"request illegal","displayTips":{"zh":"这次调用没通过渠道校验，可能不是官方客户端发出的..."}}
```

**根因**：WorkBuddy 服务端对出站请求做**渠道校验**，要求携带会话头三件套 —— `X-Conversation-ID`、`X-Conversation-Request-ID`、`X-Conversation-Message-ID`；缺任一头即判「非官方客户端」（11128）。旧版网关转发时未恒发这三件套，因此被拦截。官方客户端自带完整头，不受影响；此问题**与账号额度、登录态无关**。

**修复**：升级到 **v2026.10.171833 或更新版本**（已含 commit `092c5e3`：chat 出站恒发会话头三件套，修国际版 11128 渠道校验拦截）。升级后重启网关，重发请求即恢复 200。

**验证**：`POST http://127.0.0.1:57891/v1/messages`（Anthropic 兼容）携带 `Authorization: Bearer sk-wb-…`、`model: deepseek-v4.1-flash`，返回 200 与正常文本即为修复完成。

**关联参考**：DeepSeek Harness 集成示例见 [GATEWAY-CLIENT-INTEGRATION.md](docs/guides/GATEWAY-CLIENT-INTEGRATION.md)。

## 自动化

计划任务支持**分钟粒度**时点（如 `09:37`）、多个时点，以及**随机抖动**（±5/15/30 分钟）——让每天的实际触发时刻错开，而不是整点齐发。

| 侧 | 任务 |
| --- | --- |
| WorkBuddy（6 类） | 签到 · 猫猫旅行 · 活跃地图 · Token 保活 · 开学季 · 夜猫子 |
| Trae（2 类） | 签到 · JWT 自动续期 |

<details>
<summary><b>自动轮换策略（WorkBuddy，防积分过期）</b></summary>

后台定时把 **CodeBuddy CLI 后续启动**的默认账号设为「最紧迫」的账号（最早到期且仍有剩余积分）。依次判定：

1. 有效账号（查询成功、未过期、有剩余积分）
2. 紧迫度检查（最紧迫的剩余超过 `min_urgency_hours`，默认 72h → 不切）
3. 已是目标 → 不切
4. 冷却期（`cooldown_minutes`，默认 120）
5. 活跃保护（最近 `active_guard_minutes`，默认 30 内有对话 → 不切）
6. 价值过滤（剩余积分低于 `min_remaining_credits` → 不值得切）
7. 防抖动（差异小于 `min_gap_hours`，默认 24 → 不切）

**生效边界**：只更新后续会话使用的默认账号，不热切换当前会话——需 ACP 重新加载会话或重启 CLI。

</details>

<details>
<summary><b>应用级能力（六个作用域共用，不随作用域切换）</b></summary>

| 能力 | 说明 |
| --- | --- |
| 应用设置（顶栏 ⚙） | 主题（浅 / 深 / 跟随系统）、界面语言（简体中文 / English）、开机静默启动、关闭窗口进托盘、自动更新 |
| 全局备份与恢复 | 一次覆盖全部作用域，按账号 / 配置 / 网关分区勾选，**默认不含凭据** |
| Skill 管理 | 扫描 `skills/*/SKILL.md`，逐项启停（行级重写、其余字节原样保留），支持跨区域搬迁与 zip 导入 |
| 客户端设置托管 | 统一读写上游客户端自己的 `settings.json`：6 槽位规格表 + 格式指纹 + 字节级最小补丁 + 备份 / 恢复 / 差异对比 |
| 会话悬浮窗（仅桌面端） | 独立小窗展示跨来源的任务状态，只读、可点回原会话 |
| 客户端会话监听（默认关） | 在 WorkBuddy `settings.json` 装 hook，把任务状态实时送进悬浮窗（接收端 `127.0.0.1:57893`） |

</details>

## 数据与隐私

- 全部数据落在本机 `~/.buddy-switch/`，不上传任何第三方服务；界面偏好只存本地。
- 账号库按作用域分家（如 `accounts.json` / `accounts.global.json`、`trae/checkin_accounts*.json`）。
- 导出备份**默认不含 token**；显式勾选并二次确认后才会写明文，页面同步警告该文件可直接用于登录。
- 网关 Key 只存哈希与前缀；**仓库不提交本地数据**，发布前会扫描 token 模式。

## 开发

```mermaid
flowchart TB
    FE["src/ · React 19 + Vite<br/>6 目的地 · api.ts 双通道"] --> H["宿主：Tauri commands ｜ HTTP (axum)"]
    H --> CORE["crates/buddy-switch-core<br/>账号 · 认证 · 签到 · 会话 · 作用域"]
    H --> GW["crates/buddy-switch-gateway<br/>WB / Trae / Qoder / 统一"]
    CORE --> DATA[("~/.buddy-switch/")]
    GW --> DATA
```

```bash
npm install
npm run tauri dev     # 桌面端开发
npm run build         # 前端构建 + 契约 / 作用域 / 平台门禁
npm run check:api     # 只校验前端 API 契约
```

```
crates/
  buddy-switch-core/     # 账号、认证、OAuth、进程、切换、会话、签到、区域、Trae / Qoder 模块
  buddy-switch-gateway/  # OpenAI / Anthropic 兼容网关（四套：WB / Trae / Qoder / 统一）
  buddy-switch-server/   # HTTP server + CLI（axum API + rust-embed 内嵌前端）
src-tauri/               # 桌面宿主（Tauri command 薄包装 + 托盘 + 网关宿主）
src/                     # 前端：components / pages / lib / locales
npm/                     # npm 包（尚未发布）
```

环境要求、发布流程、目录约定见 [docs/guides/DEVELOPMENT.md](docs/guides/DEVELOPMENT.md)。
**全部文档的索引在 [docs/README.md](docs/README.md)** —— 按生命周期分区：指南 / 现行规格 / 只读历史 / 原型。

## 参与贡献

**这个项目的形态就是被贡献推着长出来的**：Qoder 平台、统一网关、随机抖动、多语言 README，都来自 Issue 与 PR。不需要"很懂"才能参与：

| 你可以做的事 | 起点 |
| --- | --- |
| 🐞 **报 Bug**（附版本 + 日志截图） | [新建 Issue](https://github.com/NextAgentX/trae-workbuddy-switch/issues/new) |
| 🧩 **接新平台**（Galaxy / 其它 IDE） | 平台差异只在 `src/lib/platforms/registry.ts` 与 core 的 `product.rs` 各声明一次，页面组件不用改 |
| 🌍 **修翻译 / 补语言** | 界面文案在 `src/locales/domains/*.zh.ts` ↔ `*.en.ts` 一一对应；README 八语种也欢迎校订 |
| 📸 **更新截图** | `scripts/gen-readme-shots.mjs` 一条命令重拍全部浅/深色图（见文件头注释） |
| 🧪 **补测试** | 仓里大量护栏测试都是贡献者补的；先读 `docs/guides/DEVELOPMENT.md` 的门禁清单 |

完整指引（能做什么、PR 自检清单、代码约定）见 **[CONTRIBUTING.md](CONTRIBUTING.md)**（[English](CONTRIBUTING.en.md)）。

## 支持这个项目

- ⭐ **点个 Star** —— 这是最直接的帮助：它让更多人搜到这个工具，也让作者知道值得继续维护。
- 🐛 **把问题说清楚** —— 一个能复现的 Issue 比十句"不好用"有用得多，通常下一版就修了。
- ☕ **请作者喝杯饮料**（完全自愿，不影响任何功能）

<table>
  <thead>
    <tr><th>微信支付</th><th>支付宝</th></tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/donate-wechat.png" alt="微信支付收款码" width="240" /></td>
      <td><img src="docs/images/donate-alipay.jpg" alt="支付宝收款码" width="240" /></td>
    </tr>
  </tbody>
</table>

## 致谢

- [changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch) — 借鉴 WorkBuddy 账号数据格式与多账号切换思路。
- [Sliverkiss/workbuddy2api](https://github.com/Sliverkiss/workbuddy2api) — 借鉴把模型额度封装成 OpenAI 兼容接口的思路。

以上项目各自遵循其自身许可；本项目为独立实现，与它们不存在隶属、合作或背书关系，相关名称与商标归各自权利人所有。

## 免责声明

完整条款见 **[docs/policies/DISCLAIMER.md](docs/policies/DISCLAIMER.md)**（[English](docs/policies/DISCLAIMER.en.md)），共 6 条：非官方第三方工具、服务条款与合规自理、数据写入与备份、自动化行为风险、API 网关暴露风险、按「原样」提供。使用即表示已阅读并同意；如不同意，请立即停止使用并卸载。

## 许可

**[PolyForm Noncommercial License 1.0.0](./LICENSE)**：个人非商业使用允许，**商业使用不被授权**。

- 条款全文：[LICENSE](./LICENSE)
- 白话说明与常见问题：[docs/policies/LICENSING.md](docs/policies/LICENSING.md)（[English](docs/policies/LICENSING.en.md)）
- 商业授权请通过本仓库 Issues 联系作者。

## Star 趋势

<a href="https://www.star-history.com/?repos=NextAgentX%2Ftrae-workbuddy-switch&type=date&legend=top-left">

 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=NextAgentX/trae-workbuddy-switch&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=NextAgentX/trae-workbuddy-switch&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=NextAgentX/trae-workbuddy-switch&type=date&legend=top-left" />
 </picture>
</a>
