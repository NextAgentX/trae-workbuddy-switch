import { invoke } from "@tauri-apps/api/core";
import type {
  AccountMeta,
  AccountRecord,
  AccountStrategy,
  AccountStrategyMap,
  ApiKeyRecord,
  AppStatus,
  AutoRotateConfig,
  CatalogSnapshot,
  CodeBuddyCliInstallResult,
  CodeBuddyCliStatus,
  CodeBuddyCliSwitchResult,
  CodeBuddyCnIdeStatus,
  CodeBuddyCnIdeSwitchResult,
  CheckinConfig,
  CheckinLog,
  CheckinResult,
  CreateApiKeyResult,
  CreditExpiry,
  CreditStatistics,
  TokenStatistics,
  CopyResult,
  MigrateResult,
  GatewayConfig,
  GatewayLogEntry,
  GatewayStatus,
  GithubConfig,
  ImportPreviewAccount,
  ImportResult,
  OAuthPollResult,
  OAuthStartResult,
  Region,
  RegionFilter,
  RotateLog,
  RotateStatus,
  ScheduleConfig,
  ScheduleRunResult,
  Session,
  SwitchConfig,
  SwitchResult,
  TravelConfig,
  TravelStatus,
  UpdateInfo,
} from "./types";
import { demoModeEnabled, demoUnavailableMessage } from "./demo-mode";
import { localizeCodedStrings, localizeError } from "./error-code";
import { t } from "./i18n";
import { screenshotDemoResponse } from "./screenshot-demo";
import type {
  TraeAccount,
  TraeAccountsOverview,
  TraeApiKeyCreated,
  TraeApiKeyRecord,
  TraeCapabilities,
  TraeCheckinReport,
  TraeCheckinStatus,
  TraeCreditsOverview,
  TraeDeviceResetReport,
  TraeEnvStatus,
  TraeExportRecord,
  TraeGatewayConfigRaw,
  TraeGatewayLogEntry,
  TraeGatewayStatus,
  TraeGroup,
  TraeImportPreview,
  TraeLogQuery,
  TraeLogsResponse,
  TraeOAuthPollResult,
  TraeOAuthStartResult,
  TraeProfilesOverview,
  TraeSettings,
  TraeSwitchOutcome,
  TraeTokenScope,
  TraeLegacyMergeReport,
  TraeTokenStatistics,
  TraeVariantId,
  TraeVariantsStatus,
} from "./trae-types";

/**
 * 双通道适配层：
 * - 桌面 App（Tauri）：`invoke` 调用 Rust commands
 * - webui（浏览器）：HTTP fetch 调用本地 buddy-switch 服务（127.0.0.1）
 */
const API_BASE = "http://127.0.0.1:57890";

const DEMO_READ_COMMANDS = new Set([
  "get_status", "get_accounts", "get_codebuddy_cli_status", "get_codebuddy_cn_ide_status", "get_checkin_status",
  "get_credit_expiry", "get_credit_statistics", "get_auto_checkin_config",
  "get_token_statistics",
  "get_checkin_logs", "get_auto_rotate_config", "rotate_status", "get_rotate_logs",
  "get_github_config", "check_update", "get_launch_at_login_enabled", "switch_progress",
  "get_travel_status", "get_auto_travel_config", "get_schedule_config",
  "get_switch_config",
  // API 网关只读命令（演示站需返回虚构数据，否则 build:demo 报错）
  "get_gateway_config", "gateway_status", "list_api_keys", "get_gateway_models",
  "get_account_strategy", "get_gateway_logs",
  // Trae 分区只读命令（演示站需返回虚构数据，否则 build:demo 报错）。
  // 只登记**只读**命令：写操作（签到 / 增删账号 / 切换 / 重置设备…）一律不进这里，
  // 由 `DemoAction` 包裹后在演示模式下统一提示不可操作。
  "get_trae_env", "get_trae_variants", "get_trae_capabilities", "get_trae_accounts",
  "get_trae_checkin_status",
  "get_trae_credits", "get_trae_token_statistics", "get_trae_logs", "get_trae_profiles",
  "get_trae_settings", "get_trae_gateway_config", "trae_gateway_status",
  "get_trae_gateway_models", "list_trae_api_keys", "get_trae_gateway_logs",
]);

export function isDemoMode(): boolean {
  return demoModeEnabled;
}

export function isWebui(): boolean {
  return typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window);
}

/** Tauri mobile 也注入内部 API；用现有平台 UA 约定把桌面宿主与移动宿主区分开。 */
function isMobilePlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  const ua = navigator.userAgent;
  return (
    /Android|iPhone|iPad|iPod/i.test(ua) ||
    (ua.includes("Macintosh") && navigator.maxTouchPoints > 1)
  );
}

/** 是否为提供桌面专属能力的 Tauri 宿主。 */
export function isDesktop(): boolean {
  return !isWebui() && !isMobilePlatform();
}

type Route = { method: "GET" | "POST"; path: string };

