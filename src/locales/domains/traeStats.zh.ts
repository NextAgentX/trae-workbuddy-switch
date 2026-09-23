/**
 * 文案域：**Trae 产品线 · 统计与 API 服务页**（中文，键的权威之一）。
 *
 * 归属文件：
 * `src/pages/TraeTokenStatsPage.tsx`、`src/pages/TraeCreditsPage.tsx`、
 * `src/pages/TraeApiServicePage.tsx`
 *
 * ⚠️ 命名空间用 `trae`（不是 `traeStats`）：与另两个 Trae 域共用前缀、靠二级段区分
 * （`trae.page.*` / `trae.comp.*` / `trae.gateway.*` / `trae.stats.*`），
 * 因为它们本就是同一产品线的文案。
 *
 * 键前缀：`trae.stats.`
 */
export const zh = {
  // =====================================================================
  // TraeTokenStatsPage.tsx —— 「Token 统计」页
  // =====================================================================
  "trae.stats.token.title": "Token 统计",
  "trae.stats.token.subtitle": "汇总经过本机 Trae 网关的调用用量。",
  "trae.stats.token.loadFailed": "无法读取 Token 统计",
  "trae.stats.token.retry": "重试",
  "trae.stats.token.refresh": "刷新",

  // ---- 数据源边界提示 ----
  "trae.stats.token.sourceTitle": "数据来源",
  "trae.stats.token.sourceFallback": "只统计经过本机 Trae 网关的调用。",

  // ---- 时间窗口 ----
  "trae.stats.token.range.7d": "近 7 天",
  "trae.stats.token.range.30d": "近 30 天",
  "trae.stats.token.range.90d": "近 90 天",
  "trae.stats.token.range.all": "全部",
  "trae.stats.token.rangeAria": "统计范围",

  // ---- 版本（产品线）范围条 ----
  "trae.stats.token.scopeTitle": "版本范围",
  "trae.stats.token.scopeNote": "仅筛选下方统计口径，不改变时间窗口",
  "trae.stats.token.scope.unlabeled": "本机未标注",
  "trae.stats.token.scope.unlabeledHint": "升级前未带产品线归属的旧日志",
  "trae.stats.token.scope.cn": "国内版",
  "trae.stats.token.scope.cnHint": "归属国内区域的调用（两条程序位合计）",
  "trae.stats.token.scope.global": "国际版",
  "trae.stats.token.scope.globalHint": "归属国际版区域的调用",
  "trae.stats.token.scope.all": "全部",
  "trae.stats.token.scope.allHint": "所有产品线的调用",

  // ---- 空态 ----
  "trae.stats.token.emptyTitle": "当前窗口内没有任何网关调用",
  "trae.stats.token.emptyBody": "到「API 服务」页启用网关，再把客户端的 Base URL 指过来，调用就会记在这里。",

  // ---- 汇总卡 ----
  "trae.stats.token.summaryTitle": "用量汇总",
  "trae.stats.token.series.total": "总 Token",
  "trae.stats.token.series.input": "输入",
  "trae.stats.token.series.output": "输出",
  "trae.stats.token.summary.inputOutputHint": "输入 {input} · 输出 {output}",
  "trae.stats.token.summary.records": "请求数",
  "trae.stats.token.summary.errorsHint": "其中失败 {count}",
  "trae.stats.token.summary.avgLatency": "平均耗时",
  "trae.stats.token.summary.p95Latency": "P95 耗时",
  "trae.stats.token.summary.streamRequests": "流式请求",
  "trae.stats.token.summary.activeAccounts": "活跃账号",

  // ---- 整年热力网格 ----
  "trae.stats.heatmap.title": "Token 活动（最近一年）",
  "trae.stats.heatmap.activeDays": "{count} 个活跃日",
  "trae.stats.heatmap.aria": "最近一年每日 Token 活动热力图，共 {count} 个活跃日",
  "trae.stats.heatmap.cellAria": "{date}使用了 {tokens} 个 Token",
  "trae.stats.heatmap.tooltip": "{date} 使用了 {tokens} 个 Token",
  "trae.stats.heatmap.tooltipCalls": " · {count} 次调用",

  // ---- 每日趋势 ----
  "trae.stats.token.trendTitle": "每日用量趋势",

  // ---- 按模型 × 按天 ----
  "trae.stats.token.modelDaily.title": "按模型 × 按天",
  "trae.stats.token.modelDaily.note": "堆叠柱＝每日 Token（按模型拆分）；折线＝当日调用次数",
  "trae.stats.token.modelDaily.empty": "暂无可展示的按模型按天数据",
  "trae.stats.token.modelDaily.calls": "调用次数",
  "trae.stats.token.modelDaily.callsAxis": "调用次数（右轴）",

  // ---- 按模型 ----
  "trae.stats.token.byModel": "按模型",
  "trae.stats.token.byModelEmpty": "暂无模型数据",
  "trae.stats.token.times": "{count} 次",
  "trae.stats.token.failed": "失败 {count}",

  // ---- 按账号 ----
  "trae.stats.token.byAccount": "按账号",
  "trae.stats.token.byAccountEmpty": "暂无账号数据",

  // ---- 响应状态码分布 ----
  "trae.stats.token.statusTitle": "响应状态分布",

  // ---- 平台不支持的维度 ----
  "trae.stats.token.unsupportedTitle": "平台不支持的维度",
  "trae.stats.token.unsupportedOnly": "（仅 {on}）",

  // ---- 日志解析失败 ----
  "trae.stats.token.parseErrors": "日志中有 {count} 条记录无法解析（缺少时间戳），已跳过。",

  // =====================================================================
  // TraeCreditsPage.tsx —— 「积分统计」页
  // =====================================================================
  "trae.stats.credits.title": "积分统计",
  "trae.stats.credits.subtitle": "查看每个 Trae 账号的剩余积分、每日变化与历史趋势。",
  "trae.stats.credits.sync": "同步积分",
  "trae.stats.credits.syncing": "同步中",
  "trae.stats.credits.refreshDone": "积分数据已刷新",
  "trae.stats.credits.refreshFailed": "刷新失败",
  "trae.stats.credits.loadFailed": "无法读取 Trae 积分数据",
  "trae.stats.credits.overviewAria": "积分总览",

  // ---- 趋势图系列 ----
  "trae.stats.credits.series.total": "积分总数",
  "trae.stats.credits.series.earned": "获得积分",
  "trae.stats.credits.series.consumed": "消耗积分",

  // ---- 总览指标 ----
  "trae.stats.credits.metric.total": "可用积分总额",
  "trae.stats.credits.metric.totalHint": "全部账号合计",
  "trae.stats.credits.metric.average": "平均可用积分",
  "trae.stats.credits.metric.averageHint": "总额 ÷ 账号数",
  "trae.stats.credits.metric.accounts": "账号数",
  "trae.stats.credits.metric.accountsHint": "已同步 {count}",
  "trae.stats.credits.metric.todayEarned": "今日新增积分",
  "trae.stats.credits.metric.todayConsumed": "今日消耗积分",
  "trae.stats.credits.cacheUpdated": "积分缓存更新时间：{time}",
  "trae.stats.credits.historyDays": "历史明细保留 {days} 天",

  // ---- 趋势区 ----
  "trae.stats.credits.trendTitle": "近 {days} 日积分趋势",
  "trae.stats.credits.trendNote": "数据来自每日积分快照；执行签到或同步积分后才会产生当天的点。",
  "trae.stats.credits.trendEmptyAccounts": "尚无账号数据。添加账号后这里会展示积分趋势。",
  "trae.stats.credits.trendEmptyData": "暂无趋势数据。执行一次签到或「同步积分」后即可看到每日变化。",

  // ---- 平台不支持的维度 ----
  "trae.stats.credits.unsupportedTitle": "平台不支持的维度",
  "trae.stats.credits.unsupportedOnly": "（仅 {on}）",

  // ---- 账号积分明细 ----
  "trae.stats.credits.detailTitle": "账号积分明细",
  "trae.stats.credits.detailNote": "数据来源为签到快照与「同步积分」的结果；Trae 侧不存在「产生这些积分的请求用量」口径，故不设分栏。",
  "trae.stats.credits.detail.byAccount": "按账号",
  "trae.stats.credits.detail.accountCount": "共 {count} 个账号",
  "trae.stats.credits.detail.empty": "尚无账号数据。",
  "trae.stats.credits.col.rank": "排名",
  "trae.stats.credits.col.account": "账号",
  "trae.stats.credits.col.group": "分组",
  "trae.stats.credits.col.expires": "积分到期",
  "trae.stats.credits.col.remaining": "剩余可用积分",
  "trae.stats.credits.ungrouped": "未分组",
  "trae.stats.credits.notSynced": "未同步",

  // =====================================================================
  // TraeApiServicePage.tsx —— 「API 服务」页
  // =====================================================================
  "trae.stats.api.title": "API 服务",
  "trae.stats.api.subtitle": "把 Trae 的模型额度以 OpenAI 兼容接口提供给本机工具。",
  "trae.stats.api.loadFailed": "无法读取网关状态",
  "trae.stats.api.retry": "重试",
  "trae.stats.api.refresh": "刷新",

  // ---- 网关卡 ----
  "trae.stats.api.gateway.enable": "启用 API 网关",
  "trae.stats.api.gateway.desc": "开启后本机 AI 工具可通过下方地址调用 Trae 模型（上游 {upstream}）。",
  "trae.stats.api.gateway.bindAddr": "监听地址",
  "trae.stats.api.gateway.bindLoopback": "127.0.0.1（仅本机）",
  "trae.stats.api.gateway.bindLan": "0.0.0.0（局域网）",
  "trae.stats.api.gateway.port": "监听端口",
  "trae.stats.api.gateway.statusLabel": "状态：",
  "trae.stats.api.gateway.running": "运行中",
  "trae.stats.api.gateway.stopped": "已停止",
  "trae.stats.api.gateway.openDataDir": "打开数据目录",
  "trae.stats.api.gateway.noteLead": "默认端口 7864，与 WorkBuddy 网关（57891）错开——两者都占用 ",
  "trae.stats.api.gateway.noteTail": "。",
  "trae.stats.api.gateway.lanWarning": "局域网模式下，同网段任何设备拿到 Key 都能消耗你的 Trae 积分。",
  "trae.stats.api.gateway.lastError": "最近一次错误",

  // ---- 接入地址卡 ----
  "trae.stats.api.endpoint.title": "接入地址",
  "trae.stats.api.endpoint.baseUrl": "Base URL",
  "trae.stats.api.copy": "复制",
  "trae.stats.api.endpoint.copied": "Base URL 已复制",
  "trae.stats.api.endpoint.keyPrefix": "最近 Key 前缀",
  "trae.stats.api.endpoint.noKey": "尚无可用 Key",

  // ---- 操作结果提示 ----
  "trae.stats.api.toast.savedStarted": "网关已保存并启动",
  "trae.stats.api.toast.savedDisabled": "网关已保存（未启用监听）",
  "trae.stats.api.toast.saveFailed": "保存失败",
  "trae.stats.api.toast.portInvalid": "端口无效",
  "trae.stats.api.toast.portRange": "请输入 1–65535 之间的整数",
  "trae.stats.api.toast.logsCleared": "日志已清空",
  "trae.stats.api.toast.clearFailed": "清空失败",
  "trae.stats.api.toast.unsupported": "当前平台不支持",
  "trae.stats.api.toast.unsupportedDesc": "该能力仅在 Windows 提供。",
  "trae.stats.api.toast.dirOpened": "已打开 Trae 数据目录",
  "trae.stats.api.toast.dirOpenFailed": "打开数据目录失败",

  // ---- 局域网风险确认 ----
  "trae.stats.api.lanDialog.title": "允许局域网访问？",
  "trae.stats.api.lanDialog.desc": "监听 0.0.0.0 后，同网段（含公共 Wi-Fi）的任何设备只要拿到 API Key，就能消耗你的 Trae 积分。请只在可信网络下开启。",
  "trae.stats.api.lanDialog.cancel": "取消",
  "trae.stats.api.lanDialog.confirm": "我已确认，开启",
} as const;
