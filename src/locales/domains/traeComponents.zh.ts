/**
 * 文案域：**Trae 产品线 · 组件**（中文，键的权威之一）。
 *
 * 归属文件：
 * `src/components/trae-account-card.tsx`、`src/components/trae-oauth-login-dialog.tsx`、
 * `src/components/trae-import-accounts-dialog.tsx`、`src/components/trae-export-accounts-dialog.tsx`、
 * `src/components/trae-variant-bar.tsx`、`src/components/trae-variant-switch.tsx`、
 * `src/components/gateway/trae-{model-list,request-log,integration-guide,account-pool-card,api-key-table}.tsx`
 *
 * ⚠️ 命名空间用 `trae`（不是 `traeComponents`）：与页面域共用前缀、靠二级段区分
 * （`trae.page.*` / `trae.comp.*`），因为两者本就是同一产品的文案。
 *
 * 键前缀：`trae.comp.`
 */
export const zh = {
  // =====================================================================
  // trae-types.ts —— 程序位 / 标识的展示名
  // =====================================================================
  // 品牌名中英同形，仍走词表：新增程序位时 `TranslationKey` 会编译期提醒补英文条目。
  "trae.program.traeWork": "TraeWork",
  "trae.program.traeCode": "TraeCode",
  "trae.program.traeWorkGlobal": "TraeWork AI",
  "trae.program.traeCodePending": "TraeCode（待实测）",

  // =====================================================================
  // trae-account-card.tsx —— Trae 账号卡片
  // =====================================================================
  // ---- 时长 / JWT 状态 ----
  "trae.comp.card.hours.unknown": "未知",
  "trae.comp.card.hours.expired": "已过期",
  "trae.comp.card.hours.minutes": "{count} 分钟",
  "trae.comp.card.hours.hours": "{count} 小时",
  "trae.comp.card.hours.days": "{count} 天",
  "trae.comp.card.jwt.valid": "有效 {hours}",
  "trae.comp.card.jwt.warn": "临期 {hours}",
  "trae.comp.card.jwt.expired": "已过期",
  "trae.comp.card.jwt.unparsable": "无法解析",

  // ---- 到期 ----
  "trae.comp.card.expiry.permanent": "长期有效",
  "trae.comp.card.expiry.short": "{date} 到期",
  "trae.comp.card.expiry.cooldownUntil": "冷却至 {time}",
  "trae.comp.card.expiry.earliest": "最早到期 {time}",
  "trae.comp.card.expiry.none": "暂无到期时间",
  "trae.comp.card.expiry.noneShort": "暂无到期",

  // ---- 正文行标签 ----
  "trae.comp.card.label.device": "设备",
  "trae.comp.card.label.jwtExpiry": "JWT 到期",
  "trae.comp.card.label.addedAt": "加入时间",
  "trae.comp.card.label.updatedAt": "最近更新",
  "trae.comp.card.label.group": "分组",

  // ---- 状态标签 ----
  "trae.comp.card.chip.checked": "已签到",
  "trae.comp.card.chip.unchecked": "未签到",
  "trae.comp.card.chip.cooling": "冷却中",
  "trae.comp.card.chip.jwtAuto": "自动刷新 JWT",
  "trae.comp.card.chip.jwtManual": "支持刷新 JWT",
  "trae.comp.card.cooldown.until": " · 至 {time}",
  "trae.comp.card.cooldown.fallback": "冷却中（{type}）",

  // ---- 程序切换控件 ----
  "trae.comp.card.currentOf": "{label} 当前账号",
  "trae.comp.card.switch.tip": "切换为 {label} 当前账号（会重启 {label}）",
  "trae.comp.card.switch.missing": "未检测到 {label}",
  "trae.comp.card.switch.aria": "切换到 {label}",
  "trae.comp.card.switch.ariaBusy": "正在切换到 {label}",
  "trae.comp.card.switch.busy": "切换中…",

  // ---- 操作菜单 ----
  "trae.comp.card.manage.aria": "管理账号 {name}",
  "trae.comp.card.manage.title": "更多账号操作",
  "trae.comp.card.menu.save": "保存登录态",
  "trae.comp.card.menu.refreshJwt": "刷新 JWT",
  "trae.comp.card.menu.checkin": "手动签到",
  "trae.comp.card.menu.thaw": "解除冷却",
  "trae.comp.card.menu.delete": "删除账号",

  // ---- 积分区 ----
  "trae.comp.card.credits.remaining": "剩余积分",
  "trae.comp.card.credits.unknown": "积分未查询",
  "trae.comp.card.credits.notQueried": "尚未查询到积分，点右上角菜单「刷新积分」重试",
  "trae.comp.card.section.expiring": "近期到期",
  "trae.comp.card.section.info": "账号信息",
  "trae.comp.card.package.fallback": "积分包",
  "trae.comp.card.package.tip": "{name} · 剩余 {remaining} / {total} · {expiry}",
  "trae.comp.card.package.remaining": "{credits} 积分",
  "trae.comp.card.package.empty": "暂无可用积分",

  // ---- 页脚按钮 ----
  "trae.comp.card.action.saving": "保存中…",
  "trae.comp.card.action.saveTip": "把当前 Trae 登录态备份到该账号槽位",
  "trae.comp.card.action.refreshing": "刷新中…",
  "trae.comp.card.action.refreshJwtTip": "用 refresh token 换一份新的 JWT",
  "trae.comp.card.footer.updatedAt": "{time} 更新",
  "trae.comp.card.footer.noUpdate": "未记录更新时间",
  "trae.comp.card.footer.updatedTip": "该账号在本地账号库中的最近更新时间",

  // ---- 详情弹窗 ----
  "trae.comp.card.detail.open": "查看账号详情",
  "trae.comp.card.detail.title": "账号详情",
  "trae.comp.card.detail.subtitle": "{name} · UID {uid}",
  "trae.comp.card.detail.accountId": "账号 ID",
  "trae.comp.card.detail.enabledPrograms": "已启用的程序",
  "trae.comp.card.detail.creditsExpiry": "积分到期",
  "trae.comp.card.detail.jwtStatus": "JWT 状态",
  "trae.comp.card.detail.jwtAutoRefresh": "自动刷新 JWT",
  "trae.comp.card.detail.deviceId": "设备标识",
  "trae.comp.card.detail.cooldown": "冷却",
  "trae.comp.card.detail.ungrouped": "未分组",
  "trae.comp.card.detail.none": "无",
  "trae.comp.card.detail.notQueried": "未查询",
  "trae.comp.card.detail.jwtAutoOn": "已开启",
  "trae.comp.card.detail.jwtAutoOff": "可手动刷新",
  "trae.comp.card.detail.noRefreshToken": "无 refresh token",

  // =====================================================================
  // trae-oauth-login-dialog.tsx —— OAuth 网页登录
  // =====================================================================
  "trae.comp.oauth.title": "OAuth 网页登录 · {variant}",
  "trae.comp.oauth.desc.lead": "在浏览器中登录",
  "trae.comp.oauth.desc.mid": "并授权，应用会自动接住回调并把账号采集到",
  "trae.comp.oauth.desc.tail": "的账号库，无需粘贴任何令牌。",
  "trae.comp.oauth.error.fallback": "登录失败",
  "trae.comp.oauth.error.timeout": "等待授权超时（{seconds} 秒）。请确认浏览器里已完成授权；若已授权但仍超时，通常是授权页没有把回调打回本机监听端口（端口见下方）。可先关闭弹窗后重试；若反复超时，请确认本机 17388 端口未被其它程序占用。也可改用「导入本机账号」。",
  "trae.comp.oauth.start": "开始 {variant} 网页登录",
  "trae.comp.oauth.startBusy": "正在为 {variant} 发起登录…",
  "trae.comp.oauth.waiting": "正在等待授权，请在浏览器完成登录…",
  "trae.comp.oauth.remaining": "剩余 {time}",
  "trae.comp.oauth.callback": "本机回调监听：",
  "trae.comp.oauth.result": "已添加账号：{name}",
  "trae.comp.oauth.close": "关闭",
  "trae.comp.oauth.done": "完成",
  "trae.comp.oauth.retry": "重新发起登录",
  "trae.comp.oauth.retryBusy": "正在发起登录…",

  // =====================================================================
  // trae-import-accounts-dialog.tsx —— 导入账号
  // =====================================================================
  "trae.comp.import.title": "导入账号",
  "trae.comp.import.desc": "选择 Trae 账号备份 JSON，勾选要导入的账号。",
  "trae.comp.import.uid": "UID · {uid}",
  "trae.comp.import.item": "第 {index} 项",
  "trae.comp.import.chooseFile": "选择文件",
  "trae.comp.import.parsing": "正在解析…",
  "trae.comp.import.summary": "共 {total} 个账号，已选 {count} 个",
  "trae.comp.import.selectAll": "全选",
  "trae.comp.import.deselectAll": "取消全选",
  "trae.comp.import.missingJwt": "缺少 JWT",
  "trae.comp.import.cancel": "取消",
  "trae.comp.import.busy": "导入中…",
  "trae.comp.import.submit": "导入勾选账号",

  // =====================================================================
  // trae-export-accounts-dialog.tsx —— 导出账号
  // =====================================================================
  "trae.comp.export.title": "导出账号",
  "trae.comp.export.desc": "勾选要导出的 Trae 账号，导出为 JSON 文件。",
  "trae.comp.export.uid": "UID · {uid}",
  "trae.comp.export.reveal.windows": "在资源管理器中显示",
  "trae.comp.export.reveal.linux": "在文件管理器中显示",
  "trae.comp.export.reveal.mac": "在 Finder 中显示",
  "trae.comp.export.saveTitle": "导出 Trae 账号",
  "trae.comp.export.warningTitle": "安全提示",
  "trae.comp.export.warningBody": "导出文件含 Cloud-IDE-JWT，等同密码，请勿上传网盘或发送给他人。",
  "trae.comp.export.empty": "暂无账号可导出。",
  "trae.comp.export.summary": "共 {total} 个账号，已选 {count} 个",
  "trae.comp.export.selectAll": "全选",
  "trae.comp.export.deselectAll": "取消全选",
  "trae.comp.export.successTitle": "导出成功",
  "trae.comp.export.done": "完成",
  "trae.comp.export.cancel": "取消",
  "trae.comp.export.submit": "导出勾选账号",
} as const;
