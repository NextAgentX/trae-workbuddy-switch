/**
 * 文案域：**Trae 产品线 · 统计与 API 服务页**（英文）。
 *
 * ⚠️ 不写类型注解：键名合法性由 `src/locales/en.ts` 单点校验。域文件保持零依赖。
 * 键必须与 `traeStats.zh.ts` **一一对应**（缺哪条就回落中文）。
 */
export const en = {
  // =====================================================================
  // TraeTokenStatsPage.tsx — Token usage page
  // =====================================================================
  "trae.stats.token.title": "Token usage",
  "trae.stats.token.subtitle": "Aggregated usage of calls that went through the local Trae gateway.",
  "trae.stats.token.loadFailed": "Cannot read token statistics",
  "trae.stats.token.retry": "Retry",
  "trae.stats.token.refresh": "Refresh",

  // ---- Data-source boundary ----
  "trae.stats.token.sourceTitle": "Data source",
  "trae.stats.token.sourceFallback": "Only calls that went through the local Trae gateway are counted.",

  // ---- Time window ----
  "trae.stats.token.range.7d": "Last 7 days",
  "trae.stats.token.range.30d": "Last 30 days",
  "trae.stats.token.range.90d": "Last 90 days",
  "trae.stats.token.range.all": "All time",
  "trae.stats.token.rangeAria": "Statistics range",

  // ---- Product-line scope bar ----
  "trae.stats.token.scopeTitle": "Product line",
  "trae.stats.token.scopeNote": "Filters the statistics below only; the time window is unchanged",
  "trae.stats.token.scope.unlabeled": "Unlabeled on this machine",
  "trae.stats.token.scope.unlabeledHint": "Older logs recorded before the upgrade, without a product line",
  "trae.stats.token.scope.cn": "CN",
  "trae.stats.token.scope.cnHint": "Calls attributed to the CN region (both client slots combined)",
  "trae.stats.token.scope.global": "Global",
  "trae.stats.token.scope.globalHint": "Calls attributed to the global region",
  "trae.stats.token.scope.all": "All",
  "trae.stats.token.scope.allHint": "Calls from every product line",

  // ---- Empty state ----
  "trae.stats.token.emptyTitle": "No gateway calls in the current window",
  "trae.stats.token.emptyBody": "Enable the gateway on the API service page and point your client's Base URL at it; calls will then be recorded here.",

  // ---- Summary card ----
  "trae.stats.token.summaryTitle": "Usage summary",
  "trae.stats.token.series.total": "Total tokens",
  "trae.stats.token.series.input": "Input",
  "trae.stats.token.series.output": "Output",
  "trae.stats.token.summary.inputOutputHint": "Input {input} · Output {output}",
  "trae.stats.token.summary.records": "Requests",
  "trae.stats.token.summary.errorsHint": "{count} of them failed",
  "trae.stats.token.summary.avgLatency": "Average latency",
  "trae.stats.token.summary.p95Latency": "P95 latency",
  "trae.stats.token.summary.streamRequests": "Streaming requests",
  "trae.stats.token.summary.activeAccounts": "Active accounts",

  // ---- Full-year heatmap ----
  "trae.stats.heatmap.title": "Token activity (last 12 months)",
  "trae.stats.heatmap.activeDays": "{count} active days",
  "trae.stats.heatmap.aria": "Heatmap of daily token activity over the last 12 months, {count} active days in total",
  "trae.stats.heatmap.cellAria": "{date}: {tokens} tokens used",
  "trae.stats.heatmap.tooltip": "{date}: {tokens} tokens used",
  "trae.stats.heatmap.tooltipCalls": " · {count} calls",

  // ---- Daily trend ----
  "trae.stats.token.trendTitle": "Daily usage trend",

  // ---- By model × by day ----
  "trae.stats.token.modelDaily.title": "By model × by day",
  "trae.stats.token.modelDaily.note": "Stacked bars = tokens per day (split by model); the line = calls that day",
  "trae.stats.token.modelDaily.empty": "No per-model, per-day data to show yet",
  "trae.stats.token.modelDaily.calls": "Calls",
  "trae.stats.token.modelDaily.callsAxis": "Calls (right axis)",

  // ---- By model ----
  "trae.stats.token.byModel": "By model",
  "trae.stats.token.byModelEmpty": "No model data yet",
  "trae.stats.token.times": "{count} calls",
  "trae.stats.token.failed": "{count} failed",

  // ---- By account ----
  "trae.stats.token.byAccount": "By account",
  "trae.stats.token.byAccountEmpty": "No account data yet",

  // ---- Response status distribution ----
  "trae.stats.token.statusTitle": "Response status distribution",

  // ---- Dimensions the platform cannot provide ----
  "trae.stats.token.unsupportedTitle": "Dimensions not supported by the platform",
  "trae.stats.token.unsupportedOnly": " ({on} only)",

  // ---- Log parse failures ----
  "trae.stats.token.parseErrors": "{count} records in the log could not be parsed (missing timestamp) and were skipped.",

  // =====================================================================
  // TraeCreditsPage.tsx — Credits page
  // =====================================================================
  "trae.stats.credits.title": "Credits",
  "trae.stats.credits.subtitle": "Remaining credits, daily change and history for each Trae account.",
  "trae.stats.credits.sync": "Sync credits",
  "trae.stats.credits.syncing": "Syncing",
  "trae.stats.credits.refreshDone": "Credits refreshed",
  "trae.stats.credits.refreshFailed": "Failed to refresh",
  "trae.stats.credits.loadFailed": "Cannot read Trae credits",
  "trae.stats.credits.overviewAria": "Credits overview",

  // ---- Trend series ----
  "trae.stats.credits.series.total": "Total credits",
  "trae.stats.credits.series.earned": "Credits earned",
  "trae.stats.credits.series.consumed": "Credits spent",

  // ---- Overview metrics ----
  "trae.stats.credits.metric.total": "Total available credits",
  "trae.stats.credits.metric.totalHint": "All accounts combined",
  "trae.stats.credits.metric.average": "Average available credits",
  "trae.stats.credits.metric.averageHint": "Total ÷ number of accounts",
  "trae.stats.credits.metric.accounts": "Accounts",
  "trae.stats.credits.metric.accountsHint": "{count} synced",
  "trae.stats.credits.metric.todayEarned": "Credits earned today",
  "trae.stats.credits.metric.todayConsumed": "Credits spent today",
  "trae.stats.credits.cacheUpdated": "Credits cache updated: {time}",
  "trae.stats.credits.historyDays": "History kept for {days} days",

  // ---- Trend section ----
  "trae.stats.credits.trendTitle": "Credits trend over the last {days} days",
  "trae.stats.credits.trendNote": "Data comes from daily credits snapshots; a point for today appears once a check-in or credits sync has run.",
  "trae.stats.credits.trendEmptyAccounts": "No account data yet. Add an account and the credits trend shows up here.",
  "trae.stats.credits.trendEmptyData": "No trend data yet. Run a check-in or “Sync credits” once to see the daily change.",

  // ---- Dimensions the platform cannot provide ----
  "trae.stats.credits.unsupportedTitle": "Dimensions not supported by the platform",
  "trae.stats.credits.unsupportedOnly": " ({on} only)",

  // ---- Per-account credits breakdown ----
  "trae.stats.credits.detailTitle": "Per-account credits",
  "trae.stats.credits.detailNote": "The data comes from check-in snapshots and “Sync credits”. Trae has no “request usage that produced these credits” metric, so there is no split view.",
  "trae.stats.credits.detail.byAccount": "By account",
  "trae.stats.credits.detail.accountCount": "{count} accounts",
  "trae.stats.credits.detail.empty": "No account data yet.",
  "trae.stats.credits.col.rank": "Rank",
  "trae.stats.credits.col.account": "Account",
  "trae.stats.credits.col.group": "Group",
  "trae.stats.credits.col.expires": "Credits expire",
  "trae.stats.credits.col.remaining": "Remaining credits",
  "trae.stats.credits.ungrouped": "Ungrouped",
  "trae.stats.credits.notSynced": "Not synced",

  // =====================================================================
  // TraeApiServicePage.tsx — API service page
  // =====================================================================
  "trae.stats.api.title": "API service",
  "trae.stats.api.subtitle": "Serve Trae's model quota to local tools through an OpenAI-compatible API.",
  "trae.stats.api.loadFailed": "Cannot read gateway status",
  "trae.stats.api.retry": "Retry",
  "trae.stats.api.refresh": "Refresh",

  // ---- Gateway card ----
  "trae.stats.api.gateway.enable": "Enable API gateway",
  "trae.stats.api.gateway.desc": "Once enabled, local AI tools can call Trae models through the address below (upstream {upstream}).",
  "trae.stats.api.gateway.bindAddr": "Listen address",
  "trae.stats.api.gateway.bindLoopback": "127.0.0.1 (this machine only)",
  "trae.stats.api.gateway.bindLan": "0.0.0.0 (LAN)",
  "trae.stats.api.gateway.port": "Listen port",
  "trae.stats.api.gateway.statusLabel": "Status:",
  "trae.stats.api.gateway.running": "Running",
  "trae.stats.api.gateway.stopped": "Stopped",
  "trae.stats.api.gateway.openDataDir": "Open data directory",
  "trae.stats.api.gateway.noteLead": "The default port is 7864, deliberately clear of the WorkBuddy gateway (57891) — both serve ",
  "trae.stats.api.gateway.noteTail": ".",
  "trae.stats.api.gateway.lanWarning": "In LAN mode, any device on the same network that gets hold of a key can spend your Trae credits.",
  "trae.stats.api.gateway.lastError": "Most recent error",

  // ---- Endpoint card ----
  "trae.stats.api.endpoint.title": "Endpoint",
  "trae.stats.api.endpoint.baseUrl": "Base URL",
  "trae.stats.api.copy": "Copy",
  "trae.stats.api.endpoint.copied": "Base URL copied",
  "trae.stats.api.endpoint.keyPrefix": "Latest key prefix",
  "trae.stats.api.endpoint.noKey": "No usable key yet",

  // ---- Action feedback ----
  "trae.stats.api.toast.savedStarted": "Gateway saved and started",
  "trae.stats.api.toast.savedDisabled": "Gateway saved (listening disabled)",
  "trae.stats.api.toast.saveFailed": "Failed to save",
  "trae.stats.api.toast.portInvalid": "Invalid port",
  "trae.stats.api.toast.portRange": "Enter a whole number between 1 and 65535",
  "trae.stats.api.toast.logsCleared": "Logs cleared",
  "trae.stats.api.toast.clearFailed": "Failed to clear",
  "trae.stats.api.toast.unsupported": "Not supported on this platform",
  "trae.stats.api.toast.unsupportedDesc": "This capability is only available on Windows.",
  "trae.stats.api.toast.dirOpened": "Trae data directory opened",
  "trae.stats.api.toast.dirOpenFailed": "Failed to open the data directory",

  // ---- LAN risk confirmation ----
  "trae.stats.api.lanDialog.title": "Allow LAN access?",
  "trae.stats.api.lanDialog.desc": "Once listening on 0.0.0.0, any device on the same network — public Wi-Fi included — can spend your Trae credits as long as it holds an API key. Only enable this on a network you trust.",
  "trae.stats.api.lanDialog.cancel": "Cancel",
  "trae.stats.api.lanDialog.confirm": "I understand, enable it",
};
