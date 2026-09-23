import { toast } from "sonner";

import { t } from "@/lib/i18n";

/** 复制文本到剪贴板；失败时给出可读提示。用于接入指引 / Base URL / 一次性 Key。 */
export async function copyText(text: string, label = t("shared.clipboard.copied")): Promise<void> {
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
    toast.success(label);
  } catch {
    toast.error(t("shared.clipboard.failed"));
  }
}
