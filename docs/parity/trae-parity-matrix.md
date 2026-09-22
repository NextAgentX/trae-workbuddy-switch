# Trae 对齐 WorkBuddy · 差距矩阵与需求池（增量 PRD）

> 目标：把 Trae 分区的**交互与功能骨架**补齐到与 WorkBuddy 分区一致。
> 只统一骨架，不搬文案；数据源与文案按 Trae 实情落地。
> 所有判定均给出 `文件:行号` 证据；读不出或存在歧义处显式标注 **（未证实）**。
> 生成方式：静态阅读代码，未运行、未改任何源文件。

---

## 0. TL;DR

- 差距总量 **76 条**：页面级 32 条、组件级 12 条、后端命令级 32 条。
- 分类构成：**A（可直接照搬）49 条**、**B（需后端补聚合/换语义）10 条**、**C（平台不可能，必须显式 Unsupported）17 条**。
- 优先级构成：**P0 18 条**、**P1 27 条**、**P2 31 条**。
- 一句话结论：Trae 五大页面**已经搭好同构骨架**（Accounts/TokenStats/CreditStats/ApiService 均有同名页且结构对应），
  真正的差距集中在三类——① region 维度 → 变体维度的**映射收尾**（A）；② **数据源不同**导致的聚合字段缺口（B，主要是按模型官方消耗、热力/项目维度）；③ **Trae 生态根本不存在的能力**（C，自动旅行、CodeBuddy CLI/IDE、多 Key、会话/记忆迁移）。

---

## 1. 口径与分类定义

| 记号 | 含义 | 处理原则 |
|:--|:--|:--|
| **A** | 可直接照搬 | Trae 侧已有等价能力，收敛骨架/样式即可 |
| **B** | 需后端补聚合或换语义 | 能力存在但数据源/字段集不同，需在 Trae 后端新增聚合或改写语义 |
| **C** | 平台不可能 | Trae 生态不存在该对象，**必须显式声明 Unsupported，绝不伪造成功** |
| **P0** | 截图点名的三页核心交互（账号管理/Token 统计/积分统计） | 本轮必须做 |
| **P1** | 骨架一致性有感知、不影响主流程 | 本轮应做 |
| **P2** | 锦上添花或明确不做 | 可延后 |

维度口径（贯穿全文，**不得混用**）：
- WorkBuddy 用 `Region`（国内版 `cn` / 国际版 `global` / 合并 `all`）——`src/lib/region.ts`、`src/components/region-bar.tsx:25`。
- Trae **没有 region**，对应位置是产品线变体 `TraeVariant`（`TraeWork` / `TraeCn`）——`crates/buddy-switch-core/src/modules/trae/variant.rs:71-76`、`src/components/trae-variant-switch.tsx:33`。
- 三态「合并/all」是**筛选维度**，不是第三种变体；**不得**往 `TraeVariant` 里加「合并」值（`variant.rs:17-25` 明确二者正交）。
- Trae 凭据是 `Cloud-IDE-JWT`（VSCode 系），**不叫 Token**——`TraeAccountsPage.tsx:77`、`trae-import-accounts-dialog.tsx:176`。

---

## 2. 差距矩阵

### 2.1 页面级（五对逐对）

#### 2.1.1 Accounts 账号管理（WorkBuddy `AccountsPage.tsx` ↔ Trae `TraeAccountsPage.tsx`）

