import { useEffect, useState } from "react";
import { ArrowUpCircle, ExternalLink, Loader2, RefreshCw, Save, Settings2 } from "lucide-react";

import { DemoAction } from "@/components/demo-action";
import { SettingsFieldRow, SettingsGroup } from "@/components/settings-primitives";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { UpdateInstallDialog } from "@/components/update-install-dialog";
import * as api from "@/lib/api";
import { isLocale, setLocale, useLocale, useT } from "@/lib/i18n";
import { getThemePreference, setThemePreference, type ThemePreference } from "@/lib/theme";
import type { GithubConfig, UpdateInfo } from "@/lib/types";
import { GITHUB_RELEASE_URL, GITHUB_REPOSITORY_URL, openReleaseUrl } from "@/lib/update";
import { cn } from "@/lib/utils";
import { useAccountsStore } from "@/stores/accounts";

/**
 * **应用级设置**的唯一实现：语言 / 外观（主题）/ 开机自启 / 自动更新。
 *
 * ## 为什么这四块可以跨产品共用
 *
 * 它们改的是**本应用**的状态 —— 界面语言、主题偏好、系统登录项、GitHub 更新源，
 * 读的也是**本应用**的版本号（`update::APP_VERSION`），与 WorkBuddy / Trae 两条
 * 产品线无关。`docs/parity` 早已把它们定性为「应用级能力 / 全局共用项，不计为
 * Trae 差距」（`trae-parity-matrix.md:103,231,236`、`trae-parity-design.md:225`）。
 *
 * 与 `settings-primitives.tsx` 的分工：那里共享**纯版式原语**（无取数、无判断），
 * 这里共享**应用级业务块**（含取数）。**产品耦合的块不得搬进来** —— 依赖 `Region`
 * 的（版本与账号库 / 权限检测 / 网关）留在 `SettingsPage`，依赖 `TraeVariant` 的
 * 留在 `TraeSettingsPage`；`gateway/*` 那类耦合 `useGatewayStore` 的组件也照旧各自实现
 * （见 `trae-parity-design.md §1.5`）。
 *
 * 历史：这三块原先只存在于 WorkBuddy 设置页，Trae 设置页完全没有 —— 同一件事
 * 在两个产品上的可得性不一致。抽到共享模块后两边走同一份实现，只可能一起变。
 *
 * ## 语言为什么也放在这里而不是各产品设置页
 *
 * 界面语言是**整个应用**的属性：两个产品分区共用同一套控件与文案，把它放进某一个
 * 产品的设置页，就会出现「在 Trae 里改不了语言」这种取决于当前分区的能力差异 ——
 * 正是本模块当初要消灭的那类不一致。判据仍是「有无产品耦合」：语言完全没有。
 */

