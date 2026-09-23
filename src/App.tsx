import { useEffect, useMemo, useRef, useState, type ComponentType } from "react";
import { BrowserRouter, HashRouter, Navigate, NavLink, Outlet, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { ArrowUp, MessagesSquare, Server, Settings, Sparkles, User } from "lucide-react";

import { cn } from "@/lib/utils";
import * as api from "@/lib/api";
import type { UpdateInfo } from "@/lib/types";
import AccountsPage from "@/pages/AccountsPage";
import ApiServicePage from "@/pages/ApiServicePage";
import CreditStatsPage from "@/pages/CreditStatsPage";
import TokenStatsPage from "@/pages/TokenStatsPage";
import SettingsPage from "@/pages/SettingsPage";
import TraeAccountsPage from "@/pages/TraeAccountsPage";
import TraeApiServicePage from "@/pages/TraeApiServicePage";
import TraeCreditsPage from "@/pages/TraeCreditsPage";
import TraeSettingsPage from "@/pages/TraeSettingsPage";
import TraeTokenStatsPage from "@/pages/TraeTokenStatsPage";
import { StatusDot, AppIconMark, TraeVariantMark, WorkBuddyMark } from "@/components/product-marks";
import { DonateButton } from "@/components/donate-dialog";
import { AppSettingsEntry } from "@/components/app-settings";
import { UpdateInstallDialog } from "@/components/update-install-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Toaster } from "@/components/ui/sonner";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { demoModeEnabled, pagesDemoHostingEnabled } from "@/lib/demo-mode";
import { useT } from "@/lib/i18n";
import type { TranslationKey } from "@/locales/zh";
import { TRAE_VARIANTS_KEY, loadTraeVariantStatuses } from "@/lib/trae-variant-status";
import type { TraeVariantStatus } from "@/lib/trae-types";
import { useCachedResource } from "@/lib/use-cached-resource";
import { useTraeVariant } from "@/lib/use-trae-variant";
import { useCreditAutoRefresh } from "@/lib/use-credit-auto-refresh";
import { useWorkbuddyStatusRefresh } from "@/lib/use-workbuddy-status-refresh";
import { useAccountsStore } from "@/stores/accounts";

/**
 * 产品分区。
 *
 * WorkBuddy 与 Trae 是两套完全独立的体系（独立账号库、独立客户端、独立配置），
 * 侧栏一次只展示其中一个产品的导航与页面，由顶部的产品 Tab 决定。
 *
 * ## ★ Trae 只占**一个**侧栏分区，两条产品线在分区内部切换
 *
 * `TRAE SOLO CN`（界面名 `Trae Work`）与 `Trae CN` 确实可以同机并存、各有独立
 * 安装目录与账号库，但**这不意味着侧栏要排三个 Tab**：
 *
 * - 侧栏的职责是「选产品」，而 Trae 的两条线**共用全部页面与路由**
 *   （`/trae/accounts` 等）。把它们拆成两个顶级分区，等于让同一组页面
 *   在侧栏出现两遍，用户要在两个看起来一样的入口之间做无意义的二选一。
 * - 变体是**数据维度**而非**页面维度**，因此正确的载体是 Trae 页面内部的
 *   产品线切换器（见 `TraeAccountsPage` 的变体 Tab），而不是侧栏。
 * - 侧栏 220px 宽度下三个 Tab 连产品名都排不下（详见 `ProductSwitch` 的注释），
 *   压成纯图标后两条 Trae 又几乎无法区分 —— 这本身就是「不该有三个 Tab」的信号。
 *
 * 变体仍然由 URL 的 `?line=` 承载（`useTraeVariant`）：它决定 Trae 分区内部
 * 看哪条线，并且刷新 / 分享 / 前进后退都能保持。
 */
type Product = "workbuddy" | "trae";

interface NavItem {
  to: string;
  /**
   * 文案**键**而非成品文案。
   *
   * 这张表是模块级常量，拿不到 `useT()`；把键留在这里、在渲染处翻译，是唯一能让
   * 「表格仍是纯数据」与「语言可切换」同时成立的写法。写成成品中文会让语言切换
   * 对整条侧栏失效（表格在模块加载时就已经定型）。
   */
  labelKey: TranslationKey;
  icon: ComponentType<{ className?: string }>;
  /** 仅在该路径完全匹配时高亮：用于产品首页，避免其子页面同时点亮两个条目。 */
  end?: boolean;
}

/**
 * 产品 → 导航项。
 *
 * 两个产品的导航刻意用同一张表描述、由同一段 JSX 渲染：
 * 侧栏只认这张表，因此「加一个产品的页面」或「调整条目顺序」都只改数据，不改渲染逻辑。
 *
 * **Trae 与 WorkBuddy 之间逐字同构**：同样的五项、同样的顺序、同样的路径尾段。
 * 这是刻意的约束而非巧合——产品页面结构一致时，用户在产品间来回切换不需要重建位置感。
 * Trae 侧不保留「概览 / 一键签到 / 登录态快照 / 系统日志」四个独立导航项：
 * 这些能力全部下沉到对应页面内部（签到并入账号管理、快照与运行日志并入设置），
 * 避免同一件事在不同产品上出现在不同位置。
 *
 * **Trae 的两条产品线共用同一组路径**（`/trae/...`）：路由本身不带变体维度，
 * 「当前是哪条线」由 URL 的 `?line=` 承载、由页面内部的变体切换器驱动
 * （见 `useTraeVariant`）。这样做的理由是：变体是**数据维度**而不是**页面维度** ——
 * 两条线的页面结构完全相同，再造一套 `/trae-cn/...` 路由会让每个页面组件被迫复制一份。
 */
const PRODUCT_NAV: Record<Product, readonly NavItem[]> = {
  workbuddy: [
    { to: "/", end: true, labelKey: "nav.accounts", icon: User },
    { to: "/token-stats", labelKey: "nav.tokenStats", icon: MessagesSquare },
    { to: "/credit-stats", labelKey: "nav.credits", icon: Sparkles },
    { to: "/api-service", labelKey: "nav.apiService", icon: Server },
    { to: "/settings", labelKey: "nav.settings", icon: Settings },
  ],
  trae: [
    { to: "/trae/accounts", labelKey: "nav.accounts", icon: User },
    { to: "/trae/token-stats", labelKey: "nav.tokenStats", icon: MessagesSquare },
    { to: "/trae/credits", labelKey: "nav.credits", icon: Sparkles },
    { to: "/trae/api-service", labelKey: "nav.apiService", icon: Server },
    { to: "/trae/settings", labelKey: "nav.settings", icon: Settings },
  ],
};

/**
 * 产品显示名对应的**文案键**（侧栏顶部产品切换器与导航无障碍标签的唯一来源）。
 *
 * 存键而不存成品文案：这是模块级常量，拿不到 `useT()`。渲染处 `t(PRODUCT_LABEL_KEY[…])`
 * 一秒翻译一次，语言切换才会反映到侧栏 —— 存成中文会让侧栏永远停在启动时的语言。
 *
 * Trae 分区显示 **`TraeWork`**：它管的就是 TraeWork 这条产品线
 * （国内版 `TRAE SOLO CN` 与国际版 `TRAE SOLO`），名字与客户端
 * `product.json` 的 `nameAlias`（`TraeWork CN` / `TraeWork`）及系统注册表显示名一致。
 *
 * 但**区域（国内版 / 国际版）仍然不写进侧栏**：它由分区内部顶部的区域切换器表达。
 * 把区域名塞进侧栏会让「国内版 / 国际版」看起来像两个独立产品，
 * 而它们共用同一组页面、同一套账号管理方式。同理，程序线（TraeWork / TraeCode）
 * 是**账号卡片上的一枚按钮**，也不进侧栏。
 *
 * 注意 `Product` 的**标识**仍是 `trae`（路由前缀 `/trae/...` 与 `PRODUCT_NAV` 的键都不动）：
 * 本次只改展示名，改标识会牵动路由表与全部 `/trae` 链接。
 */
const PRODUCT_LABEL_KEY: Record<Product, TranslationKey> = {
  // 产品名**刻意不翻译**：`WorkBuddy` / `TraeWork` 是商标，两种语言下写法相同，
  // 因此这两个键的 zh/en 值一致。走词表而非硬编码，是为了让「语言切换后侧栏整体重渲染」
  // 这件事在所有文案上保持一致行为，不给未来的改名留特例。
  workbuddy: "product.workbuddy",
  trae: "product.trae",
};

/** 产品首页：切到某产品时，若当前路由不属于它，就落到这里。 */
const PRODUCT_HOME: Record<Product, string> = {
  workbuddy: "/",
  trae: "/trae/accounts",
};

/** 路由 → 产品。Trae 的全部路由都在 `/trae` 前缀下，其余归 WorkBuddy。 */
function productFromPath(pathname: string): Product {
  return pathname === "/trae" || pathname.startsWith("/trae/") ? "trae" : "workbuddy";
}

function navLinkClass({ isActive }: { isActive: boolean }): string {
  return cn(
    "flex items-center gap-2.5 rounded-lg px-3 py-2.5 text-sm outline-none transition-colors focus-visible:ring-2 focus-visible:ring-sidebar-ring/50",
    isActive
      ? "bg-foreground/[0.06] font-medium text-foreground"
      : "text-muted-foreground hover:bg-foreground/[0.04] hover:text-foreground",
  );
}

/**
 * 每条 Trae 产品线的运行状态。
 *
 * 只在 Trae 分区激活时探测：在 WorkBuddy 分区下这次探测没有意义，
 * 而演示模式下 `get_trae_variants` 会直接抛错（按「未运行」处理即可）。
 *
 * **返回两条线各自的运行状态**（而不是单一的 true/false）：
 * 侧栏底部那颗圆点跟随**当前选中的那条线**，若只探测「Trae 是否在运行」，
 * 在「Trae Work 已关闭、Trae CN 在运行」时就会显示错误的绿灯。
 *
 * ## 走快照缓存的两个理由
 *
 * 1. **不让圆点在每次切换时先灰一下再变绿**：不缓存的话，切回 Trae 分区的那一帧
 *    状态是空的，圆点会闪一次「未运行」。
 * 2. **`freshMs` 给 60 秒**：这颗圆点是**装饰性**的（真实状态在 Trae 页面的状态条
 *    上），不值得每次切换都重探一次；窗口之外照旧后台重校验，每分钟也仍有一次定时刷新。
 *
 * 键在「不在 Trae 分区」时置为 `null`（不缓存、不探测），避免在 WorkBuddy 分区下白跑。
 */
function useTraeVariantRunning(active: boolean): Record<string, boolean> {
  const { data, refresh } = useCachedResource<TraeVariantStatus[]>(
    active ? TRAE_VARIANTS_KEY : null,
    loadTraeVariantStatuses,
    { freshMs: 60 * 1000 },
  );

  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => void refresh(), 60 * 1000);
    return () => window.clearInterval(timer);
  }, [active, refresh]);

  return useMemo(
    () => Object.fromEntries((data ?? []).map((item) => [item.variant, item.running])),
    [data],
  );
}

