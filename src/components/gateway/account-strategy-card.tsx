import { useState } from "react";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";

import { Card } from "@/components/ui/card";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import * as api from "@/lib/api";
import { useT } from "@/lib/i18n";
import { REGIONS, regionDescriptor } from "@/lib/region";
import { cn } from "@/lib/utils";
import type { TranslationKey } from "@/locales/zh";
import type { AccountMeta, AccountStrategy, Region } from "@/lib/types";
import { useAccountsStore } from "@/stores/accounts";
import { useGatewayStore } from "@/stores/gateway";

/** 只存键不存文案：语言切换时整张表才会跟着变。 */
const STRATEGY_OPTIONS: { value: string; labelKey: TranslationKey }[] = [
  { value: "current", labelKey: "wbStats.gateway.strategyCurrent" },
  { value: "pinned", labelKey: "wbStats.gateway.strategyPinned" },
  { value: "max_credits", labelKey: "wbStats.gateway.strategyMaxCredits" },
];

function accountLabel(account: AccountMeta): string {
  return account.nickname || account.email || account.uid || account.id;
}


/** 账号策略选择 + 当前选用账号展示（P0-8）。按 region 各自独立配置。 */
export function AccountStrategyCard({ className }: { className?: string }) {
  const t = useT();
  return (
    <Card className={cn("gap-0 py-0", className)}>
      <div className="border-b border-border/60 px-5 py-3">
        <span className="text-sm font-semibold">{t("wbStats.gateway.strategyTitle")}</span>
      </div>
      <div className="divide-y divide-border/60">
        {REGIONS.map((region) => (
          <RegionStrategyRow key={region} region={region} />
        ))}
      </div>
      <div className="border-t border-border/60 px-5 py-3 text-xs leading-5 text-muted-foreground">
        <p>{t("wbStats.gateway.strategyHint1")}</p>
        <p>{t("wbStats.gateway.strategyHint2")}</p>
        <p>{t("wbStats.gateway.strategyHint3")}</p>
      </div>
    </Card>
  );
}

function RegionStrategyRow({ region }: { region: Region }) {
  const t = useT();
  const view = useGatewayStore((s) => s.strategies[region]);
  const saveStrategy = useGatewayStore((s) => s.saveStrategy);
  const accounts = useAccountsStore((s) => (region === "cn" ? s.accounts : s.global.accounts));
  const [saving, setSaving] = useState(false);

  const strategy = view.strategy;

  async function persist(next: AccountStrategy) {
    setSaving(true);
    try {
      await saveStrategy(region, next);
      toast.success(t("wbStats.gateway.strategySaved"));
    } catch (e) {
      toast.error(t("wbStats.gateway.strategySaveFail"), { description: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  function onKindChange(next: string) {
    if (next === strategy.kind) return;
    if (next === "pinned") {
      const first = accounts[0]?.id;
      if (!first) {
        toast.error(t("wbStats.gateway.versionNoAccount", { version: regionDescriptor(region).versionLabel }));
        return;
      }
      const pinnedId = strategy.kind === "pinned" ? strategy.account_id : first;
      void persist({ kind: "pinned", account_id: pinnedId });
      return;
    }
    if (next === "max_credits") {
      void persist({ kind: "max_credits" });
      return;
    }
    void persist({ kind: "current" });
  }

  const selected = view.selected;
  const selectedText = selected
    ? selected.nickname || selected.email || selected.uid || selected.id
    : view.note || t("wbStats.gateway.noAvailableAccount");

  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-5 py-3">
      <span className="w-16 shrink-0 text-sm font-medium">{regionDescriptor(region).versionLabel}</span>
      <Select value={strategy.kind} onValueChange={onKindChange} disabled={saving}>
        <SelectTrigger
          size="sm"
          className="w-40"
          aria-label={t("wbStats.gateway.strategyAria", { version: regionDescriptor(region).versionLabel })}
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {STRATEGY_OPTIONS.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {t(option.labelKey)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {strategy.kind === "pinned" && (
        <Select
          value={strategy.account_id}
          onValueChange={(accountId) => void persist({ kind: "pinned", account_id: accountId })}
          disabled={saving || accounts.length === 0}
        >
          <SelectTrigger size="sm" className="w-44" aria-label={t("wbStats.gateway.pinnedAria")}>
            <SelectValue placeholder={t("wbStats.gateway.selectAccount")} />
          </SelectTrigger>
          <SelectContent>
            {accounts.map((account) => (
              <SelectItem key={account.id} value={account.id}>
                {accountLabel(account)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}

      <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
        {saving && <Loader2 className="size-3.5 animate-spin" />}
        {t("wbStats.gateway.current")}
        <span className="font-medium text-foreground">{selectedText}</span>
      </span>
    </div>
  );
}