| 编号 | WorkBuddy 侧（功能点 + 文件:行号） | Trae 侧现状（文件:行号 / 无） | 分类 | 优先级 |
|:--|:--|:--|:--:|:--:|
| G-A01 | 双产品线状态 Tabs（cn/global）`AccountsPage.tsx:213-225`，页头副标题「分别管理国内版与国际版」`:207-209` | 变体切换器 `TraeAccountsPage.tsx:274-277` + `trae-variant-switch.tsx:33` | A | P0 |
| G-A02 | region 由页内 Tab 状态驱动 | 变体由侧栏/URL `useTraeVariant()` 驱动 `TraeAccountsPage.tsx:86-95` | A | P0 |
| G-A03 | 自动签到**定时**开关 `AccountsPage.tsx:428-441`（保存失败提示 `:437`） | 「跳过今日已签到」开关 `TraeAccountsPage.tsx:189-202`（语义替身，无调度器） | B | P1 |
| G-A04 | 自动旅行开关 + travelMap 轮询 `AccountsPage.tsx:114-131, 258-261, 443-455` | 无（`TraeAccountsPage.tsx:80` 注释声明不存在） | C | P2 |
| G-A05 | 「CodeBuddy CLI 接入」折叠卡 | 无（`TraeAccountsPage.tsx:80`） | C | P2 |
| G-A06 | 账号卡积分包进度条 + 近期到期 `account-card.tsx:447-465` | `trae-account-card.tsx` 无该区块（无按包进度，见 `credits.rs:572-648`） | B | P1 |
| G-A07 | 批量签到单命令 `checkin_all` | 两步：`traeCheckin` + `traeRefreshCredits` `TraeAccountsPage.tsx:214-233` | A | P0 |
| G-A08 | 切换账号对话框（会话/记忆/连接器迁移）`switch-account-dialog.tsx:34, 269-307` | 无对话框；卡片内联 `api.traeSwitchAccount` | C | P1 |
| G-A09 | OAuth 扫码登录 `oauth-login-dialog.tsx` | `trae-oauth-login-dialog.tsx` 存在且 `oauthOpen` 状态在 `TraeAccountsPage.tsx:104`，但同文件 `:77` 注释称「只支持粘贴 Cloud-IDE-JWT」 | A | P1 （**未证实**：注释与组件并存，需澄清） |
| G-A10 | 添加账号对话框（多字段） | 粘贴 JWT 添加 `trae_add_account`（`api.rs:162`） | A | P0 |

> 已对齐、无需改动：页头标题/副标题、「添加与迁移账号」横幅、环境说明行、空态卡、账号工具栏（`账号[N]` 徽章 + 紧凑 + 刷新）、卡片栅格、删除确认、导入/导出对话框（见 `TraeAccountsPage.tsx:66-81`）。

#### 2.1.2 TokenStats Token 统计（`TokenStatsPage.tsx` ↔ `TraeTokenStatsPage.tsx`）

| 编号 | WorkBuddy 侧 | Trae 侧现状 | 分类 | 优先级 |
|:--|:--|:--|:--:|:--:|
| G-T01 | `RegionBar` `TokenStatsPage.tsx:1365-1366`，region 为数据源 `:1271,1277-1296` | 无 region 条（应用变体条，Trae 无 region） | A | P0 |
| G-T02 | 产品来源 Tabs（WorkBuddy / CodeBuddy CLI / IDE）`TokenStatsPage.tsx:35, 396-411` | 无（Trae 只有本机网关一个数据源，`token_stats.rs:1-17`） | C | P1 |
| G-T03 | Token 总览（堆叠比例条 + 4 指标）`TokenStatsPage.tsx:381-411` | 8 指标（含平均/P95 耗时、流式、活跃账号）`TraeTokenStatsPage.tsx` | A（Trae 更全） | P0 |
| G-T04 | Token 与调用趋势（堆叠柱+折线右轴）`TokenStatsPage.tsx:623` | 每日趋势折线 | A | P1 |
| G-T05 | 「Token 活动」整年热力网格 `TokenStatsPage.tsx:813-879` | 无 | B | P2 |
| G-T06 | 「用量分布」按项目/按模型 Top8 `TokenStatsPage.tsx:1068-1118` | 按模型柱+表、按账号 | B | P1 |

> Trae 侧已有的诚实数据源提示（`TraeTokenStatsPage.tsx:204-217`「只统计经过本机 Trae 网关的调用」）**必须保留**。

#### 2.1.3 CreditStats 积分统计（`CreditStatsPage.tsx` ↔ `TraeCreditsPage.tsx`）

