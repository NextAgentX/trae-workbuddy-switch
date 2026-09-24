// 与 Rust 后端命令返回结构对齐的类型定义（对照 server.py 各 API 响应）

/** 目标版本：cn=国内版（WorkBuddy），global=国际版（WorkBuddy AI）。缺省一律按 "cn" 处理。 */
export type Region = "cn" | "global";

/**
 * 统计查询范围：`cn` / `global` 单版，`all` 为两版合并。
 * 与「实体归属」的 `Region` 正交 —— `"all"` 只出现在查询范围，绝不出现在实体归属字段。
 */
export type RegionFilter = Region | "all";

export interface AccountMeta {
  id: string;
  uid: string | null;
  email: string | null;
  nickname: string | null;
  enterpriseName: string | null;
  expiresAt: number | null;
  refreshExpiresAt: number | null;
  refreshedAt: number | null;
  createdAt: number | null;
  needsRelogin: boolean;
  needsReloginReason: string | null;
  /**
   * 用户自填备注（例如「DS4.1 额度 · 10/03 解禁」）。
   *
   * 后端**原样透传**：未设置时为 `null` 或缺失，前端按「有值才渲染」处理。
   */
  remark?: string | null;
  /** 该账号所属版本（后端 region 化后返回；缺省视为 "cn"）。 */
  region?: Region;
}

/** 认证文件 domain 与目标 region 不符时的结构化信息（安全红线 F）。 */
export interface RegionMismatch {
  /** 实际读到的登录域，如 "www.workbuddy.ai"。 */
  actualDomain: string;
  /** 期望的认证文件名，如 "workbuddy-desktop.info"。 */
  expectedFile: string;
  /** 期望的路径覆盖环境变量名，如 "WORKBUDDY_AUTH_FILE"。 */
  envVar: string;
  actualRegion?: Region;
  expectedRegion?: Region;
}

export interface AppStatus {
  running: boolean;
  authFile: string;
  current: {
    uid: string | null;
    nickname: string | null;
    email: string | null;
  } | null;
  appPath: string;
  version: string;
  /** 该 region 客户端是否已安装（后端探测；缺省时前端按登录态/账号库兜底判定）。 */
  installed?: boolean;
  /** 认证文件 domain 与目标 region 不符时返回（缺省表示无冲突）。 */
  regionMismatch?: RegionMismatch | null;
  /** 该状态对应的 region（后端 region 化后返回；缺省视为 "cn"）。 */
  region?: Region;
}

export interface OAuthStartResult {
  loginId: string;
  verificationUri: string;
  expiresIn: number;
}

export interface OAuthPollResult {
  done: boolean;
  result?: AccountMeta;
  error?: string;
}

/** 导出文件中的完整账号记录（含 token，仅导出命令返回；字段与账号库原始记录一致）。 */
export interface AccountRecord {
  id?: string;
  uid?: string | null;
  nickname?: string | null;
  email?: string | null;
  access_token?: string | null;
  refresh_token?: string | null;
  token_type?: string | null;
  domain?: string | null;
  expiresAt?: number | null;
  refreshExpiresAt?: number | null;
  auth_raw?: unknown;
  profile_raw?: unknown;
  createdAt?: number | null;
  [key: string]: unknown;
}

/** 导入文件账号的脱敏预览（不含 token）。 */
export interface ImportPreviewAccount {
  index: number;
  uid: string | null;
  nickname: string | null;
  email: string | null;
  hasToken: boolean;
}

/** 导入结果计数。 */
export interface ImportResult {
  ok: boolean;
  imported: number;
  skipped: number;
  overwritten: number;
}

export interface Session {
  id: string;
  title: string;
  cwd: string;
  updatedAt: number;
  hasHistory: boolean;
  /** WorkBuddy playground（侧栏「任务」）；缺省视为空间会话。 */
  isPlayground?: boolean;
}

