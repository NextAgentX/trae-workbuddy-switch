import { useEffect, useState } from "react";

import { TraeVariantMark } from "@/components/product-marks";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useT } from "@/lib/i18n";
import {
  TRAE_VARIANT_FALLBACK,
  loadTraeVariantLogins,
  loadTraeVariantStatuses,
  type TraeVariantLogins,
} from "@/lib/trae-variant-status";
import type { TraeRegionId, TraeVariantStatus } from "@/lib/trae-types";
import { cn } from "@/lib/utils";
import { useTraeVariant } from "@/lib/use-trae-variant";

/**
 * 页头下方的**全宽产品线状态条**（账号页专用，替代页头右侧的 `TraeVariantSwitch`）。
 *
 * ## 结构照 WorkBuddy `AccountsPage.tsx` 的 region Tabs
 *
 * `Tabs + TabsList(h-auto gap-1 p-1)` + **两行 Tab**：
 * 第一行「状态圆点 + 变体名」，第二行「已登录: 账号名 / 未登录 / 未检测到」。
 * 状态圆点配色对齐 `AccountsPage.tsx` 的 `RegionTab`。
 *
 * ## 载体仍是 URL `?line=`（`useTraeVariant`）
 *
 * `value` / `onValueChange` 直接映射到 URL，切换**只改 URL**——页面 `loadAll`
 * 依赖 `variant` 会自动重取全部数据。
 *
 * ## 刻意**不使用** `TabsContent`
 *
 * 若为每个变体各放一个 `<TabsContent>` 面板，两个面板会各自挂载一次数据获取，
 * 每次切换都触发重复取数。这里不给任何 `TabsContent`：切换只改 URL，
 * 由既有 `loadAll` 重取一份数据即可。
 *
 * ## 数据来源：优先由宿主注入（单次渲染只读一遍）
 *
 * 宿主（账号页）为了渲染账号卡片上的「每个程序一枚切换按钮」，**本来就要**拿到
 * 各线的当前登录账号（`logins`）与各线的安装状态（`statuses`）。此时若本组件
 * 再自己读一遍，同一次渲染里会有两处各读一次 `get_trae_profiles`，
 * 且两者理论上可能给出不一致的快照（一个已刷新、一个还没）。
 *
 * 因此 `statuses` / `logins` 都是**可选注入**：
 * - 宿主传了 → 一律以宿主为准，本组件不再取数；
 * - 宿主没传（本组件被独立使用）→ 才走 `loadTraeVariantStatuses` /
 *   `loadTraeVariantLogins` 自行取数，回落策略与宿主完全同源（共用同一模块），
 *   不存在第二份兜底表。
 *
 * ## 第二行的刷新语义（`refreshKey`）
 *
 * 第二行展示的是**各条线自身的登录态**，与「当前选中哪条线」无关。因此自行取数时
 * 依赖里不含 `variant`——各线登录态不是「当前选中哪条线」的函数，不该因为它变就重读。
 *
 * ⚠️ 但别把这句话读成「切变体不会重读」，二者**不等价**：
 * 宿主 `TraeAccountsPage` 的 `loadAll` 自身依赖 `[variant]`（切变体要换数据源），
 * 而它成功返回时递增 `refreshKey`。所以切变体时本组件**仍会**随 `loadAll` 重读一次。
 * 这次重读是**幂等**的（读的依旧是各线自身的登录态，结果不变），不是缺陷。
 *
 * 真正要保证的是另一件事：账号被改动（切换登录态 / 新增 / 删除 / OAuth）后，
 * 宿主 `loadAll` 成功返回 → 递增 `refreshKey` → 本组件重读各线登录态（或由宿主
 * 直接注入新值）→ 第二行与页面主体保持一致，不会出现「主体已是新账号、顶部还写着旧账号」的谎报。
 */
