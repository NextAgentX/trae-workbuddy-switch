# 性能审计报告 — Buddy Switch

> 审计日期：2026-09-24 · 范围：`src/`（React 前端）、`crates/buddy-switch-core`、`crates/buddy-switch-gateway`、`crates/buddy-switch-server`、`src-tauri`
> 方法：静态读码 + 本机实测。**每条结论都标注「已核实」的依据（文件:行号 / 实测数字）**；无法确证的归入 §5「待实测」。
> 约束：所有修复方案均以**不改变可观察行为**为前提，并逐条列出必须保留的语义与建议护栏。

> ## ⚠️ 已复核，请以复核版为准
>
> 本报告经**第二遍逐条复核**（查 git 引入提交 + 注释上下文 + 仓库内对照实现 + 测试），结论有实质修正。
> **复核结论见 [`perf-audit-2026-09-24-review.md`](./perf-audit-2026-09-24-review.md)**，其中：
>
> - **★ 撤回**：P1-4「sticky 内存泄漏」**判错** —— `get`/`bind`/`unbind`/`gc` 生产代码零调用，
>   表恒为空，**不存在泄漏**；真相是「整套粘性机制未接线」。详见复核报告。
> - **下调严重度**：P0-1（调用路径是冷的，且已有 3s 缓存 + spawn_blocking 两道缓解）；
>   P0-2（桌面端本地加载，代价是解析而非下载，收益需先量化）。
> - **改判为「有意为之，不得擅动」**：M3（`freshMs=0` 是文档化的 SWR 语义）；
>   P1-6 的 2s 轮询本身（用户可开关的「日志实时跟随」功能）；P0-3 的「每次重同步」。
> - **需先向作者确认**：P0-4（注释把「不缓存」写成设计属性，但未给理由）。
> - **需分级处理**：P1-3（55 个同步命令中，只有做慢 IO 的那些该改，微秒级配置读取不必改）。
> - **证据强化**：P0-1 / P1-2 / P1-3 / P1-6 的「是遗漏而非有意」判定，均靠**仓库内已有的对照实现**确立。
>
> 下文保留原始分析（含被修正的表述），**判断请以复核报告为准**。

---

## 0. 结论速览

按「收益 ÷ 风险」排序，最值得做的六件事：

| # | 问题 | 量级 | 风险 | 复核后 |
|---|---|---|---|---|
| 1 | 进程检测每次起 N 个 `tasklist` 子进程 | 单次 208ms，Global 区域一轮 ≈ 564ms | 低（改动局限在 `process.rs`） | 真问题，但**路径是冷的** ⇒ 降为 P1 |
| 2 | 首屏单 chunk **1.40 MiB**，零路由懒加载 | 冷启动解析 1.4MB JS | 低 | 真问题，**影响需先量化** |
| 3 | Trae 网关**每个请求**重建整个账号池 | 3 次磁盘读 + N 次哈希派生 | 中（需保住「写即可见」语义） | 真问题；**重同步本身是有意设计** |
| 4 | token 统计每次**全量重扫 + 全量 JSON 解析** | 随日志量线性增长 | 中（需等价性护栏） | **⚠️ 需先向作者确认** |
| 5 | 静态资源**无任何缓存头** | 每次刷新重传 1.4MB | 低（但 index.html 必须 `no-cache`） | 真问题（从未考虑过） |
| 6 | WorkBuddy 网关每请求 **5 次全局写锁** | 全局串行化 | 中 | 真问题；**底层设计有意** |

**已核实且表现良好的部分**（不要动）：上游 `reqwest::Client` 全局复用（含连接池 `pool_max_idle_per_host(20)`）；`stores/resources.ts` 的 SWR 快照（同键去重 + 序号防旧写回 + 上限 32，未见缓存穿透）；`trae/pool.rs::pick` 选号是 O(n) 单遍无锁；`ApiServicePage` 已用 `Promise.all` 并行 7 请求；`use-cached-resource.ts` 的 `freshMs=0`（**有意为之的 SWR 语义**）。

---

## 1. 实测基线（本机，2026-09-24）

进程枚举各方案单次耗时（`Measure-Command`，取单次）：

| 方案 | 耗时 | 说明 |
|---|---|---|
| `tasklist /NH`（全量） | **208 ms** | 一次拿全部进程 |
| `tasklist /FI "IMAGENAME eq X" /FO CSV /NH` | **141 ms** | 当前实现用的形式 |
| `Get-Process`（WinAPI） | **33 ms** | 快 6.3×，且不产生子进程 |
| 启动 `powershell -NoProfile -Command` | **329 ms** | 最贵，仅弹窗/一次性场景可用 |

其他基线：

- `dist/assets/index-*.js` = **1,464,712 字节（1.40 MiB）**，CSS 86,561 字节。全部页面与 recharts 在同一 chunk。
- `src-tauri/src/commands.rs`：**81 个** `#[tauri::command]`，其中 **26 个 async / 55 个同步**。
- `spawn_blocking` 出现次数：`buddy-switch-core` **0**、`buddy-switch-gateway` **0**、`buddy-switch-server/api.rs` **15**。
- 前端 `React.memo` / `memo(` 使用次数：**0**。

