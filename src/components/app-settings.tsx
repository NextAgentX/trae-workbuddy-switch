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
import { getThemePreference, setThemePreference, type ThemePreference } from "@/lib/theme";
import type { GithubConfig, UpdateInfo } from "@/lib/types";
import { GITHUB_RELEASE_URL, GITHUB_REPOSITORY_URL, openReleaseUrl } from "@/lib/update";
import { cn } from "@/lib/utils";
import { useAccountsStore } from "@/stores/accounts";

/**
 * **应用级设置**的唯一实现：外观（主题）/ 开机自启 / 自动更新。
 *
 * ## 为什么这三块可以跨产品共用
 *
 * 它们改的是**本应用**的状态 —— 主题偏好、系统登录项、GitHub 更新源，
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
 */

/** 自动更新：检查公开 GitHub Releases 源 + 安装签名更新。 */
function UpdateCard() {
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
        setMsg({ type: "err", text: r.message || r.error || "检查失败" });
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
        setMsg({ type: "err", text: "代理地址格式不正确，请填写 HTTP/HTTPS 地址，例如 http://127.0.0.1:7897" });
        return;
      }
    }

    setProxySaving(true);
    setMsg(null);
    try {
      const saved = await api.saveGithubConfig({ ...githubConfig, proxy: value });
      setGithubConfig(saved);
      setProxyUrl(saved.proxy ?? "");
      setMsg({ type: "ok", text: value ? "更新代理已保存" : "已关闭更新代理" });
    } catch (e) {
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setProxySaving(false);
    }
  }

  return (
    <SettingsGroup
      id="settings-updates"
      title="自动更新"
    >
      <CardContent className="space-y-0 p-0">
        <div className="border-b border-border/60 px-4 py-3 text-sm sm:px-5">
          当前版本：<span className="font-mono">v{version || "?"}</span>
        </div>

        <div className="flex min-w-0 items-center justify-between gap-3 border-b border-border/60 bg-muted/25 px-4 py-3 text-sm sm:px-5">
          <div className="min-w-0 flex-1">
            <div className="font-medium">公开更新源</div>
            <div className="truncate text-xs text-muted-foreground">{GITHUB_REPOSITORY_URL}</div>
          </div>
          <DemoAction><Button
            variant="ghost"
            size="icon"
            title="打开 GitHub Release"
            onClick={() => void openReleaseUrl(GITHUB_RELEASE_URL)}
          >
            <ExternalLink />
          </Button></DemoAction>
        </div>

        <SettingsFieldRow
          label="更新代理地址"
          description="仅用于 GitHub 更新检查和安装包下载；留空表示关闭显式代理。"
          htmlFor="update-proxy"
          className="bg-muted/25"
          operational
        >
          <Input
            id="update-proxy"
            className="w-full sm:w-80"
            value={proxyUrl}
            onChange={(event) => setProxyUrl(event.target.value)}
            placeholder="例如 http://127.0.0.1:7897"
            spellCheck={false}
            autoComplete="off"
          />
        </SettingsFieldRow>

        <div className="flex flex-wrap gap-2 border-b border-border/60 bg-muted/25 px-4 py-3 sm:px-5">
          <DemoAction><Button size="sm" variant="outline" onClick={() => void saveProxy()} disabled={proxySaving}>
            {proxySaving ? <Loader2 className="animate-spin" /> : <Save />}
            保存代理
          </Button></DemoAction>
        </div>

        <div className="flex flex-wrap gap-2 border-b-0 border-border/60 px-4 py-3 sm:px-5">
          <DemoAction><Button size="sm" variant="outline" onClick={check} disabled={checking}>
            {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />}
            检查更新
          </Button></DemoAction>
        </div>

        {info?.ok && (
          <Alert variant="default" className={cn("!w-auto mx-4 my-4 sm:mx-5", info.hasUpdate && "border-primary/35 bg-primary/[0.06]")}>
            {info.hasUpdate && <ArrowUpCircle className="text-primary" />}
            <AlertDescription className="space-y-2">
              <AlertTitle className={cn(info.hasUpdate && "text-primary")}>{info.hasUpdate ? "发现新版本" : "更新检查完成"}</AlertTitle>
              <div className="text-sm">
                {info.hasUpdate
                  ? `发现新版本 v${info.latest}（当前 v${info.current}）`
                  : `已是最新版本 v${info.current}`}
                {info.releaseName && <span className="text-muted-foreground"> · {info.releaseName}</span>}
              </div>
              {info.hasUpdate && (
                <DemoAction><Button size="sm" onClick={() => setInstallOpen(true)}>
                  <ArrowUpCircle />
                  立即升级
                </Button></DemoAction>
              )}
              {info.releaseUrl && (
                <DemoAction><Button
                  variant="link"
                  size="sm"
                  className="h-auto p-0"
                  onClick={() => void openReleaseUrl(info.releaseUrl)}
                >
                  打开 GitHub Release
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
      setMsg({ type: "ok", text: authoritative ? "已开启开机自启" : "已关闭开机自启" });
    } catch (e) {
      // 失败时恢复到最后一次确认的状态，并显示可读错误。
      setEnabled(previous);
      setMsg({ type: "err", text: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <SettingsGroup
      id="settings-startup"
      title="启动设置"
    >
      <CardContent className="space-y-0 p-0">
        <SettingsFieldRow
          className="border-b-0"
          label="开机时静默启动到托盘"
          description="开关直接反映系统登录项状态；之后可从托盘「打开主界面」恢复"
          htmlFor="startup-silent"
          operational
        >
          <Switch
            id="startup-silent"
            checked={enabled ?? false}
            disabled={busy || enabled === null}
            onCheckedChange={(v) => void onToggle(v)}
            aria-label="开机时静默启动到托盘"
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
  const [theme, setTheme] = useState<ThemePreference>(getThemePreference);

  function onThemeChange(value: string) {
    if (value !== "system" && value !== "light" && value !== "dark") return;
    setThemePreference(value);
    setTheme(value);
  }

  return (
    <SettingsGroup
      id="settings-appearance"
      title="外观"
    >
      <CardContent className="space-y-0 p-0">
        <SettingsFieldRow
          className="border-b-0"
          label="主题"
          description="选择浅色、深色，或跟随系统外观自动切换"
          htmlFor="appearance-theme"
        >
          <Select value={theme} onValueChange={onThemeChange}>
            <SelectTrigger id="appearance-theme" size="sm" className="w-full sm:w-40" aria-label="主题">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="system">系统</SelectItem>
              <SelectItem value="light">浅色</SelectItem>
              <SelectItem value="dark">深色</SelectItem>
            </SelectContent>
          </Select>
        </SettingsFieldRow>
      </CardContent>
    </SettingsGroup>
  );
}

/**
 * 三个应用级设置块，**门禁与原先设置页逐条对齐**。
 *
 * 门禁必须留在这里而不是交给调用方：本模块同时被侧栏（常驻）与设置页引用，
 * 把判断写在页面上会让两条路径各写一遍 —— 这正是当初 `settings-primitives`
 * 出现漂移的同一种病。
 */
export function AppSettingsGroups() {
  return (
    <>
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
 * 这三块含取数（`getGithubConfig` / `getLaunchAtLoginEnabled`）与更新相关的
 * 轮询，常驻挂载会让每次启动都多付一次 IPC；Dialog 是**懒挂载**，不开就不取数。
 * 侧栏 220px 也放不下「更新代理地址」这类字段行。
 *
 * ⚠️ `UpdateCard` 内含 `UpdateInstallDialog`，所以这里是**嵌套 Dialog**。
 * Radix 的 `DismissableLayer` 维护层栈，Esc 只关最上层，行为正确 —— 但不要
 * 把「点升级时先关外层」当优化：外层一关，`UpdateCard` 会随之卸载，
 * 升级弹窗的 `installOpen` 状态就丢了。
 */
export function AppSettingsEntry() {
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
          通用设置
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>通用设置</DialogTitle>
          <DialogDescription>
            不区分 WorkBuddy 与 TraeWork，对 Buddy Switch 本身全局生效。
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-[70vh] space-y-6 overflow-y-auto pr-1">
          <AppSettingsGroups />
        </div>
      </DialogContent>
    </Dialog>
  );
}
