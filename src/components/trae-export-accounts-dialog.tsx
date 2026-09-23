import { useEffect, useState } from "react";
import { Download, FolderOpen, Loader2 } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import * as api from "@/lib/api";
import { useT, type Translate } from "@/lib/i18n";
import type { TraeAccount, TraeVariantId } from "@/lib/trae-types";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  accounts: TraeAccount[];
  /** 从哪个产品线的账号库导出（两条线的账号库分家）。 */
  variant: TraeVariantId;
  /** 导出完成后回调（参数为导出的账号数）。 */
  onExported?: (count: number) => void;
}

/** 账号展示名（与账号卡片一致）。 */
function accountLabel(a: TraeAccount, t: Translate): string {
  return a.name || t("trae.comp.export.uid", { uid: a.userId });
}

/**
 * 导出文件名：`trae-accounts-YYYY-MM-DD.json`。
 *
 * 刻意与 WorkBuddy 的 `buddy-switch-accounts-*.json` 区分：
 * 两份账号库的导出文件会被放在同一个下载目录里，文件名必须能自证归属，
 * 否则用户回导时极易拿错文件（Trae 的记录形状含 `UserID` 大写键，
 * 导进 WorkBuddy 会被当成无 uid 的条目）。
 */
function exportFileName(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `trae-accounts-${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}.json`;
}

/** 前端 Blob 下载（webui 浏览器用）。 */
function downloadJson(filename: string, data: unknown) {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/** 桌面端在系统文件管理器中定位导出文件（Windows 为资源管理器）。 */
async function revealInDir(path: string): Promise<void> {
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

/** 按平台显示文件管理器文案（与 update-install-dialog 的 userAgent 判断一致）。 */
function revealLabel(t: Translate): string {
  const ua = navigator.userAgent;
  if (ua.includes("Windows")) return t("trae.comp.export.reveal.windows");
  if (ua.includes("Linux")) return t("trae.comp.export.reveal.linux");
  return t("trae.comp.export.reveal.mac");
}

/**
 * 导出 Trae 账号弹框：多选账号 → 后端导出完整记录（含 JWT）→ 下载 JSON。
 *
 * 与 WorkBuddy 的 `ExportAccountsDialog` 同构（同样的勾选列表 / 保存对话框 /
 * 资源管理器定位三段），差异只有两处：账号主键是 `userId` 而非 `id`，
 * 以及导出文件的键名保持 Trae 参考实现形状（`UserID` 大写）。
 */
export function TraeExportAccountsDialog({ open, onOpenChange, accounts, variant, onExported }: Props) {
  const t = useT();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  /** 桌面端导出成功后的文件路径（webui 用浏览器下载，无此状态）。 */
  const [savedPath, setSavedPath] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setSelected(new Set());
      setBusy(false);
      setError("");
      setSavedPath(null);
    }
  }, [open]);

  const allSelected = accounts.length > 0 && selected.size === accounts.length;

  function toggleAll() {
    setSelected(allSelected ? new Set() : new Set(accounts.map((a) => a.userId)));
  }

  function toggle(userId: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(userId)) next.delete(userId);
      else next.add(userId);
      return next;
    });
  }

  async function doExport() {
    if (busy || selected.size === 0) return;
    setBusy(true);
    setError("");
    try {
      const ids = [...selected];
      if (api.isWebui()) {
        // webui：浏览器 Blob 下载
        const res = await api.traeExportAccounts(ids, variant);
        downloadJson(exportFileName(), res.accounts);
        onExported?.(res.accounts.length);
        onOpenChange(false);
      } else {
        // 桌面端：系统保存对话框选位置 → 后端写入该路径（WKWebView 不支持 `<a download>`）
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({
          title: t("trae.comp.export.saveTitle"),
          defaultPath: exportFileName(),
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (!path) return; // 用户取消保存对话框
        const res = await api.traeExportAccountsToPath(ids, path, variant);
        setSavedPath(res.path);
        onExported?.(ids.length);
      }
    } catch (e) {
      setError(api.asError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="min-w-0 overflow-x-hidden">
        <DialogHeader>
          <DialogTitle>{t("trae.comp.export.title")}</DialogTitle>
          <DialogDescription>{t("trae.comp.export.desc")}</DialogDescription>
        </DialogHeader>

        <Alert variant="warning">
          <AlertTitle>{t("trae.comp.export.warningTitle")}</AlertTitle>
          <AlertDescription>{t("trae.comp.export.warningBody")}</AlertDescription>
        </Alert>

        {accounts.length === 0 ? (
          <p className="py-4 text-center text-sm text-muted-foreground">{t("trae.comp.export.empty")}</p>
        ) : (
          <>
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">
                {t("trae.comp.export.summary", { total: accounts.length, count: selected.size })}
              </span>
              <button type="button" className="text-primary hover:underline" onClick={toggleAll}>
                {allSelected ? t("trae.comp.export.deselectAll") : t("trae.comp.export.selectAll")}
              </button>
            </div>
            <div className="max-h-56 space-y-1 overflow-y-auto pr-1">
              {accounts.map((a) => (
                <label
                  key={a.userId}
                  className="flex cursor-pointer items-center gap-3 rounded-md border px-3 py-2 hover:bg-accent/50"
                >
                  <input
                    type="checkbox"
                    className="size-4 accent-primary"
                    checked={selected.has(a.userId)}
                    onChange={() => toggle(a.userId)}
                  />
                  <span className="min-w-0 flex-1 truncate text-sm">{accountLabel(a, t)}</span>
                </label>
              ))}
            </div>
          </>
        )}

        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        {savedPath && (
          <Alert>
            <AlertTitle>{t("trae.comp.export.successTitle")}</AlertTitle>
            <AlertDescription className="space-y-2">
              <span className="block break-all font-mono text-xs">{savedPath}</span>
              <Button variant="outline" size="sm" onClick={() => void revealInDir(savedPath)}>
                <FolderOpen />
                {revealLabel(t)}
              </Button>
            </AlertDescription>
          </Alert>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={busy}>
            {savedPath ? t("trae.comp.export.done") : t("trae.comp.export.cancel")}
          </Button>
          {!savedPath && (
            <Button onClick={doExport} disabled={busy || selected.size === 0}>
              {busy ? <Loader2 className="animate-spin" /> : <Download />}
              {t("trae.comp.export.submit")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