| 编号 | WorkBuddy 侧 | Trae 侧现状 | 分类 | 优先级 |
|:--|:--|:--|:--:|:--:|
| G-C01 | `RegionBar` `CreditStatsPage.tsx:22` + region 持久化 `:58-78` | 无 region 条 | A | P0 |
| G-C02 | 4 指标卡 `CreditStatsPage.tsx:288` | 5 指标（可用总额/平均/账号数/今日新增/今日消耗） | A | P0 |
| G-C03 | 「官方积分消耗」按模型堆叠柱 `CreditStatsPage.tsx:212, 384-385, 483` | 无（Trae 无按模型官方消耗 `credits.rs:572-648`） | C | P1 |
| G-C04 | 「按模型类型」Top8（含合计角标）`CreditStatsPage.tsx:754-771` | 无 | C | P1 |
| G-C05 | 「积分明细」内部 Tabs（明细 / 请求用量）`CreditStatsPage.tsx:601-697` | 近 7 日趋势 + 账号积分明细表（无内部 Tabs） | A | P1 |

#### 2.1.4 ApiService API 服务（`ApiServicePage.tsx` ↔ `TraeApiServicePage.tsx`）

| 编号 | WorkBuddy 侧 | Trae 侧现状 | 分类 | 优先级 |
|:--|:--|:--|:--:|:--:|
| G-P01 | 网关开关 + 监听地址/端口 `ApiServicePage.tsx`（WB 端口 57891） | 同构，端口 7864（`TraeApiServicePage.tsx:126-142`） | A | P0 |
| G-P02 | 接入地址「按版本」`REGIONS.map` `ApiServicePage.tsx:188-205` | 单条 Base URL `TraeApiServicePage.tsx:397-418` | A | P0 |
| G-P03 | 多 Key 表（list/create/revoke/delete）`ApiServicePage.tsx:208-212` + `gateway/api-key-table` | 单 Key 卡 `TraeApiServicePage.tsx:420-452`（只存一把 `TraeSettings::apiKey`） | C | P1 |
| G-P04 | `AccountStrategyCard`（get/save_account_strategy） | 账号池卡 `TraeApiServicePage.tsx:454-551` | B | P2 |
| G-P05 | `ModelList` + `refresh_gateway_models` | `get_trae_gateway_models`（**无 refresh**） | A | P1 |
| G-P06 | `IntegrationGuide` 独立组件 | 内联渲染 | A | P2 |
| G-P07 | `RequestLog` + `clear_gateway_logs` | `get/clear_trae_gateway_logs`（`api.rs:206-209`） | A | P1 |

#### 2.1.5 Settings 设置（`SettingsPage.tsx` ↔ `TraeSettingsPage.tsx`）

| 编号 | WorkBuddy 侧 | Trae 侧现状 | 分类 | 优先级 |
|:--|:--|:--|:--:|:--:|
| G-S01 | 引入 `settings-primitives` `SettingsPage.tsx:9` | 同样引入 `TraeSettingsPage.tsx:28` | — 已对齐 | — |
| G-S02 | region 相关设置项 | 变体相关（侧栏驱动 `TraeSettingsPage.tsx:107`） | A | P1 |
| G-S03 | （WB 无独立能力面板） | 「平台能力」面板 + `CapabilityBadge` `TraeSettingsPage.tsx:1198-1205` | A（Trae 侧更细） | P1 |
| G-S04 | （WB 无设备标识重置） | 6 层设备标识重置 `TraeSettingsPage.tsx:1159-1195`；非 Windows 返回 Unsupported `platform.rs:940-959` | A（含 C 分支） | P1 |
| G-S05 | 外观 / 开机自启 / 自动更新（应用级） | **已收口到共享模块** `src/components/app-settings.tsx`（两模块同一份实现）；入口固定在侧栏底部「版本号上方」（`App.tsx` → `AppSettingsEntry`），不再各自出现在设置页 | — 非差距 | — |
| G-S06 | 无 | 「登录态快照」profiles `TraeSettingsPage.tsx:604-826` | A（反向，Trae 独有） | P2 |

