import { useState } from "react";
import { KeyRound, Loader2, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { DemoAction } from "@/components/demo-action";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import * as api from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { useT } from "@/lib/i18n";
import { REGIONS, regionDescriptor } from "@/lib/region";
import { cn } from "@/lib/utils";
import type { ApiKeyRecord, Region } from "@/lib/types";
import { useGatewayStore } from "@/stores/gateway";

function formatDate(ts: number): string {
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return "—";
  return `${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

/** API Key 列表 + 创建对话框 + 一次性明文展示 + 吊销 / 删除（P0-3）。 */
export function ApiKeyTable({ className }: { className?: string }) {
  const t = useT();
  const keys = useGatewayStore((s) => s.keys);
  const createKey = useGatewayStore((s) => s.createKey);
  const revokeKey = useGatewayStore((s) => s.revokeKey);
  const deleteKey = useGatewayStore((s) => s.deleteKey);

  const [createOpen, setCreateOpen] = useState(false);
  const [name, setName] = useState("");
  const [region, setRegion] = useState<Region>("cn");
  const [creating, setCreating] = useState(false);
  const [plaintext, setPlaintext] = useState<{ value: string; name: string } | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<ApiKeyRecord | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<ApiKeyRecord | null>(null);
  const [busy, setBusy] = useState(false);

  function openCreate() {
    setName("");
    setRegion("cn");
    setCreateOpen(true);
  }

  async function onCreate() {
    const trimmed = name.trim();
    if (!trimmed) {
      toast.error(t("wbStats.gateway.nameRequired"));
      return;
    }
    setCreating(true);
    try {
      const result = await createKey(trimmed, region);
      const value = result.key;
      if (!value) {
        toast.error(t("wbStats.gateway.createdNoPlaintext"));
        return;
      }
      setCreateOpen(false);
      setPlaintext({ value, name: trimmed });
    } catch (e) {
      toast.error(t("wbStats.gateway.createFail"), { description: api.asError(e) });
    } finally {
      setCreating(false);
    }
  }

  async function confirmRevoke() {
    if (!revokeTarget) return;
    setBusy(true);
    try {
      await revokeKey(revokeTarget.id);
      toast.success(t("wbStats.gateway.revokedToast"), { description: revokeTarget.name });
      setRevokeTarget(null);
    } catch (e) {
      toast.error(t("wbStats.gateway.revokeFail"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    setBusy(true);
    try {
      await deleteKey(deleteTarget.id);
      toast.success(t("wbStats.gateway.deletedToast"), { description: deleteTarget.name });
      setDeleteTarget(null);
    } catch (e) {
      toast.error(t("wbStats.gateway.deleteFail"), { description: api.asError(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className={cn("gap-0 py-0", className)}>
      <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
        <span className="text-sm font-semibold">API Key</span>
        <DemoAction>
          <Button size="sm" onClick={openCreate}>
            <KeyRound />
            {t("wbStats.gateway.createKey")}
          </Button>
        </DemoAction>
      </div>

      <div className="px-5 py-3">
        {keys.length === 0 ? (
          <p className="py-4 text-center text-sm text-muted-foreground">{t("wbStats.gateway.noKeys")}</p>
        ) : (
          <div className="min-w-0 overflow-x-auto">
            <table className="w-full min-w-[560px] text-left text-sm">
              <thead>
                <tr className="text-xs text-muted-foreground">
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.name")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.version")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.prefix")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.createdAt")}</th>
                  <th className="pb-2 pr-4 font-medium">{t("wbStats.table.status")}</th>
                  <th className="pb-2 font-medium">{t("wbStats.table.actions")}</th>
                </tr>
              </thead>
              <tbody>
                {keys.map((key) => {
                  const revoked = key.revoked;
                  return (
                    <tr key={key.id} className="border-t border-border/60">
                      <td className="py-2 pr-4 font-medium">{key.name}</td>
                      <td className="py-2 pr-4">
                        <Badge variant="secondary" className="rounded-md">
                          {regionDescriptor(key.region).versionLabel}
                        </Badge>
                      </td>
                      <td className="py-2 pr-4 font-mono text-xs text-muted-foreground">{key.prefix}…</td>
                      <td className="py-2 pr-4 text-xs text-muted-foreground">{formatDate(key.createdAt)}</td>
                      <td className="py-2 pr-4">
                        {revoked ? (
                          <Badge variant="secondary" className="rounded-md text-muted-foreground">
                            {t("wbStats.gateway.revoked")}
                          </Badge>
                        ) : (
                          <Badge variant="success" className="rounded-md">
                            {t("wbStats.gateway.enabled")}
                          </Badge>
                        )}
                      </td>
                      <td className="py-2">
                        {revoked ? (
                          <Button variant="ghost" size="sm" className="text-destructive hover:text-destructive" onClick={() => setDeleteTarget(key)}>
                            <Trash2 />
                            {t("wbStats.gateway.delete")}
                          </Button>
                        ) : (
                          <Button variant="ghost" size="sm" onClick={() => setRevokeTarget(key)}>
                            {t("wbStats.gateway.revoke")}
                          </Button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* 创建对话框 */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.createTitle")}</DialogTitle>
            <DialogDescription>{t("wbStats.gateway.createDesc")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="key-name">{t("wbStats.gateway.nameLabel")}</Label>
              <Input
                id="key-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("wbStats.gateway.namePlaceholder")}
                spellCheck={false}
                autoComplete="off"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="key-region">{t("wbStats.gateway.regionLabel")}</Label>
              <Select value={region} onValueChange={(value) => setRegion(value as Region)}>
                <SelectTrigger id="key-region" className="w-full" aria-label={t("wbStats.gateway.regionAria")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {REGIONS.map((r) => (
                    <SelectItem key={r} value={r}>
                      {regionDescriptor(r).gatewayLabel}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={creating}>
              {t("wbStats.gateway.cancel")}
            </Button>
            <Button onClick={() => void onCreate()} disabled={creating}>
              {creating && <Loader2 className="animate-spin" />}
              {t("wbStats.gateway.create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 一次性明文展示 */}
      <Dialog open={plaintext !== null} onOpenChange={(open) => !open && setPlaintext(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.createdTitle")}</DialogTitle>
            <DialogDescription>{t("wbStats.gateway.createdDesc")}</DialogDescription>
          </DialogHeader>
          {plaintext && (
            <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2.5">
              <code className="min-w-0 flex-1 break-all font-mono text-xs">{plaintext.value}</code>
              <Button variant="outline" size="sm" onClick={() => void copyText(plaintext.value, t("wbStats.gateway.keyCopied"))}>
                {t("wbStats.gateway.copy")}
              </Button>
            </div>
          )}
          <DialogFooter>
            <Button onClick={() => setPlaintext(null)}>{t("wbStats.gateway.savedClose")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 吊销确认 */}
      <Dialog open={revokeTarget !== null} onOpenChange={(open) => !open && setRevokeTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.revokeTitle")}</DialogTitle>
            <DialogDescription>
              {t("wbStats.gateway.revokeDesc", { name: revokeTarget?.name ?? "" })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setRevokeTarget(null)} disabled={busy}>
              {t("wbStats.gateway.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmRevoke()} disabled={busy}>
              {t("wbStats.gateway.revoke")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 删除确认 */}
      <Dialog open={deleteTarget !== null} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbStats.gateway.deleteTitle")}</DialogTitle>
            <DialogDescription>{t("wbStats.gateway.deleteDesc", { name: deleteTarget?.name ?? "" })}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)} disabled={busy}>
              {t("wbStats.gateway.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => void confirmDelete()} disabled={busy}>
              {t("wbStats.gateway.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