---

## 2. P0 — 收益最大，优先处理

### P0-1 进程枚举：每次检测起 N 个 `tasklist` 子进程

> **复核定性（详见复核报告）**：**确认为遗漏，非有意为之** —— 同一团队已在
> `trae/platform.rs:1079-1083` 做过同样优化并写明理由（「后者在装了多个渠道时要起 4 个子进程，这里只起 1 个」），
> 由 `19287bb` 引入；`process.rs` 这段来自初始提交 `461827e`，属**改 Trae 侧时漏改 WorkBuddy 侧**。
> **但严重度下调为 P1**：调用路径是**冷的**（仅状态探测 + 切换/结束流程），
> 且服务端已有 3s 缓存（`server/api.rs:28-46`）、Tauri 侧 `get_status` 已 async + spawn_blocking
> （`commands.rs:88-94`，注释写明就是为这个子进程）。**最有价值的是 `wait_windows_pids_gone`**，
> 因为它直接决定切换要等多久 —— 优先级应排在 `windows_workbuddy_process_rows_for` 之前。

**证据（已核实）**
- `crates/buddy-switch-core/src/modules/process.rs:519-525` — `windows_tasklist_image_rows(image)` 每次调用起一个 `tasklist` 子进程。
- `process.rs:530-540` — `windows_workbuddy_process_rows_for` **对每个映像名各调一次**：CN 区域 2 个名 → 2 次；**Global 区域 4 个名（`WorkBuddyAI` / `CodeBuddyAI` / `WorkBuddy AI` / `CodeBuddy AI`，见 `process.rs:65-75`）→ 4 次**。
- `process.rs:543-551` — `is_windows_pid_running(pid)` **每个 PID 一次** `tasklist`。
- `process.rs:554-570` — `wait_windows_pids_gone` 每 500ms 对**每个 PID** 再各起一次，直到超时。

**影响（含量化）**
Global 区域一次 `windows_workbuddy_process_rows_for` ≈ 4 × 141ms ≈ **564 ms**。等待进程退出时，若 3 个 PID 存活，每轮轮询 ≈ 423ms，轮询本身成了耗时主体。
缓解现状：`crates/buddy-switch-server/src/api.rs:28-46` 有 3 秒 `RUNNING_CACHE`，但**只覆盖服务端这一条路径**；Tauri 侧与 `wait_windows_pids_gone` 无缓存。

**修复方案（推荐两步走）**

*第一步（低风险，先做）*：把「按名过滤」改成「取一次全量 → 内存过滤」。
- 新增 `windows_tasklist_all_rows()`：`tasklist /FO CSV /NH` 一次拿全量。
- `windows_workbuddy_process_rows_for` 改为「全量 → `filter_windows_workbuddy_rows_for`」，从 4 次子进程降到 **1 次**（省 ~423ms）。
- `wait_windows_pids_gone` 改为「每轮取一次全量 → 用 `HashSet<u32>` 比对存活」，从 N 次/轮降到 **1 次/轮**。

*第二步（收益更大，单独一轮）*：换 `sysinfo` crate 或 Win32 `CreateToolhelp32Snapshot`，33ms 且无子进程，彻底消除黑窗风险。

**⚠️ 不破坏功能的关键约束**
1. `process.rs:549` 的 `None => true`（**查询失败视为进程仍在运行**）是保守设计，批量改造后**必须保留**同一语义 —— 否则查询失败会被误判为「进程已退出」，切换流程可能在客户端还活着时就动手。
2. 全量 `tasklist` 输出**多出「会话名 / 会话# / 内存使用」等列**，`parse_tasklist_csv`（`process.rs:360`）是按列索引解析的，必须核对列位一致性，别让内存列里的逗号把解析搞错。
3. `filter_windows_workbuddy_rows_for` 的精确映像名匹配语义不能变（现有护栏 `process.rs:1616-1627`）。

**建议护栏**
- 用真实全量 CSV 做夹具的新单测。
- **对照测试**：断言「全量 + 内存过滤」的结果与「逐个 `/FI` 查询」的结果**逐条相等**（这正是本项目的「阳性/阴性对照」纪律）。
- 断言 `None => true` 分支仍为 `true`。

---

### P0-2 首屏单 chunk 1.40 MiB，零路由懒加载