> 两设置页**共用** `settings-primitives.tsx:33/47/76`（`SettingsGroup`/`SettingsRow`/`SettingsFieldRow`），不得各写一份。
>
> **应用级三块**（外观 / 开机自启 / 自动更新）另有一层共享：`components/app-settings.tsx`。
> 共享的判据是**有无产品耦合** —— 纯版式原语、以及不读 `Region`/`TraeVariant` 的
> 应用级业务块可以共享；依赖产品上下文的块（版本与账号库、网关、登录态快照、
> 设备标识…）**仍然各自实现**，不得因为「长得像」而合并。

### 2.2 组件级

| 编号 | WorkBuddy 组件 | Trae 侧现状 | 分类 | 优先级 |
|:--|:--|:--|:--:|:--:|
| G-C-01 | `region-bar.tsx:25`（三态 cn/global/all） | `trae-variant-switch.tsx:33`（两变体 + 运行状态点 `:64-95`） | A | P0 |
| G-C-02 | `account-card.tsx`（header/body/footer，`ProductCurrentState:199-226`） | `trae-account-card.tsx`（同构，注释 `:100-107`） | A | P0 |
| G-C-03 | `import-accounts-dialog.tsx`（`hasToken:170`） | `trae-import-accounts-dialog.tsx`（`hasJwt:176`，`variant` 参数） | A（已对齐） | P1 |
| G-C-04 | `export-accounts-dialog.tsx`（主键 `id`，`buddy-switch-accounts-*.json:36`） | `trae-export-accounts-dialog.tsx`（主键 `userId`，`trae-accounts-*.json:40-44`） | A（已对齐） | P1 |
| G-C-05 | `oauth-login-dialog.tsx:184` | `trae-oauth-login-dialog.tsx:369`（+`traeOAuthCancel:187-202`、超时倒计时 `:153-185`、回调端口 17388 `:307-313`） | A（已对齐，**未证实**见 G-A09） | P1 |
| G-C-06 | `switch-account-dialog.tsx:34`（会话/记忆/连接器迁移） | **无** | C | P1 |
| G-C-07 | `gateway/api-key-table.tsx` | 无独立文件，内联于 `TraeApiServicePage.tsx:420-452` | C（单 Key） | P1 |
| G-C-08 | `gateway/account-strategy-card.tsx` | 无独立文件，内联账号池卡 `:454-551` | B | P2 |
| G-C-09 | `gateway/model-list.tsx` | 无独立文件，内联 | A | P2 |
| G-C-10 | `gateway/integration-guide.tsx` | 无独立文件，内联 | A | P2 |
| G-C-11 | `gateway/request-log.tsx` | 无独立文件，内联 | A | P2 |
| G-C-12 | `settings-primitives.tsx` | 两页共用（同 G-S01） | — 无差距 | — |

> 说明：Trae 侧未把网关子区块拆成 5 个组件，而是内联在页面里。是否拆分属**工程口味**，不构成功能差距；若为消除漂移建议拆分（P2）。

### 2.3 后端命令级（一一映射）

WorkBuddy 命令表见 `src-tauri/src/lib.rs:155-217`；Trae 命令表见 `src-tauri/src/lib.rs:220-260`。