export function TraeVariantBar({
  /** 各产品线的环境状态（并排视角）。不传则本组件自行探测。 */
  statuses,
  /** 各产品线各自的当前登录账号。不传则本组件自行读取。 */
  logins,
  /**
   * 账号 / 登录态被改动后，由宿主在 `loadAll` **成功**返回时递增的修订号。
   *
   * **它只承担「重读触发」这一件事**（仅在自行取数时有用），不是 `variant` 的替身：
   * 自行取数的 effect 依赖里不含 `variant`（各线登录态与「选中哪条线」无关）。
   */
  refreshKey,
  className,
}: {
  statuses?: TraeVariantStatus[];
  logins?: TraeVariantLogins;
  refreshKey?: number;
  className?: string;
}) {
  const t = useT();
  const [variant, setVariant] = useTraeVariant();
  const [probed, setProbed] = useState<TraeVariantStatus[] | null>(null);
  const [fetchedLogins, setFetchedLogins] = useState<TraeVariantLogins>({});

  const shouldProbe = statuses === undefined;
  useEffect(() => {
    if (!shouldProbe) return;
    let disposed = false;
    void loadTraeVariantStatuses().then((items) => {
      if (!disposed) setProbed(items);
    });
    return () => {
      disposed = true;
    };
  }, [shouldProbe]);

  // 渲染用的变体集合：注入值优先，其次自探结果，最后是兜底值。
  // 它是**唯一**的遍历源（登录态读取也用它）。
  const items = statuses ?? probed ?? TRAE_VARIANT_FALLBACK;

  const shouldFetchLogins = logins === undefined;
  useEffect(() => {
    if (!shouldFetchLogins) return;
    let disposed = false;
    void loadTraeVariantLogins(items).then((next) => {
      if (!disposed) setFetchedLogins(next);
    });
    return () => {
      disposed = true;
    };
  }, [shouldFetchLogins, refreshKey, items]);

  const resolvedLogins = logins ?? fetchedLogins;

  return (
    <Tabs value={variant} onValueChange={(value) => setVariant(value as TraeRegionId)} className={className}>
      <TabsList className="h-auto gap-1 p-1">
        {items.map((item) => {
          // 区域条目的登录态取**该区域主程序**（首个程序位）的 —— 登录态是客户端级的，
          // 而区域标识（`cn`/`global`）本身没有对应的客户端。
          const mark = item.programs?.[0]?.variant ?? item.variant;
          const login = resolvedLogins[mark] ?? null;
          const active = item.variant === variant;
          const presence = login ? "logged-in" : item.installed ? "installed" : "absent";
          // `login.name` 已经是「账号库里的名字，查不到则回落 uid」——
          // 回落链刻意只写在 `loadTraeVariantLogins` 一处，见 `TraeVariantLogin.name`。
          const presenceText = login
            ? t("trae.variant.bar.loggedIn", { name: login.name })
            : item.installed
              ? t("trae.variant.bar.notLoggedIn")
              : t("trae.variant.bar.notDetected");
          return (
            <TabsTrigger
              key={item.variant}
              value={item.variant}
              className="h-auto flex-col items-start gap-0.5 rounded-lg px-4 py-2 text-left"
            >
              <span
                className={cn(
                  "flex items-center gap-1.5 text-[13px] font-medium",
                  active ? "text-foreground" : "text-muted-foreground",
                )}
              >
                <span
                  className={cn(
                    "inline-block size-2 rounded-full",
                    presence === "logged-in"
                      ? "bg-primary"
                      : presence === "installed"
                        ? "bg-muted-foreground/40"
                        : "border border-muted-foreground/50",
                  )}
                  aria-hidden="true"
                />
                {/* 图标同登录态：区域用**主程序**的图标（区域自己不是客户端）。 */}
                <TraeVariantMark variant={mark} size={15} />
                {item.variantLabel}
              </span>
              <span className="pl-3.5 text-[11px] font-normal text-muted-foreground">{presenceText}</span>
            </TabsTrigger>
          );
        })}
      </TabsList>
    </Tabs>
  );
}
