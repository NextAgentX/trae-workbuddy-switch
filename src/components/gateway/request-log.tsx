import { useState } from "react";
import { Eraser, Loader2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { DemoAction } from "@/components/demo-action";
import * as api from "@/lib/api";
import { useT } from "@/lib/i18n";
import { regionDescriptor } from "@/lib/region";
import { cn } from "@/lib/utils";
import type { GatewayLogEntry } from "@/lib/types";
import { useGatewayStore } from "@/stores/gateway";

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
  if (status >= 200 && status < 300) return "text-emerald-600";
  if (status === 429) return "text-amber-600";
  return "text-destructive";
}

/** 最近 N 条请求日志（仅元数据）+ 清空（P1-1）。 */
export function RequestLog({ className }: { className?: string }) {
  const t = useT();
  const logs = useGatewayStore((s) => s.logs);
  const clearLogs = useGatewayStore((s) => s.clearLogs);
  const loadLogs = useGatewayStore((s) => s.loadLogs);
  const [clearing, setClearing] = useState(false);

  async function onClear() {
    setClearing(true);
    try {
      await clearLogs();
      await loadLogs();
      toast.success(t("wbStats.gateway.cleared"));
    } catch (e) {
      toast.error(t("wbStats.gateway.clearFail"), { description: api.asError(e) });
    } finally {
      setClearing(false);
    }
  }

  return (
    <Card className={cn("gap-0 py-0", className)}>
      <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
        <span className="text-sm font-semibold">{t("wbStats.gateway.requestLog")}</span>
        <DemoAction>
          <Button variant="ghost" size="sm" onClick={() => void onClear()} disabled={clearing || logs.length === 0}>
            {clearing ? <Loader2 className="animate-spin" /> : <Eraser />}
            {t("wbStats.gateway.clear")}
          </Button>
        </DemoAction>
      </div>

      <div className="px-5 py-3">
        {logs.length === 0 ? (
          <p className="py-4 text-center text-sm text-muted-foreground">{t("wbStats.gateway.noRequestsLog")}</p>
        ) : (
          <div className="max-h-80 overflow-y-auto pr-1">
            {logs.slice(0, 50).map((entry, index) => (
              <LogRow key={`${entry.ts}-${index}`} entry={entry} />
            ))}
          </div>
        )}
      </div>
    </Card>
  );
}

function LogRow({ entry }: { entry: GatewayLogEntry }) {
  return (
    <div className="border-b border-border/60 py-2 text-xs last:border-b-0">
      <div className="grid min-w-0 grid-cols-[auto_auto_minmax(0,1fr)_auto_auto_auto] items-center gap-3 tabular-nums">
        <span className="text-muted-foreground">{formatClock(entry.ts)}</span>
        <span className="text-muted-foreground">{regionDescriptor(entry.region).versionLabel}</span>
        <span className="truncate font-medium" title={entry.account ?? undefined}>
          {entry.account || "—"}
          {entry.model && <span className="ml-2 font-normal text-muted-foreground">{entry.model}</span>}
        </span>
        <span className={cn("font-medium", statusTone(entry.status))}>{entry.status}</span>
        <span className="text-muted-foreground">{entry.latencyMs ? `${(entry.latencyMs / 1000).toFixed(1)}s` : "—"}</span>
        <span className="text-right text-muted-foreground">
          {entry.promptTokens != null || entry.completionTokens != null
            ? `${formatTokens((entry.promptTokens ?? 0) + (entry.completionTokens ?? 0))} tok`
            : "—"}
        </span>
      </div>
    </div>
  );
}