/** Tauri command → HTTP 路由映射（webui 模式）。 */
const ROUTES: Record<string, Route> = {
  get_status: { method: "GET", path: "/api/status" },
  get_accounts: { method: "GET", path: "/api/accounts" },
  get_codebuddy_cli_status: { method: "GET", path: "/api/codebuddy-cli/status" },
  install_codebuddy_cli_helper: { method: "POST", path: "/api/codebuddy-cli/install-helper" },
  switch_codebuddy_cli_account: { method: "POST", path: "/api/codebuddy-cli/switch" },
  get_codebuddy_cn_ide_status: { method: "GET", path: "/api/codebuddy-cn-ide/status" },
  switch_codebuddy_cn_ide_account: { method: "POST", path: "/api/codebuddy-cn-ide/switch" },
  detect_codebuddy_cn_ide_account: { method: "POST", path: "/api/codebuddy-cn-ide/detect" },
  delete_account: { method: "POST", path: "/api/delete" },
  set_account_remark: { method: "POST", path: "/api/account/remark" },
  oauth_start: { method: "POST", path: "/api/oauth/start" },
  oauth_status: { method: "POST", path: "/api/oauth/status" },
  import_local: { method: "POST", path: "/api/import-local" },
  export_accounts: { method: "POST", path: "/api/export-accounts" },
  export_accounts_to_path: { method: "POST", path: "/api/export-accounts-to-path" },
  preview_import_accounts: { method: "POST", path: "/api/import/preview" },
  import_accounts: { method: "POST", path: "/api/import" },
  switch_account: { method: "POST", path: "/api/switch" },
  list_sessions: { method: "GET", path: "/api/sessions" },
  copy_sessions: { method: "POST", path: "/api/sessions/copy" },
  migrate_account_data: { method: "POST", path: "/api/migrate/account" },
  get_checkin_status: { method: "GET", path: "/api/checkin/status" },
  get_credit_expiry: { method: "POST", path: "/api/credits" },
  get_credit_statistics: { method: "GET", path: "/api/credits/stats" },
  get_token_statistics: { method: "GET", path: "/api/token-stats" },
  checkin: { method: "POST", path: "/api/checkin" },
  checkin_all: { method: "POST", path: "/api/checkin/all" },
  get_auto_checkin_config: { method: "GET", path: "/api/checkin/config" },
  save_auto_checkin_config: { method: "POST", path: "/api/checkin/config" },
  get_checkin_logs: { method: "GET", path: "/api/checkin/logs" },
  get_travel_status: { method: "GET", path: "/api/travel/status" },
  get_auto_travel_config: { method: "GET", path: "/api/travel/config" },
  save_auto_travel_config: { method: "POST", path: "/api/travel/config" },
  get_switch_config: { method: "GET", path: "/api/switch/config" },
  save_switch_config: { method: "POST", path: "/api/switch/config" },
  get_auto_rotate_config: { method: "GET", path: "/api/rotate/config" },
  save_auto_rotate_config: { method: "POST", path: "/api/rotate/config" },
  get_schedule_config: { method: "GET", path: "/api/schedule/config" },
  save_schedule_config: { method: "POST", path: "/api/schedule/config" },
  run_schedule_task: { method: "POST", path: "/api/schedule/run" },
  rotate_status: { method: "GET", path: "/api/rotate/status" },
  run_rotate: { method: "POST", path: "/api/rotate/run" },
  get_rotate_logs: { method: "GET", path: "/api/rotate/logs" },
  refresh_account_token: { method: "POST", path: "/api/refresh-token" },
  get_github_config: { method: "GET", path: "/api/update/config" },
  save_github_config: { method: "POST", path: "/api/update/config" },
  check_update: { method: "GET", path: "/api/update/check" },
  switch_progress: { method: "GET", path: "/api/switch/progress" },
  open_accounts_dir: { method: "POST", path: "/api/accounts/open-dir" },
  // API 网关（对照架构设计 A-3.7）
  get_gateway_config: { method: "GET", path: "/api/gateway/config" },
  save_gateway_config: { method: "POST", path: "/api/gateway/config" },
  gateway_status: { method: "GET", path: "/api/gateway/status" },
  list_api_keys: { method: "GET", path: "/api/gateway/keys" },
  create_api_key: { method: "POST", path: "/api/gateway/keys" },
  revoke_api_key: { method: "POST", path: "/api/gateway/keys/revoke" },
  delete_api_key: { method: "POST", path: "/api/gateway/keys/delete" },
  get_gateway_models: { method: "GET", path: "/api/gateway/models" },
  refresh_gateway_models: { method: "POST", path: "/api/gateway/models/refresh" },
  get_account_strategy: { method: "GET", path: "/api/gateway/strategy" },
  save_account_strategy: { method: "POST", path: "/api/gateway/strategy" },
  get_gateway_logs: { method: "GET", path: "/api/gateway/logs" },
  clear_gateway_logs: { method: "POST", path: "/api/gateway/logs/clear" },

  // ---- Trae 模块 ----
  // 键名必须与 src-tauri/src/lib.rs 的 invoke_handler 登记名、
  // 以及 crates/buddy-switch-server/src/api.rs 的路由 path+method 三方一致，
  // 由 scripts/check-api-contract.cjs 在构建前校验。
  get_trae_env: { method: "GET", path: "/api/trae/env" },
  get_trae_variants: { method: "GET", path: "/api/trae/variants" },
  get_trae_capabilities: { method: "GET", path: "/api/trae/capabilities" },
  get_trae_accounts: { method: "GET", path: "/api/trae/accounts" },
  get_trae_checkin_status: { method: "GET", path: "/api/trae/checkin/status" },
  get_trae_credits: { method: "GET", path: "/api/trae/credits" },
  get_trae_token_statistics: { method: "GET", path: "/api/trae/token-stats" },
  get_trae_logs: { method: "GET", path: "/api/trae/logs" },
  get_trae_profiles: { method: "GET", path: "/api/trae/profiles" },
  get_trae_settings: { method: "GET", path: "/api/trae/settings" },
  save_trae_settings: { method: "POST", path: "/api/trae/settings" },
  trae_add_account: { method: "POST", path: "/api/trae/accounts/add" },
  trae_update_account: { method: "POST", path: "/api/trae/accounts/update" },
  trae_delete_account: { method: "POST", path: "/api/trae/accounts/delete" },
  // 账号迁移：与 WorkBuddy 的 import_local / export_accounts* / import* 同构。
  // 命令名不与 WorkBuddy 侧重名（多 `trae_` 前缀），因此 ROUTES 里必须逐条列出。
  trae_import_local_account: { method: "POST", path: "/api/trae/accounts/import-local" },
  // Trae OAuth 登录：与 WorkBuddy 的 oauth_start / oauth_status 同构，但**不带 region**
  // （Trae 只有一套账号库与一套上游，见 crates/buddy-switch-core/src/modules/trae/mod.rs）。
  trae_oauth_start: { method: "POST", path: "/api/trae/oauth/start" },
  trae_oauth_status: { method: "POST", path: "/api/trae/oauth/status" },
  trae_oauth_cancel: { method: "POST", path: "/api/trae/oauth/cancel" },
  trae_export_accounts: { method: "POST", path: "/api/trae/accounts/export" },
  trae_export_accounts_to_path: { method: "POST", path: "/api/trae/accounts/export-to-path" },
  trae_preview_import_accounts: { method: "POST", path: "/api/trae/accounts/import/preview" },
  trae_import_accounts: { method: "POST", path: "/api/trae/accounts/import" },
  trae_group_op: { method: "POST", path: "/api/trae/groups" },
  trae_checkin: { method: "POST", path: "/api/trae/checkin" },
  trae_refresh_credits: { method: "POST", path: "/api/trae/credits/refresh" },
  trae_refresh_jwt: { method: "POST", path: "/api/trae/refresh-jwt" },
  trae_clear_cooldown: { method: "POST", path: "/api/trae/cooldown/clear" },
  trae_merge_legacy_regions: { method: "POST", path: "/api/trae/legacy-merge" },
  trae_switch_account: { method: "POST", path: "/api/trae/switch" },
  trae_save_login: { method: "POST", path: "/api/trae/login/save" },
  trae_backup_profile: { method: "POST", path: "/api/trae/profiles/backup" },
  trae_restore_profile: { method: "POST", path: "/api/trae/profiles/restore" },
  trae_delete_profile: { method: "POST", path: "/api/trae/profiles/delete" },
  trae_reset_device: { method: "POST", path: "/api/trae/device/reset" },
  // Trae API 网关管理面（网关本体走独立端口，见 src/pages/TraeApiServicePage）。
  get_trae_gateway_config: { method: "GET", path: "/api/trae/gateway/config" },
  save_trae_gateway_config: { method: "POST", path: "/api/trae/gateway/config" },
  trae_gateway_status: { method: "GET", path: "/api/trae/gateway/status" },
  get_trae_gateway_models: { method: "GET", path: "/api/trae/gateway/models" },
  // 多 Key 管理（含归属产品线）：GET 列表 / POST 创建同一路径。
  list_trae_api_keys: { method: "GET", path: "/api/trae/gateway/keys" },
  create_trae_api_key: { method: "POST", path: "/api/trae/gateway/keys" },
  revoke_trae_api_key: { method: "POST", path: "/api/trae/gateway/keys/revoke" },
  delete_trae_api_key: { method: "POST", path: "/api/trae/gateway/keys/delete" },
  // 打开 Trae 数据目录（非 Windows 返回结构化 Unsupported）。
  open_trae_data_dir: { method: "POST", path: "/api/trae/open-data-dir" },
  get_trae_gateway_logs: { method: "GET", path: "/api/trae/gateway/logs" },
  clear_trae_gateway_logs: { method: "POST", path: "/api/trae/gateway/logs/clear" },
};