/** 侧栏顶部的产品切换：下方的导航与主区域页面都跟随它。 */
function ProductSwitch({
  product,
  onChange,
}: {
  product: Product;
  onChange: (next: Product) => void;
}) {
  const t = useT();

  /*
   * 两个产品 Tab：**图标 + 文字**。
   *
   * ## 为什么现在是两个而不是三个
   *
   * 曾经这里是三个 Tab（WorkBuddy / Trae Work / Trae CN），因为两条 Trae 产品线
   * 可以同机并存。但侧栏固定 220px、扣掉 `px-3` 后 tablist 只有 195px，
   * 三等分每格约 60px，而 12px 字号下 `WorkBuddy` = 65px、`Trae Work` = 56px、
   * `Trae CN` = 43px —— **三个全都放不下**。当时的妥协是压成纯图标 + Tooltip。
   *
   * 但纯图标本身就是一个更强的信号：**用户分不清两个几乎一样的 Trae 图标该点哪个**。
   * 两条线共用全部页面与路由，拆成两个顶级入口本来就多一层无意义的二选一，
   * 因此合并成一个 `Trae`。产品线的选择下沉到 Trae 页面内部（那里宽度充足，
   * 可以完整展示 `Trae Work` / `Trae CN` 并带各自的运行状态）。
   *
   * ## 为什么两个 Tab 也**不能等分**（浏览器实测，别再改回 `grid-cols-2`）
   *
   * 合并成两个后，`grid-cols-2` 的等分**仍然放不下** `WorkBuddy`：
   * tablist 外框 195px、扣 `p-1` 后内容区 187px、`gap-0.5` 占 2px ⇒ 每格 92px；
   * 再减 `px-2`（16px）+ 图标 15px + `gap-1.5`（6px），**留给文字的只有 55px**，
   * 而 12px/500 的 `WorkBuddy` 实测 `scrollWidth = 64px` —— 短 9px，被截成 `WorkBu…`。
   *
   * 因此改成**按内容自适应并居中**（`grid-cols-[auto_auto]` + `justify-center`）。
   * **实测（2026-09-21，1333×900，`px-1.5`）**：`WorkBuddy` 格 97px、`TraeWork` 格 85px，
   * 加 2px 间隙共 184px ≤ 内容区 193px，**余量 9px**；两个标签的
   * `scrollWidth == clientWidth`（64/64、52/52）⇒ **均未被截断**。
   *
   * 刻意**不**用「缩字号 / 压 padding 硬塞进等分格」：要等分放下 `WorkBuddy`，
   * `padX + gap` 只能有 13px（12px 字号）或 16px（11px 字号），余量仅 1~3px；
   * 换 DPI / 字体回退时会再次截断，不可靠。按内容排布不依赖这点余量。
   *
   * label 上的 `truncate` 保留，作为将来产品名变长时的兜底（当前不触发）。
   *
   * 两个图标仍然如实反映「这是两个独立体系」——这正是 WorkBuddy 侧的做法。
   * 注意第二格的标签是 **`TraeWork`**（本分区管的产品线），而它内部的**区域**
   * （国内版 / 国际版）与**程序位**（TraeWork / TraeCode）都在页面里选，见
   * `useTraeVariant` 与账号页的状态条 —— 侧栏只表达「进哪个产品分区」。
   */
  return (
    <Tabs
      value={product}
      onValueChange={(value) => onChange(value as Product)}
      className="mb-3 shrink-0"
    >
      <TabsList
        className="grid h-9 w-full grid-cols-[auto_auto] justify-center gap-0.5 rounded-xl border border-sidebar-border bg-sidebar-accent/60 p-1"
        aria-label={t("product.switchAria")}
      >
        <TabsTrigger
          value="workbuddy"
          className="h-7 w-full min-w-0 gap-1.5 rounded-lg px-1.5 text-xs font-medium data-[state=active]:bg-primary/15 data-[state=active]:shadow-none"
        >
          <WorkBuddyMark size={15} />
          <span className="truncate">{t(PRODUCT_LABEL_KEY.workbuddy)}</span>
        </TabsTrigger>
        <TabsTrigger
          value="trae"
          className="h-7 w-full min-w-0 gap-1.5 rounded-lg px-1.5 text-xs font-medium data-[state=active]:bg-primary/15 data-[state=active]:shadow-none"
        >
          {/* 两个产品的图标各自如实呈现；Trae 分区用 TraeWork 的图标
              （这里不区分区域与程序位，都在页面内部选）。 */}
          <TraeVariantMark variant="trae_work" size={15} />
          <span className="truncate">{t(PRODUCT_LABEL_KEY.trae)}</span>
        </TabsTrigger>
      </TabsList>
    </Tabs>
  );
}

