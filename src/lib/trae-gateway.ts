/**
 * Trae 网关字段归一化（与 `@/lib/gateway` 平行的第二套）。
 *
 * 为什么另起一个文件而不是扩 `gateway.ts`：两个网关的类型没有任何共用字段
 * （region / Key 库 / 目录缓存 vs 账号池 / 单一 Key），混在一个文件里只会让
 * 「这个 helper 是给谁用的」变成需要读实现才能回答的问题。
 *
 * **序列化风格**：Rust 侧 `TraeGatewayConfig` 与 `TraeGatewayStatusView` 都是
 * snake_case（与 WorkBuddy 网关刻意保持一致）。因此这里以 snake_case 为准，
 * camelCase 只作防御性兜底。
 */

import { t } from "@/lib/i18n";
import type {
  TraeGatewayAccountStatus,
  TraeGatewayConfig,
  TraeGatewayConfigRaw,
  TraeGatewayLogEntry,
  TraeGatewayPoolSummary,
  TraeGatewayStatus,
} from "@/lib/trae-types";

/** 默认配置（与 Rust `TraeGatewayConfig::default` 对齐）。 */
export const DEFAULT_TRAE_GATEWAY_CONFIG: TraeGatewayConfig = {
  enabled: false,
  bindAddr: "127.0.0.1",
  port: 7864,
  allowNonLoopback: false,
  logKeep: 200,
  logBodies: false,
  maxBodyMb: 8,
  defaultModel: "deepseek-v4-flash",
  maxRotate: 3,
};

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function asBoolean(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

/** 取 camelCase 优先、snake_case 兜底的字段。 */
function pick<T>(record: Record<string, unknown>, camel: string, snake: string): T | undefined {
  return (record[camel] ?? record[snake]) as T | undefined;
}

/** 归一化 `get_trae_gateway_config` 的返回。 */
export function normalizeTraeGatewayConfig(raw: unknown): TraeGatewayConfig {
  const record = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const read = (camel: string, snake: string) => record[camel] ?? record[snake];
  return {
    enabled: asBoolean(record.enabled) ?? DEFAULT_TRAE_GATEWAY_CONFIG.enabled,
    bindAddr:
      asString(read("bindAddr", "bind_addr")) ?? DEFAULT_TRAE_GATEWAY_CONFIG.bindAddr,
    port: asNumber(record.port) ?? DEFAULT_TRAE_GATEWAY_CONFIG.port,
    allowNonLoopback:
      asBoolean(read("allowNonLoopback", "allow_non_loopback")) ??
      DEFAULT_TRAE_GATEWAY_CONFIG.allowNonLoopback,
    logKeep: asNumber(read("logKeep", "log_keep")) ?? DEFAULT_TRAE_GATEWAY_CONFIG.logKeep,
    logBodies:
      asBoolean(read("logBodies", "log_bodies")) ?? DEFAULT_TRAE_GATEWAY_CONFIG.logBodies,
    maxBodyMb:
      asNumber(read("maxBodyMb", "max_body_mb")) ?? DEFAULT_TRAE_GATEWAY_CONFIG.maxBodyMb,
    defaultModel:
      asString(read("defaultModel", "default_model")) ??
      DEFAULT_TRAE_GATEWAY_CONFIG.defaultModel,
    maxRotate:
      asNumber(read("maxRotate", "max_rotate")) ?? DEFAULT_TRAE_GATEWAY_CONFIG.maxRotate,
  };
}

/**
 * 前端形状 → 后端 snake_case。
 *
 * 后端同时接受两种写法（`#[serde(alias)]`），但**写回时统一用 snake_case**：
 * 磁盘上的配置文件应该只有一种风格，否则人工编辑时会出现「同一个键两种拼法」。
 */
export function toTraeGatewayConfigRaw(config: TraeGatewayConfig): TraeGatewayConfigRaw {
  return {
    enabled: config.enabled,
    bind_addr: config.bindAddr,
    port: config.port,
    allow_non_loopback: config.allowNonLoopback,
    log_keep: config.logKeep,
    log_bodies: config.logBodies,
    max_body_mb: config.maxBodyMb,
    default_model: config.defaultModel,
    max_rotate: config.maxRotate,
  };
}

function normalizePool(raw: unknown): TraeGatewayPoolSummary {
  const record = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  return {
    total: asNumber(record.total) ?? 0,
    available: asNumber(record.available) ?? 0,
    cooling: asNumber(record.cooling) ?? 0,
    disabled: asNumber(record.disabled) ?? 0,
    expired: asNumber(record.expired) ?? 0,
    zeroCredits: asNumber(pick(record, "zeroCredits", "zero_credits")) ?? 0,
    totalCredits: asNumber(pick(record, "totalCredits", "total_credits")) ?? 0,
  };
}

function normalizeAccount(raw: unknown): TraeGatewayAccountStatus | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  const uid = asString(record.uid);
  if (!uid) return null;
  const status = asString(record.status) ?? "available";
  return {
    uid,
    name: asString(record.name) ?? uid,
    status: status as TraeGatewayAccountStatus["status"],
    credits: asNumber(record.credits) ?? null,
    creditsExpireAt: asNumber(pick(record, "creditsExpireAt", "credits_expire_at")) ?? null,
    cooling: asBoolean(record.cooling) ?? false,
    cooldownUntil: asNumber(pick(record, "cooldownUntil", "cooldown_until")) ?? null,
    cooldownReason: asString(pick(record, "cooldownReason", "cooldown_reason")) ?? null,
    disabled: asBoolean(record.disabled) ?? false,
    deviceIdMasked: asString(pick(record, "deviceIdMasked", "device_id_masked")) ?? null,
  };
}

