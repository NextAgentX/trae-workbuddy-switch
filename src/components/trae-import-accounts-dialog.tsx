import { useEffect, useRef, useState } from "react";
import { FileUp, Loader2 } from "lucide-react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
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
import type { TraeImportPreviewAccount, TraeVariantId } from "@/lib/trae-types";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 导入到哪个产品线的账号库（两条线的账号库分家）。 */
  variant: TraeVariantId;
  /** 导入完成后回调（参数为导入结果计数）。 */
  onImported?: (result: { imported: number; skipped: number; overwritten: number }) => void;
}

/** 导入预览账号展示名（脱敏展示：名字 / UID）。 */
function previewLabel(a: TraeImportPreviewAccount, t: Translate): string {
  return a.name || (a.userId ? t("trae.comp.import.uid", { uid: a.userId }) : t("trae.comp.import.item", { index: a.index + 1 }));
}

/**
 * 导入 Trae 账号弹框：选 JSON 文件 → 后端解析预览 → 勾选账号 → 导入合并。
 *
 * 与 WorkBuddy 的 `ImportAccountsDialog` 同构。预览只回「有无 JWT / refresh_token」
 * 的布尔量，**不回明文**——脱敏发生在 Rust 侧（`export_import::preview_accounts`），
 * 前端拿不到也可能是刻意不拿。
 */
export function TraeImportAccountsDialog({ open, onOpenChange, variant, onImported }: Props) {
  const t = useT();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [fileName, setFileName] = useState("");
  const [fileText, setFileText] = useState("");
  const [preview, setPreview] = useState<TraeImportPreviewAccount[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [parsing, setParsing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (open) {
      setFileName("");
      setFileText("");
      setPreview([]);
      setSelected(new Set());
      setParsing(false);
      setBusy(false);
      setError("");
    }
  }, [open]);

  function chooseFile() {
    fileInputRef.current?.click();
  }

  function onFileChange(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    // 清空 value，允许再次选择同一文件
    e.target.value = "";
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const text = String(reader.result ?? "");
      setFileName(file.name);
      setFileText(text);
      setParsing(true);
      setError("");
      api
        .traePreviewImportAccounts(text)
        .then((res) => {
          setPreview(res.accounts);
          setSelected(new Set(res.accounts.map((a) => a.index)));
        })
        .catch((err) => {
          setPreview([]);
          setSelected(new Set());
          setError(api.asError(err));
        })
        .finally(() => setParsing(false));
    };
    reader.readAsText(file);
  }

  const allSelected = preview.length > 0 && selected.size === preview.length;

  function toggleAll() {
    setSelected(allSelected ? new Set() : new Set(preview.map((a) => a.index)));
  }

  function toggle(index: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }

  async function doImport() {
    if (busy || parsing || !fileText || selected.size === 0) return;
    setBusy(true);
    setError("");
    try {
      const res = await api.traeImportAccounts(fileText, [...selected], variant);
      onImported?.(res);
      onOpenChange(false);
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
          <DialogTitle>{t("trae.comp.import.title")}</DialogTitle>
          <DialogDescription>{t("trae.comp.import.desc")}</DialogDescription>
        </DialogHeader>

        <input
          ref={fileInputRef}
          type="file"
          accept=".json,application/json"
          className="hidden"
          onChange={onFileChange}
        />

        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={chooseFile} disabled={busy}>
            <FileUp />
            {t("trae.comp.import.chooseFile")}
          </Button>
          {fileName && <span className="truncate text-xs text-muted-foreground">{fileName}</span>}
        </div>

        {parsing && (
          <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
            <Loader2 className="animate-spin" /> {t("trae.comp.import.parsing")}
          </div>
        )}

        {!parsing && preview.length > 0 && (
          <>
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">
                {t("trae.comp.import.summary", { total: preview.length, count: selected.size })}
              </span>
              <button type="button" className="text-primary hover:underline" onClick={toggleAll}>
                {allSelected ? t("trae.comp.import.deselectAll") : t("trae.comp.import.selectAll")}
              </button>
            </div>
            <div className="max-h-56 space-y-1 overflow-y-auto pr-1">
              {preview.map((a) => (
                <label
                  key={a.index}
                  className="flex cursor-pointer items-center gap-3 rounded-md border px-3 py-2 hover:bg-accent/50"
                >
                  <input
                    type="checkbox"
                    className="size-4 accent-primary"
                    checked={selected.has(a.index)}
                    onChange={() => toggle(a.index)}
                  />
                  <span className="min-w-0 flex-1 truncate text-sm">{previewLabel(a, t)}</span>
                  {!a.hasJwt && <Badge variant="outline">{t("trae.comp.import.missingJwt")}</Badge>}
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

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={busy}>
            {t("trae.comp.import.cancel")}
          </Button>
          <Button onClick={doImport} disabled={busy || parsing || selected.size === 0}>
            {busy ? t("trae.comp.import.busy") : t("trae.comp.import.submit")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