/** 自动更新：检查公开 GitHub Releases 源 + 安装签名更新。 */
function UpdateCard() {
  const t = useT();
  const version = useAccountsStore((s) => s.status?.version);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [installOpen, setInstallOpen] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);
  const [githubConfig, setGithubConfig] = useState<GithubConfig>({});
  const [proxyUrl, setProxyUrl] = useState("");
  const [proxySaving, setProxySaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void api
      .getGithubConfig()
      .then((config) => {
        if (cancelled) return;
        setGithubConfig(config);
        setProxyUrl(config.proxy ?? "");
      })
      .catch((e) => {
        if (!cancelled) setMsg({ type: "err", text: api.asError(e) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function check() {
    setChecking(true);
    setMsg(null);
    try {
      const r = await api.checkUpdate(proxyUrl, true);
      setInfo(r);
      if (!r.ok) {
        setMsg({ type: "err", text: r.message || r.error || t("appSettings.update.checkFailed") });
      }
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setChecking(false);
    }
  }

  async function saveProxy() {
    const value = proxyUrl.trim();
    if (value) {
      try {
        const parsed = new URL(value);
        if (!parsed.hostname || !["http:", "https:"].includes(parsed.protocol)) {
          throw new Error("unsupported proxy protocol");
        }
      } catch {
        setMsg({ type: "err", text: t("appSettings.update.proxy.invalid") });
        return;
      }
    }

    setProxySaving(true);
    setMsg(null);
    try {
      const saved = await api.saveGithubConfig({ ...githubConfig, proxy: value });
      setGithubConfig(saved);
      setProxyUrl(saved.proxy ?? "");
      setMsg({
        type: "ok",
        text: value ? t("appSettings.update.proxy.saved") : t("appSettings.update.proxy.cleared"),
      });
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setProxySaving(false);
    }
  }

  return (
    <SettingsGroup
      id="settings-updates"
      title={t("appSettings.update.title")}
    >
      <CardContent className="space-y-0 p-0">
        <div className="border-b border-border/60 px-4 py-3 text-sm sm:px-5">
          {t("appSettings.update.current")}
          <span className="font-mono">v{version || "?"}</span>
        </div>

        <div className="flex min-w-0 items-center justify-between gap-3 border-b border-border/60 bg-muted/25 px-4 py-3 text-sm sm:px-5">
          <div className="min-w-0 flex-1">
            <div className="font-medium">{t("appSettings.update.source")}</div>
            <div className="truncate text-xs text-muted-foreground">{GITHUB_REPOSITORY_URL}</div>
          </div>
          <DemoAction><Button
            variant="ghost"
            size="icon"
            title={t("appSettings.update.openReleaseTitle")}
            onClick={() => void openReleaseUrl(GITHUB_RELEASE_URL)}
          >
            <ExternalLink />
          </Button></DemoAction>
        </div>

        <SettingsFieldRow
          label={t("appSettings.update.proxy.label")}
          description={t("appSettings.update.proxy.description")}
          htmlFor="update-proxy"
          className="bg-muted/25"
          operational
        >
          <Input
            id="update-proxy"
            className="w-full sm:w-80"
            value={proxyUrl}
            onChange={(event) => setProxyUrl(event.target.value)}
            placeholder={t("appSettings.update.proxy.placeholder")}
            spellCheck={false}
            autoComplete="off"
          />
        </SettingsFieldRow>

        <div className="flex flex-wrap gap-2 border-b border-border/60 bg-muted/25 px-4 py-3 sm:px-5">
          <DemoAction><Button size="sm" variant="outline" onClick={() => void saveProxy()} disabled={proxySaving}>
            {proxySaving ? <Loader2 className="animate-spin" /> : <Save />}
            {t("appSettings.update.proxy.save")}
          </Button></DemoAction>
        </div>

        <div className="flex flex-wrap gap-2 border-b-0 border-border/60 px-4 py-3 sm:px-5">
          <DemoAction><Button size="sm" variant="outline" onClick={check} disabled={checking}>
            {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            {t("appSettings.update.check")}
          </Button></DemoAction>
        </div>

        {info?.ok && (
          <Alert variant="default" className={cn("!w-auto mx-4 my-4 sm:mx-5", info.hasUpdate && "border-primary/35 bg-primary/[0.06]")}>
            {info.hasUpdate && <ArrowUpCircle className="text-primary" />}
            <AlertDescription className="space-y-2">
              <AlertTitle className={cn(info.hasUpdate && "text-primary")}>
                {info.hasUpdate ? t("appSettings.update.foundTitle") : t("appSettings.update.doneTitle")}
              </AlertTitle>
              <div className="text-sm">
                {info.hasUpdate
                  ? t("appSettings.update.foundDetail", {
                      latest: info.latest ?? "?",
                      current: info.current ?? "?",
                    })
                  : t("appSettings.update.upToDate", { current: info.current ?? "?" })}
                {info.releaseName && <span className="text-muted-foreground"> · {info.releaseName}</span>}
              </div>
              {info.hasUpdate && (
                <DemoAction><Button size="sm" onClick={() => setInstallOpen(true)}>
                  <ArrowUpCircle />
                  {t("appSettings.update.installNow")}
                </Button></DemoAction>
              )}
              {info.releaseUrl && (
                <DemoAction><Button
                  variant="link"
                  size="sm"
                  className="h-auto p-0"
                  onClick={() => void openReleaseUrl(info.releaseUrl!)}
                >
                  {t("appSettings.update.openRelease")}
                </Button></DemoAction>
              )}
            </AlertDescription>
          </Alert>
        )}
        {msg && (
          <Alert
            variant={msg.type === "err" ? "destructive" : "default"}
            className="!w-auto mx-4 my-4 sm:mx-5"
          >
            <AlertDescription>{msg.text}</AlertDescription>
          </Alert>
        )}
        <UpdateInstallDialog
          open={installOpen}
          onOpenChange={setInstallOpen}
          update={info}
        />
      </CardContent>
    </SettingsGroup>
  );
}

/** 开机自启（仅桌面端渲染）：开关直接反映系统自启注册状态，切换立即生效。 */
function StartupCard() {
  const t = useT();
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api
      .getLaunchAtLoginEnabled()
      .then((value) => {
        if (!cancelled) setEnabled(value);
      })
      .catch((e) => {
        if (!cancelled) setMsg({ type: "err", text: api.asError(e) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function onToggle(value: boolean) {
    if (busy || enabled === null) return;
    const previous = enabled;
    setBusy(true);
    setMsg(null);
    try {
      // 后端回读 OS 权威状态；即使与请求一致，也以回读值显示。
      const authoritative = await api.setLaunchAtLoginEnabled(value);
      setEnabled(authoritative);
      setMsg({
        type: "ok",
        text: authoritative
          ? t("appSettings.startup.enabled")
          : t("appSettings.startup.disabled"),
      });
    } catch (e) {
      // 失败时恢复到最后一次确认的状态，并显示可读错误。
      setEnabled(previous);
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  const label = t("appSettings.startup.silent.label");

  return (
    <SettingsGroup
      id="settings-startup"
      title={t("appSettings.startup.title")}
    >
      <CardContent className="space-y-0 p-0">
        <SettingsFieldRow
          className="border-b-0"
          label={label}
          description={t("appSettings.startup.silent.description")}
          htmlFor="startup-silent"
          operational
        >
          <Switch
            id="startup-silent"
            checked={enabled ?? false}
            disabled={busy || enabled === null}
            onCheckedChange={(v) => void onToggle(v)}
            aria-label={label}
          />
        </SettingsFieldRow>

        {msg && (
          <Alert
            variant={msg.type === "err" ? "destructive" : "default"}
            className="!w-auto mx-4 my-4 sm:mx-5"
          >
            <AlertDescription>{msg.text}</AlertDescription>
          </Alert>
        )}
      </CardContent>
    </SettingsGroup>
  );
}

/** 外观：主题选择（持久化到 localStorage）。 */
function AppearanceCard() {
  const t = useT();
  const [theme, setTheme] = useState<ThemePreference>(getThemePreference);

  function onThemeChange(value: string) {
    if (value !== "system" && value !== "light" && value !== "dark") return;
    setThemePreference(value);
    setTheme(value);
  }

  return (
    <SettingsGroup
      id="settings-appearance"
      title={t("appSettings.appearance.title")}
    >
      <CardContent className="space-y-0 p-0">
        <SettingsFieldRow
          className="border-b-0"
          label={t("appSettings.appearance.theme.label")}
          description={t("appSettings.appearance.theme.description")}
          htmlFor="appearance-theme"
        >
          <Select value={theme} onValueChange={onThemeChange}>
            <SelectTrigger
              id="appearance-theme"
              size="sm"
              className="w-full sm:w-40"
              aria-label={t("appSettings.appearance.theme.aria")}
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="system">{t("appSettings.theme.system")}</SelectItem>
              <SelectItem value="light">{t("appSettings.theme.light")}</SelectItem>
              <SelectItem value="dark">{t("appSettings.theme.dark")}</SelectItem>
            </SelectContent>
          </Select>
        </SettingsFieldRow>
      </CardContent>
    </SettingsGroup>
  );
}

/**
 * 语言：界面语言选择。
 *
 * 与主题**同一个归宿**（localStorage，设备级、不上传、不入后端配置）：两者都是
 * 「这台机器上这个人怎么看界面」的偏好，没有跨设备同步的诉求。做成后端配置反而会
 * 让 webui 与桌面端两条通道都要各加一组命令与登记点（见 §二「两条通道形状一致」）。
 *
 * 两个选项的文案是**自名的**（`简体中文` / `English`）：语言菜单用目标语言写自己的名字，
 * 是通行做法 —— 当前界面语言看不懂时，用户仍能认出自己要去哪一项。
 */
function LanguageCard() {
  const t = useT();
  const locale = useLocale();

  return (
    <SettingsGroup
      id="settings-language"
      title={t("appSettings.language.title")}
    >
      <CardContent className="space-y-0 p-0">
        <SettingsFieldRow
          className="border-b-0"
          label={t("appSettings.language.label")}
          description={t("appSettings.language.description")}
          htmlFor="appearance-language"
        >
          <Select
            value={locale}
            onValueChange={(value) => {
              if (isLocale(value)) setLocale(value);
            }}
          >
            <SelectTrigger
              id="appearance-language"
              size="sm"
              className="w-full sm:w-40"
              aria-label={t("appSettings.language.label")}
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="zh">{t("appSettings.language.zh")}</SelectItem>
              <SelectItem value="en">{t("appSettings.language.en")}</SelectItem>
            </SelectContent>
          </Select>
        </SettingsFieldRow>
      </CardContent>
    </SettingsGroup>
  );
}

/**
 * 四个应用级设置块，**门禁与原先设置页逐条对齐**。
 *
 * 门禁必须留在这里而不是交给调用方：本模块同时被侧栏（常驻）与设置页引用，
 * 把判断写在页面上会让两条路径各写一遍 —— 这正是当初 `settings-primitives`
 * 出现漂移的同一种病。
 *
 * 顺序把**语言放在最前**：它改的是其余所有文案的语言，用户在找「设置」时最可能先要它。
 */
export function AppSettingsGroups() {
  return (
    <>
      <LanguageCard />
      <AppearanceCard />
      {api.isDesktop() || api.isDemoMode() ? <StartupCard /> : null}
      {api.isWebui() && !api.isDemoMode() ? null : <UpdateCard />}
    </>
  );
}

/**
 * 侧栏底部「通用设置」入口 —— 固定在**版本号上方**。
 *
 * ## 为什么落在这里
 *
 * 侧栏底部区块（`App.tsx` 里那个 `<section>`）是两个产品分区**唯一共用**的渲染
 * 区域：上方的导航与右侧主区域都随产品切换，只有这里是恒定的。它也不属于
 * `PRODUCT_NAV`，因此不会把「侧栏固定 5 项、路由 5:5」的约束改成 6:6。
 *
 * ## 为什么是 Dialog 而不是内联控件
 *
 * 这些块含取数（`getGithubConfig` / `getLaunchAtLoginEnabled`）与更新相关的
 * 轮询，常驻挂载会让每次启动都多付一次 IPC；Dialog 是**懒挂载**，不开就不取数。
 * 侧栏 220px 也放不下「更新代理地址」这类字段行。
 *
 * ⚠️ `UpdateCard` 内含 `UpdateInstallDialog`，所以这里是**嵌套 Dialog**。
 * Radix 的 `DismissableLayer` 维护层栈，Esc 只关最上层，行为正确 —— 但不要
 * 把「点升级时先关外层」当优化：外层一关，`UpdateCard` 会随之卸载，
 * 升级弹窗的 `installOpen` 状态就丢了。
 */
export function AppSettingsEntry() {
  const t = useT();
  const [open, setOpen] = useState(false);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 w-full justify-start gap-2 rounded-lg px-2 text-xs font-normal text-sidebar-foreground/70 hover:bg-foreground/[0.04] hover:text-sidebar-foreground"
        >
          <Settings2 className="size-3.5" aria-hidden="true" />
          {t("appSettings.entry")}
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("appSettings.title")}</DialogTitle>
          <DialogDescription>{t("appSettings.description")}</DialogDescription>
        </DialogHeader>
        <div className="max-h-[70vh] space-y-6 overflow-y-auto pr-1">
          <AppSettingsGroups />
        </div>
      </DialogContent>
    </Dialog>
  );
}