export interface CopyResult {
  id: string;
  newId: string;
  jsonlCopied: boolean;
  /** 是否成功写入目标账号的 sessions 索引行；false 表示索引库不可写（已降级为只复制 jsonl 正文）。 */
  sessionRowWritten?: boolean;
  mappingWritten: boolean;
  backup: string;
  /** 命中去重账本：该会话此前已复制到目标账号且副本仍存活，本次跳过。 */
  deduplicated?: boolean;
  /** 本次是否成功写入去重账本（仅未去重时出现）。 */
  ledgerWritten?: boolean;
  /** 降级警告（如「目标索引不可写，已复制正文，请重启 WorkBuddy 重建索引」）。 */
  warning?: string;
}

export interface SwitchResult {
  ok: boolean;
  account: string;
  backup: string | null;
  sessionCopy?: {
    sourceUid: string;
    targetUid: string;
    copied: CopyResult[];
    /** 因已存在存活副本而被跳过的会话（去重命中）。 */
    skipped?: CopyResult[];
    errors?: { id: string; error: string }[];
  };
}

/** Memory 合并计数；失败时为 `{ error }`。 */
export interface MemoryMergeSection {
  targetLines: number;
  sourceLines: number;
  appended: number;
  skippedDuplicate: number;
  changed: boolean;
  /** 改前原文的备份文件路径；未改写或目标原本不存在时为 null。 */
  backup?: string | null;
}

/** Connector 逐文件合并计数。 */
export interface ConnectorFileMerge {
  file: string;
  addedKeys: number;
  keptTargetKeys: number;
  droppedDuplicateElements: number;
  changed: boolean;
}

/** Connector 合并汇总；失败时为 `{ error }`。 */
export interface ConnectorMergeSection {
  skipped: boolean;
  addedKeys: number;
  droppedDuplicateElements: number;
  changed: boolean;
  files: ConnectorFileMerge[];
  /** 改前原文的备份**目录**（该轮多个文件同处一目录）；未改写时为 null。 */
  backup?: string | null;
}

/**
 * 账号数据迁移结果。memory / connectors 两项各自独立成败：
 * 失败项为 `{ error: string }`，成功项为对应的计数对象。
 */
export interface MigrateResult {
  sourceUid: string;
  targetUid: string;
  memory?: MemoryMergeSection | { error: string };
  connectors?: ConnectorMergeSection | { error: string };
  /** 是否有任意一项实际改写了文件。 */
  changed: boolean;
}

export interface CheckinConfig {
  enabled: boolean;
  keepalive_days: number;
  lazy_refresh_hours: number;
}

export interface CheckinLog {
  ts: number;
  accountId: string | null;
  email: string;
  result: string;
  error?: string;
}

export interface CheckinResult {
  result: string;
  error?: string;
}

export interface TravelConfig {
  enabled: boolean;
}

/** 账号切换与账号列表展示配置（`~/.buddy-switch/switch_config.json`，全局单份）。 */
export interface SwitchConfig {
  /** 切换账号时默认勾选「复制会话」。默认 `false`（不改变既有切换语义）。 */
  copy_sessions_by_default: boolean;
  /** 把当前登录账号排到账号列表第一位。默认 `true`。 */
  pin_current_account: boolean;
}

export type TravelStatusLabel = "untraveled" | "no-buddy" | "traveling" | "finished";

export interface TravelStatus {
  label: TravelStatusLabel;
  rewardCredit: number | null;
  locationName?: string | null;
  arriveAt?: number | null;
}

/**
 * 定时任务的排程配置（全局单份，无需 region；也不需要「当前产品」——两个产品各占一条任务）。
 *
 * 对照 `buddy-switch-core::modules::schedule::ScheduleConfig` 的扁平序列化（`schedule_to_value`）：
 * 每类任务各有独立的 `*_hours`（0-23 的整数列表）与独立的 `*_enabled` 开关。
 */