function queryString(args?: Record<string, unknown>): string {
  if (!args) return "";
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(args)) {
    if (value === undefined || value === null) continue;
    params.set(key, String(value));
  }
  const text = params.toString();
  return text ? `?${text}` : "";
}

async function httpCall<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const route = ROUTES[cmd];
  if (!route) throw new Error(t("shared.api.unsupportedInWebui", { cmd }));
  let res: Response;
  try {
    const url =
      route.method === "GET"
        ? `${API_BASE}${route.path}${queryString(args)}`
        : `${API_BASE}${route.path}`;
    res = await fetch(url, {
      method: route.method,
      headers: { "Content-Type": "application/json" },
      body: route.method === "POST" ? JSON.stringify(args ?? {}) : undefined,
    });
  } catch {
    throw new Error(t("shared.api.unreachable", { base: API_BASE }));
  }
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(data.message || data.error || t("shared.api.requestFailed", { status: res.status }));
  }
  // 数据带上来的错误（`error` / `warning` 这类字段）在**这里**统一本地化，
  // 而不是靠每个渲染点自己记得剥结构尾 —— 见 `error-code.ts` 的说明。
  return localizeCodedStrings(data) as T;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (demoModeEnabled) {
    if (cmd === "get_credit_statistics" && args?.refresh === true) {
      throw new Error(demoUnavailableMessage());
    }
    if (!DEMO_READ_COMMANDS.has(cmd)) throw new Error(demoUnavailableMessage());
    return localizeCodedStrings(screenshotDemoResponse(cmd, args)) as T;
  }
  if (!isWebui()) return localizeCodedStrings(await invoke<T>(cmd, args)) as T;
  return httpCall<T>(cmd, args);
}

// ---------------------------------------------------------------------------
// 状态 / 账号
// ---------------------------------------------------------------------------

/** region 作为普通字段放进 args：GET 走 query、POST 走 JSON body（B-8.4）。缺省不传即后端按 cn 处理。 */
function regionArg(region?: Region): Record<string, unknown> {
  return region ? { region } : {};
}

export function getStatus(region?: Region): Promise<AppStatus> {
  return call("get_status", region ? { region } : undefined);
}

export function getAccounts(region?: Region): Promise<{ accounts: AccountMeta[] }> {
  return call("get_accounts", region ? { region } : undefined);
}

export function getCodebuddyCliStatus(): Promise<CodeBuddyCliStatus> {
  return call("get_codebuddy_cli_status");
}

export function installCodebuddyCliHelper(): Promise<CodeBuddyCliInstallResult> {
  return call("install_codebuddy_cli_helper");
}

export function switchCodebuddyCliAccount(accountId: string): Promise<CodeBuddyCliSwitchResult> {
  if (demoModeEnabled) {
    return new Promise((resolve, reject) => {
      window.setTimeout(() => {
        try {
          resolve(screenshotDemoResponse("switch_codebuddy_cli_account", { accountId }) as CodeBuddyCliSwitchResult);
        } catch (error) {
          reject(error);
        }
      }, 1200);
    });
  }
  return call("switch_codebuddy_cli_account", { accountId });
}

export function getCodebuddyCnIdeStatus(): Promise<CodeBuddyCnIdeStatus> {
  return call("get_codebuddy_cn_ide_status");
}

export function switchCodebuddyCnIdeAccount(
  accountId: string,
  restart = true,
): Promise<CodeBuddyCnIdeSwitchResult> {
  return call("switch_codebuddy_cn_ide_account", { accountId, restart });
}

export function detectCodebuddyCnIdeAccount(): Promise<{
  ok: boolean;
  found: boolean;
  matched?: boolean;
  accountId?: string;
  message?: string;
}> {
  return call("detect_codebuddy_cn_ide_account");
}


export function deleteAccount(accountId: string, region?: Region): Promise<{ ok: boolean }> {
  return call("delete_account", { accountId, ...regionArg(region) });
}

export function oauthStart(region?: Region): Promise<OAuthStartResult> {
  return call("oauth_start", region ? { region } : undefined);
}

export function oauthStatus(loginId: string, region?: Region): Promise<OAuthPollResult> {
  return call("oauth_status", { loginId, ...regionArg(region) });
}

export function importLocal(region?: Region): Promise<{ ok: boolean; account: AccountMeta }> {
  return call("import_local", region ? { region } : undefined);
}

export function exportAccounts(accountIds: string[], region?: Region): Promise<{ ok: boolean; accounts: AccountRecord[] }> {
  return call("export_accounts", { accountIds, ...regionArg(region) });
}

/** 桌面端：把完整记录写入用户选择的路径（系统保存对话框产物）。 */
export function exportAccountsToPath(
  accountIds: string[],
  path: string,
  region?: Region,
): Promise<{ ok: boolean; path: string }> {
  return call("export_accounts_to_path", { accountIds, path, ...regionArg(region) });
}

export function previewImportAccounts(
  fileText: string,
  region?: Region,
): Promise<{ accounts: ImportPreviewAccount[]; total: number }> {
  return call("preview_import_accounts", { fileText, ...regionArg(region) });
}

export function importAccounts(fileText: string, indexes: number[], region?: Region): Promise<ImportResult> {
  return call("import_accounts", { fileText, indexes, ...regionArg(region) });
}

export function switchAccount(args: {
  accountId: string;
  region?: Region;
  restart?: boolean;
  shareSessions?: boolean;
  copySessionIds?: string[];
  /** 会话复制的**来源**版本；缺省与 `region` 相同（同版本内切换）。 */
  sourceRegion?: Region;
}): Promise<SwitchResult> {
  return call("switch_account", args as unknown as Record<string, unknown>);
}

