import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useT } from "@/lib/i18n";
import { REGION_FILTERS, regionFilterLabel } from "@/lib/region";
import type { RegionFilter } from "@/lib/types";
import { cn } from "@/lib/utils";

interface RegionBarProps {
  /** 当前查询范围（cn / global / all）。 */
  value: RegionFilter;
  onChange: (next: RegionFilter) => void;
  /** 重新取数中：控件保持可点，仅禁用并给出无障碍提示。 */
  disabled?: boolean;
  /** 无障碍标签，便于区分多个页面上的同款控件。 */
  ariaLabel?: string;
  className?: string;
}

function isRegionFilter(value: string): value is RegionFilter {
  return value === "cn" || value === "global" || value === "all";
}

/**
 * 页面级范围切换条（层级 1）：决定「数据源是什么」，全页共享。
 * 文案统一走 `regionFilterLabel`（cn→国内版 / global→国际版 / all→合并）。
 */
export function RegionBar({ value, onChange, disabled, ariaLabel, className }: RegionBarProps) {
  const t = useT();
  return (
    <Tabs
      className={cn("min-w-0 gap-0", className)}
      value={value}
      onValueChange={(next) => {
        if (!isRegionFilter(next)) return;
        onChange(next);
      }}
    >
      <TabsList className="h-auto max-w-full flex-wrap" aria-label={ariaLabel ?? t("shared.region.scope.aria")}>
        {REGION_FILTERS.map((filter) => (
          <TabsTrigger
            key={filter}
            className="max-w-full whitespace-normal"
            value={filter}
            disabled={disabled}
            aria-busy={disabled && value === filter}
          >
            {regionFilterLabel(filter)}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  );
}