| 编号 | WorkBuddy 命令（文件:行号） | Trae 命令（文件:行号 / 无） | 分类 | 优先级 |
|:--|:--|:--|:--:|:--:|
| G-B-01 | `get_status` `lib.rs:155` | `get_trae_env` `lib.rs:220`（返回体不同，`platform.rs:986-1005`） | A | P0 |
| G-B-02 | `get_accounts` `lib.rs:156` | `get_trae_accounts` `lib.rs:223` | A | P0 |
| G-B-03 | `oauth_start` / `oauth_status` `lib.rs:164-165` | `trae_oauth_start/status` `lib.rs:235-236`（+`cancel:237`） | A | P0 |
| G-B-04 | `delete_account` `lib.rs:163` | `trae_delete_account` `lib.rs:233` | A | P0 |
| G-B-05 | `import_local` `lib.rs:166` | `trae_import_local_account` `lib.rs:234` | A | P0 |
| G-B-06 | `export_accounts` / `_to_path` `lib.rs:167-168` | `trae_export_accounts` / `_to_path` `lib.rs:238-239` | A | P0 |
| G-B-07 | `preview_import_accounts` / `import_accounts` `lib.rs:169-170` | `trae_preview_import_accounts` / `trae_import_accounts` `lib.rs:240-241` | A | P0 |
| G-B-08 | `switch_account` `lib.rs:171`（含迁移，`api.rs:552-570`） | `trae_switch_account` `lib.rs:247`（**无**迁移语义） | A | P1 |
| G-B-09 | `switch_progress` `lib.rs:172` | 无 | C | P2 |
| G-B-10 | `list_sessions` / `copy_sessions` `lib.rs:173-174` | 无 | C | P1 |
| G-B-11 | `migrate_account_data` `lib.rs:175`（memory/connectors，`api.rs:676-697`） | 无 | C | P1 |
| G-B-12 | `checkin` / `checkin_all` `lib.rs:184-185` | `trae_checkin` `lib.rs:243`（无 `_all`，前端循环） | A | P0 |
| G-B-13 | `get_checkin_status` `lib.rs:180` | `get_trae_checkin_status` `lib.rs:224` | A | P1 |
| G-B-14 | `get_checkin_logs` `lib.rs:188` | `get_trae_logs` `lib.rs:227` | A | P1 |
| G-B-15 | `get/save_auto_checkin_config` `lib.rs:186-187` | `save_trae_settings`（`checkinSkipChecked`）`lib.rs:229-230` | B | P1 |
| G-B-16 | `get_travel_status` / `get/save_auto_travel_config` `lib.rs:189-191` | 无（`travel.rs` 仅 WorkBuddy） | C | P2 |
| G-B-17 | `get/save_schedule_config` `lib.rs:192-193` | 无 | C | P2 |
| G-B-18 | `refresh_account_token` `lib.rs:194` | `trae_refresh_jwt` `lib.rs:245` | A | P1 |
| G-B-19 | `get/save_auto_rotate_config` / `rotate_status` / `run_rotate` / `get_rotate_logs` `lib.rs:195-199` | 无 | C | P2 |
| G-B-20 | `get/save_github_config` `lib.rs:200-201` | 无 | C | P2 |
| G-B-21 | `get_codebuddy_cli_status` / `install_codebuddy_cli_helper` / `switch_codebuddy_cli_account` / `get_codebuddy_cn_ide_status` / `switch_codebuddy_cn_ide_account` / `detect_codebuddy_cn_ide_account` `lib.rs:157-162` | 无 | C | P2 |
| G-B-22 | `get_credit_statistics` `lib.rs:182` | `get_trae_credits` `lib.rs:225`（数据源=签到快照，`credits.rs:572-648`） | B | P1 |
| G-B-23 | `get_token_statistics` `lib.rs:183` | `get_trae_token_statistics` `lib.rs:226`（返回体**刻意不同**，`token_stats.rs:219`） | B | P1 |
| G-B-24 | `get/save_gateway_config` / `gateway_status` `lib.rs:206-208` | 同名 Trae 版 `lib.rs:254-256` | A | P0 |
| G-B-25 | `list_api_keys` / `create_api_key` / `revoke_api_key` / `delete_api_key` `lib.rs:209-212` | 仅 `regenerate_trae_api_key` `lib.rs:258` | C | P1 |
| G-B-26 | `get/save_account_strategy` `lib.rs:215-216` | 无 | B | P2 |
| G-B-27 | `get_gateway_models` / `refresh_gateway_models` `lib.rs:213-214` | `get_trae_gateway_models` `lib.rs:257`（无 refresh） | A | P2 |
| G-B-28 | `get_gateway_logs` / `clear_gateway_logs` `lib.rs:217-218` | `get/clear_trae_gateway_logs` `lib.rs:259-260` | A | P1 |
| G-B-29 | `get_credit_expiry` `lib.rs:181` | 无独立命令（并入 `get_trae_credits`） | A | P2 |
| G-B-30 | （无） | `trae_reset_device` `lib.rs:252` → `platform::reset_device_identity_for` `platform.rs:1082`；非 Windows 层返回 Unsupported `platform.rs:940-959, 1341` | A（含 C 分支） | P1 |
| G-B-31 | （无） | `trae_save_login` / `trae_backup/restore/delete_profile` `lib.rs:248-251` | A（反向，Trae 独有设备/快照层） | P2 |
| G-B-32 | `open_permission_settings` / `check_auth_permission` / `reveal_app_in_finder` / `open_accounts_dir` `lib.rs:176-179` | 无对应（Trae 有 `trae_data_dir` 但无 open-dir 命令） | B | P2 |