/**
 * 设置账号备注（**字段级更新**）。
 *
 * 刻意**不**提供「整条账号写回」的口子：前端手上只有脱敏的 `AccountMeta`，
 * 整条写回会把 `access_token` / `refresh_token` 一并抹掉（账号当场失效且无报错）。
 * 传空串即清空备注。
 */
export function setAccountRemark(
  accountId: string,
  remark: string,
  region?: Region,
): Promise<AccountMeta> {
  return call("set_account_remark", { accountId, remark, ...regionArg(region) });
}

/** 读取账号切换与账号列表展示配置（全局单份）。 */
export function getSwitchConfig(): Promise<SwitchConfig> {
  return call("get_switch_config");
}

export function saveSwitchConfig(config: SwitchConfig): Promise<SwitchConfig> {
  return call("save_switch_config", {
    config: config as unknown as Record<string, unknown>,
  });
}

/** 切换进度（webui 轮询用；桌面端走事件，此函数无副作用）。 */
export function switchProgress(): Promise<{ running: boolean; progress: string | null }> {
  return call("switch_progress");
}

export function listSessions(region?: Region): Promise<{
  sessions: Session[];
  current: string | null;
}> {
  return call("list_sessions", region ? { region } : undefined);
}

export function copySessions(
  targetAccountId: string,
  sessionIds: string[],
  region?: Region,
  sourceRegion?: Region,
): Promise<{
  sourceUid: string;
  targetUid: string;
  copied: CopyResult[];
  skipped?: CopyResult[];
  errors?: { id: string; error: string }[];
}> {
  return call("copy_sessions", {
    targetAccountId,
    sessionIds,
    ...regionArg(region),
    ...(sourceRegion ? { sourceRegion } : {}),
  });
}

/**
 * 把源账号的 Memory / Connector 合并到目标账号（带去重）。
 *
 * 只处理普通文件，不触碰 `workbuddy.db`，因此无需关闭 WorkBuddy。
 * `sourceAccountId` 缺省时取当前登录账号；`memory` / `connectors` 缺省均为 true。
 */
export function migrateAccountData(
  targetAccountId: string,
  options?: {
    sourceAccountId?: string;
    memory?: boolean;
    connectors?: boolean;
    region?: Region;
    /** 数据来源版本；缺省与 `region` 相同（同版本内迁移）。 */
    sourceRegion?: Region;
  },
): Promise<MigrateResult> {
  const { region, sourceRegion, ...rest } = options ?? {};
  return call("migrate_account_data", {
    targetAccountId,
    ...rest,
    ...regionArg(region),
    ...(sourceRegion ? { sourceRegion } : {}),
  });
}

/** 打开系统设置授权面板（桌面端专用；webui 模式由服务进程权限决定，无操作）。 */
export function openPermissionSettings(
  target?: "app_management" | "all_files",
): Promise<void> {
  if (demoModeEnabled) return Promise.reject(new Error(demoUnavailableMessage()));
  if (isWebui()) return Promise.resolve();
  return call("open_permission_settings", { target: target ?? "app_management" });
}

/** 权限自检：桌面端写探针；webui 模式由服务进程权限决定。 */
export function checkAuthPermission(): Promise<{
  ok: boolean;
  message?: string;
  error?: string;
  dir?: string;
  hint?: string;
}> {
  if (demoModeEnabled) return Promise.reject(new Error(demoUnavailableMessage()));
  if (isWebui()) {
    return Promise.resolve({
      ok: true,
      message: t("shared.api.webuiPermissionByProcess"),
      hint: "",
    });
  }
  return call("check_auth_permission");
}

/** 在 Finder 中显示当前 App（桌面端专用；webui 无操作）。 */
export function revealAppInFinder(): Promise<void> {
  if (demoModeEnabled) return Promise.reject(new Error(demoUnavailableMessage()));
  if (isWebui()) return Promise.resolve();
  return call("reveal_app_in_finder");
}

// ---------------------------------------------------------------------------
// 阶段 3：签到 + token 刷新
// ---------------------------------------------------------------------------

export async function getCheckinStatus(accountId: string, region?: Region): Promise<{
  ok: boolean;
  todayCheckedIn: boolean;
  error?: string;
  raw?: unknown;
}> {
  if (demoModeEnabled) {
    return screenshotDemoResponse("get_checkin_status", { accountId, ...regionArg(region) }) as {
      ok: boolean;
      todayCheckedIn: boolean;
      error?: string;
      raw?: unknown;
    };
  }
  if (isWebui()) {
    // webui 端为批量接口，按 accountId 过滤
    const all = await httpCall<{
      accounts: {
        accountId: string;
        email: string;
        ok: boolean;
        todayCheckedIn: boolean;
        error?: string;
        raw?: unknown;
      }[];
    }>("get_checkin_status", region ? { region } : undefined);
    const one = all.accounts.find((a) => a.accountId === accountId);
    return one
      ? { ok: one.ok, todayCheckedIn: one.todayCheckedIn, error: one.error, raw: one.raw }
      : { ok: false, todayCheckedIn: false, error: t("shared.api.accountNotFound") };
  }
  return call("get_checkin_status", { accountId, ...regionArg(region) });
}

export function getCreditExpiry(accountId: string, region?: Region): Promise<CreditExpiry> {
  return call("get_credit_expiry", { accountId, ...regionArg(region) });
}

export function getCreditStatistics(refresh = false, region?: RegionFilter): Promise<CreditStatistics> {
  const args: Record<string, unknown> = {};
  if (refresh) args.refresh = true;
  if (region) args.region = region;
  return call("get_credit_statistics", Object.keys(args).length > 0 ? args : undefined);
}

export function getTokenStatistics(days?: number, region?: RegionFilter): Promise<TokenStatistics> {
  const args: Record<string, unknown> = {};
  if (days) args.days = days;
  if (region) args.region = region;
  return call("get_token_statistics", Object.keys(args).length > 0 ? args : undefined);
}

export function checkin(accountId: string, region?: Region): Promise<CheckinResult> {
  return call("checkin", { accountId, ...regionArg(region) });
}

export function checkinAll(region?: Region): Promise<{
  accounts: { accountId: string; email: string; result: string; error?: string }[];
  status?: string;
  reason?: string;
}> {
  return call("checkin_all", region ? { region } : undefined);
}

export function getAutoCheckinConfig(): Promise<CheckinConfig> {
  return call("get_auto_checkin_config");
}

export function saveAutoCheckinConfig(config: CheckinConfig): Promise<CheckinConfig> {
  return call("save_auto_checkin_config", {
    config: config as unknown as Record<string, unknown>,
  });
}

export function getCheckinLogs(): Promise<{ logs: CheckinLog[] }> {
  return call("get_checkin_logs");
}