export interface ScheduleConfig {
  /** 自动签到的小时点（0-23），可多个，如 [9, 21]。 */
  checkin_hours: number[];
  /** 派猫猫旅行的小时点。 */
  travel_hours: number[];
  /** 活跃地图上报的小时点。 */
  activity_hours: number[];
  /** token 保活的小时点。 */
  keepalive_hours: number[];
  /** 开学季任务的小时点。 */
  school_hours: number[];
  /** 夜猫子（猫猫领取）的小时点。 */
  cat_hours: number[];
  /** Trae 分区自动签到的小时点（第二条产品线，签的是 Trae 自己的区域账号库）。 */
  trae_checkin_hours: number[];
  /** 自动签到开关（签 WorkBuddy 的账号）。 */
  checkin_enabled: boolean;
  /** 派猫猫旅行开关。 */
  travel_enabled: boolean;
  /** 活跃地图上报开关。 */
  activity_enabled: boolean;
  /** token 保活开关。 */
  keepalive_enabled: boolean;
  /** 开学季任务开关。 */
  school_enabled: boolean;
  /** 夜猫子任务开关。 */
  cat_enabled: boolean;
  /**
   * Trae 自动签到开关（签 Trae 的区域账号库）。
   *
   * ⚠️ **默认 `false`**（后端的默认值，与本产品其他六类不同）：它是本产品新增的能力，
   * 且会对用户没授权过的外部服务发请求 —— 默认打开等于升级后凭空拿凭据去签到。
   * 界面因此**不得**把它显示成默认开启。
   */
  trae_checkin_enabled: boolean;
  /** 活跃上报每账号每天的对话次数（后端将 0 / 负数归一为 1）。 */
  activity_report_count: number;
}

/**
 * 「立即执行」某一类定时任务的结果（`POST /api/schedule/run`）。
 *
 * 形状随任务而异：签到 / 活跃上报 / 保活按 region 逐段返回；旅行返回派出与领取两段；
 * 开学季 / 夜猫子只返回单段结果。各段即业务模块自身的返回，原样透传便于定位到具体账号。
 */
export interface ScheduleRunResult {
  /** 被执行的任务标识（与 `SCHEDULE_TASKS` 的 key 一致）。 */
  task: string;
  /** 按 region 逐段的结果（签到 / 活跃上报 / 保活）。 */
  regions?: Array<Record<string, unknown>>;
  /** 旅行派出一段的结果。 */
  dispatched?: Record<string, unknown>;
  /** 旅行领取一段的结果。 */
  claimed?: Record<string, unknown>;
  /** 单段结果（开学季 / 夜猫子）。 */
  result?: Record<string, unknown>;
}

export interface AutoRotateConfig {
  enabled: boolean;
  check_interval_minutes: number;
  cooldown_minutes: number;
  min_gap_hours: number;
  min_urgency_hours: number;
  active_guard_minutes: number;
  min_remaining_credits: number;
}

export interface RotateLog {
  ts: number;
  action: string;
  reason?: string | null;
  from?: { id: string; name?: string | null } | null;
  to?: { id: string; name?: string | null } | null;
}

export interface RotateStatus {
  config: AutoRotateConfig;
  cliConfigured: boolean;
  activeAccountId: string | null;
  activeAccountName: string | null;
  lastCheckAt: number | null;
  lastSwitchAt: number | null;
}

export interface CreditResource {
  packageCode: string | null;
  packageName: string | null;
  total: number;
  remaining: number;
  used: number;
  status: number | null;
  expireAt: number | null;
  expired: boolean;
  expiringSoon: boolean;
}

