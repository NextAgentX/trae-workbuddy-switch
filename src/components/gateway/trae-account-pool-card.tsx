import { Users } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { useT } from "@/lib/i18n";
import { TRAE_POOL_STATUS_LABELS } from "@/lib/trae-gateway";
import type { TraeGatewayStatus } from "@/lib/trae-types";
import { cn } from "@/lib/utils";

function formatCredits(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return "—";
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

function formatCount(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return "0";
  return new Intl.NumberFormat("zh-CN").format(value);
}

/** 池五态计数格（与页面内联版逐字一致：可路由 / 冷却中 / 会话失效 / 积分过期 / 零积分）。 */
const POOL_TILES = [
  { key: "available", labelKey: "trae.gateway.pool.tile.available", tone: "ok" as const },
  { key: "cooling", labelKey: "trae.gateway.pool.tile.cooling", tone: "warn" as const },
  { key: "disabled", labelKey: "trae.gateway.pool.tile.disabled", tone: "danger" as const },
  { key: "expired", labelKey: "trae.gateway.pool.tile.expired", tone: "warn" as const },
  { key: "zeroCredits", labelKey: "trae.gateway.pool.tile.zeroCredits", tone: "muted" as const },
] as const;

/**
 * Trae 网关「账号池」卡（承接自 `TraeApiServicePage` 的内联区块）。
 *
 * 替代 WorkBuddy 的 `AccountStrategyCard`：Trae 无 `accountStrategy` 后端契约，
 * 只有 `pool` 五态计数、逐账号可路由状态，以及后端给出的 `diagnose` 排查串。
 * 本组件是**纯展示**，状态由页面（`trae_gateway_status`）传入。
 */
export function TraeAccountPoolCard({
  status,
  className,
}: {
  status: TraeGatewayStatus | null;
  className?: string;
}) {
  const t = useT();
  const pool = status?.pool;

  return (
    <Card className={cn("gap-0 py-0", className)}>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <Users className="size-4 stroke-[1.75]" />
          {t("trae.gateway.pool.title")}
        </div>
        <span className="text-xs text-muted-foreground">
          {t("trae.gateway.pool.totalRequests", { count: formatCount(status?.totalRequests ?? 0) })}
        </span>
      </div>

      <div className="grid grid-cols-2 gap-0 border-b border-border/60 sm:grid-cols-5">
        {POOL_TILES.map((item, index) => (
          <div
            key={item.key}
            className={cn(
              "flex flex-col items-center justify-center px-3 py-4 text-center",
              index > 0 && "border-l border-border/60",
            )}
          >
            <span className="text-xs text-muted-foreground">{t(item.labelKey)}</span>
            <span
              className={cn(
                "mt-2 text-2xl font-semibold tabular-nums tracking-[-0.02em]",
                item.tone === "ok"
                  ? "text-emerald-600 dark:text-emerald-400"
                  : item.tone === "warn"
                    ? "text-amber-600 dark:text-amber-400"
                    : item.tone === "danger"
                      ? "text-destructive"
                      : "text-muted-foreground",
              )}
            >
              {pool?.[item.key] ?? 0}
            </span>
          </div>
        ))}
      </div>

      {(status?.accounts.length ?? 0) === 0 ? (
        <p className="px-5 py-6 text-center text-sm text-muted-foreground">
          {t("trae.gateway.pool.empty")}
        </p>
      ) : (
        <div className="divide-y divide-border/60">
          {status?.accounts.map((account) => {
            const meta = TRAE_POOL_STATUS_LABELS[account.status];
            return (
              <div key={account.uid} className="flex flex-wrap items-center gap-3 px-5 py-3">
                <span className="min-w-0 flex-1 truncate text-sm font-medium">{account.name}</span>
                <span className="text-xs text-muted-foreground tabular-nums">
                  {t("trae.gateway.pool.credits", { credits: formatCredits(account.credits) })}
                </span>
                <Badge
                  variant={meta.tone === "danger" ? "destructive" : "secondary"}
                  className={cn(
                    "shrink-0",
                    meta.tone === "ok" && "text-emerald-600 dark:text-emerald-400",
                    meta.tone === "warn" && "text-amber-600 dark:text-amber-400",
                  )}
                >
                  {meta.label}
                </Badge>
                {account.cooldownReason && (
                  <span className="w-full truncate text-xs text-muted-foreground sm:w-auto">
                    {account.cooldownReason}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}

      {(status?.diagnose.length ?? 0) > 0 && (
        <div className="border-t border-border/60 px-5 py-3">
          <p className="text-xs text-muted-foreground">
            {t("trae.gateway.pool.diagnoseNote")}
          </p>
          <ul className="mt-2 space-y-1">
            {status?.diagnose.map((line) => (
              <li key={line} className="break-all font-mono text-xs text-muted-foreground">
                {line}
              </li>
            ))}
          </ul>
        </div>
      )}
    </Card>
  );
}
