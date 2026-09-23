import { useEffect, useState } from "react";

import { TraeVariantMark } from "@/components/product-marks";
import * as api from "@/lib/api";
import { useT } from "@/lib/i18n";
import { TRAE_VARIANT_FALLBACK } from "@/lib/trae-variant-status";
import type { TraeRegionId, TraeVariantStatus } from "@/lib/trae-types";
import { cn } from "@/lib/utils";
import { useTraeVariant } from "@/lib/use-trae-variant";

/**
 * Trae 分区内部的**产品线切换器**。
 *
 * ## 为什么产品线选择在这里，而不在侧栏
 *
 * 侧栏原先排了三个 Tab（WorkBuddy / Trae Work / Trae CN），因为两条 Trae 产品线
 * 可以同机并存。但两条线**共用全部页面与路由**（`/trae/...`），拆成两个顶级入口
 * 相当于把同一组页面在侧栏列两遍，用户要在两个看起来一样的入口之间做无意义的二选一；
 * 而侧栏 220px 宽度下连产品名都排不下（三个都得压成纯图标），
 * 压成图标后两个 Trae 又几乎无法区分 —— 这本身就是「不该在侧栏分」的信号。
 *
 * 因此侧栏合并为一个 `Trae`，产品线的选择下沉到这里：**页面主区域宽度充足**，
 * 可以完整展示线名 + 图标 + 运行状态，辨识成本归零。
 *
 * ## 载体仍是 URL 的 `?line=`
 *
 * 变体不做成组件内部 state：它必须能刷新保持、能被分享、能配合前进后退
 * （详见 `useTraeVariant` 的说明）。本组件只是它的一个「可视化 + 可点击」的外壳。
 *
 * ## 为什么把两条线都列出来（而不是只显示当前这条）
 *
 * 与前身 `TraeVariantMarks` 一致：用户需要一眼看到**本机装了哪几条线、各自在不在跑**，
 * 才能判断该切到哪条。只显示当前线会让「另一条线装了没」变成必须靠记忆的信息。
 */
export function TraeVariantSwitch({
  /** 已由外部拿到的变体数据；不传则本组件自行拉取（避免同页两次探测出不一致的快照）。 */
  statuses,
  className,
}: {
  statuses?: TraeVariantStatus[];
  className?: string;
}) {
  const t = useT();
  const [variant, setVariant] = useTraeVariant();
  const [local, setLocal] = useState<TraeVariantStatus[] | null>(null);

  const shouldFetch = statuses === undefined;
  useEffect(() => {
    if (!shouldFetch) return;
    let disposed = false;
    void (async () => {
      try {
        const result = await api.getTraeVariants();
        if (!disposed) setLocal(result.variants ?? []);
      } catch {
        // 演示模式 / 后端不可用：按「一条都探不到」处理，退回静态的两条线兜底。
        if (!disposed) setLocal([]);
      }
    })();
    return () => {
      disposed = true;
    };
  }, [shouldFetch]);

  const probed = statuses ?? local;

  /*
   * 探测不到任何区域时**仍然把两个区域列出来**（而不是 `return null`）。
   *
   * 这是与前一版实现的关键差别：切换器是**选版本/区域的控件**，它必须在后端不可用
   * 时依然可用（否则用户连切回另一个区域的入口都没有，只能改 URL）。
   * 探测结果只用来补「运行状态」，不决定「有哪些选项」——
   * 选项集合由 Rust 侧 `TraeRegion::all()` 固定（`platform::variants_status` 按区域列条目），
   * 前端不维护第二份清单，因此这里的兜底值只是「状态未知」的占位，
   * 不是一份会漂移的选项表。
   *
   * ⚠️ 契约在 2026-09-21 由「产品线」改为「**区域**」：切换器切的是区域（国内版/国际版），
   * 账号库/端点/冷却都按区域分家；程序位（TraeWork / TraeCode）是卡片上的第二层，
   * 由账号页的每程序一枚按钮表达，**不出现在本控件里**。
   *
   * `variantLabel` 由**共享兜底表**给出（`TRAE_VARIANT_FALLBACK`，与账号页的状态条同源），
   * 不再在本文件里另写一份 —— 两份清单迟早漂移，而漂移的表现是
   * 「同一个区域在两个控件里叫不同名字」。探测成功时显示的永远是后端返回的 `variantLabel`。
   */
  const fallback: TraeVariantStatus[] = TRAE_VARIANT_FALLBACK;

  const items: TraeVariantStatus[] =
    probed && probed.length > 0 ? probed : fallback;

  return (
    <div
      className={cn(
        "inline-flex items-center gap-1 rounded-xl border border-border bg-muted/40 p-1",
        className,
      )}
      role="tablist"
      aria-label={t("trae.variant.switch.aria")}
    >
      {items.map((item) => {
        const active = item.variant === variant;
        const state = item.running
          ? t("trae.variant.switch.running")
          : item.installed
            ? t("trae.variant.switch.installed")
            : t("trae.variant.switch.notDetected");
        const title = item.version
          ? t("trae.variant.switch.tipVersion", {
              label: item.variantLabel,
              state,
              version: item.version,
            })
          : t("trae.variant.switch.tip", { label: item.variantLabel, state });
        return (
          <button
            key={item.variant}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => setVariant(item.variant as TraeRegionId)}
            title={title}
            className={cn(
              "inline-flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs outline-none transition-colors",
              "focus-visible:ring-2 focus-visible:ring-ring/50",
              active
                ? "bg-background font-medium text-foreground shadow-sm"
                : "text-muted-foreground hover:bg-background/60 hover:text-foreground",
            )}
          >
            {/* 图标用该区域**主程序**的（区域自身不是客户端）。 */}
            <TraeVariantMark variant={item.programs?.[0]?.variant ?? item.variant} size={18} />
            <span>{item.variantLabel}</span>
            {/* 运行状态点：`installed` 但未运行时用暗点，未安装则完全不显示点——
                这里只表达「此刻在不在跑」，安装与否放在 title 里，不占视觉额度。 */}
            {item.installed && (
              <span
                aria-hidden
                className={cn(
                  "size-1.5 rounded-full",
                  item.running ? "bg-emerald-500" : "bg-muted-foreground/40",
                )}
              />
            )}
          </button>
        );
      })}
    </div>
  );
}