export interface CreditExpiry {
  ok: boolean;
  accountId?: string | null;
  accountName?: string;
  updatedAt?: number;
  totalCapacity?: number;
  totalRemaining?: number;
  expiringSoonRemaining?: number;
  expiredRemaining?: number;
  soonestExpireAt?: number | null;
  expiringSoon?: boolean;
  expired?: boolean;
  resources?: CreditResource[];
  error?: string;
  /**
   * **机器可读**的失败原因（后端 `credits::ENCRYPTED_CREDENTIAL_REASON`）。
   *
   * `"encrypted_credential"` = 该账号的凭据是客户端 5.6 的加密信封，我方解不开
   * ⇒ 签到 / 积分 / Token 统计都做不了，只能用「切换账号」。
   *
   * ⚠️ 判据必须用这个字段，**不要**去匹配 `error` 的中文文案 ——
   * 文案一改，界面上「该怎么办」的引导就静默消失，且不会变红。
   */
  reason?: string | null;
}

export interface CreditStatsSummary {
  currentRemaining: number;
  currentCapacity: number;
  usageToday: number;
  usage7Days: number;
  usageThisMonth: number;
  todayCheckedInAccounts: number;
  todaySuccess: number;
  todayAlready: number;
  todayFailed: number;
}

export interface CreditStatsDailyPoint {
  date: string;
  usage: number;
  /** 官方用量按模型聚合（全量，不受请求明细条数限制）；本地观察口径下为空 */
  models?: { model: string; requestCount: number; credit: number }[];
}

export interface CreditStatsAccount {
  accountId: string;
  accountName: string;
  isCurrent: boolean;
  currentRemaining: number | null;
  totalCapacity: number | null;
  lastSnapshotAt: number | null;
  usageToday: number;
  usage7Days: number;
  usageThisMonth: number;
  checkedInToday: boolean | null;
  checkinStatusToday: string | null;
  lastCheckinAt: number | null;
  lastCheckinResult: string | null;
  /** 按账号的逐日观察消耗（缺省兼容旧后端）；官方可用时趋势图优先使用官方 daily */
  daily?: CreditStatsDailyPoint[];
  /** 账号归属版本（后端注入；合并视图据此渲染归属徽标） */
  region?: Region;
}

export interface CreditStatsUsageEvent {
  kind: "usage";
  ts: number;
  date: string;
  accountId: string;
  accountName: string;
  amount: number;
}

export interface CreditStatsCheckinEvent {
  kind: "checkin";
  ts: number;
  date: string;
  accountId: string | null;
  accountName: string;
  result: string;
  error?: string | null;
}

export type CreditStatsEvent = CreditStatsUsageEvent | CreditStatsCheckinEvent;

export type CreditOfficialUsageStatus = "complete" | "partial" | "unavailable";

export interface CreditOfficialUsageSummary {
  usageToday: number;
  usage7Days: number;
  usageThisMonth: number;
}

export interface CreditOfficialUsageModel {
  model: string;
  requestCount: number;
  credit: number;
}

export interface CreditOfficialUsageAccount {
  accountId: string;
  accountName: string;
  ok: boolean;
  requestCount: number;
  detailTruncated: boolean;
  usageToday: number | null;
  usage7Days: number | null;
  usageThisMonth: number | null;
  error?: string | null;
  reportedTotal?: number | null;
  fetchedCount?: number;
  /** 缺省兼容旧后端响应。 */
  models?: CreditOfficialUsageModel[];
  /** 按账号的逐日官方消耗（全量聚合，不受 requests 明细上限影响；缺省兼容旧后端） */
  daily?: CreditStatsDailyPoint[];
}

export interface CreditOfficialUsageRequest {
  accountId: string;
  accountName: string;
  requestId: string;
  credit: number;
  model: string;
  client: string;
  requestTime: string;
}

export interface CreditOfficialUsageError {
  accountId: string;
  accountName: string;
  error: string;
}

export interface CreditOfficialUsage {
  status: CreditOfficialUsageStatus;
  rangeStart: string;
  rangeEnd: string;
  /** 官方用量最近一次采集时间；缓存命中时保持采集当时的时间。 */
  collectedAt?: number;
  summary: CreditOfficialUsageSummary;
  daily: CreditStatsDailyPoint[];
  accounts: CreditOfficialUsageAccount[];
  requests: CreditOfficialUsageRequest[];
  /** 官方全部有效请求按模型汇总；不受 requests 明细上限影响。 */
  models?: CreditOfficialUsageModel[];
  detailLimitPerAccount: number;
  errors: CreditOfficialUsageError[];
}