> **复核定性（详见复核报告）**：**确认为未做，非有意为之** —— `App.tsx` 中 lazy/Suspense/懒加载/chunk
> 相关字样**零匹配**；全仓性能相关提交只有 `e8c90a7 perf: Trae 分区切换不再整页重载`（做的是**数据层** SWR），
> 打包层从未涉及。
> **但影响需诚实下调**：桌面端从内嵌资源/本地磁盘加载、WebUI 走 `127.0.0.1`，**都不跨网络**
> ⇒ 1.40 MiB 的代价主要是**解析（数十 ms 量级）**，不是下载。
> recharts 在产物中的**确切占比未实测**（`node_modules/recharts` 5.1MB 不代表产物占比）。
> ⇒ **建议先量化再投入**：先拆一次 chunk 或跑可视化分析拿到 recharts / radix / 其余 的真实字节数，
> 再决定是否值得引入懒加载的复杂度（它牵动路由层 `Suspense` 与嵌入资源护栏，不是零成本）。

**证据（已核实）**
- `src/App.tsx:8-17` — 10 个页面全部静态 `import`。
- `dist/assets/index-CCFynCPV.js` 实测 1,464,712 字节；`vite.config.ts:24-54` 无 `build.rollupOptions.output.manualChunks`。
- recharts 被 `components/ui/chart.tsx:2`（`import * as RechartsPrimitive`）与 4 个统计页引入，全部进首屏。

**影响**：冷启动必须解析 1.4MB JS。打开「账号」页也会连带加载 4 个统计页的图表库 —— 用户从未访问的页面在付代价。

**修复方案**
1. 每个 `Route` 改 `const P = lazy(() => import("@/pages/P"))`，在 `<Routes>` 外层加**一个** `<Suspense fallback={...}>`。
2. `manualChunks` 把 `recharts` / `lucide-react` / `@radix-ui/*` 拆成独立 vendor chunk（recharts 尤其大）。
3. 顺带确认 `components/ui/chart.tsx` 是否真被使用，未用可删。

**⚠️ 不破坏功能的关键约束**
1. `App.tsx` 用 `BrowserRouter`，深链是常态 —— `<Suspense>` 必须包在**路由层**，不能只包单个页面，否则切路由会整页闪骨架。
2. **演示/截图模式已核实不受影响**：`src/lib/screenshot-demo.ts` 是**请求层 mock**，不依赖 DOM 同步挂载，懒加载后路由导航仍能拿到假数据。
3. `dist/` 与 `dist-demo/` 的 base 差异（`vite.config.ts:11-21`）不能碰；现有护栏 `api::tests::embedded_index_html_references_only_embedded_assets` 会校验 index.html 只引用已嵌入资源 —— **改了 chunk 划分后必须跑它**。
4. 改完前端**必须重编宿主**（`rust-embed` 编译期嵌入 `dist`），且重编前先停掉在跑的 serve，否则 `LNK1104`。

---

### P0-3 Trae 网关：每个请求重建整个账号池

**证据（已核实）**
- `crates/buddy-switch-gateway/src/trae/routes.rs:382-388` — `attempt_once` 每次选号前都 `pool.sync_for(variant)`；**换号重试会重复执行**。
- `trae/pool.rs:239-278` — `sync_for` 每次读 **3 份 JSON**（`load_remaining_for` / `load_cooldowns_for` / `account::entries_for_region`），并对**每个账号**执行 `device::derive(&uid)` + `rand_hex_salted(64, "mach", Some(&uid))`（`pool.rs:253-258`），重建整个 `Vec<TraePoolEntry>`。
- 注释 `routes.rs:385` 明确说明「每次选号前重新同步」是**刻意的正确性设计**（另一入口可能刚写冷却/刷新积分）。

**影响**：每个请求（含每次换号）3 次磁盘读 + N 次哈希派生 + 全量 Vec 分配。并发下成为吞吐瓶颈。

**修复方案 —— 不要简单加 TTL（会破坏上述正确性语义）**

正确做法是把 `sync_for` 的**三项成本拆开，各自用不影响语义的方式消除**：

1. **设备派生缓存（纯函数，最安全）**：`device::derive(uid)` 与 `rand_hex_salted(64, "mach", Some(&uid))` 都**只是 uid 的纯函数**（`pool.rs:237` 注释已确认「设备标识不需要分家」）。按 `uid` 缓存结果即可 —— 键空间 = 账号数，天然有界。这一项直接消掉 N 次哈希派生。
2. **文件读按 mtime 缓存**：3 份 JSON 改成「先 `metadata()` 比对 `(mtime, size)`，未变则复用上次解析结果」。**这保留了「另一个入口一写就立刻可见」的语义**（写必然改 mtime），同时消除重复 parse。这才是符合原设计的优化。
3. `apply_error` 写回后**主动标脏**，下次 `sync_for` 强制重读（写路径自己知道数据变了）。

**⚠️ 不破坏功能的关键约束**
1. 账号库按**区域**分家（`pool.rs:246-249`）—— 缓存键必须含 `variant.region()`，否则 CN/Global 串台（这是本项目踩过的坑）。
2. 缓存必须**可失效**，且能被测试重置（参照 `HomeOverrideGuard` 的做法）。
3. 必须保留 `sync_for` 的「以入参校正 `self.variant`」行为（`pool.rs:242`），否则 `apply_error` 会写错区域。

