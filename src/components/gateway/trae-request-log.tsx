import { Eraser, Loader2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { DemoAction } from "@/components/demo-action";
import { useT } from "@/lib/i18n";
import type { TraeGatewayLogEntry } from "@/lib/trae-types";
import { cn } from "@/lib/utils";

function formatClock(ts: number): string {
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return "—";
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}:${String(date.getSeconds()).padStart(2, "0")}`;
}

function formatTokens(value: number | null | undefined): string {
  if (value == null) return "—";
  return new Intl.NumberFormat("zh-CN").format(value);
}

function statusTone(status: number): string {
  if (status >= 200 && status < 300) return "text-emerald-600 dark:text-emerald-400";
  if (status === 429) return "text-amber-600 dark:text-amber-400";
  return "text-destructive";
}

/**
 * 最近 N 条 Trae 网关请求日志（仅元数据）+ 清空。
 *
 * 对齐 WorkBuddy `gateway/request-log.tsx` 骨架，但**不搬 `regionDescriptor`**：
 * Trae 无 region，日志行里也就没有版本列。
 *
 * 本组件是**纯展示**：数据与清空动作由页面持有（页面的 `loadAll` 已经拉了日志，
 * 组件再自拉一次只会得到两份可能不同步的快照）。
 */
export function TraeRequestLog({
  logs,
  clearing,
  onClear,
  className,
}: {
  logs: TraeGatewayLogEntry[];
  clearing: boolean;
  onClear: () => void;
  className?: string;
}) {
  const t = useT();

  return (
    <Card className={cn("gap-0 py-0", className)}>
      <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
        <span className="text-sm font-semibold">
          {t("trae.gateway.log.title", { count: logs.length })}
        </span>
        <DemoAction>
          <Button variant="ghost" size="sm" disabled={clearing || logs.length === 0} onClick={onClear}>
            {clearing ? <Loader2 className="animate-spin" /> : <Eraser />}
            {t("trae.gateway.log.clear")}
          </Button>
        </DemoAction>
      </div>

      {logs.length === 0 ? (
        <p className="px-5 py-6 text-center text-sm text-muted-foreground">
          {t("trae.gateway.log.empty")}
        </p>
      ) : (
        <div className="max-h-80 overflow-y-auto">
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-card text-xs text-muted-foreground">
              <tr className="border-b border-border/60">
                <th className="px-5 py-2 text-left font-medium">{t("trae.gateway.log.col.time")}</th>
                <th className="px-3 py-2 text-left font-medium">{t("trae.gateway.log.col.account")}</th>
                <th className="px-3 py-2 text-left font-medium">{t("trae.gateway.log.col.model")}</th>
                <th className="px-3 py-2 text-right font-medium">{t("trae.gateway.log.col.status")}</th>
                <th className="px-3 py-2 text-right font-medium">{t("trae.gateway.log.col.latency")}</th>
                <th className="px-5 py-2 text-right font-medium">{t("trae.gateway.log.col.tokens")}</th>
              </tr>
            </thead>
            <tbody>
              {[...logs].reverse().map((entry, index) => (
                <tr key={`${entry.ts}-${index}`} className="border-b border-border/40 last:border-0">
                  <td className="whitespace-nowrap px-5 py-2 font-mono text-xs">{formatClock(entry.ts)}</td>
                  <td className="max-w-[8rem] truncate px-3 py-2 text-xs text-muted-foreground">
                    {entry.account ? entry.account.slice(-8) : "—"}
                  </td>
                  <td className="max-w-[10rem] truncate px-3 py-2 text-xs text-muted-foreground">
                    {entry.model ?? "—"}
                  </td>
                  <td className={cn("px-3 py-2 text-right font-mono text-xs", statusTone(entry.status))}>
                    {entry.status}
                  </td>
                  <td className="px-3 py-2 text-right font-mono text-xs text-muted-foreground">
                    {entry.latencyMs}ms
                  </td>
                  <td className="px-5 py-2 text-right font-mono text-xs text-muted-foreground">
                    {formatTokens((entry.promptTokens ?? 0) + (entry.completionTokens ?? 0))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}