export async function getTravelStatus(accountId: string, region?: Region): Promise<TravelStatus> {
  if (demoModeEnabled) {
    return screenshotDemoResponse("get_travel_status", { accountId, ...regionArg(region) }) as TravelStatus;
  }
  if (isWebui()) {
    // webui 端为批量接口，按 accountId 过滤
    const all = await httpCall<{
      accounts: { accountId: string; email: string; label: TravelStatus["label"]; rewardCredit: number | null; locationName?: string | null; arriveAt?: number | null }[];
    }>("get_travel_status", region ? { region } : undefined);
    const one = all.accounts.find((a) => a.accountId === accountId);
    return one
      ? { label: one.label, rewardCredit: one.rewardCredit, locationName: one.locationName ?? null, arriveAt: one.arriveAt ?? null }
      : { label: "untraveled", rewardCredit: null, locationName: null, arriveAt: null };
  }
  return call("get_travel_status", { accountId, ...regionArg(region) });
}

export function getAutoTravelConfig(): Promise<TravelConfig> {
  return call("get_auto_travel_config");
}

export function saveAutoTravelConfig(config: TravelConfig): Promise<TravelConfig> {
  return call("save_auto_travel_config", {
    config: config as unknown as Record<string, unknown>,
  });
}

export function getAutoRotateConfig(): Promise<AutoRotateConfig> {
  return call("get_auto_rotate_config");
}

export function saveAutoRotateConfig(config: AutoRotateConfig): Promise<AutoRotateConfig> {
  return call("save_auto_rotate_config", {
    config: config as unknown as Record<string, unknown>,
  });
}

// ---------------------------------------------------------------------------
// 定时任务排程（六类任务，全局单份，无需 region）
// ---------------------------------------------------------------------------

export function getScheduleConfig(): Promise<ScheduleConfig> {
  return call("get_schedule_config");
}

export function saveScheduleConfig(config: ScheduleConfig): Promise<ScheduleConfig> {
  return call("save_schedule_config", {
    config: config as unknown as Record<string, unknown>,
  });
}

/** 立即执行某一类定时任务（不等排程到点），用于保存排程后当场自证是否生效。 */
export function runScheduleTask(task: string): Promise<ScheduleRunResult> {
  return call("run_schedule_task", { task });
}

export function getRotateStatus(): Promise<RotateStatus> {
  return call("rotate_status");
}

export function runRotate(): Promise<{ status: string; reason?: string; error?: string; to?: string }> {
  return call("run_rotate");
}

export function getRotateLogs(): Promise<{ logs: RotateLog[] }> {
  return call("get_rotate_logs");
}

export function refreshAccountToken(accountId: string, region?: Region): Promise<AccountMeta> {
  return call("refresh_account_token", { accountId, ...regionArg(region) });
}

// ---------------------------------------------------------------------------
// API 网关（对照架构设计 A-3.7）
// ---------------------------------------------------------------------------

export function getGatewayConfig(): Promise<GatewayConfig> {
  return call("get_gateway_config");
}

export function saveGatewayConfig(config: GatewayConfig): Promise<GatewayConfig> {
  return call("save_gateway_config", { config: config as unknown as Record<string, unknown> });
}

export function gatewayStatus(): Promise<GatewayStatus> {
  return call("gateway_status");
}

export function listApiKeys(): Promise<{ keys: ApiKeyRecord[] }> {
  return call("list_api_keys");
}

export function createApiKey(name: string, region: Region): Promise<CreateApiKeyResult> {
  return call("create_api_key", { name, region });
}

export function revokeApiKey(id: string): Promise<{ ok: boolean }> {
  return call("revoke_api_key", { id });
}

export function deleteApiKey(id: string): Promise<{ ok: boolean }> {
  return call("delete_api_key", { id });
}

export function getGatewayModels(region: Region): Promise<CatalogSnapshot> {
  return call("get_gateway_models", { region });
}

export function refreshGatewayModels(region: Region): Promise<CatalogSnapshot> {
  return call("refresh_gateway_models", { region });
}

export function getAccountStrategy(): Promise<AccountStrategyMap> {
  return call("get_account_strategy");
}

export function saveAccountStrategy(region: Region, strategy: AccountStrategy): Promise<{ ok: boolean }> {
  return call("save_account_strategy", {
    region,
    strategy: strategy as unknown as Record<string, unknown>,
  });
}

export function getGatewayLogs(): Promise<{ logs: GatewayLogEntry[] }> {
  return call("get_gateway_logs");
}

export function clearGatewayLogs(): Promise<{ ok: boolean }> {
  return call("clear_gateway_logs");
}

/** 在系统文件管理器中打开该版本的账号库所在目录（设置页）。 */
export function openAccountsDir(region?: Region): Promise<{ ok: boolean }> {
  return call("open_accounts_dir", region ? { region } : undefined);
}

// ---------------------------------------------------------------------------
// 阶段 4：自动更新
// ---------------------------------------------------------------------------

export function getGithubConfig(): Promise<GithubConfig> {
  return call("get_github_config");
}

export function saveGithubConfig(config: GithubConfig): Promise<GithubConfig> {
  return call("save_github_config", {
    config: config as unknown as Record<string, unknown>,
  });
}

export function checkUpdate(proxy?: string, force?: boolean): Promise<UpdateInfo> {
  return call("check_update", { proxy: proxy?.trim() || null, force: force ?? false });
}

/** 重启 App（桌面端专用；webui 无操作）。守卫在 wrapper 内部，保证「webui 不可达」由本函数自证。 */
export function relaunchApp(): Promise<void> {
  if (demoModeEnabled) return Promise.reject(new Error(demoUnavailableMessage()));
  if (isWebui()) return Promise.resolve();
  return call("relaunch_app");
}

// ---------------------------------------------------------------------------
// 开机自启（仅桌面端；webui 不提供同名接口，卡片也不在 webui 渲染）
// ---------------------------------------------------------------------------

/** 查询系统当前的开机自启注册状态（桌面端）。 */
export function getLaunchAtLoginEnabled(): Promise<boolean> {
  if (demoModeEnabled) return call("get_launch_at_login_enabled");
  if (!isDesktop()) return Promise.resolve(false);
  return call("get_launch_at_login_enabled");
}

/** 注册 / 移除系统开机自启，返回回读后的权威状态（桌面端）。 */
export function setLaunchAtLoginEnabled(enabled: boolean): Promise<boolean> {
  if (demoModeEnabled) return Promise.reject(new Error(demoUnavailableMessage()));
  if (!isDesktop()) return Promise.resolve(false);
  return call("set_launch_at_login_enabled", { enabled });
}

/**
 * 把 Tauri command / HTTP 抛出的错误统一为 Error，并**按当前语言**渲染。
 *
 * 这里是全应用错误文案的唯一裁决点：后端把「文本 + 错误码 + 参数」编进同一个字符串
 * （见 `lib/error-code.ts`），本函数解出码后交给 `localizeError` 选文案。
 * 因此**所有已经用 `asError(e)` 的调用点无需逐个改造**，就同时获得中英两种文案。
 *
 * 中文界面下结果与改造前**逐字节相同**：`localizeError` 对中文直接返回后端原文，
 * 而结构尾已在解码时剥掉。
 */
