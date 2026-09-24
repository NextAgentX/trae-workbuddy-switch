/**
 * 文案域：**跨切面共享文案**（中文，键的权威之一）。
 *
 * 归属文件（`lib/` 与 `stores/` 里的**用户可见**文案，以及两个产品共用的展示组件）：
 * `src/lib/region.ts`、`src/lib/trae-variant-status.ts`、`src/lib/trae-client.ts`、
 * `src/lib/gateway.ts`、`src/lib/trae-gateway.ts`、`src/lib/use-cached-resource.ts`、
 * `src/lib/types.ts`、`src/lib/trae-types.ts`、`src/lib/demo-mode.ts`、
 * `src/lib/update.ts`、`src/lib/clipboard.ts`、`src/lib/stacked-bar-visuals.ts`、
 * `src/stores/resources.ts`、`src/stores/accounts.ts`、`src/stores/gateway.ts`、
 * `src/components/product-marks.tsx`、`src/components/region-bar.tsx`、
 * `src/components/demo-action.tsx`
 *
 * ⚠️ 这些文件大多是**模块级常量表**（如「状态 → 显示标签」的映射）。必须把
 * **键**存进表里（`labelKey: "shared.status.x"`），渲染处再 `t(...)` ——
 * 存中文会让整张表在语言切换时失效（表在模块加载时就定型了）。
 *
 * 键前缀：`shared.`
 */