/**
 * 侧栏底部的应用信息。
 *
 * 这里展示的是**本应用**（Buddy Switch）的版本，而不是所管理客户端的版本：
 * `status.version` 来自 `update::APP_VERSION`，此前挂在「WorkBuddy」名下会让人
 * 误以为它是 WorkBuddy 客户端的版本号。状态圆点跟随当前选中的产品。
 *
 * **只负责内容**：底部区块的边框、外边距与水平内边距归 `Layout` 里的那个
 * `<section>`——同一块里还要放打赏入口（见 `DonateButton`），而它在 webui 下
 * **仍然显示**（版本行则不显示），把容器留在 `Layout` 才能只写一处。
 */
function AppFooter({
  product,
  running,
  version,
}: {
  product: Product;
  running: boolean;
  version: string | undefined;
}) {
  const t = useT();
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);

  useEffect(() => {
    let disposed = false;

    async function checkForUpdate() {
      try {
        const result = await api.checkUpdate();
        if (!disposed) setInfo(result.ok ? result : null);
      } catch {
        // 左下角只展示可操作的升级状态，网络错误不打扰正常使用。
      }
    }

    void checkForUpdate();
    const timer = window.setInterval(() => void checkForUpdate(), 30 * 60 * 1000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, []);

  const hasUpdate = Boolean(info?.ok && info.hasUpdate && info.latest);
  const productLabel = t(PRODUCT_LABEL_KEY[product]);

  return (
    <>
      <div className="flex items-center gap-2 text-[13px] text-sidebar-foreground">
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="inline-flex">
              <StatusDot on={running} />
            </span>
          </TooltipTrigger>
          <TooltipContent side="top">
            {t(running ? "sidebar.running" : "sidebar.notRunning", { product: productLabel })}
          </TooltipContent>
        </Tooltip>
        <span className="min-w-0 flex-1 truncate">{t("sidebar.version")}</span>
        <div className="flex shrink-0 items-center gap-1.5">
          {/* 这里固定显示「版本」二字而非产品名：产品名已在侧栏顶部 Tab 与标题栏出现，
              重复一遍反而挤占了版本号的位置。版本号用更小字号并保持 tabular-nums，
              让数字在版本号变化时纵向对齐、不跳动。 */}
          <span className="shrink-0 text-[11px] tabular-nums text-sidebar-foreground/50">v{version || "?"}</span>
          {hasUpdate && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  type="button"
                  size="icon"
                  className="size-5 rounded-full p-0"
                  aria-label={t("sidebar.update")}
                  onClick={() => setDialogOpen(true)}
                >
                  <ArrowUp className="size-3" strokeWidth={2.5} aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="top">{t("sidebar.update")}</TooltipContent>
            </Tooltip>
          )}
        </div>
      </div>
      <UpdateInstallDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        update={info}
      />
    </>
  );
}