> **契约同步纪律提醒（3 处，另加 1 处类型）**：任何新增/改动命令必须同步
> `src/lib/api.ts`（`ROUTES` + wrapper）、`src-tauri/src/lib.rs`（`invoke_handler`）、
> `crates/buddy-switch-server/src/api.rs`（路由），类型放 `src/lib/trae-types.ts`；
> 由 `scripts/check-api-contract.cjs` 校验。**本 PRD 不改任何源码。**

---

## 3. 需求池

每条 = 用户故事 + 可证伪的验收标准。**P0 = 截图点名的三页核心交互。**

### 3.1 P0（本轮必须）

| ID | 用户故事 | 验收标准（可证伪） |
|:--|:--|:--|
| R-P0-1 | 作为 Trae 用户，我希望账号页的产品线切换与 WorkBuddy 的区域切换**位置与形态一致**，以便不重新学习 | 账号页页头右侧存在变体切换器（两变体 + 运行状态点），点击后 `?line=` 变化且四份数据（accounts/env/credits/checkin）按新变体重取（`TraeAccountsPage.tsx:117-143`） |
| R-P0-2 | 作为 Trae 用户，我希望账号卡片的骨架与 WorkBuddy 一致 | 卡片含 header/body/footer 三段与「当前账号」角标；`trae-account-card.tsx` 与 `account-card.tsx` 逐段对齐（差异仅字段名与无积分包进度条） |
| R-P0-3 | 作为 Trae 用户，我希望「全部签到」一步把签到+积分刷新都做完 | 点一次按钮后，卡片积分数字发生变化或出现「积分刷新失败」warning（`TraeAccountsPage.tsx:214-233`）；不得只签到不刷新 |
| R-P0-4 | 作为 Trae 用户，我希望 Token 统计页有与 WorkBuddy 同形的总览与趋势 | 页面含总览指标区 + 每日趋势图；数据源提示 Alert 明确「只统计经过本机 Trae 网关的调用」（`TraeTokenStatsPage.tsx:204-217`） |
| R-P0-5 | 作为 Trae 用户，我希望积分统计页有与 WorkBuddy 同形的指标卡与明细 | 页面含 ≥4 指标卡与账号积分明细表；总额=各账号剩余之和（`TraeAccountsPage.tsx:280-283` 口径一致） |
| R-P0-6 | 作为 Trae 用户，我希望 API 服务页有与 WorkBuddy 同形的网关开关与接入地址 | 页面含网关开关 + 监听地址/端口 + 单条 Base URL；端口为 Trae 专用 7864，与 WB 57891 不冲突（`TraeApiServicePage.tsx:126-142`） |
| R-P0-7 | 作为 Trae 用户，我希望添加/导入/导出账号的对话框与 WorkBuddy 同形 | 三对话框与 WB 版同构；导入用 `hasJwt`、导出文件名 `trae-accounts-*.json`（`trae-import-accounts-dialog.tsx:176`、`trae-export-accounts-dialog.tsx:40-44`） |