**建议护栏**
- 「同一 uid 两次 `device::derive` 结果相同」。
- 「改动 cooldown 文件后，下一次 `sync_for` 能看到新值」（防缓存过度）。
- 「CN / Global 两池互不污染」。

---

### P0-4 token 统计：每次全量重扫 + 全量 JSON 解析

> **复核定性（详见复核报告）**：**⚠️ 需先向作者确认，不得直接按缺陷处理。**
> `token_stats.rs:1048` 把「**实时聚合，不缓存、不落库**」写成了**设计属性**（断言式表述），
> 但**未给出理由**，且来自初始提交、无增量历史可追。
> - 若是**刻意**（大概率是为避免「落库聚合值与真实日志不一致」这类陈旧状态）⇒ **可加内存缓存、但不可落盘**，
>   且必须明确失效条件。
> - 若只是**描述现状** ⇒ 那就是未做的优化。
>
> **可以先做、且无争议的那一半**：`ide_workspace_meta` 被调在会话循环体内（`token_stats.rs:719`），
> 同一份 `index.json` 被读 N 次 —— 这是**纯冗余**，与「是否缓存聚合结果」无关，可独立先改。

**证据（已核实）**
- `crates/buddy-switch-core/src/modules/token_stats.rs:439-507` — `source()` 先 `files()` 递归 `read_dir` 收集全部 `.jsonl`（`:440-441`），再对**每个文件** `File::open` + `BufReader::lines()`，**逐行** `serde_json::from_str::<Value>(&line)`（`:468`）。
- `token_stats.rs:1048-1077` 注释自认 `region=all` 是**实时聚合、不缓存、不落库**。
- `token_stats.rs:612-642` / `:719` — `ide_workspace_meta()` 每次 `read_to_string` 整个 workspace `index.json` 再遍历找 id，而它被调在**每个会话 path 的循环体内** ⇒ 同 workspace 下 N 个会话把同一份索引**读解析 N 次**。

**影响**：`~/.workbuddy/projects` 等日志目录随使用无限增长，每次打开统计页都全量重扫重建整棵 `Value`；IDE 源部分叠加 O(会话 × 索引) 重复解析。

**修复方案**
1. **按 `(path, mtime, size)` 缓存单文件聚合结果**（`SourceCollector` 的 per-session 部分），缓存键含 region。
2. `ide_workspace_meta` 的索引解析**提到循环外**：按 workspace 目录建一次 `HashMap<conv_id, (title, model)>`。
3. 追加式 jsonl 可记录 offset 做**增量解析**（收益更大，但复杂度高，可作第二步）。
4. 用精简 `#[derive(Deserialize)]` 结构体替代 `Value`（仅取需要的字段）—— 收益明显，但**风险最高**（代码里用 `value.get("aiTitle")` 等动态取值 + `record_project` 可能读多种字段名），**必须先把被读字段全部枚举清楚**，建议单独一轮。

**⚠️ 不破坏功能的关键约束**
1. **等价性是唯一验收标准**：缓存前后输出必须**逐字节相同**。
2. **mtime 精度**：同秒内多次写入可能 mtime 不变 ⇒ 缓存键必须含 `size`，或提供手动失效入口。
3. 标题/摘要的读取顺序语义（`token_stats.rs:472-481`：`aiTitle` 优先于 `summary`，且**不受时间 cutoff 影响**）必须原样保留。
4. 缓存必须能被测试重置，且按 region 分家。

**建议护栏**：新增「同一份日志连续解析两次，输出 `assert_eq!`」+「缓存命中路径与冷路径输出相同」的等价性测试。

---

### P0-5 静态资源无任何缓存头

**证据（已核实）**
- `crates/buddy-switch-server/src/api.rs:1855-1884` — `static_handler` 只设 `Content-Type`，**没有 `Cache-Control` / `ETag` / `Last-Modified`**。
- `api.rs:1877` — `Body::from(f.data.into_owned())` 每次把整个嵌入文件拷成新 `Vec`。

**影响**：浏览器每次刷新都**全量重传** 1.40MB JS + 86KB CSS。这是 webui 路径下最直接、最容易被用户感知的浪费。

**修复方案**
- 带内容哈希的 `assets/*` → `Cache-Control: public, max-age=31536000, immutable`。
- `index.html` → `Cache-Control: no-cache`（**必须**，否则更新后仍加载旧 chunk 引用，直接白屏）+ `ETag`。
- `into_owned()` 的拷贝收益有限（1.4MB memcpy ≈ 0.2ms 量级），**优先级低**，不必为它引入复杂度。

**⚠️ 不破坏功能的关键约束**
1. **`index.html` 绝不能长缓存** —— SPA 深链回退也走 `index.html`（`api.rs:1858-1871`），缓存它会同时破坏「更新生效」和「深链」两件事。
2. SPA 回退时 `Content-Type` 必须按**实际被服务的资源名**推导（`api.rs:1860-1865` 的注释说明了为什么），加缓存头时别顺手改成按请求路径推导。
3. 哈希资源与 `index.html` 要**分别判定**，别一刀切。