function Layout() {
  const t = useT();
  const location = useLocation();
  const navigate = useNavigate();

  /** 每个产品各自记住上次停留的页面，来回切换不会丢上下文。 */
  const lastPathRef = useRef<Record<Product, string>>({ ...PRODUCT_HOME });

  /**
   * 当前的产品分区。
   *
   * 路径只能回答「是 WorkBuddy 还是 Trae」，回答不了「Trae 里是看哪条产品线」——
   * 两条线共用 `/trae/...`。因此变体由 URL 里的 `?line=` 查询参数承载，
   * 但它**不再是分区维度**：它决定 Trae 页面内部展示哪条线，
   * 由 `TraeAccountsPage` 等页面自己读取（见 `useTraeVariant`）。
   */
  const [traeVariant, setTraeVariant] = useTraeVariant();
  const product: Product = productFromPath(location.pathname);

  const appVersion = useAccountsStore((s) => s.status?.version || s.global.status?.version);
  const workbuddyRunning = useAccountsStore((s) =>
    Boolean(s.status?.running || s.global.status?.running),
  );
  const traeRunning = useTraeVariantRunning(product === "trae");
  // Trae 分区的状态圆点跟随**当前选中的那条产品线**（而不是「Trae 是否有任意一条在跑」）：
  // 在「Trae Work 已关闭、Trae CN 在运行」时，只探「Trae 是否运行」会显示错误的绿灯。
  const running =
    product === "workbuddy" ? workbuddyRunning : Boolean(traeRunning[traeVariant]);

  const hasUnifiedTitleBar =
    api.isDesktop() && typeof navigator !== "undefined" && navigator.userAgent.includes("Macintosh");
  useCreditAutoRefresh();
  useWorkbuddyStatusRefresh();

  useEffect(() => {
    lastPathRef.current[product] = location.pathname;
  }, [product, location.pathname]);

  function switchProduct(next: Product) {
    if (next === product) return;
    // Trae 内部的产品线由 `?line=` 保持，切走再切回来时**不应该被重置**——
    // 用户上次看的是 Trae CN，回来时就还该是 Trae CN。这里的 `setTraeVariant`
    // 只在切**向** Trae 且 URL 尚无该参数时兜底（默认变体不写进 URL，
    // 因此这一步通常是空操作，保留它是为了「切走时 URL 上残留了别的产品的参数」
    // 这类边界不会让变体漂到错误的值）。
    if (next === "trae") setTraeVariant(traeVariant);
    navigate(lastPathRef.current[next] || PRODUCT_HOME[next]);
  }

  return (
    <div className="flex h-screen min-h-0 overflow-hidden bg-background">
      {hasUnifiedTitleBar ? (
        <div
          data-tauri-drag-region
          className="fixed inset-x-0 top-0 z-50 h-8"
          aria-hidden="true"
        />
      ) : null}
      <aside
        className={cn(
          "flex min-h-0 w-[220px] shrink-0 flex-col border-r border-sidebar-border bg-sidebar px-3 pb-4",
          hasUnifiedTitleBar ? "pt-20" : "pt-4",
        )}
      >
        <div className="flex items-center gap-2.5 px-1 pb-5">
          <AppIconMark size={36} className="drop-shadow-sm" />
          <div className="min-w-0">
            <div
              className="truncate text-[15px] leading-5 tracking-[-0.02em] text-sidebar-foreground/90"
              style={{
                fontFamily: '"Bricolage Grotesque Variable", "SF Pro Display", ui-sans-serif, sans-serif',
                fontWeight: 640,
              }}
            >
              Buddy Switch
            </div>
            {demoModeEnabled && (
              <Badge variant="secondary" className="mt-1 h-5 border-0 px-1.5 text-[10px] text-sidebar-foreground/60 shadow-none">
                {t("app.demoBadge")}
              </Badge>
            )}
          </div>
        </div>

        <ProductSwitch product={product} onChange={switchProduct} />

        <nav
          className="flex min-h-0 flex-1 flex-col gap-0.5"
          aria-label={t("product.navAria", { product: t(PRODUCT_LABEL_KEY[product]) })}
        >
          {PRODUCT_NAV[product].map((item) => (
            <NavLink key={item.to} to={item.to} end={item.end} className={navLinkClass}>
              <item.icon className="size-4" />
              {t(item.labelKey)}
            </NavLink>
          ))}
        </nav>

        {/* 侧栏底部区块：**通用设置**入口紧贴**版本号上方**，打赏在其上。
            选择这一位置的依据是「这里是侧栏唯一不随产品切换的区域」，因此**应用级设置**
            （外观 / 开机自启 / 自动更新）的入口固定在此：两个产品分区走同一份实现，
            见 `AppSettingsEntry`。它也不属于 `PRODUCT_NAV`，所以不会把
            「侧栏 5 项 / 路由 5:5」的既有约束变成 6:6。
            打赏在 webui 下**也显示**（版本行不显示），因此容器放在这里、
            由各子项共用边框与内边距，而不是塞进 `AppFooter`。 */}
        <section className="mt-auto flex flex-col gap-2.5 border-t border-sidebar-border px-2 pt-3 text-xs">
          <DonateButton />
          <AppSettingsEntry />
          {api.isWebui() && !demoModeEnabled ? null : (
            <AppFooter product={product} running={running} version={appVersion} />
          )}
        </section>
      </aside>
      <main
        className={cn(
          "min-w-0 flex-1 overflow-y-auto bg-background overscroll-contain",
          hasUnifiedTitleBar && "pt-16 [&>div]:pt-4",
        )}
      >
        <Outlet />
      </main>
    </div>
  );
}

export default function App() {
  const Router = pagesDemoHostingEnabled ? HashRouter : BrowserRouter;

  return (
    <TooltipProvider delayDuration={250}>
      <Router>
        <Routes>
          <Route element={<Layout />}>
            <Route path="/" element={<AccountsPage />} />
            <Route path="/credit-stats" element={<CreditStatsPage />} />
            <Route path="/token-stats" element={<TokenStatsPage />} />
            <Route path="/api-service" element={<ApiServicePage />} />
            <Route path="/settings" element={<SettingsPage />} />
            {/* Trae 侧与 WorkBuddy 侧逐条同构；`/trae` 本身重定向到账号管理，
                避免旧书签或外部链接落在空路由上。 */}
            <Route path="/trae" element={<Navigate to="/trae/accounts" replace />} />
            <Route path="/trae/accounts" element={<TraeAccountsPage />} />
            <Route path="/trae/token-stats" element={<TraeTokenStatsPage />} />
            <Route path="/trae/credits" element={<TraeCreditsPage />} />
            <Route path="/trae/api-service" element={<TraeApiServicePage />} />
            <Route path="/trae/settings" element={<TraeSettingsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
        <Toaster />
      </Router>
    </TooltipProvider>
  );
}