> P0 共 **7 条**（覆盖矩阵中的 18 条 P0 差距行）。

### 3.2 P1（本轮应做）

| ID | 用户故事 | 验收标准 |
|:--|:--|:--|
| R-P1-1 | 作为 Trae 用户，我希望搜索**跳过已签到**能有与 WB「自动签到」等价的可见开关 | 账号工具栏存在该开关，切换即持久化（`TraeAccountsPage.tsx:189-202`）；文案明确它只是「批量签到策略」，不暗示后台定时 |
| R-P1-2 | 作为 Trae 用户，我希望账号卡能显示积分包到期信息 | 若 Trae 数据可取到包级到期，则显示；取不到则显示「无包级数据」而非假进度条（依赖 B 类后端聚合） |
| R-P1-3 | 作为 Trae 用户，我希望 Token 统计有「按模型/按账号」分布 | 分布区存在，且分母=同页总览同期的总量（可交叉验证） |
| R-P1-4 | 作为 Trae 用户，我希望积分明细能区分「明细 / 请求用量」 | 若数据支持请求用量分栏则加 Tabs；不支持则单栏并注明口径 |
| R-P1-5 | 作为 Trae 用户，我希望网关模型列表可手动刷新 | 若新增 refresh 命令，则三处契约同步且 `check-api-contract.cjs` 通过 |
| R-P1-6 | 作为 Trae 用户，我希望网关请求日志可查看与清空 | 日志区存在且「清空」后列表为空（`api.rs:206-209`） |

> P1 共 **6 条**（覆盖矩阵中的 27 条 P1 差距行）。

### 3.3 P2（可延后）

- 网关子区块组件化拆分（`gateway/*` → Trae 专用组件，消除漂移）。
- 「Token 活动」整年热力网格、「用量分布」按项目维度（**依赖 B 类后端聚合，Trae 网关日志无项目字段，需先证实数据可得性**）。
- 目标/渠道类增强（自动轮换、GitHub 配置、定时任务）——**C 类，见第 4 节**。
- 账号策略卡语义对齐（`account_strategy` → 账号池卡）。

---

## 4. 不支持清单（Unsupported）

> 原则：**平台做不到的必须显式声明，绝不伪造成功**（`platform.rs:1-16` 设计原则）。
> C 类共 17 条，按「为什么做不到的证据 + 界面表达」列出。