export interface CreditStatistics {
  /** 查询范围（后端返回：cn / global / all）；缺省兼容旧后端。 */
  region?: RegionFilter;
  generatedAt: number;
  retentionDays: number;
  coverageStartAt: number | null;
  summary: CreditStatsSummary;
  daily: CreditStatsDailyPoint[];
  accounts: CreditStatsAccount[];
  events: CreditStatsEvent[];
  /** 官方接口不可用时仍使用上述本地观察字段；缺省兼容旧后端。 */
  officialUsage?: CreditOfficialUsage;
}

export interface TokenStatsTotals { total: number; input: number; output: number; cacheRead: number; cacheWrite: number; uncachedInput: number; records: number; cacheHitRate: number | null; }
export interface TokenStatsGroup extends TokenStatsTotals { key: string; title?: string | null; project?: string; sessionId?: string; }
export interface TokenStatsSource { source: "workbuddy" | "codebuddy-cli" | "codebuddy-ide" | "workbuddy-ai"; summary: TokenStatsTotals; models: TokenStatsGroup[]; projects: TokenStatsGroup[]; sessions: TokenStatsGroup[]; daily: TokenStatsGroup[]; /** Optional model-specific daily series for trend filtering. */ dailyByModel?: Record<string, TokenStatsGroup[]>; hours: TokenStatsGroup[]; filesScanned: number; parseErrors: number; coverageStartAt?: number | null; coverageEndAt?: number | null; }
export interface TokenStatistics { /** 查询范围（后端返回：cn / global / all）；缺省兼容旧后端。 */ region?: RegionFilter; generatedAt: number; rangeDays?: number | null; sources: TokenStatsSource[]; }

export interface CodeBuddyCliStatus {
  configured: boolean;
  authMode?: "settings-env" | "api-key-helper";
  environmentOverride?: boolean;
  settingsPresent: boolean;
  helperPresent: boolean;
  helperSupportsAccountIds: boolean;
  helperCurrent?: boolean;
  migrationRequired?: boolean;
  syncPending?: boolean;
  activeIndex: number | null;
  activeAccountId: string | null;
  activeAccountName: string | null;
  accountCount: number;
  statePath: string;
}

export interface CodeBuddyCliSwitchResult {
  ok: boolean;
  configured: boolean;
  synced: boolean;
  verified?: boolean;
  authMode?: "settings-env" | "api-key-helper";
  activeIndex?: number;
  activeAccountId?: string;
  source?: string;
  skipped?: boolean;
  message?: string;
  error?: string;
}

export interface CodeBuddyCliInstallResult {
  ok: boolean;
  configured: boolean;
  helperPresent: boolean;
  helperSupportsAccountIds: boolean;
  verified?: boolean;
  authMode?: "settings-env" | "api-key-helper";
  message?: string;
  error?: string;
}

export interface GithubConfig {
  owner?: string;
  repo?: string;
  proxy?: string;
}

export interface UpdateInfo {
  ok: boolean;
  current?: string;
  latest?: string;
  latestTag?: string;
  hasUpdate?: boolean;
  releaseName?: string;
  releaseUrl?: string;
  publishedAt?: string;
  error?: string;
  message?: string;
}

/** CodeBuddy CN IDE（桌面客户端）状态；与 CodeBuddy CLI 独立。 */
export interface CodeBuddyCnIdeStatus {
  installed: boolean;
  running: boolean;
  dataDir: string | null;
  dbPath: string | null;
  dbExists: boolean;
  appPath: string | null;
  activeAccountId: string | null;
  activeAccountName: string | null;
  detectedFrom?: string;
  statePath?: string;
}

export interface CodeBuddyCnIdeSwitchResult {
  ok: boolean;
  account: string;
  accountId: string;
  dbPath?: string;
  restarted?: boolean;
  message?: string;
}