export const zh = {
  // ---- 版本区域（region.ts / trae-types.ts / trae-variant-status.ts） ----
  "shared.region.version.cn": "国内版",
  "shared.region.version.global": "国际版",
  "shared.region.gateway.cn": "国内版 (WorkBuddy)",
  "shared.region.gateway.global": "国际版 (WorkBuddy AI)",
  "shared.region.filter.all": "合并",

  // ---- 统计范围切换条（region-bar.tsx） ----
  "shared.region.scope.aria": "统计范围",

  // ---- Trae 网关账号池状态（trae-gateway.ts） ----
  "shared.trae.poolStatus.available": "可用",
  "shared.trae.poolStatus.cooling": "冷却中",
  "shared.trae.poolStatus.disabled": "会话失效",
  "shared.trae.poolStatus.expired": "积分过期",
  "shared.trae.poolStatus.noCredits": "零积分",

  // ---- 剪贴板（clipboard.ts） ----
  "shared.clipboard.copied": "已复制",
  "shared.clipboard.failed": "复制失败，请手动选择后复制",

  // ---- 演示模式遮罩（demo-action.tsx） ----
  "shared.demo.unavailable": "演示模式下不可操作",

  // ---- HTTP 通道自身抛出的错误（api.ts） ----
  // 这些错误的产生者就是前端（拿不到路由 / 连不上本地服务 / 非 2xx），
  // 后端无从为它们提供码，因此直接走词表。
  "shared.api.unsupportedInWebui": "webui 模式暂不支持该操作: {cmd}",
  "shared.api.unreachable": "无法连接 Buddy Switch 服务（{base}），请先运行 `buddy-switch`",
  "shared.api.requestFailed": "请求失败 ({status})",
  // ---- 权限检测（api.ts） ----
  "shared.api.webuiPermissionByProcess": "webui 模式由服务进程（终端启动）的权限决定，无需额外授权",
  "shared.api.accountNotFound": "未找到账号",

  // =====================================================================
  // ---- 演示夹具（screenshot-demo.ts：演示站 / 截图站的假数据） ----
  // =====================================================================
  // ⚠️ 这些文案**只在演示模式下出现**，但演示站本身就是给人看的一站，
  // 因此同样要中英两版。模型名、协议键、路径、uid 刻意不入表（它们是契约）。
  // ---- 假账号 ----
  "shared.demo.account.a": "测试 A",
  "shared.demo.account.aRemark": "DS4.1 额度 10/03 解禁",
  "shared.demo.account.b": "测试 B",
  "shared.demo.account.c": "测试 C",
  // ---- 积分包名 ----
  "shared.demo.pack.fission": "CodeBuddy 个人版国内运营裂变包",
  "shared.demo.pack.credits": "CodeBuddy 个人版积分包",
  "shared.demo.pack.trial": "CodeBuddy 新用户体验包",
  "shared.demo.pack.checkin": "CodeBuddy 签到赠送积分",
  "shared.demo.pack.activity": "CodeBuddy 活动奖励积分",
  // ---- 自动旅行 ----
  "shared.demo.travel.cafe": "咖啡馆",
  "shared.demo.travel.gym": "健身房",
  // ---- 自动轮换日志 ----
  "shared.demo.rotate.reasonPinned": "当前账号仍是积分到期最紧迫的可用账号",
  "shared.demo.rotate.reasonExpiring": "目标账号积分将在 5 天内到期",
  // ---- Token 统计 ----
  "shared.demo.session.tokenDashboard": "完善 Token 统计仪表盘与本地用量分析",
  "shared.demo.session.agentAcp": "设计 Agent 与 ACP 管理设置",
  "shared.demo.session.characterAudio": "补全角色成语双音频",
  "shared.demo.session.accountCard": "统一账号卡片视觉和交互",
  // ---- 模型目录 ----
  "shared.demo.catalog.limitedFree": "限时免费",
  "shared.demo.catalog.promo": "促销",
  "shared.demo.catalog.globalStale": "上游接口可能已变更，当前展示上次成功缓存",
  // ---- 账号策略 ----
  "shared.demo.strategy.realtime": "请求时实时择优",
  // ---- Trae：假账号名 / 分组 / 套餐 / 冷却 ----
  "shared.demo.trae.name.main": "主号",
  "shared.demo.trae.name.altA": "小号 A",
  "shared.demo.trae.name.altB": "小号 B",
  "shared.demo.trae.group.spare": "备用",
  "shared.demo.trae.pack.month": "Work 月度包",
  "shared.demo.trae.pack.checkinBonus": "签到赠送包",
  "shared.demo.trae.pack.cnPlan": "国内套餐包",
  "shared.demo.trae.pack.trial": "试用包",
  "shared.demo.trae.cooldown.rateLimited": "请求过于频繁，请稍后再试",
  "shared.demo.trae.cooldown.sessionDead": "会话已失效，需重新登录",
  "shared.demo.trae.cooldown.reasonRateLimited": "请求过于频繁",
  "shared.demo.trae.cooldown.reasonSessionDead": "会话已失效",
  // ---- Trae：签到 ----
  "shared.demo.trae.checkin.ok": "签到成功",
  "shared.demo.trae.checkin.already": "今日已签到",
  "shared.demo.trae.checkin.sessionDead": "会话已失效，需重新登录",
  "shared.demo.trae.warn.altBSessionDead": "小号 B 会话已失效，请重新登录后再签到",
  // ---- Trae：程序位展示名（`label` / `nameAlias`） ----
  "shared.demo.trae.program.work": "TraeWork",
  "shared.demo.trae.program.workCn": "TraeWork CN",
  "shared.demo.trae.program.code": "TraeCode",
  "shared.demo.trae.program.codeCn": "TraeCode CN",
  "shared.demo.trae.program.workAi": "TraeWork AI",
  "shared.demo.trae.program.workGlobal": "TraeWork",
  "shared.demo.trae.program.codeAi": "Trae AI",
  // 「待实测」是**状态标注**，不是品牌名 —— 它是这条目唯一需要翻译的部分。
  "shared.demo.trae.program.codePending": "TraeCode（待实测）",
  // ---- Trae：API Key 名 / 网关诊断 / 日志 ----
  "shared.demo.trae.key.cursorCn": "Cursor (国内版)",
  "shared.demo.trae.key.cherryGlobal": "Cherry (国际版)",
  "shared.demo.trae.key.legacy": "旧 Key（升级迁移）",
  "shared.demo.trae.diagnose.main": "主号(7481920:可用,积分=120)",
  "shared.demo.trae.diagnose.altA": "小号 A(7481999:冷却中,积分=65)",
  "shared.demo.trae.diagnose.altB": "小号 B(7482044:会话失效（需重新登录）,积分=8)",
  "shared.demo.trae.gatewayUpstreamError": "账号「{name}」上游失败（HTTP {status}）",
  "shared.demo.trae.tokenSource": "Trae API 网关",
  "shared.demo.trae.tokenNote": "只统计经过本机 Trae 网关的调用；直接在 Trae IDE 里对话不产生记录。",
  "shared.demo.trae.logNote": "只读取本机纯文本运行日志（app / checkin / switcher）；网关请求日志在「网关请求日志」标签页。",
  "shared.demo.trae.log.app": "运行",
  "shared.demo.trae.log.checkin": "签到",
  "shared.demo.trae.log.switch": "切换",
  // 运行日志正文（**逐字对齐 `store::append_log` 的真实格式**：演示行与真机行必须同形）
  "shared.demo.trae.log.checkinDone": "签到完成: 成功 {ok}/已签到 {already}/失败 {failed}/总计 {total}",
  "shared.demo.trae.log.thawed": "自动解冻账号 {uid}: 剩余积分 {credits}，冷却已清除",
  "shared.demo.trae.log.savedLogin": "保存登录态: user={uid} 文件数={count}",
  "shared.demo.trae.log.jwtRefreshed": "JWT 自动刷新成功: user={uid} 新到期={hours}h",
  "shared.demo.trae.log.deviceReset": "设备标识重置完成: {count} 项生效",
  // ---- Trae：平台做不到的维度（置灰卡） ----
  "shared.demo.cap.travel": "自动旅行",
  "shared.demo.cap.travelReason": "Trae 客户端没有该活动接口，本工具也无对应后端实现。",
  "shared.demo.cap.cli": "CodeBuddy CLI / IDE 接入",
  "shared.demo.cap.cliReason": "CodeBuddy 属 WorkBuddy 生态，Trae 分区不提供该客户端的接入与切换。",
  "shared.demo.cap.migration": "会话 / 记忆 / 连接器迁移",
  "shared.demo.cap.migrationReason": "Trae 登录态是一组 Cloud-IDE-JWT 文件，没有会话树 / 记忆 / 连接器对象可迁移。",
  "shared.demo.cap.sessionTree": "会话列表 / 复制会话 / 切换进度流",
  "shared.demo.cap.sessionTreeReason": "Trae 的账号切换是文件级快照替换，不存在会话列表与切换进度事件流。",
  "shared.demo.cap.officialByModel": "官方积分消耗按模型",
  "shared.demo.cap.officialByModelReason": "Trae 积分只来自签到快照，不存在「产生这些积分的请求用量」这一口径的数据源。",
  "shared.demo.cap.cacheMetrics": "缓存读取 / 写入 / 命中率",
  "shared.demo.cap.cacheMetricsReason": "Trae 网关日志与上传链路都没有 cache 字段，上游也不回传——无从记录",
  "shared.demo.cap.projectDimension": "按项目维度统计",
  "shared.demo.cap.projectDimensionReason": "网关日志的 project_id / session_id 是每请求新生成的 uuid，不对应客户端项目",
  "shared.demo.cap.sessionCost": "调用最贵的会话",
  "shared.demo.cap.sessionCostReason": "无稳定会话标识，无法把多次请求归并成一个会话成本",
  // ---- 演示模式的写操作应答（DemoAction 拦截后仍要给出「像真的」的回执） ----
  "shared.demo.switchDone": "演示切换已完成",
  "shared.demo.updateTitle": "更新提示演示",
  "shared.demo.error.accountMissing": "账号不存在",
  "shared.demo.error.missingReadOnly": "演示模式缺少只读数据: {command}",
  // ---- 拼接用的标点与分隔符 ----
  // ⚠️ 这些是「语言相关」的排版符号，不能硬编码：中文用全角（：，；（））、
  // 英文用半角（: , ; ( )）。硬编码的症状是「英文界面里冒出中文标点」，
  // 例如 "Primary：已登录"，且不会被「残留中文」扫描抓到（只占一两个字符）。
  "shared.punct.colon": "：",
  "shared.punct.comma": "，",
  "shared.punct.semicolon": "；",
  "shared.punct.period": "。",
  "shared.punct.openParen": "（",
  "shared.punct.closeParen": "）",
} as const;