---

## 3. P1 — 值得做，中等改动

### P1-1 WorkBuddy 网关：每请求 5 次全局写锁

**证据（已核实）** `crates/buddy-switch-gateway/src/routes/relay.rs` — 单个请求内对 `state.pool.write().await` 加锁：`:84`（同步账号）、`:121`（acquire）、`:147`（release）、`:157`（note_success）、`:176`（apply_upstream_error）。`Pool` 位于 `Arc<RwLock<Pool>>`（`state.rs:248`），**全局写锁把所有请求的选号串行化**。

**修复**：① 账号同步从「每请求」改为**节流/事件驱动**（间隔或按账号库 mtime）；② `acquire`/`release` 这类小状态更新改为原子量或分片锁；③ `select_account` 走读锁。**约束**：`sync_accounts` 的「增量 upsert、不删除既有账号以保留治理历史」语义（`relay.rs:81`）必须保留。

### P1-2 core / gateway 在 async 里直接做阻塞 IO

**证据（已核实）** `spawn_blocking` 在 `buddy-switch-core` 与 `buddy-switch-gateway` 中**各 0 处**，而 `buddy-switch-server/api.rs` 有 15 处。已定位的阻塞点：`trae/handlers.rs:503-513`（async 中调 `list_account_views_for`）、`trae/checkin.rs:360-374`（async 中调 `entries_for` / `list_account_views_for` / `load_cooldowns_for` / `load_remaining_for`，循环内还有 `device::ensure_for_variant` 阻塞 fs）。

**影响**：阻塞 tokio worker 线程；多账号并发签到/刷新时线程池被占满，整体吞吐下降。
**修复**：把同步 IO 包进 `tokio::task::spawn_blocking`。**约束**：`spawn_blocking` 要求 `'static`，需把数据 `Arc` 化；注意别把「持锁跨 await」引入进来。

### P1-3 Tauri：55 / 81 个命令是同步的

> **复核定性（详见复核报告）**：**确认为遗漏，且判据是明文约定** —— `commands.rs:90-92` 与 `:150-151`
> 自己写着判据：「**会起子进程**」或「**慢 IO**」必须 async + spawn_blocking，理由是「避免阻塞主线程造成页面卡顿」。
> **⇒ 55 个不能一刀切，必须分级**：
> - **该改**（慢 IO）：`get_trae_logs`(`:1306+` 全量读日志)、`list_sessions`(`:483`)、`import_local`(`:262`)、
>   `get_trae_accounts`(`:1275`)、`get_trae_variants`(`:1260` 含进程探测)、`check_auth_permission`(`:338` 含写盘)。
> - **不必改**（微秒级配置读取，改 async 反而多一次线程调度）：`get_switch_config`、`get_schedule_config`、
>   `get_auto_checkin_config` 等。
>
> 原报告「55 个同步」的表述易被读成「55 个都该改」，此处更正。

**证据（已核实）** 81 个命令中 26 个 async、**55 个同步**。同文件 `:88-94`（`get_status`）、`:152-157`、`:171-175`、`:633`、`:1333` 已按上述约定走 `spawn_blocking` —— 说明做慢 IO 却仍同步的那些是**遗漏而非设计**。

**影响**：文件 IO 跑在 Tauri 主线程，账号页频繁刷新时可能卡 UI。
**修复**：按 `:633` 的既有模式改 `async` + `spawn_blocking`，**按上表分级**。**约束**：**命令名与参数名不能变**（`scripts/check-api-contract.cjs` 校验「`api.ts` ROUTES ←→ Tauri invoke ←→ server 路由」三方一致 + 「单 `Value` 参数命令必须恰好传该键」）—— 改完**必须跑门禁**，且注意门禁的「（提示）」行。

### P1-4 粘性会话表：★ **本条原判有误，已撤回**（详见复核报告）

**原判**：「`gc` 从不被调度 ⇒ `bindings` 无界增长 ⇒ 长跑内存泄漏」。

**复核事实**：`crates/buddy-switch-gateway/src/sticky.rs` 的公开方法 `get` / `bind` / `unbind` / `gc`，
在全仓（排除自身）**一个都没被调用**；唯一外部引用是 `routes/status.rs:29` 的 `state.sticky.read().await.len()`
⇒ 该表**恒为空**，`sticky_sessions` 永远是 `0`。

**更正后的结论**：**不存在内存泄漏**（无增长）。真实情况是**整套粘性会话机制未接线** —— 属未完成/预留功能。
按本项目「不保留死代码」纪律，应明确去留：**接上写路径**，或**连 `sticky_ttl_ms` 配置项与 status 字段一并移除**。