export function asError(e: unknown): string {
  if (typeof e === "string") return localizeError(e);
  if (e instanceof Error) return localizeError(e.message);
  if (e === null || e === undefined) return t("common.unknownError");
  const serialized = JSON.stringify(e);
  return localizeError(serialized ?? t("common.unknownError"));
}

// ---------------------------------------------------------------------------
// Trae 模块
// ---------------------------------------------------------------------------
//
// 每个 wrapper 都必须以裸 `call(…)` 形式发起调用（字符串字面量为命令名）：
// `scripts/check-api-contract.cjs` 正是据此扫描出调用点，再校验
// 「ROUTES 条目 ←→ Tauri invoke_handler 登记 ←→ server 路由」三方一致。
// 注意不要在注释里写出形如 `call(` 加引号命令名的字样——那会被扫描器当成真实调用点。
// 演示模式（demoModeEnabled）下这些命令不在 DEMO_READ_COMMANDS 中，会统一抛出
// 「演示模式不可用」，页面侧按空态/提示处理即可。

/**
 * 把可选的 `variant` 组装成调用参数（`undefined` 表示「不传」）。
 *
 * 不传与传 `null` 对 Rust 侧**是同一件事**（`Option<String>` 都反序列化成 `None`），
 * 但少传一个键能让请求体更干净、也让「老调用点行为不变」这件事在代码里显式可见。
 * 因此**统一走本函数**，不要在各 wrapper 里重复这段判断。
 */
function variantArgs(variant?: TraeVariantId | null): Record<string, unknown> | undefined {
  if (!variant) return undefined;
  return { variant };
}

/** Trae 客户端安装/运行/数据目录状态（**自动挑中的那一条**，单一视角）。 */
export function getTraeEnv(): Promise<TraeEnvStatus> {
  return call("get_trae_env");
}

/**
 * **全部** Trae 产品线的独立环境状态（并排视角）。
 *
 * 与 [`getTraeEnv`] 的分工：`getTraeEnv` 回答「自动挑中的是哪一条」（用于页面标题、
 * 诊断文案）；本函数回答「每条各自是什么状态」，用于**并排**渲染多个产品图标
 * （对齐 WorkBuddy 右上角三个独立产品图标）。
 *
 * 演示模式下该命令会抛错，调用方按空数组处理即可。
 */
export function getTraeVariants(): Promise<TraeVariantsStatus> {
  return call("get_trae_variants");
}

/** 当前平台的能力与受限项说明。 */
export function getTraeCapabilities(): Promise<TraeCapabilities> {
  return call("get_trae_capabilities");
}

/** 账号 + 分组 + 计数（`variant` 决定读哪个账号库）。 */
export function getTraeAccounts(variant?: TraeVariantId | null): Promise<TraeAccountsOverview> {
  return call("get_trae_accounts", variantArgs(variant));
}

/** 最近一次签到摘要与冷却明细（`variant` 决定读哪条产品线的数据）。 */
export function getTraeCheckinStatus(variant?: TraeVariantId | null): Promise<TraeCheckinStatus> {
  return call("get_trae_checkin_status", variantArgs(variant));
}

/** 剩余积分、签到明细与每日趋势（`variant` 决定读哪条产品线的数据）。 */
export function getTraeCredits(variant?: TraeVariantId | null): Promise<TraeCreditsOverview> {
  return call("get_trae_credits", variantArgs(variant));
}

/**
 * Token 统计（聚合本机 Trae 网关请求日志）。
 *
 * `days` 为统计窗口天数；不传或传 `<= 0` 表示全部历史。
 * `scope` 为**变体范围**筛选维度：`work` / `cn` / `unlabeled` / `all`（不传 = `all`）。
 * 只统计**经过本网关**的调用——直接在 Trae IDE 里对话不产生记录。
 */
export function getTraeTokenStatistics(
  days?: number,
  scope?: TraeTokenScope,
): Promise<TraeTokenStatistics> {
  const args: Record<string, unknown> = {};
  if (days !== undefined) args.days = days;
  if (scope !== undefined) args.scope = scope;
  return call("get_trae_token_statistics", Object.keys(args).length > 0 ? args : undefined);
}

/**
 * 运行日志（系统日志页的「运行日志」标签页）。
 *
 * 只读本机 `logs/` 下的纯文本日志（app / checkin / switcher）；文件不存在时返回空列表，
 * 不抛错——新装用户三个文件都还没有。
 */
export function getTraeLogs(query?: TraeLogQuery): Promise<TraeLogsResponse> {
  const args: Record<string, unknown> = {};
  if (query?.kind && query.kind !== "all") args.kind = query.kind;
  if (query?.date) args.date = query.date;
  if (query?.keyword) args.keyword = query.keyword;
  if (query?.limit) args.limit = query.limit;
  if (query?.variant) args.variant = query.variant;
  return call("get_trae_logs", Object.keys(args).length > 0 ? args : undefined);
}

/** 登录态快照总览（按产品线分家）。 */
export function getTraeProfiles(variant?: TraeVariantId | null): Promise<TraeProfilesOverview> {
  return call("get_trae_profiles", variantArgs(variant));
}

/** Trae 模块设置。 */
export function getTraeSettings(): Promise<TraeSettings> {
  return call("get_trae_settings");
}

/** 局部更新设置：只需提交要改的键。 */
export function saveTraeSettings(patch: Partial<TraeSettings>): Promise<TraeSettings> {
  return call("save_trae_settings", { patch: patch as Record<string, unknown> });
}

/** 手动添加账号（粘贴 JWT）。`variant` 决定写进哪个账号库。 */
export function traeAddAccount(
  name: string,
  jwt: string,
  groupId?: string | null,
  variant?: TraeVariantId | null,
): Promise<{ userId: string; accounts: TraeAccount[] }> {
  return call("trae_add_account", {
    name,
    jwt,
    groupId: groupId ?? null,
    variant: variant ?? null,
  });
}

/** 改名 / 换 JWT。 */
export function traeUpdateAccount(
  userId: string,
  patch: { name?: string; jwt?: string },
  variant?: TraeVariantId | null,
): Promise<{ accounts: TraeAccount[] }> {
  return call("trae_update_account", {
    userId,
    name: patch.name ?? null,
    jwt: patch.jwt ?? null,
    variant: variant ?? null,
  });
}

/** 删除账号（可选一并删除登录态快照）。 */
export function traeDeleteAccount(
  userId: string,
  deleteProfile = false,
  variant?: TraeVariantId | null,
): Promise<{ deleted: string; profileDeleted: boolean; accounts: TraeAccount[] }> {
  return call("trae_delete_account", { userId, deleteProfile, variant: variant ?? null });
}

