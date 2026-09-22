/**
 * 文案域：**应用外壳**（侧栏 / 产品切换 / 通用设置弹层 / 通用兜底）。
 *
 * 域划分见 `src/locales/zh.ts` 的组合入口。改动本文件请遵守那里的三条约定。
 */
export const zh = {
  // ---- 应用外壳 / 侧栏 ----
  "app.name": "Buddy Switch",
  "app.demoBadge": "演示模式",
  "product.workbuddy": "WorkBuddy",
  "product.trae": "TraeWork",
  "product.switchAria": "切换产品",
  "product.navAria": "{product} 导航",

  "nav.accounts": "账号管理",
  "nav.tokenStats": "Token 统计",
  "nav.credits": "积分统计",
  "nav.apiService": "API 服务",
  "nav.settings": "设置",

  "sidebar.version": "版本",
  "sidebar.running": "{product} 运行中",
  "sidebar.notRunning": "{product} 未运行",
  "sidebar.update": "更新",

  // ---- 通用设置（应用级设置弹层）----
  "appSettings.title": "通用设置",
  "appSettings.description": "不区分 WorkBuddy 与 TraeWork，对 Buddy Switch 本身全局生效。",
  "appSettings.entry": "通用设置",

  "appSettings.appearance.title": "外观",
  "appSettings.appearance.theme.label": "主题",
  "appSettings.appearance.theme.description": "选择浅色、深色，或跟随系统外观自动切换",
  "appSettings.appearance.theme.aria": "主题",
  "appSettings.theme.system": "系统",
  "appSettings.theme.light": "浅色",
  "appSettings.theme.dark": "深色",

  "appSettings.language.title": "语言",
  "appSettings.language.label": "界面语言",
  "appSettings.language.description": "切换后立即生效；偏好保存在本机，不上传。",
  "appSettings.language.zh": "简体中文",
  "appSettings.language.en": "English",

  "appSettings.startup.title": "启动设置",
  "appSettings.startup.silent.label": "开机时静默启动到托盘",
  "appSettings.startup.silent.description": "开关直接反映系统登录项状态；之后可从托盘「打开主界面」恢复",
  "appSettings.startup.enabled": "已开启开机自启",
  "appSettings.startup.disabled": "已关闭开机自启",

  "appSettings.update.title": "自动更新",
  "appSettings.update.current": "当前版本：",
  "appSettings.update.source": "公开更新源",
  "appSettings.update.openRelease": "打开 GitHub Release",
  "appSettings.update.openReleaseTitle": "打开 GitHub Release",
  "appSettings.update.proxy.label": "更新代理地址",
  "appSettings.update.proxy.description": "仅用于 GitHub 更新检查和安装包下载；留空表示关闭显式代理。",
  "appSettings.update.proxy.placeholder": "例如 http://127.0.0.1:7897",
  "appSettings.update.proxy.save": "保存代理",
  "appSettings.update.proxy.saved": "更新代理已保存",
  "appSettings.update.proxy.cleared": "已关闭更新代理",
  "appSettings.update.proxy.invalid": "代理地址格式不正确，请填写 HTTP/HTTPS 地址，例如 http://127.0.0.1:7897",
  "appSettings.update.check": "检查更新",
  "appSettings.update.checkFailed": "检查失败",
  "appSettings.update.foundTitle": "发现新版本",
  "appSettings.update.doneTitle": "更新检查完成",
  "appSettings.update.foundDetail": "发现新版本 v{latest}（当前 v{current}）",
  "appSettings.update.upToDate": "已是最新版本 v{current}",
  "appSettings.update.installNow": "立即升级",

  // ---- 通用兜底 ----
  "common.unknownError": "未知错误",
} as const;