**若决定接上**：`server/main.rs:33-57` 的「后台任务注册表」纪律要求 —— 任何后台循环**必须登记进
`BackgroundTask` 枚举**（注释原文：「登记即执行——不要在本表之外直接 `tokio::spawn` 后台循环」），
且已有测试守住该表。**约束**：GC 不得驱逐未过期绑定（单测 `gc_drops_only_expired_entries` 已锁定）。

### P1-5 official_usage 缓存命中时深拷贝整个大 payload

**证据（已核实）** `crates/buddy-switch-core/src/modules/official_usage.rs:237-249` 命中缓存即 `cached.clone()`；`:251-256` 写入再 clone 一次。注释 `:63-64` 述 payload 含全账号 `requests[]` 明细，约 600KB/账号。

**修复**：缓存改 `Arc<Value>`，命中时只克隆 `Arc`（引用计数 +1），消除每次统计请求的整块深拷贝。**约束**：调用方若需要 `&mut`，改在拿到 `Arc` 后再 `Arc::try_unwrap` 或按需 clone 一次。

### P1-6 前端：整 store 订阅 + 无门控的 2 秒轮询

> **复核定性（详见复核报告）—— 本条必须拆成两条，结论相反：**
>
> **(a) 2 秒轮询本身 = 有意为之，不得擅动。** 它是**用户可开关的功能**：
> `TraeSettingsPage.tsx:133` `const [autoRefresh, setAutoRefresh] = useState(true)`，界面上有开关；
> 由 `e8c90a7 perf: Trae 分区切换不再整页重载` 引入，且带说明意图的注释
> （「自动刷新只在『没有未提交的输入』时跑：否则用户正在输入关键字…看起来像在抖」）。
> ⇒ 这是有意的「日志实时跟随」，**2s 是产品选择，擅自拉长间隔会改变用户可见行为**。
>
> **(b) 缺 `visibilitychange` 门控 = 真遗漏。** 仓库里**已有两套正确实现可对照**：
> `use-workbuddy-status-refresh.ts:79-82`（visibilitychange + focus + blur）、
> `use-credit-auto-refresh.ts:69-77`（visibilitychange + Tauri `main-window-visible` 事件）；
> 而 `TraeSettingsPage.tsx:163-167` 与 `AccountsPage.tsx:427-439` **都没有**。
> ⇒ 「窗口不可见时暂停轮询」**不改变任何用户可见行为**（用户看不到时本就无需刷新），
> 且现成模式可直接复用（连 Tauri 的窗口可见事件通道都备好了）⇒ **安全的遗漏修复**。
> **但间隔数值不要动。**

**证据（已核实）**
- `src/pages/CreditStatsPage.tsx:1475` — `useAccountsStore()` **未传 selector**，订阅整个 store。而该 store 每 30 分钟刷新积分、每 60 秒刷新状态，且 `loadCredits` 按账号逐个 `setState` ⇒ 每次都会让这个 1900+ 行、含多个 recharts 图表的页面**整页重渲染**。
- `src/pages/TraeSettingsPage.tsx:165` — `setInterval(() => void refresh(), 2000)`，`refresh()` 走 `force: true`（`use-cached-resource.ts:62-65`）⇒ 每 2 秒强制发一次 `get_trae_logs`，且**没有 `visibilitychange` 门控**（窗口最小化/切后台仍持续请求）。
- `src/pages/AccountsPage.tsx:427-439` — 60 秒旅行状态轮询同样**无门控**。

**修复**：① `CreditStatsPage` 拆成多条带 selector 的订阅（或 `useShallow` 合并为稳定对象）；② 给 `TraeSettingsPage` 与 `AccountsPage` 的轮询**加可见性门控**（照抄上面两套现成实现），**间隔保持原值不变**。

### P1-7 出站请求体多轮 parse / serialize

**证据（已核实）** `crates/buddy-switch-gateway/src/outbound/mod.rs:75` `serde_json::from_str::<Value>` → 改写 → `:98` `.to_string()`；上游路径 `core/modules/upstream.rs` 另有一轮。大 prompt（工具定义多）时 CPU 显著。
**修复**：合并为**单次解析 → 原地改写 → 单次序列化**；无改写需求时**直接透传原始字节**。
**约束**：`outbound/mod.rs:351+` 有一批断言产出 JSON 结构的测试（含 `prompt_cache_key`、effort 调整、tool_call 序列），**必须全绿**。

### P1-8 `dir_stats` 每次概览递归整个快照树

**证据（已核实）** `crates/buddy-switch-core/src/modules/trae/profile.rs:1000-1010` 递归 `dir_stats`；`:1035` 对每个槽位调用；`overview_for`（`:1847` 附近）每次调用。
**影响**：快照含 `User/globalStorage` 等大树，每次概览遍历全部账号快照的完整文件树。
**修复**：按槽位 mtime 缓存 size/count，或列表视图只读顶层 mtime。

---

## 4. P2 — 低风险顺手清理