/**
 * 从 Trae 客户端登录态导入当前账号（对齐 WorkBuddy 的「导入本机账号」）。
 *
 * 客户端已登录时读 `Cloud-IDE-JWT`：账号已存在则**覆盖刷新 JWT**（保留名字与分组），
 * 不存在则新建。因此重复点击是安全的，不会产生重复条目。
 *
 * `variant` 决定读**哪条产品线**的 userData（`"trae_work"` / `"trae_cn"`）：
 * Trae 多条产品线可同机并存，不指定时后端按默认变体处理（保持向后兼容）。
 * 分区页会把当前管理的产品线传进来，避免「在 Trae Work 分区导入却读了 Trae CN」。
 */
export function traeImportLocalAccount(variant?: TraeVariantId | null): Promise<{
  userId: string;
  name: string;
  accounts: TraeAccount[];
}> {
  return call("trae_import_local_account", variantArgs(variant));
}

// ---------------------------------------------------------------------------
// Trae OAuth 登录（浏览器授权 + 本地回调监听）
// ---------------------------------------------------------------------------
//
// 与 WorkBuddy 的 `oauthStart` / `oauthStatus` 同构，但**不带 region 参数**：
// Trae 只有一套账号库与一套上游，加 region 会是永远被忽略的假参数。

/**
 * 发起登录：后端在本机 `127.0.0.1` 起临时回调监听并返回授权 URL。
 *
 * **本函数不会打开浏览器**——调用方拿到 `verificationUri` 后自行打开。
 * 分开的理由：webui 场景没有系统浏览器可开，只能把链接展示给用户点。
 *
 * ## `variant` 是**必填**（编译期护栏）
 *
 * 两条产品线各有自己的客户端、数据目录与账号库，授权页也因此不同。
 * 漏传 = 后端按默认变体（`Trae Work`）处理，于是「在 Trae CN 页面点登录」
 * 实际会发起 Trae Work 的登录，账号还会落到 Trae Work 的账号库 ——
 * 用户看到的是「登录成功了但 Trae CN 的列表还是空的」。
 *
 * 这里**必填**是有意的：漏传从「运行时静默走错产品线」变成「编译不过」。
 * 变体的唯一来源是 {@link useTraeVariant}（URL `?line=` 承载），它**永远**返回
 * 一个变体，所以必填不会让任何调用点写不出来。
 *
 * ## 为什么只在这一层必填
 *
 * Tauri 命令（`src-tauri/src/commands.rs`）与 server 路由仍收 `Option<String>`：
 * 那是**跨进程线协议**，老版本客户端可能不传，收紧会破坏兼容。
 * **「编译期护栏」与「wire 兼容」是两个层次，不要一起改。**
 */
export function traeOAuthStart(variant: TraeVariantId): Promise<TraeOAuthStartResult> {
  return call("trae_oauth_start", variantArgs(variant));
}

/**
 * 轮询登录结果。
 *
 * **永不抛错**：`{ done: false }` 表示「还没好」，前端据此继续轮询。
 * 终态时后端会把会话摘掉，再轮询会得到 `done: true` + `error: 不存在或已过期`。
 */
export function traeOAuthStatus(loginId: string): Promise<TraeOAuthPollResult> {
  return call("trae_oauth_status", { loginId });
}

/** 取消登录（用户关掉对话框），后端随即释放监听端口。 */
export function traeOAuthCancel(loginId: string): Promise<{ cancelled: boolean }> {
  return call("trae_oauth_cancel", { loginId });
}

/** 导出勾选账号的完整记录（含 JWT），由前端落盘。 */
export function traeExportAccounts(
  userIds: string[],
  variant?: TraeVariantId | null,
): Promise<{ accounts: TraeExportRecord[] }> {
  return call("trae_export_accounts", { userIds, variant: variant ?? null });
}

/** 桌面端：把完整记录写入用户选择的路径（系统保存对话框产物）。 */
export function traeExportAccountsToPath(
  userIds: string[],
  path: string,
  variant?: TraeVariantId | null,
): Promise<{ path: string; count: number }> {
  return call("trae_export_accounts_to_path", { userIds, path, variant: variant ?? null });
}

/** 解析导入文件并回传脱敏预览（不含 JWT 明文，只报有无）。 */
export function traePreviewImportAccounts(fileText: string): Promise<TraeImportPreview> {
  return call("trae_preview_import_accounts", { fileText });
}

/** 按选中索引导入账号，返回计数与最新账号列表。 */
export function traeImportAccounts(
  fileText: string,
  indexes: number[],
  variant?: TraeVariantId | null,
): Promise<{ imported: number; skipped: number; overwritten: number; accounts: TraeAccount[] }> {
  return call("trae_import_accounts", { fileText, indexes, variant: variant ?? null });
}

/** 分组操作：`create` / `update` / `delete` / `move`。 */
export function traeGroupOp(
  action: "create" | "update" | "delete" | "move",
  params: Record<string, unknown>,
  variant?: TraeVariantId | null,
): Promise<{ id?: string; groups?: TraeGroup[]; accounts?: TraeAccount[] }> {
  return call("trae_group_op", { action, params, variant: variant ?? null });
}

/**
 * 批量签到。
 *
 * 桌面端会同时派发 `trae-checkin-progress` 事件（逐账号进度）；webui 只在结束时
 * 拿到完整报告。两端的**返回值形状相同**。
 *
 * `variant` 包在 `options` 里（而不是像其他 wrapper 那样放顶层）：
 * 签到的入参本就整体是一个 options 对象，后端也按这个契约解析，
 * 再单独加一个顶层字段会让两条通道的契约分叉。
 */
export function traeCheckin(options?: {
  scope?: "all" | "selected" | `group:${string}`;
  userIds?: string[];
  skipCheckedIn?: boolean;
  skipExpired?: boolean;
  retry?: number;
  variant?: TraeVariantId | null;
}): Promise<TraeCheckinReport> {
  return call("trae_checkin", { options: options ?? {} });
}

/** 刷新剩余积分（不传 userId 即刷新该产品线全部）。 */
export function traeRefreshCredits(
  userId?: string,
  variant?: TraeVariantId | null,
): Promise<{ scope: "single" | "all"; userId?: string; credits?: number; refreshed: number }> {
  return call("trae_refresh_credits", { userId: userId ?? null, variant: variant ?? null });
}

/** 用 refresh_token 换新 JWT。 */
export function traeRefreshJwt(
  userId: string,
  variant?: TraeVariantId | null,
): Promise<{ userId: string; jwt: string; jwtExpHours: number | null; accounts: TraeAccount[] }> {
  return call("trae_refresh_jwt", { userId, variant: variant ?? null });
}

