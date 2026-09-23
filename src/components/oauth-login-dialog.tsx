import { useEffect, useState } from "react";
import { ExternalLink } from "lucide-react";

import { Alert, AlertDescription } from "@/components/ui/alert";
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
import { useT } from "@/lib/i18n";
import type { AccountMeta, Region } from "@/lib/types";
import { useAccountsStore } from "@/stores/accounts";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 目标版本；缺省为国内版。 */
  region?: Region;
}

/** OAuth 扫码登录采集：发起 → 打开浏览器 → 轮询采集结果 → 入库。 */
export function OAuthLoginDialog({ open, onOpenChange, region }: Props) {
  const t = useT();
  const reconcileAccounts = useAccountsStore((s) => s.reconcileAccounts);

  const [busy, setBusy] = useState(false);
  const [loginId, setLoginId] = useState<string | null>(null);
  const [uri, setUri] = useState("");
  const [error, setError] = useState("");
  const [result, setResult] = useState<AccountMeta | null>(null);

  // 打开时重置
  useEffect(() => {
    if (open) {
      setBusy(false);
      setLoginId(null);
      setUri("");
      setError("");
      setResult(null);
    }
  }, [open]);

  // 轮询采集结果
  useEffect(() => {
    if (!loginId) return;
    let timer: number | undefined;
    let cancelled = false;

    const poll = async () => {
      try {
        const res = await api.oauthStatus(loginId, region);
        if (res.done) {
          if (res.result) {
            await reconcileAccounts(region);
            if (!cancelled) setResult(res.result);
          } else if (!cancelled) {
            setError(res.error || t("wbAccounts.dialog.oauthLoginFail"));
          }
          if (timer !== undefined) window.clearInterval(timer);
          return;
        }
        timer = window.setTimeout(poll, 1500);
      } catch (e) {
        if (!cancelled) setError(api.asError(e));
        if (timer !== undefined) window.clearInterval(timer);
      }
    };
    poll();

    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [loginId, reconcileAccounts, region]);

  async function start() {
    setBusy(true);
    setError("");
    try {
      const res = await api.oauthStart(region);
      setLoginId(res.loginId);
      setUri(res.verificationUri);
      // 按当前宿主能力打开验证页
      await openInBrowser(res.verificationUri);
    } catch (e) {
      setError(api.asError(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("wbAccounts.dialog.oauthTitle")}</DialogTitle>
          <DialogDescription>
            {t("wbAccounts.dialog.oauthDesc")}
          </DialogDescription>
        </DialogHeader>

        {!loginId && !result && (
          <div className="space-y-3">
            <Button onClick={start} disabled={busy} className="w-full">
              {busy ? t("wbAccounts.dialog.oauthStarting") : t("wbAccounts.dialog.oauthStart")}
            </Button>
          </div>
        )}

        {loginId && !result && (
          <div className="space-y-3">
            <Alert>
              <ExternalLink className="size-4" />
              <AlertDescription className="break-all">
                <a
                  href={uri}
                  target="_blank"
                  rel="noreferrer"
                  className="text-primary underline-offset-2 hover:underline"
                  onClick={(e) => {
                    // WebUI 直接使用浏览器默认链接行为，确保即使自动弹窗被拦截
                    // 也能通过用户点击打开验证页。
                    if (api.isWebui()) return;
                    e.preventDefault();
                    void openInBrowser(uri);
                  }}
                >
                  {uri}
                </a>
              </AlertDescription>
            </Alert>
            <p className="text-sm text-muted-foreground">
              {t("wbAccounts.dialog.oauthWaiting")}
            </p>
          </div>
        )}

        {result && (
          <Alert>
            <AlertDescription>
              {t("wbAccounts.dialog.oauthCollected", { name: result.nickname || result.email || result.id })}
            </AlertDescription>
          </Alert>
        )}

        {error && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("wbAccounts.dialog.oauthClose")}
          </Button>
          {result && (
            <Button onClick={() => onOpenChange(false)}>{t("wbAccounts.dialog.oauthDone")}</Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** WebUI 使用浏览器新标签页，Tauri 使用系统 opener。 */
async function openInBrowser(url: string): Promise<void> {
  if (api.isWebui()) {
    // 浏览器环境没有 Tauri 注入的 invoke；window.open 被拦截时由弹窗中的
    // 原生链接作为兜底，因此这里不把拦截视为 OAuth 失败。
    try {
      window.open(url, "_blank", "noopener,noreferrer");
    } catch {
      // 忽略自动弹窗失败；弹窗中已展示的原生链接仍可点击。
    }
    return;
  }

  const { openUrl } = await import("@tauri-apps/plugin-opener");
  return openUrl(url);
}
