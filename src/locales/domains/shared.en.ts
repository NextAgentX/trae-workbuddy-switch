/**
 * 文案域：**跨切面共享文案**（英文）。
 *
 * ⚠️ 不写类型注解：键名合法性由 `src/locales/en.ts` 单点校验。域文件保持零依赖。
 * 键必须与 `shared.zh.ts` **一一对应**（缺哪条就回落中文）。
 */
export const en = {
  // ---- Version regions (region.ts / trae-types.ts / trae-variant-status.ts) ----
  "shared.region.version.cn": "CN version",
  "shared.region.version.global": "Global version",
  "shared.region.gateway.cn": "CN version (WorkBuddy)",
  "shared.region.gateway.global": "Global version (WorkBuddy AI)",
  "shared.region.filter.all": "Combined",

  // ---- Statistics scope bar (region-bar.tsx) ----
  "shared.region.scope.aria": "Statistics scope",

  // ---- Trae gateway account pool status (trae-gateway.ts) ----
  "shared.trae.poolStatus.available": "Available",
  "shared.trae.poolStatus.cooling": "Cooling",
  "shared.trae.poolStatus.disabled": "Session expired",
  "shared.trae.poolStatus.expired": "Credits expired",
  "shared.trae.poolStatus.noCredits": "No credits",

  // ---- Clipboard (clipboard.ts) ----
  "shared.clipboard.copied": "Copied",
  "shared.clipboard.failed": "Copy failed — select the text and copy it manually",

  // ---- Demo mode guard (demo-action.tsx) ----
  "shared.demo.unavailable": "Not available in demo mode",

  "shared.api.unsupportedInWebui": "Not supported in webui mode: {cmd}",
  "shared.api.unreachable":
    "Cannot reach the Buddy Switch service ({base}). Start it with `buddy-switch` first.",
  "shared.api.requestFailed": "Request failed ({status})",
  "shared.api.webuiPermissionByProcess":
    "In webui mode permissions come from the service process (started in a terminal); no extra authorization is needed",
  "shared.api.accountNotFound": "Account not found",

  // =====================================================================
  // ---- Demo fixtures (screenshot-demo.ts: fake data for the demo/screenshot site) ----
  // =====================================================================
  // Model names, protocol keys, paths and uids stay out of the word list — they are contracts.
  // ---- Fake accounts ----
  "shared.demo.account.a": "Test A",
  "shared.demo.account.aRemark": "DS4.1 quota unlocks on 10/03",
  "shared.demo.account.b": "Test B",
  "shared.demo.account.c": "Test C",
  // ---- Credit package names ----
  "shared.demo.pack.fission": "CodeBuddy Personal CN Growth Pack",
  "shared.demo.pack.credits": "CodeBuddy Personal Credits Pack",
  "shared.demo.pack.trial": "CodeBuddy New User Pack",
  "shared.demo.pack.checkin": "CodeBuddy Check-in Bonus Credits",
  "shared.demo.pack.activity": "CodeBuddy Campaign Reward Credits",
  // ---- Auto travel ----
  "shared.demo.travel.cafe": "Cafe",
  "shared.demo.travel.gym": "Gym",
  // ---- Auto-rotate log ----
  "shared.demo.rotate.reasonPinned": "The current account is still the available one with the most urgent credit expiry",
  "shared.demo.rotate.reasonExpiring": "The target account's credits expire within 5 days",
  // ---- Token statistics ----
  "shared.demo.session.tokenDashboard": "Polish the token statistics dashboard and local usage analysis",
  "shared.demo.session.agentAcp": "Design Agent and ACP management settings",
  "shared.demo.session.characterAudio": "Complete the character idiom dual audio",
  "shared.demo.session.accountCard": "Unify account card visuals and interactions",
  // ---- Model catalog ----
  "shared.demo.catalog.limitedFree": "Limited-time free",
  "shared.demo.catalog.promo": "Promo",
  "shared.demo.catalog.globalStale": "The upstream API may have changed; showing the last successful cache",
  // ---- Account strategy ----
  "shared.demo.strategy.realtime": "Chosen at request time",
  // ---- Trae: fake account names / group / packages / cooldown ----
  "shared.demo.trae.name.main": "Primary",
  "shared.demo.trae.name.altA": "Alt A",
  "shared.demo.trae.name.altB": "Alt B",
  "shared.demo.trae.group.spare": "Spare",
  "shared.demo.trae.pack.month": "Work Monthly Pack",
  "shared.demo.trae.pack.checkinBonus": "Check-in Bonus Pack",
  "shared.demo.trae.pack.cnPlan": "CN Plan Pack",
  "shared.demo.trae.pack.trial": "Trial Pack",
  "shared.demo.trae.cooldown.rateLimited": "Too many requests, please try again later",
  "shared.demo.trae.cooldown.sessionDead": "Session expired, please sign in again",
  "shared.demo.trae.cooldown.reasonRateLimited": "Rate limited",
  "shared.demo.trae.cooldown.reasonSessionDead": "Session expired",
  // ---- Trae: check-in ----
  "shared.demo.trae.checkin.ok": "Check-in succeeded",
  "shared.demo.trae.checkin.already": "Already checked in today",
  "shared.demo.trae.checkin.sessionDead": "Session expired, please sign in again",
  "shared.demo.trae.warn.altBSessionDead": "Alt B's session expired; sign in again before checking in",
  // ---- Trae: program display names (`label` / `nameAlias`) ----
  "shared.demo.trae.program.work": "TraeWork",
  "shared.demo.trae.program.workCn": "TraeWork CN",
  "shared.demo.trae.program.code": "TraeCode",
  "shared.demo.trae.program.codeCn": "TraeCode CN",
  "shared.demo.trae.program.workAi": "TraeWork AI",
  "shared.demo.trae.program.workGlobal": "TraeWork",
  "shared.demo.trae.program.codeAi": "Trae AI",
  "shared.demo.trae.program.codePending": "TraeCode (untested)",
  // ---- Trae: API key names / gateway diagnostics / logs ----
  "shared.demo.trae.key.cursorCn": "Cursor (CN)",
  "shared.demo.trae.key.cherryGlobal": "Cherry (Global)",
  "shared.demo.trae.key.legacy": "Legacy key (migrated on upgrade)",
  "shared.demo.trae.diagnose.main": "Primary(7481920:available,credits=120)",
  "shared.demo.trae.diagnose.altA": "Alt A(7481999:cooling,credits=65)",
  "shared.demo.trae.diagnose.altB": "Alt B(7482044:session expired (sign in again),credits=8)",
  "shared.demo.trae.gatewayUpstreamError": "Account \"{name}\" failed upstream (HTTP {status})",
  "shared.demo.trae.tokenSource": "Trae API gateway",
  "shared.demo.trae.tokenNote": "Counts only calls through the local Trae gateway; chatting directly in the Trae IDE produces no records.",
  "shared.demo.trae.logNote": "Reads only plain-text runtime logs (app / checkin / switcher); gateway request logs live in the \"Gateway request logs\" tab.",
  "shared.demo.trae.log.app": "Runtime",
  "shared.demo.trae.log.checkin": "Check-in",
  "shared.demo.trae.log.switch": "Switch",
  // Runtime log lines (mirroring the exact `store::append_log` formats)
  "shared.demo.trae.log.checkinDone": "Check-in done: ok {ok}/already {already}/failed {failed}/total {total}",
  "shared.demo.trae.log.thawed": "Auto-thawed account {uid}: {credits} credits left, cooldown cleared",
  "shared.demo.trae.log.savedLogin": "Saved login state: user={uid} files={count}",
  "shared.demo.trae.log.jwtRefreshed": "JWT auto-refresh succeeded: user={uid} new expiry={hours}h",
  "shared.demo.trae.log.deviceReset": "Device identifier reset complete: {count} items applied",
  // ---- Trae: unsupported capabilities (greyed-out cards) ----
  "shared.demo.cap.travel": "Auto travel (cat dispatch)",
  "shared.demo.cap.travelReason": "The Trae client has no API for that activity, and this tool has no matching backend implementation.",
  "shared.demo.cap.cli": "CodeBuddy CLI / IDE integration",
  "shared.demo.cap.cliReason": "CodeBuddy belongs to the WorkBuddy ecosystem; the Trae section does not offer integration or switching for that client.",
  "shared.demo.cap.migration": "Session / memory / connector migration",
  "shared.demo.cap.migrationReason": "Trae's login state is a set of Cloud-IDE-JWT files; there are no session tree / memory / connector objects to migrate.",
  "shared.demo.cap.sessionTree": "Session list / copy session / switch progress stream",
  "shared.demo.cap.sessionTreeReason": "Trae account switching swaps a file-level snapshot; there is no session list or switch progress event stream.",
  "shared.demo.cap.officialByModel": "Official credit usage by model",
  "shared.demo.cap.officialByModelReason": "Trae credits come only from check-in snapshots; there is no data source for \"the request usage that produced these credits\".",
  "shared.demo.cap.cacheMetrics": "Cache read / write / hit rate",
  "shared.demo.cap.cacheMetricsReason": "Neither the Trae gateway logs nor the upload path has cache fields, and the upstream does not return them — there is nothing to record",
  "shared.demo.cap.projectDimension": "Statistics by project",
  "shared.demo.cap.projectDimensionReason": "The gateway log's project_id / session_id are freshly generated uuids per request and do not correspond to client projects",
  "shared.demo.cap.sessionCost": "Most expensive session",
  "shared.demo.cap.sessionCostReason": "No stable session identifier, so multiple requests cannot be merged into one session cost",
  // ---- Demo mode write-action replies ----
  "shared.demo.switchDone": "Demo switch completed",
  "shared.demo.updateTitle": "Update prompt demo",
  "shared.demo.error.accountMissing": "Account not found",
  "shared.demo.error.missingReadOnly": "Demo mode is missing read-only data: {command}",
  // ---- Punctuation & separators used for string concatenation ----
  "shared.punct.colon": ": ",
  "shared.punct.comma": ", ",
  "shared.punct.semicolon": "; ",
  "shared.punct.period": ". ",
  "shared.punct.openParen": " (",
  "shared.punct.closeParen": ")",
};
