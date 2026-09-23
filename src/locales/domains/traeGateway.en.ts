/**
 * 文案域：**Trae 产品线 · 产品线切换与网关管理**（英文）。
 *
 * ⚠️ 不写类型注解：键名合法性由 `src/locales/en.ts` 单点校验。域文件保持零依赖。
 * 键必须与 `traeGateway.zh.ts` **一一对应**（缺哪条就回落中文）。
 */
export const en = {
  // =====================================================================
  // trae-variant-bar.tsx —— variant status bar (accounts page)
  // =====================================================================
  "trae.variant.bar.loggedIn": "Signed in: {name}",
  "trae.variant.bar.notLoggedIn": "Not signed in",
  "trae.variant.bar.notDetected": "Not detected",

  // =====================================================================
  // trae-variant-switch.tsx —— variant switcher
  // =====================================================================
  "trae.variant.switch.aria": "Select Trae version",
  "trae.variant.switch.running": "Running",
  "trae.variant.switch.installed": "Installed",
  "trae.variant.switch.notDetected": "Not detected",
  "trae.variant.switch.tip": "{label}: {state}",
  "trae.variant.switch.tipVersion": "{label}: {state} · v{version}",

  // =====================================================================
  // gateway/trae-model-list.tsx —— model list
  // =====================================================================
  "trae.gateway.models.title": "Model list",
  "trae.gateway.models.summary": "{count} total · default {model}",
  "trae.gateway.models.note": "Trae models are client-side constants and never refresh from upstream; the list below is exactly what is exposed.",
  "trae.gateway.models.empty": "No model data yet.",

  // =====================================================================
  // gateway/trae-request-log.tsx —— request log
  // =====================================================================
  "trae.gateway.log.title": "Request log (last {count})",
  "trae.gateway.log.clear": "Clear",
  "trae.gateway.log.empty": "No requests yet. Once the gateway is running, every call from a client is recorded here (metadata only by default, no bodies).",
  "trae.gateway.log.col.time": "Time",
  "trae.gateway.log.col.account": "Account",
  "trae.gateway.log.col.model": "Model",
  "trae.gateway.log.col.status": "Status",
  "trae.gateway.log.col.latency": "Latency",
  "trae.gateway.log.col.tokens": "Token",

  // =====================================================================
  // gateway/trae-integration-guide.tsx —— integration guide
  // =====================================================================
  "trae.gateway.guide.title": "Integration guide",
  "trae.gateway.guide.copy": "Copy code",
  "trae.gateway.guide.copied": "Code copied",
  "trae.gateway.guide.noKey": "sk-trae-… (create a key above first)",
  "trae.gateway.guide.streamNote": "The Trae upstream only supports streaming; when a request sets `stream: false`, this gateway aggregates locally and returns it at once, with longer first-byte latency.",
  // Field labels inside the copyable snippet
  "trae.gateway.guide.snippet.apiBase": "API Base URL",
  "trae.gateway.guide.snippet.apiKey": "API Key",
  "trae.gateway.guide.snippet.model": "Model",

  // =====================================================================
  // gateway/trae-account-pool-card.tsx —— account pool
  // =====================================================================
  "trae.gateway.pool.title": "Account pool",
  "trae.gateway.pool.totalRequests": "{count} requests total",
  "trae.gateway.pool.tile.available": "Routable",
  "trae.gateway.pool.tile.cooling": "Cooling down",
  "trae.gateway.pool.tile.disabled": "Session invalid",
  "trae.gateway.pool.tile.expired": "Credits expired",
  "trae.gateway.pool.tile.zeroCredits": "Zero credits",
  "trae.gateway.pool.empty": "The account pool is empty. Add a Trae account under \"Account management\" first.",
  "trae.gateway.pool.credits": "{credits} credits",
  "trae.gateway.pool.diagnoseNote": "When a request reports \"no available account\", check the reasons below one by one:",

  // =====================================================================
  // gateway/trae-api-key-table.tsx —— API Key list / create / revoke / delete
  // =====================================================================
  // ---- Toasts ----
  "trae.gateway.key.loadFailed": "Failed to load the API Key list",
  "trae.gateway.key.nameRequired": "Please enter a name",
  "trae.gateway.key.noPlaintext": "Created successfully but no plaintext returned; please retry",
  "trae.gateway.key.createFailed": "Create failed",
  "trae.gateway.key.revoked": "Revoked",
  "trae.gateway.key.revokeFailed": "Revoke failed",
  "trae.gateway.key.deleted": "Deleted",
  "trae.gateway.key.deleteFailed": "Delete failed",
  "trae.gateway.key.copied": "API Key copied",

  // ---- List ----
  "trae.gateway.key.create": "Create API Key",
  "trae.gateway.key.loading": "Loading…",
  "trae.gateway.key.empty": "No API Keys created yet.",
  "trae.gateway.key.neverUsed": "Never used",
  "trae.gateway.key.statusRevoked": "Revoked",
  "trae.gateway.key.statusActive": "Enabled",
  "trae.gateway.key.revoke": "Revoke",
  "trae.gateway.key.delete": "Delete",

  // ---- Table headers ----
  "trae.gateway.key.name": "Name",
  "trae.gateway.key.col.variant": "Owned version",
  "trae.gateway.key.col.prefix": "Prefix",
  "trae.gateway.key.col.createdAt": "Created",
  "trae.gateway.key.col.lastUsed": "Last used",
  "trae.gateway.key.col.status": "Status",
  "trae.gateway.key.col.actions": "Actions",

  // ---- Create dialog ----
  "trae.gateway.key.createDesc": "Each key can only access the models and account pool of its own version.",
  "trae.gateway.key.namePlaceholder": "e.g. Cursor",
  "trae.gateway.key.variant": "Owned version",
  "trae.gateway.key.cancel": "Cancel",
  "trae.gateway.key.createSubmit": "Create",

  // ---- One-time plaintext ----
  "trae.gateway.key.createdTitle": "API Key created",
  "trae.gateway.key.createdDesc": "The full key is shown only this once; copy and save it immediately.",
  "trae.gateway.key.copy": "Copy",
  "trae.gateway.key.saved": "I have saved it, close",

  // ---- Revoke / delete confirmations ----
  "trae.gateway.key.revokeTitle": "Revoke API Key",
  "trae.gateway.key.revokeDesc": "After revocation, \"{name}\" becomes invalid immediately (401) and remains listed as \"revoked\".",
  "trae.gateway.key.deleteTitle": "Delete API Key",
  "trae.gateway.key.deleteDesc": "Delete the revoked key \"{name}\"? This cannot be undone.",
};