> **⚠️ 复核提醒（详见复核报告）**：下表中**两条已改判**——
> - 「缓存默认每次挂载重校验（`freshMs` 默认 0）」：**有意为之，不是缺陷**。`use-cached-resource.ts:9-18`
>   的文档表把「已有快照 → 立刻渲染真数据 + 后台重校验」写成核心语义，即 **stale-while-revalidate**，
>   是「切分区不闪骨架」的实现手段。设 `freshMs` 属**调优取舍**（拿「少一次请求」换「最长 30s 陈旧」），
>   **应由产品决定，不该由性能审计单方面改**。
> - 「账号列表排序在 render 体内 / 零 `React.memo`」：**证据不足，降级为待实测**。全仓 `React.memo` 0 处
>   说明**不是刻意规避**（无相关约定/注释），但也**没有实测证据**表明重渲染构成瓶颈
>   （账号数通常为个位数到几十）。**需先用 React Profiler 确认，再动。**

| 项 | 位置（已核实） | 修复 |
|---|---|---|
| 账号列表排序在 render 体内 ⚠️待实测 | `src/pages/AccountsPage.tsx:714-739`（IIFE，无 `useMemo`） | 先用 Profiler 确认，再包 `useMemo` |
| 全仓零 `React.memo` ⚠️待实测 | `src/` 计 0 处；`components/account-card.tsx:246` 为普通组件 | 同上，先量化再定 |
| 缓存 `freshMs` 默认 0 ⚠️**有意为之** | `src/lib/use-cached-resource.ts:55` | **不是缺陷**，仅产品层面的调优取舍 |
| 账号页 N+1 请求 | `AccountsPage.tsx:94-115`（每账号一次 `getCheckinStatus`）、`:118-139`（每账号一次 `getTravelStatus`）、`:431` 每 60s 全量重放 | 后端加批量接口；短期先拉长间隔 + 加可见性门控 |
| 缓存默认每次挂载重校验 ⚠️**已并入上表，此行为原判，已撤回** | — | 见上表与复核报告 |
| `existing_windows_drives` 26 次 `Path::exists` | `process.rs:573-577` | 改 Win32 `GetLogicalDrives` 位图或加缓存 |
| Vec 去重 O(n²) | `process.rs:434-437`、`:462-464`、`credit_usage.rs:266-267`、`:305-306` | 改 `HashSet` |
| `p95_latency` 每次 clone + sort | `trae/token_stats.rs:72-82` | 排序一次复用，或近似分位 |
| 大图直接进 bundle | `src/assets/donate-wechat.png` 136KB、`donate-alipay.jpg` 129KB、`workbuddy-official-icon.png` 136KB、`codebuddy-cn-ide-icon.png` 108KB | 转 webp；`donate-*` 改 `import()` 懒加载 |
| 全量字体包 | `src/main.tsx:3` 引入整个 `@fontsource-variable/bricolage-grotesque`（仅侧栏标题一处用，`App.tsx:437`） | 改 `.../latin.css` 或按 weight 子集引入 |
| SSE 解析器二次复杂度 | `trae/sse.rs:97-99`：`buffer.iter().position()` 每次从 0 扫 + 命中后 `drain(..=index).collect()` 每行一次 memmove | 用游标偏移 / `VecDeque` 代替反复 `drain`，保留尾部残行 |
| 出站体每次 attempt 复制 | `trae/routes.rs:485` `.body(body.to_vec())` | 用 `Bytes` 共享而非 `to_vec` |
| 托盘菜单全量重建 + 读盘 | `src-tauri/src/tray.rs:393-400`（每次重建整个菜单）、`:405` 调 `checkin::all_accounts_checked_in_today()` | 复用 `Menu`，只 `set_text`/`set_checked` 可变项；`checked_in` 结果加短缓存 |
| 命令式驱动日志全量读 | `trae/logs.rs:104-113`、`:139-160` | 按 mtime 缓存解析结果 |
| 配置路径每次读 env | `config.rs:74-79`、`:127-146`、`:217-219` | `OnceLock` 缓存（测试可重置） |

**已核对无问题、请勿「优化」**：`App.tsx:185`（60s 侧栏圆点）与 `App.tsx:311`（30 分钟检查更新）均**已有 `active` 门控**，属合理设计；`switch-account-dialog.tsx:105` 的 600ms 轮询仅在 webui 分支且生命周期极短，收益有限。

---

## 5. 待实测确认（本次未取到真机数据）

以下结论依赖数据规模或线程模型，**未实测前不得当作「已确认」**：

1. **P0-2 的 recharts 占比（复核后新增，优先级最高）**：需先拆一次 chunk 或跑可视化分析，拿到 recharts / radix / 其余 的真实字节数，再决定是否值得引入懒加载的复杂度。
2. **P0-4 的实际耗时**：强依赖 `~/.workbuddy/projects`、`CodeBuddyExtension/Data` 的日志总量。需在真机计时确认，再决定是否值得做增量解析。（且**须先向作者确认「不缓存」是刻意还是现状**。）
3. **P1-8 的 `dir_stats` 成本**：取决于 `profiles/*` 快照的文件数，需先统计量级。
4. **P1-3 分级后各命令的真实耗时**：需用命令耗时日志确认哪些同步命令确实慢（而不是凭「它做了 IO」推断）。
5. **P0-3 的设备派生成本占比**：需 profile 确认「哈希派生」与「文件 parse」谁是大头，以决定先做哪一项。
6. **前端 `memo` / 排序的收益**：需 React Profiler 确认重渲染确实是瓶颈（账号数通常为个位数到几十）。

