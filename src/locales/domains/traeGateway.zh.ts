/**
 * 文案域：**Trae 产品线 · 产品线切换与网关管理**（中文，键的权威之一）。
 *
 * 归属文件：
 * `src/components/trae-variant-bar.tsx`、`src/components/trae-variant-switch.tsx`、
 * `src/components/gateway/trae-model-list.tsx`、`src/components/gateway/trae-request-log.tsx`、
 * `src/components/gateway/trae-integration-guide.tsx`、
 * `src/components/gateway/trae-account-pool-card.tsx`、
 * `src/components/gateway/trae-api-key-table.tsx`
 *
 * ⚠️ 命名空间用 `trae`（不是 `traeGateway`）：与另两个 Trae 域共用前缀、靠二级段区分。
 *
 * 键前缀：`trae.gateway.`（产品线切换器用 `trae.variant.*`）
 */
export const zh = {
  // =====================================================================
  // trae-variant-bar.tsx —— 产品线状态条（账号页）
  // =====================================================================
  "trae.variant.bar.loggedIn": "已登录: {name}",
  "trae.variant.bar.notLoggedIn": "未登录",
  "trae.variant.bar.notDetected": "未检测到",

  // =====================================================================
  // trae-variant-switch.tsx —— 产品线切换器
  // =====================================================================
  "trae.variant.switch.aria": "选择 Trae 版本",
  "trae.variant.switch.running": "运行中",
  "trae.variant.switch.installed": "已安装",
  "trae.variant.switch.notDetected": "未检测到",
  "trae.variant.switch.tip": "{label}：{state}",
  "trae.variant.switch.tipVersion": "{label}：{state} · v{version}",

  // =====================================================================
  // gateway/trae-model-list.tsx —— 模型清单
  // =====================================================================
  "trae.gateway.models.title": "模型清单",
  "trae.gateway.models.summary": "共 {count} 个 · 默认 {model}",
  "trae.gateway.models.note": "Trae 模型为客户端常量，不随上游刷新；下方清单即对外暴露的全部模型。",
  "trae.gateway.models.empty": "暂无模型数据。",

  // =====================================================================
  // gateway/trae-request-log.tsx —— 请求日志
  // =====================================================================
  "trae.gateway.log.title": "请求日志（最近 {count} 条）",
  "trae.gateway.log.clear": "清空",
  "trae.gateway.log.empty": "暂无请求。网关启动后，客户端发来的每次调用都会记在这里（默认只记元数据，不记正文）。",
  "trae.gateway.log.col.time": "时间",
  "trae.gateway.log.col.account": "账号",
  "trae.gateway.log.col.model": "模型",
  "trae.gateway.log.col.status": "状态",
  "trae.gateway.log.col.latency": "耗时",
  "trae.gateway.log.col.tokens": "Token",

  // =====================================================================
  // gateway/trae-integration-guide.tsx —— 接入指引
  // =====================================================================
  "trae.gateway.guide.title": "接入指引",
  "trae.gateway.guide.copy": "复制代码",
  "trae.gateway.guide.copied": "代码已复制",
  "trae.gateway.guide.noKey": "sk-trae-…（请先在上方创建 Key）",
  "trae.gateway.guide.streamNote": "Trae 上游只支持流式；请求 `stream: false` 时由本网关在本地聚合后一次性返回，首字节延迟较长。",
  // 可复制配置片段里的字段标签（给外部工具粘贴用）
  "trae.gateway.guide.snippet.apiBase": "API 地址",
  "trae.gateway.guide.snippet.apiKey": "API 密钥",
  "trae.gateway.guide.snippet.model": "模型",

  // =====================================================================
  // gateway/trae-account-pool-card.tsx —— 账号池
  // =====================================================================
  "trae.gateway.pool.title": "账号池",
  "trae.gateway.pool.totalRequests": "累计请求 {count}",
  "trae.gateway.pool.tile.available": "可路由",
  "trae.gateway.pool.tile.cooling": "冷却中",
  "trae.gateway.pool.tile.disabled": "会话失效",
  "trae.gateway.pool.tile.expired": "积分过期",
  "trae.gateway.pool.tile.zeroCredits": "零积分",
  "trae.gateway.pool.empty": "账号池为空。请先在「账号管理」中添加 Trae 账号。",
  "trae.gateway.pool.credits": "{credits} 积分",
  "trae.gateway.pool.diagnoseNote": "请求报「没有可用账号」时，按下面的原因逐条排查：",

  // =====================================================================
  // gateway/trae-api-key-table.tsx —— API Key 列表 / 创建 / 吊销 / 删除
  // =====================================================================
  // ---- toast ----
  "trae.gateway.key.loadFailed": "读取 API Key 列表失败",
  "trae.gateway.key.nameRequired": "请填写名称",
  "trae.gateway.key.noPlaintext": "创建成功但未返回明文，请重试",
  "trae.gateway.key.createFailed": "创建失败",
  "trae.gateway.key.revoked": "已吊销",
  "trae.gateway.key.revokeFailed": "吊销失败",
  "trae.gateway.key.deleted": "已删除",
  "trae.gateway.key.deleteFailed": "删除失败",
  "trae.gateway.key.copied": "API Key 已复制",

  // ---- 列表 ----
  "trae.gateway.key.create": "创建 API Key",
  "trae.gateway.key.loading": "读取中…",
  "trae.gateway.key.empty": "尚未创建 API Key。",
  "trae.gateway.key.neverUsed": "从未使用",
  "trae.gateway.key.statusRevoked": "已吊销",
  "trae.gateway.key.statusActive": "启用",
  "trae.gateway.key.revoke": "吊销",
  "trae.gateway.key.delete": "删除",

  // ---- 表头 ----
  "trae.gateway.key.name": "名称",
  "trae.gateway.key.col.variant": "归属版本",
  "trae.gateway.key.col.prefix": "前缀",
  "trae.gateway.key.col.createdAt": "创建时间",
  "trae.gateway.key.col.lastUsed": "最近使用",
  "trae.gateway.key.col.status": "状态",
  "trae.gateway.key.col.actions": "操作",

  // ---- 创建对话框 ----
  "trae.gateway.key.createDesc": "每个 Key 只能访问其归属版本的模型与账号池。",
  "trae.gateway.key.namePlaceholder": "例如 Cursor",
  "trae.gateway.key.variant": "归属版本",
  "trae.gateway.key.cancel": "取消",
  "trae.gateway.key.createSubmit": "创建",

  // ---- 一次性明文 ----
  "trae.gateway.key.createdTitle": "API Key 已创建",
  "trae.gateway.key.createdDesc": "完整 Key 只显示这一次，请立即复制保存。",
  "trae.gateway.key.copy": "复制",
  "trae.gateway.key.saved": "我已保存，关闭",

  // ---- 吊销 / 删除 确认 ----
  "trae.gateway.key.revokeTitle": "吊销 API Key",
  "trae.gateway.key.revokeDesc": "吊销后「{name}」立即失效（401），列表中保留为「已吊销」状态。",
  "trae.gateway.key.deleteTitle": "删除 API Key",
  "trae.gateway.key.deleteDesc": "确定删除已吊销的「{name}」？此操作不可撤销。",
} as const;
