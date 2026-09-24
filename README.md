<p align="center">
  <img src="public/icon-transparent.png" alt="Buddy Switch 图标" width="128" />
</p>

<p align="center">
  <strong>Buddy Switch</strong><br />
  WorkBuddy / CodeBuddy / Trae 账号切换工具
</p>

<p align="center">
  <strong>简体中文</strong> · <a href="README.en.md">English</a>
</p>

# Buddy Switch

**WorkBuddy / TraeWork 多账号管理工具**：OAuth 扫码登录、一键切换登录态、积分到期监控与自动签到、Token 用量统计，并可以把模型额度以 OpenAI / Anthropic 兼容接口提供给本机其它工具。

> ⚠️ **免责声明**：本项目是**非官方的第三方源码公开工具**（source-available，非商业许可），与 WorkBuddy、CodeBuddy、Trae / TraeWork 及其各自权利人不存在隶属、授权或背书关系。它会读写本机第三方客户端的认证数据、按你的配置自动发起请求，并可将你的模型额度通过本地接口转发给其它工具。使用前请完整阅读[免责声明](docs/DISCLAIMER.md)，自行评估风险并确保使用方式符合相关服务条款。

同一套界面提供三种形态：

| 形态 | 获取方式 | 说明 |
| --- | --- | --- |
| **桌面 App** | 从 [GitHub Releases](https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest) 下载安装包 | Tauri 打包，推荐日常使用 |
| **webui（浏览器）** | 从源码构建，见 [webui 形态](#webui-形态源码构建) | 与桌面 App 同一份前端产物，走本地 HTTP 通道 |
| **在线演示** | [GitHub Pages](https://nextagentx.github.io/trae-workbuddy-switch/) | 只读演示；账号、积分与请求记录均为虚构数据，所有业务操作已禁用 |

> **暂未提供 npm 安装**：npm 包 `@nextagentx/buddy-switch` 尚未发布，`npm i -g …` 还装不到它。
> 需要 webui 形态请按下方[从源码构建](#webui-形态源码构建)；发布后会在这里补上安装命令。


## 两个产品分区

侧栏顶部切换产品分区。两个分区**彼此独立**：独立的账号库、独立的客户端、独立的配置，互不影响。

| 分区 | 管理的客户端 | 区域 |
| --- | --- | --- |
| **WorkBuddy** | WorkBuddy 桌面客户端、CodeBuddy CLI、CodeBuddy CN IDE | 国内版（WorkBuddy）/ 国际版（WorkBuddy AI） |
| **TraeWork** | Trae Work（客户端 `TRAE SOLO CN` / `TRAE SOLO`）、Trae IDE（`Trae CN` / `Trae`） | 国内版 / 国际版 |

两个分区下的页面结构**逐条同构**：账号管理、Token 统计、积分统计、API 服务、设置。

> **Trae 侧的区域与程序位**：Trae 有两条可同机并存的产品线（Trae Work 与 Trae IDE，客户端自述别名 `TraeWork CN` / `TraeCode CN`），它们在 Trae 分区**内部**切换，不占侧栏。变体由 URL 的 `?line=` 承载，因此刷新、分享链接、前进后退都能保持当前位置。

## 快速开始

### 桌面 App

前往 [GitHub Releases](https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest) 下载对应平台的安装包：

| 平台 | 安装包 | 安装方式 |
| --- | --- | --- |
| macOS Apple Silicon（M 系列，arm64） | `BuddySwitch_<版本>_aarch64.dmg` | 打开 DMG，将 `BuddySwitch.app` 拖入「应用程序」 |
| macOS Intel（x86_64） | `BuddySwitch_<版本>_x86_64.dmg` | 打开 DMG，将 `BuddySwitch.app` 拖入「应用程序」 |
| Windows x64 | `BuddySwitch_<版本>_x64-setup.exe` | 运行安装程序并按提示完成安装 |
| Linux x64 | `BuddySwitch_<版本>_amd64.deb` / `BuddySwitch_<版本>_amd64.AppImage` | Debian/Ubuntu 安装 `.deb`；其他发行版可给 AppImage 添加执行权限后直接运行 |

macOS 首次启动若提示无法验证开发者，先在 Finder 中按住 Control 点击应用并选择「打开」，或前往「系统设置 → 隐私与安全性」选择「仍要打开」。仅当安装包来自上述官方 Releases、且系统仍提示「已损坏」时，再执行：

```bash
xattr -rd com.apple.quarantine "/Applications/BuddySwitch.app"
```

应用能启动但切换账号时提示无权限，请参阅下方 [macOS 权限说明](#macos-权限说明)。

### webui 形态（源码构建）

本项目暂未通过 npm 分发，因此 webui 形态需要自己编译。**注意顺序**：前端产物是被服务端二进制在**编译期**嵌入的（`rust-embed`），所以必须先构建前端，再编译服务端：

```bash
npm install
npm run build                                    # 1) 构建前端 → dist/
cargo build --release -p buddy-switch-server     # 2) 编译服务端（把 dist/ 嵌进去）
./target/release/buddy-switch                    # 3) 启动本地服务 + 自动打开浏览器（Windows 为 buddy-switch.exe）
```

启动后可用的子命令：

```bash
buddy-switch              # 启动本地服务 + 自动打开浏览器
buddy-switch serve        # 只起服务，不开浏览器（--port 指定端口）
buddy-switch status       # 终端查看当前账号
buddy-switch version      # 版本号
```

webui 默认监听 `127.0.0.1:57890`。界面与桌面 App 完全一致——同一份前端产物，桌面端走 Tauri 命令通道，webui 走本地 HTTP 通道。

> 改了前端但没重新编译服务端时，界面不会变（嵌入的是旧产物）。重编前请先停掉正在运行的 `buddy-switch`，否则 Windows 上会因二进制被占用而链接失败。

## 功能

### WorkBuddy 分区

| 模块 | 说明 |
| --- | --- |
| 账号管理 | OAuth 扫码登录、从本机导入、从备份文件导入、手动添加 token、删除账号、导出账号备份 |
| 账号切换 | 备份认证文件 → 关闭 WorkBuddy → 写入目标账号 → 重启，切换过程实时进度反馈 |
| 会话复制 | 将当前账号勾选的会话以新 id 复制给目标账号（jsonl 正文 + `workbuddy.db` 索引 + edge-sync 注册） |
| 自动签到 | 默认开启；启动即检查，并按排程自动补签；支持一键全部签到与签到日志 |
| 自动旅行 | 自动派发 / 领取「猫猫旅行」任务 |
| Token 保活 | 惰性刷新（操作前不足阈值才刷新）+ 每日保活（默认每天无条件刷新一次），避免 refresh token 过期 |
| 积分到期查询 | 查询每个账号的积分资源、剩余量与到期时间；7 天内到期高亮并按到期优先排序，最紧迫的标记为「建议优先使用」 |
| 积分统计 | 汇总 WorkBuddy 官方请求用量，展示每日趋势、模型分布、账号消耗与请求明细；官方数据不可用时明确回退到本地余额快照观察 |
| Token 统计 | 分别查看 WorkBuddy、CodeBuddy CLI 与 CodeBuddy IDE 的 Token 总览；输入、输出、缓存读写按 K/M/B 展示，并提供构成占比、活跃热力图、项目/模型 Top 10 与会话排行 |
| CodeBuddy CLI | 与 WorkBuddy 复用同一账号库，但默认账号独立；macOS/Linux 通过 `apiKeyHelper`，Windows 通过 `settings.json` 的 `env.CODEBUDDY_AUTH_TOKEN` 设置后续会话使用的账号 |
| CodeBuddy CN IDE | 复用同一账号库，向 `CodeBuddy CN` 桌面客户端注入 Safe Storage 凭证并重启 IDE |
| 自动轮换 | 后台定时把 CodeBuddy CLI 的后续启动账号设为积分最紧迫（最早到期）的账号；当前会话保持原账号 |
| 定时任务排程 | 六类任务——签到、猫猫旅行、活跃地图、token 保活、开学季、夜猫子——各自可独立开关并配置执行小时 |
| 切换时迁移数据 | 把当前账号的长期记忆（Memory）与连接器配置合并到目标账号（同名条目递归合并、按内容去重），支持同版本与跨版本迁移；改写前自动备份原文 |
| 权限检测 | macOS 授权引导（App 管理 / 完全磁盘访问拖拽授权 + 自动检测） |
| 自动更新 | 检查 GitHub Releases 新版本；整包更新经签名校验（tauri-updater） |

### TraeWork 分区

| 模块 | 说明 |
| --- | --- |
| 账号管理 | OAuth 网页登录（浏览器回调）、手动粘贴 `Cloud-IDE-JWT`、从本机导入、从备份文件导入、删除账号、导出账号备份 |
| 分组 | 账号可分组，列表支持按分组筛选 |
| 签到 | 一键签到并刷新积分；支持「跳过今日已签到账号」与自动续期；账号卡片展示签到状态与冷却 |
| 积分 | 逐账号展示剩余积分、逐包明细与到期时间；近 7 天积分总数 / 获得 / 消耗趋势 |
| Token 统计 | 近 7 天 / 30 天 / 90 天 / 全部历史窗口的 Token 与调用次数统计 |
| 登录态切换 | 把选定账号写入 Trae 客户端的登录态（`profiles/`），切换前自动备份；重启客户端后生效 |
| 登录态快照 | 保存 / 恢复 / 删除客户端登录态快照，便于回滚 |
| JWT 刷新 | 手动刷新账号 JWT 并查看剩余有效期（临期高亮） |
| 设备标识 | 查看与重置设备标识 |
| 环境探测 | 自动探测本机安装的产品线、客户端版本、运行状态与 userData 目录 |
| 运行日志 | 客户端数据目录、网关请求日志与操作日志集中查看 |

### API 网关（两个分区各自独立）

把当前账号的模型额度以**兼容接口**暴露给本机其它 AI 工具（Cursor、Claude Code、OpenWebUI 等），Key 与账号池按产品分区、按区域隔离。

| 能力 | 说明 |
| --- | --- |
| 兼容协议 | OpenAI 兼容（`/v1/models`、`/v1/chat/completions`）与 Anthropic 兼容（`/v1/messages`），均支持 SSE 流式 |
| 监听配置 | 默认仅监听 `127.0.0.1`；改为 `0.0.0.0` 需二次风险确认 |
| 默认端口 | WorkBuddy 网关 `57891`；Trae 网关 `7864`（Trae 另有本地 MITM 代理端口 `8899`，登录态捕获依赖它） |
| API Key | 可创建多个 Key，归属到具体区域；列表只显示前缀，**不落明文、不回显 hash**；支持吊销与删除 |
| 模型目录 | 拉取并展示可用模型清单，可手动刷新 |
| 账号策略 | 配置账号选择策略与账号池余额刷新间隔 |
| 请求日志 | 记录每次请求的模型、账号、Token 与状态，可清空 |

## 使用

### WorkBuddy

1. **添加账号**：账号页 →「OAuth 扫码添加」（device flow）、「导入本机账号」或「导入备份」
2. **切换账号**：账号卡片 →「切换」，可勾选把当前账号的会话一并复制过去，并把长期记忆与连接器配置合并到目标账号（跨版本迁移时可选数据来源版本）
3. **自动签到 / 自动旅行**：账号页顶部直接开关；设置页可调整参数、立即签到并查看日志
4. **查看积分到期**：账号页自动查询各账号积分资源；点击「刷新积分」手动更新，临期资源会高亮并按紧迫程度排序
5. **查看积分统计**：侧栏「积分统计」——总览、近 30 天趋势、模型分类、账号消耗与请求明细；筛选账号或时间范围不会重复请求官方接口，点「刷新统计」才重新采集
6. **查看 Token 统计**：侧栏「Token 统计」，选择 WorkBuddy、CodeBuddy CLI 或 CodeBuddy IDE，查看输入、输出、缓存读写与调用次数
7. **接入 CodeBuddy CLI**：账号页一键接入 / 更新认证。「切换 CodeBuddy」只更新**后续加载会话**使用的默认账号，当前运行会话不会切换——请由 ACP 重新加载会话，或重启 CodeBuddy CLI 后生效
8. **切换 CodeBuddy CN IDE**：账号卡片一键切换国内版桌面客户端（www.codebuddy.cn）。切换会关闭并重启 CodeBuddy CN；首次使用前请先手动打开并登录一次以生成 Keychain Safe Storage
9. **自动轮换**：设置 → CodeBuddy CLI 自动轮换（策略见下）
10. **更新**：应用自动检查公开 GitHub Releases，发现新版本后可在左下角直接升级

### TraeWork

1. **添加账号**：账号页 →「OAuth 网页登录」（浏览器完成授权后回调）或「粘贴 JWT」；也可「导入本机账号」「导入备份」
2. **选择区域与产品线**：页面顶部切换国内版 / 国际版；账号卡片上按程序位切换要操作的客户端（TraeWork / TraeCode）
3. **签到**：账号页「签到并刷新积分」，或对单个账号执行；开启「跳过已签到」避免重复请求
4. **切换登录态**：账号卡片 →「切换」，写入所选客户端登录态并重启客户端；切换前自动保存快照
5. **快照回滚**：设置 →「登录态快照」，可保存 / 恢复 / 删除
6. **API 服务**：侧栏「API 服务」，创建 Key 后按页面上的接入指引配置到你的工具里

### 自动轮换策略（WorkBuddy）

自动轮换的目标是防止积分过期浪费：后台定时查询所有账号的积分到期情况，把 CodeBuddy CLI 后续会话的默认账号设为「最紧迫」的账号（最早到期且仍有剩余积分）。macOS/Linux 的 `apiKeyHelper` 与 Windows 的 settings env 都不会替换正在运行会话已经持有的 token；轮换结果需在 ACP 重新加载会话或重启 CLI 后生效。为避免默认账号频繁变化，每次检查按以下顺序决策：

1. **有效账号**：查询成功、未过期、有剩余积分的账号才可被选为目标
2. **紧迫度检查**：所有账号到期都还早（最紧迫的剩余超过 `min_urgency_hours`，默认 72 小时）→ 不切
3. **已是目标**：CLI 默认账号就是最紧迫账号 → 不切
4. **冷却期**：切换后 `cooldown_minutes`（默认 120）内不重复切
5. **活跃保护**：最近 `active_guard_minutes`（默认 30）内 CLI 会话有写入（正在对话）→ 不切
6. **价值过滤**：目标账号剩余积分低于 `min_remaining_credits` → 不值得切（默认 0 关闭；每次检查会把各账号剩余积分写入日志，可据此调整）
7. **防抖动**：目标比当前早到期但差异小于 `min_gap_hours`（默认 24）→ 不切

> **生效边界**：自动轮换只更新后续 restore/load 使用的默认账号，不会热切换当前会话。macOS/Linux 下一次 helper 执行会读取最新账号；Windows 会把最新 Token 写入 settings。正在运行的会话继续使用启动或加载时取得的账号；请由 ACP 重新加载会话，或重启 CodeBuddy CLI。

配置项：`check_interval_minutes`（检查间隔，默认 5）、`cooldown_minutes`、`min_urgency_hours`、`active_guard_minutes`、`min_remaining_credits`、`min_gap_hours`。可在设置页调整，或直接编辑 `~/.buddy-switch/auto_rotate_config.json`。

## 界面预览

### WorkBuddy 账号管理

账号卡片集中展示登录状态、签到状态、积分余额与临期资源；顶部区域切换器在 WorkBuddy 国内版 / 国际版之间切换，两版账号库互相隔离。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/workbuddy-accounts-light.png" alt="WorkBuddy 账号管理页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/workbuddy-accounts-dark.png" alt="WorkBuddy 账号管理页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### 积分统计

展示官方请求用量、每日趋势、模型分布与账号消耗，并明确标注数据来源与更新时间；顶部可在国内版、国际版与合并视图之间切换。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/credit-stats-light.png" alt="积分统计页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/credit-stats-dark.png" alt="积分统计页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### TraeWork 账号管理

顶部切换国内版 / 国际版，状态条展示客户端版本、运行状态、今日签到、可用积分与 JWT 临期情况；账号卡片提供程序位切换、签到、切换登录态与冷却解除。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/trae-accounts-light.png" alt="TraeWork 账号管理页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/trae-accounts-dark.png" alt="TraeWork 账号管理页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

### API 服务

按版本分别给出 Base URL 与代表性 Key，Key 列表支持创建、吊销与删除；下方提供模型清单、账号策略、接入指引与请求日志。

<table>
  <thead>
    <tr>
      <th>浅色模式</th>
      <th>深色模式</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/api-service-light.png" alt="API 服务页（浅色模式，演示数据）" /></td>
      <td><img src="docs/images/api-service-dark.png" alt="API 服务页（深色模式，演示数据）" /></td>
    </tr>
  </tbody>
</table>

> 以上截图取自演示构建，账号、积分与请求记录均为虚构数据。

## 数据与隐私

- 所有数据都保存在本机 `~/.buddy-switch/` 下，不上传到任何第三方服务
- 账号库：WorkBuddy 为 `accounts.json`（国内版）/ `accounts.global.json`（国际版）；Trae 为 `trae/checkin_accounts.json`（国内版）/ `trae/checkin_accounts.global.json`（国际版）
- 认证文件：WorkBuddy 为 `workbuddy-desktop.info` / `workbuddy-desktop-ai.info`
- 切换账号前会先备份原认证文件；Trae 切换登录态前会先保存快照
- 网关 API Key 只保存前缀与哈希，明文仅在创建时返回一次
- 仓库不提交本地数据；发布前会扫描 token 模式（`ghp_` / `npm_` / `gho_` 等）

## macOS 权限说明

切换账号需要写入 WorkBuddy 认证文件，macOS 要求授权「App 管理」（或「完全磁盘访问」）：

1. 首次切换报「无权限」时，点「打开系统设置」
2. 优先在 **App 管理** 里打开 BuddySwitch 开关；若没有，则去 **完全磁盘访问** 把 BuddySwitch 拖进带箭头的框
3. 授权后重启本应用生效；设置页「权限检测」可随时验证

> webui 模式：由启动服务的终端进程权限决定；若终端已授权完全磁盘访问则无需额外操作。

## 开发

环境要求、构建命令、发布流程与目录结构见 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)。简要说明：

```bash
npm install
npm run tauri dev     # 桌面端开发模式
npm run build         # 构建前端（dist/）
npm run check:api     # 校验前端 API 契约与后端一致
```

仓库为 Rust workspace + Vite/React 前端：

```
crates/
  buddy-switch-core/     # 核心逻辑：账号、认证、OAuth、进程、切换、会话、签到、刷新、区域与 Trae 模块
  buddy-switch-gateway/  # OpenAI / Anthropic 兼容网关（WorkBuddy 与 Trae 各一套实现）
  buddy-switch-server/   # HTTP server + CLI（axum API + rust-embed 内嵌前端）
src-tauri/               # 桌面宿主（Tauri command 薄包装 + 托盘）
src/                     # 前端：components / pages / lib（api.ts 双通道：Tauri invoke 或 HTTP fetch）
npm/                     # npm 包（**尚未发布**）：主包 @nextagentx/buddy-switch + 5 个平台分包
```

## 参与贡献

**欢迎一切形式的贡献，也欢迎 PR。** 提 Issue、补文档、修 Bug、加功能都算 —— 完整的贡献指引（能做什么、PR 自检清单、本仓库的几条代码约定）见 **[CONTRIBUTING.md](CONTRIBUTING.md)**（[English](CONTRIBUTING.en.md)）。

## 支持这个项目

如果 Buddy Switch 帮到了你，可以请作者喝杯饮料 ☕

<table>
  <thead>
    <tr>
      <th>微信支付</th>
      <th>支付宝</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/donate-wechat.png" alt="微信支付收款码" width="260" /></td>
      <td><img src="docs/images/donate-alipay.jpg" alt="支付宝收款码" width="260" /></td>
    </tr>
  </tbody>
</table>


## 致谢

本项目在实现过程中参考了以下开源项目，在此向作者致谢：

- [changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch) —— 借鉴了 WorkBuddy 账号数据格式与多账号切换的整体思路。
- [Sliverkiss/workbuddy2api](https://github.com/Sliverkiss/workbuddy2api) —— 借鉴了将 WorkBuddy 模型额度封装为 OpenAI 兼容接口、对外提供 API 服务的思路。

以上项目各自遵循其自身许可；本项目为独立实现，与上述项目之间不存在隶属、合作或背书关系。相关名称与商标归各自权利人所有。本项目拥有独立的代码库、发布通道与数据目录，不会替换、覆盖或改写其他项目的升级源或数据目录。

## 免责声明

使用本项目即表示你已完整阅读、理解并同意全部免责条款；如不同意，请立即停止使用并卸载。

完整条款见 **[docs/DISCLAIMER.md](docs/DISCLAIMER.md)**（[English](docs/DISCLAIMER.en.md)），共 6 条：非官方第三方工具、服务条款与合规自理、数据写入与备份、自动化行为风险、API 网关暴露风险、按「原样」提供。

## 许可

本项目采用 **[PolyForm Noncommercial License 1.0.0](./LICENSE)**：**个人非商业使用是允许的，商业使用不被授权**（商业用途须事先取得作者的书面授权）。

- 条款全文：[LICENSE](./LICENSE)
- 白话说明、允许与禁止的具体情形、常见问题、以及 2026-09-22 之前版本的 MIT 说明：[docs/LICENSING.md](docs/LICENSING.md)
- 如需商业授权，请通过本仓库 Issues 联系作者。