/** 清除冷却（不传 userId 即清除该产品线全部）。 */
export function traeClearCooldown(
  userId?: string,
  variant?: TraeVariantId | null,
): Promise<{ scope: "single" | "all"; userId?: string; cleared?: number }> {
  return call("trae_clear_cooldown", { userId: userId ?? null, variant: variant ?? null });
}

/**
 * 把旧「产品线」账号库并入**国内版**区域账号库（幂等）。
 *
 * 无参数：输入是磁盘上的旧库文件，输出是合并报告。**无旧库或已并完时 `changed`
 * 为 false**，因此页面加载时调一次是安全且廉价的。合并前后端会先把目标库
 * 备份到 `trae/backups/region-merge/<utc>/`，路径随 `backup` 回传。
 */
export function traeMergeLegacyRegions(): Promise<TraeLegacyMergeReport> {
  return call("trae_merge_legacy_regions");
}

/**
 * 切换账号（含「先保存当前登录态」的兜底）。
 *
 * `variant` 决定读/写哪条产品线的快照与客户端目录。它是**数据维度**，
 * 故与其他可选参数同层放在 `options` 里，由后端 `parse_switch_options` 一并解析。
 *
 * ⚠️ **整包必须嵌在 `options` 键下**：桌面端命令签名是
 * `trae_switch_account(app, options: Value)`（单个 `Value` 参数），Tauri 按参数名
 * 取值，平铺传参会直接报 `missing required key options` —— 只有 webui 通道
 * （`api_trae_switch` 里 `body.get("options").unwrap_or(body)`）能容忍平铺。
 * 与 `traeCheckin` 同形；`scripts/check-api-contract.cjs` 的「单 Value 参数」规则
 * 会守住这条不变式。
 */
export function traeSwitchAccount(options: {
  userId: string;
  launch?: boolean;
  proxyPort?: number | null;
  resetDevice?: boolean;
  variant?: TraeVariantId | null;
}): Promise<TraeSwitchOutcome> {
  return call("trae_switch_account", {
    options: {
      userId: options.userId,
      launch: options.launch ?? true,
      proxyPort: options.proxyPort ?? null,
      resetDevice: options.resetDevice ?? false,
      variant: options.variant ?? null,
    },
  });
}

/** 保存当前登录态到指定账号槽位。 */
export function traeSaveLogin(
  userId: string,
  variant?: TraeVariantId | null,
): Promise<{ userId: string; fileCount: number }> {
  return call("trae_save_login", { userId, variant: variant ?? null });
}

/** 备份当前登录态到槽位。 */
export function traeBackupProfile(
  slot: string,
  variant?: TraeVariantId | null,
): Promise<{ slot: string; fileCount: number }> {
  return call("trae_backup_profile", { userId: slot, variant: variant ?? null });
}

/** 用槽位快照覆盖客户端登录态（高级操作，应在客户端关闭时使用）。 */
export function traeRestoreProfile(
  slot: string,
  variant?: TraeVariantId | null,
): Promise<{ slot: string; fileCount: number }> {
  return call("trae_restore_profile", { userId: slot, variant: variant ?? null });
}

/** 删除登录态快照。 */
export function traeDeleteProfile(
  slot: string,
  variant?: TraeVariantId | null,
): Promise<{ slot: string }> {
  return call("trae_delete_profile", { slot, variant: variant ?? null });
}

/** 重置 6 层设备标识。 */
export function traeResetDevice(variant?: TraeVariantId | null): Promise<TraeDeviceResetReport> {
  return call("trae_reset_device", variantArgs(variant));
}

// ---------------------------------------------------------------------------
// Trae API 网关（管理面）
// ---------------------------------------------------------------------------
//
// 网关**本体**监听独立端口（默认 7864），不经过这里的 call()：
// 它是给外部 OpenAI 客户端用的，本模块只负责「配置 / 状态 / 日志 / Key」这四件事。

/** 读取网关配置。 */
export function getTraeGatewayConfig(): Promise<unknown> {
  return call("get_trae_gateway_config");
}

/**
 * 保存配置并应用（会启动/重启独立监听）。
 *
 * 参数用 **snake_case 原始形状**：后端两种拼法都接受（`#[serde(alias)]`），
 * 但磁盘上的配置文件只应该有一种风格，否则人工编辑时会出现同一个键两种写法。
 * 页面侧用 `toTraeGatewayConfigRaw()` 从 camelCase 形状转换。
 */
export function saveTraeGatewayConfig(config: TraeGatewayConfigRaw): Promise<unknown> {
  return call("save_trae_gateway_config", { config });
}

/**
 * 网关运行状态 + 账号池摘要 + 逐账号明细。
 *
 * `variant` 决定看**哪条产品线的账号池**（池已按产品线分家）。缺省 = 默认变体，
 * 键集合不变（`status_view` 的 `api_key_prefix` 等字段始终存在）。
 */
export function getTraeGatewayStatus(variant?: TraeVariantId | null): Promise<unknown> {
  return call("trae_gateway_status", variantArgs(variant));
}

/** 对外暴露的模型清单（OpenAI `/v1/models` 形状）。 */
export function getTraeGatewayModels(): Promise<unknown> {
  return call("get_trae_gateway_models");
}

/** 多 Key 列表（含归属产品线；不含 hash 与明文）。 */
export function listTraeApiKeys(): Promise<{ keys: TraeApiKeyRecord[] }> {
  return call("list_trae_api_keys");
}

/**
 * 新建 API Key：明文**仅此一次**返回（`{ok,key,record}`）。
 *
 * `variant` 决定该 Key 归属哪条产品线，缺省 = TraeWork（默认变体）。
 */
export function createTraeApiKey(
  name: string,
  variant?: TraeVariantId | null,
): Promise<TraeApiKeyCreated> {
  const args: Record<string, unknown> = { name };
  if (variant) args.variant = variant;
  return call("create_trae_api_key", args);
}

/** 吊销 API Key（置 `revokedAt`，不物理删除）。 */
export function revokeTraeApiKey(id: string): Promise<{ ok: boolean }> {
  return call("revoke_trae_api_key", { id });
}

/** 物理删除**已吊销**的 API Key（未吊销会被后端拒绝）。 */
export function deleteTraeApiKey(id: string): Promise<{ ok: boolean }> {
  return call("delete_trae_api_key", { id });
}

/** 打开 Trae 数据目录（非 Windows 返回结构化 `Unsupported`）。 */
export function openTraeDataDir(
  variant?: TraeVariantId | null,
): Promise<{ ok?: boolean; path?: string }> {
  return call("open_trae_data_dir", variantArgs(variant));
}

/** 最近 N 条网关请求日志（元数据）。 */
export function getTraeGatewayLogs(): Promise<unknown> {
  return call("get_trae_gateway_logs");
}

/** 清空网关请求日志。 */
export function clearTraeGatewayLogs(): Promise<{ ok: boolean }> {
  return call("clear_trae_gateway_logs");
}

export type { TraeGatewayLogEntry, TraeGatewayStatus };