/**
 * 归一化 `trae_gateway_status` 的返回。
 *
 * 传入非法值时返回 null，调用方回退到「未启动」空态。
 */
export function normalizeTraeGatewayStatus(raw: unknown): TraeGatewayStatus | null {
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  const accounts = Array.isArray(record.accounts)
    ? record.accounts
        .map(normalizeAccount)
        .filter((item): item is TraeGatewayAccountStatus => item !== null)
    : [];
  const diagnose = Array.isArray(record.diagnose)
    ? record.diagnose.filter((item): item is string => typeof item === "string")
    : [];
  return {
    enabled: asBoolean(record.enabled) ?? false,
    running: asBoolean(record.running) ?? false,
    addr: asString(record.addr) ?? null,
    baseUrl: asString(pick(record, "baseUrl", "base_url")) ?? "",
    bindAddr: asString(pick(record, "bindAddr", "bind_addr")) ?? "127.0.0.1",
    port: asNumber(record.port) ?? DEFAULT_TRAE_GATEWAY_CONFIG.port,
    allowNonLoopback:
      asBoolean(pick(record, "allowNonLoopback", "allow_non_loopback")) ?? false,
    version: asString(record.version) ?? "",
    totalRequests: asNumber(pick(record, "totalRequests", "total_requests")) ?? 0,
    lastError: asString(pick(record, "lastError", "last_error")) ?? null,
    apiKeyPrefix: asString(pick(record, "apiKeyPrefix", "api_key_prefix")) ?? "",
    pool: normalizePool(record.pool),
    accounts,
    diagnose,
    upstream: asString(record.upstream) ?? "",
  };
}

/** 归一化 `get_trae_gateway_logs` 的返回（按时间升序，与后端一致）。 */
export function normalizeTraeGatewayLogs(raw: unknown): TraeGatewayLogEntry[] {
  const record = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const list = Array.isArray(record.logs)
    ? record.logs
    : Array.isArray(raw)
      ? (raw as unknown[])
      : [];
  return list
    .map((item): TraeGatewayLogEntry | null => {
      if (!item || typeof item !== "object") return null;
      const entry = item as Record<string, unknown>;
      return {
        ts: asNumber(entry.ts) ?? 0,
        endpoint: asString(entry.endpoint) ?? "",
        method: asString(entry.method) ?? "POST",
        account: asString(entry.account) ?? null,
        model: asString(entry.model) ?? null,
        status: asNumber(entry.status) ?? 0,
        latencyMs: asNumber(pick(entry, "latencyMs", "latency_ms")) ?? 0,
        promptTokens: asNumber(pick(entry, "promptTokens", "prompt_tokens")) ?? null,
        completionTokens:
          asNumber(pick(entry, "completionTokens", "completion_tokens")) ?? null,
        stream: asBoolean(entry.stream) ?? false,
        error: asString(entry.error) ?? null,
      };
    })
    .filter((item): item is TraeGatewayLogEntry => item !== null);
}

/** 账号状态 → 展示标签与语气。 */
export const TRAE_POOL_STATUS_LABELS: Record<
  TraeGatewayAccountStatus["status"],
  { label: string; tone: "ok" | "warn" | "danger" | "muted" }
> = {
  // 表在模块加载时就定型 ⇒ 只存文案键，标签在**读取时**现取（调用方照旧读 `meta.label`）。
  available: {
    get label() {
      return t("shared.trae.poolStatus.available");
    },
    tone: "ok",
  },
  cooling: {
    get label() {
      return t("shared.trae.poolStatus.cooling");
    },
    tone: "warn",
  },
  disabled: {
    get label() {
      return t("shared.trae.poolStatus.disabled");
    },
    tone: "danger",
  },
  expired: {
    get label() {
      return t("shared.trae.poolStatus.expired");
    },
    tone: "warn",
  },
  no_credits: {
    get label() {
      return t("shared.trae.poolStatus.noCredits");
    },
    tone: "muted",
  },
};