| 能力 | WorkBuddy 侧证据 | 为什么做不到（Trae 侧证据） | 界面表达 |
|:--|:--|:--|:--|
| 自动旅行（派猫猫） | `travel.rs:1-5`、`AccountsPage.tsx:114-131,443-455` | Trae 无该活动接口；`TraeAccountsPage.tsx:80` 明示不存在 | **不渲染**该开关（当前做法）；若产品要求可见，用 Alert 说明「Trae 无对应活动」 |
| CodeBuddy CLI / IDE 接入 | `lib.rs:157-162` | CodeBuddy 属 WorkBuddy 生态，Trae 无此客户端 | 不渲染该折叠卡；如需说明用 Alert |
| 会话列表 / 复制会话 | `lib.rs:173-174`、`switch-account-dialog.tsx` | Trae 登录态是 `Cloud-IDE-JWT` 文件集，无会话树概念 | 切换对话框**不出现**会话勾选区 |
| 记忆/连接器迁移 | `lib.rs:175`、`api.rs:676-697` | Trae 无「长期记忆/连接器」对象 | 切换对话框**不出现**迁移选项卡 |
| 切换进度流 | `lib.rs:172` | Trae 切换为文件级快照替换，无长流程进度 | 用行内 busy 代替进度条 |
| 多 API Key + region 绑定 | `lib.rs:209-212`、`ApiServicePage.tsx:208-212` | Trae 只存一把 `TraeSettings::apiKey`（`TraeApiServicePage.tsx:126-142`） | 展示单 Key 卡 + 「重新生成」；**不显示** Key 表格 |
| 按模型官方积分消耗 | `CreditStatsPage.tsx:212,483,754` | Trae 积分仅来自签到快照，无按模型消耗（`credits.rs:572-648`） | 该图表区**不渲染**；以「本地观察积分消耗」口径呈现（若做） |
| 按模型类型 Top8 | `CreditStatsPage.tsx:754-771` | 同上 | 不渲染 |
| 产品来源多 Tabs（CLI/IDE） | `TokenStatsPage.tsx:396-411` | Trae 仅本机网关一个数据源（`token_stats.rs:1-17`） | 单源即无 Tabs；保留数据源 Alert |
| 定时任务（签到/旅行/保活调度） | `lib.rs:27-90`、`schedule.rs` | Trae 侧无调度器（`TraeAccountsPage.tsx:78-79`） | 不出现「自动」开关；改用「跳过已签到」 |
| 自动轮换 | `lib.rs:195-199` | 无对应上游能力 | 不渲染 |
| GitHub 配置 | `lib.rs:200-201` | 无对应 | 不渲染（全局共用项除外） |
| region 国内/国际维度 | `region.rs`、`region-bar.tsx:25` | Trae 无 region，只有产品线变体（`variant.rs:17-25,71-76`） | 用变体切换器替代；**不得**在 `Region`/`TraeVariant` 加「合并」值 |
| 非 Windows 的 MachineGuid 重置 | `platform.rs`（WB 同机制） | 该层仅 Windows（`platform.rs:940-959,1160-1166`） | 返回结构化 `Unsupported`（`platform.rs:45-54`）；前端 `CapabilityBadge` 置灰（`TraeSettingsPage.tsx:1205`） |
| OAuth 扫码（若 Trae 不支持） | `oauth-login-dialog.tsx` | 见 G-A09（**未证实**） | 待澄清后决定：支持则保留对话框，不支持则改为粘贴 JWT 并 Alert |
| 账号策略（account_strategy） | `lib.rs:215-216` | Trae 用「账号池」替代，语义不同 | 展示账号池卡；若策略项无对应则隐藏该项 |
| 开机自启 / 更新检查 | `lib.rs:202-205` | 属应用级能力，非 Trae 产品能力 | 保持全局可用，**不计为 Trae 差距** |

---

## 5. 待确认问题（需用户/架构师拍板）

1. **OAuth 在 Trae 究竟支持吗？** 证据冲突：`TraeAccountsPage.tsx:77` 注释称「没有 OAuth 扫码，只支持粘贴 Cloud-IDE-JWT」，但同文件 `:104` 有 `oauthOpen` 状态、且 `trae-oauth-login-dialog.tsx`（369 行）与 `trae_oauth_start/status/cancel`（`lib.rs:235-237`）均存在。
   **推荐**：以代码为准 → OAuth **可用**，把 `:77` 注释更正；账号页保留「扫码登录」入口。
2. **Trae 是否有包级积分到期数据？** 决定 G-A06（积分包进度条）是 A 还是 C。
   **推荐**：先按 B 处理（若有 `user_entitlement_pack_list` 的到期字段则显示，否则显式「无包级数据」）。若架构师确认无，则降级为 C。
3. **「按模型消耗 / 热力网格 / 按项目」三项是否纳入本轮？** 它们全部依赖 B 类后端聚合，且 Trae 网关日志可能无项目/模型级年度字段。
   **推荐**：本轮**只做 P0/P1**，这三项延后到「数据可得性验证」通过后再排期。
4. **多 Key 是否真的完全不做？** Trae 单 Key 是既有设计（`TraeApiServicePage.tsx:126-142`）。若产品坚持照搬多 Key，则需改动 Trae 后端存储模型，属重大变更。
   **推荐**：确认「单 Key + 重新生成」为本轮终态，多 Key 明确列入 Unsupported。
5. **组件拆分粒度**：是否要求把 Trae 网关区块拆成 5 个与 WB 同名的组件文件？
   **推荐**：不强制（功能无差距）；如为消除漂移可列为 P2，由架构师决定。