// ---------------------------------------------------------------------------
// API 网关（对应架构设计 A-3.4 / A-3.7）
// ---------------------------------------------------------------------------

/** 网关运行配置（持久化到 ~/.buddy-switch/gateway_config.json）。字段名与后端 serde 序列化一致。 */
export interface GatewayConfig {
  enabled: boolean;
  bind_addr: string;
  port: number;
  allow_non_loopback: boolean;
  log_keep: number;
  log_bodies: boolean;
  per_key_rate_limit?: number | null;
}

/**
 * 网关运行状态（gateway_status 返回，规范形状）。
 * 后端原始字段（GatewayStatusView，snake_case）经 `normalizeGatewayStatus` 归一化后填充：
 * addr 为实际监听地址（由后端 `addr` / 兼容旧字段 `bind_addr` 归一），
 * allowNonLoopback 为是否允许非回环监听（由 `allow_non_loopback` 归一）。
 */
export interface GatewayStatus {
  /** 配置中的启用开关（与 GatewayConfig.enabled 同步）。 */
  enabled: boolean;
  /** 网关进程是否实际在监听。 */
  running: boolean;
  /** 实际监听地址，如 "127.0.0.1" / "0.0.0.0"。 */
  addr: string;
  /** 实际监听端口。 */
  port: number;
  /** 是否允许非回环（局域网）监听。 */
  allowNonLoopback: boolean;
  /** 后端返回的 Base URL（源自 snake_case `base_url`，如 http://127.0.0.1:57891/v1），优先使用。 */
  baseUrl?: string;
  /** 最近一次错误信息。 */
  error?: string | null;
}

/** API Key 记录（列表脱敏返回，绝不含明文）。字段名与后端 masked() 输出一致。 */
export interface ApiKeyRecord {
  id: string;
  name: string;
  region: Region;
  /** 可辨识前缀，如 "sk-wb-a1b2"。 */
  prefix: string;
  createdAt: number;
  revokedAt: number | null;
  revoked: boolean;
  lastUsedAt: number | null;
}

/** 创建 API Key 的返回：完整 key 仅此一次返回，之后无法再获取。 */
export interface CreateApiKeyResult {
  ok: boolean;
  /** 一次性完整明文（sk-wb- 前缀）。 */
  key?: string;
  /** 创建后可直接加入列表的脱敏记录。 */
  record?: ApiKeyRecord;
  error?: string;
}

/** 模型目录来源标注。 */
export type CatalogSource = "live" | "cached" | "builtin";

export interface CatalogModel {
  id: string;
  name: string;
  context_window: number;
  max_tokens: number;
  supports_images: boolean;
  credits?: string | null;
  badges: string[];
  free: boolean;
}

export interface CatalogSnapshot {
  region: Region;
  source: CatalogSource;
  fetched_at: number | null;
  models: CatalogModel[];
  /** 降级说明（如「上游接口可能已变更」）。 */
  note?: string | null;
}

/** 账号选择策略（按 region 各自独立配置）。 */
export type AccountStrategy =
  | { kind: "current" }
  | { kind: "pinned"; account_id: string }
  | { kind: "max_credits" };

/** 某 region 的策略 + 当前选用账号（selected 与 AccountMeta 同构）。 */
export interface AccountStrategyView {
  region?: Region;
  strategy: AccountStrategy;
  selected: AccountMeta | null;
  error?: string | null;
  note?: string | null;
}

/** 各 region 策略视图。 */
export type AccountStrategyMap = Record<Region, AccountStrategyView>;

/** 网关请求日志（仅元数据；默认不记录正文）。字段名与后端 logging 输出一致。 */
export interface GatewayLogEntry {
  ts: number;
  endpoint: string;
  method: string;
  region: Region;
  account: string | null;
  model: string | null;
  status: number;
  latencyMs: number;
  promptTokens?: number | null;
  completionTokens?: number | null;
  stream: boolean;
}