---

## 6. 分批实施路线图

每批**可独立提交、独立验证**，批次内改动互不依赖。**已按复核结论调整**（见批注）。

| 批次 | 内容 | 风险 | 验收方式 |
|---|---|---|---|
| **B0（新增）** | 纯量化，不改代码：拆 chunk 看 recharts 占比；真机计时 token 统计与 `dir_stats`；确认 P0-4 的「不缓存」意图 | 无 | 拿到数字后再决定 B2/B4 是否值得做 |
| **B1** | P0-1 第一步（全量 tasklist，**优先 `wait_windows_pids_gone`**）+ P0-5（缓存头）+ P1-6b（前端**只加可见性门控**，不动间隔）+ P1-6c（store selector） | 低 | 对照测试 + 前端手测 + `check:api` |
| **B2** | P0-2（懒加载 + manualChunks）—— **依赖 B0 的量化结论** | 低 | 构建产物体积对比 + 截图核对 + 重编宿主 + 嵌入护栏 |
| **B3** | P0-3（设备派生缓存 + mtime 缓存） | 中 | CN/Global 隔离 + 写后可见 三组护栏 |
| **B4** | P0-4 中**无争议的那半条**（`ide_workspace_meta` 提出循环外）+ P1-2（`api.rs:1582` 与邻居对齐） | 低 | 等价性测试（逐字节相同）+ 现有测试全绿 |
| **B5** | P1-1（网关锁粒度）+ P1-2 其余 + P1-5（`Arc` 缓存） | 中高 | 网关 e2e + 并发压测 |
| **B6** | P1-3（**仅慢 IO 的那些**命令异步化） | 中 | `check:api` 门禁 + 桌面端手测 |
| **B7** | P2 清单（顺手清理；`memo`/排序**需先有 Profiler 证据**） | 低 | 现有测试全绿 |
| **B8** | P1-4：决定粘性机制**去留**（接上 / 移除）—— 与性能无关，属清理 | 中 | 若接上需登记后台任务表 |
| **B6** | P1-3（Tauri 命令异步化，55 个，可分批） | 中 | `check-api` 门禁 + 桌面端手测 |
| **B7** | P2 清单（顺手清理） | 低 | 现有测试全绿 |

**通用纪律（本项目已固化，务必遵守）**
- cargo 一律串行 `-j 1` + `CARGO_INCREMENTAL=0`；构建/测试用 Bash。
- 改前端后**必须重编宿主**，且重编前**先停掉在跑的 serve**（否则 `LNK1104` 是锁冲突不是代码错误）。
- 反向验证的注入窗口尽量短，残留变异标记提交前必须为 0。
- 若多会话并行改同一仓库，先看 `mtime` 与改动集合是否超出自己这一轮，**别 `git checkout` 别人的文件**。

---

## 7. 「不破坏功能」约束汇总（改之前先读这一节）

| 修复 | 绝不能改的语义 |
|---|---|
| P0-1 进程枚举 | `None => true`（查询失败视为运行中）；精确映像名匹配；全量 CSV 列位 |
| P0-2 懒加载 | `Suspense` 必须在路由层；`dist`/`dist-demo` base 差异；嵌入资源护栏 |
| P0-3 网关池 | 账号库按 region 分家；`sync_for` 校正 `self.variant`；写后立即可见 |
| P0-4 统计缓存 | 输出逐字节等价；`aiTitle` 优先且不受 cutoff 影响；缓存可重置 |
| P0-5 缓存头 | `index.html` 必须 `no-cache`；回退时 MIME 按实际资源名推导 |
| P1-1 网关锁 | 增量 upsert、不删账号（保留治理历史） |
| P1-3 Tauri 命令 | 命令名与参数名不变（`check-api-contract.cjs` 门禁） |
| P1-4 粘性机制（原判「GC 泄漏」**已撤回**） | 若接上写路径：GC 不驱逐未过期绑定；后台循环**必须登记进 `BackgroundTask` 枚举**（`server/main.rs:33-57`「登记即执行」） |
| P1-5 Arc 缓存 | 调用方需要 `&mut` 时的处理路径 |
| **P1-6a 2s 轮询（有意为之）** | **间隔数值不得改动**（用户可开关的「日志实时跟随」功能）；只可加可见性门控 |
| **M3 `freshMs=0`（有意为之）** | **不得按缺陷修改**；改 `freshMs` 属产品层面的调优取舍 |

---

*本报告为纯静态分析 + 本机实测，未修改任何项目文件。*
