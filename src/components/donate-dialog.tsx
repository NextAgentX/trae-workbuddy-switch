import { useState } from "react";
import { Coffee, ExternalLink } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useT } from "@/lib/i18n";
import type { TranslationKey } from "@/locales/zh";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { GITHUB_REPOSITORY_URL, openReleaseUrl } from "@/lib/update";
import donateAlipay from "@/assets/donate-alipay.jpg";
import donateWechat from "@/assets/donate-wechat.png";

/**
 * 打赏渠道。收款码随前端产物一起打包（`src/assets/`，由 Vite 处理 base 路径），
 * 因此桌面端与 webui、以及 GitHub Pages 演示都能正常显示，不依赖外部图床。
 */
const DONATE_CHANNELS: { key: string; nameKey: TranslationKey; image: string }[] = [
  { key: "wechat", nameKey: "wbSettings.donate.wechat", image: donateWechat },
  { key: "alipay", nameKey: "wbSettings.donate.alipay", image: donateAlipay },
];

/**
 * 侧栏底部的打赏入口：**位于版本号上方**，点击后弹出收款码。
 *
 * ## 为什么做成弹窗而不是直接铺开两张码
 *
 * 侧栏固定 220px，收款码是竖版海报（约 744×1080），铺开会把版本号与运行状态挤出视野；
 * 而打赏是**低频**操作，不值得长期占据侧栏的固定高度。
 *
 * ## 为什么入口不做成图标按钮
 *
 * 侧栏底部已经有状态圆点与更新按钮两处图标入口，再加一枚无文字图标，
 * 用户只能靠猜。这里用「图标 + 文字」的整行按钮，语义自明。
 */
export function DonateButton() {
  const t = useT();
  const [open, setOpen] = useState(false);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 w-full justify-start gap-2 rounded-lg px-2 text-xs font-normal text-sidebar-foreground/70 hover:bg-foreground/[0.04] hover:text-sidebar-foreground"
        >
          <Coffee className="size-3.5" aria-hidden="true" />
          {t("wbSettings.donate.trigger")}
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("wbSettings.donate.title")}</DialogTitle>
          <DialogDescription>{t("wbSettings.donate.desc")}</DialogDescription>
        </DialogHeader>
        {/* 两张收款码的长宽比并不相同（1121×1527 / 1080×1620），若各自按宽度铺满，
            高度会差出十几像素、下面的渠道名对不齐。这里给**统一高度的容器 + object-contain**：
            图按自身比例缩放到容器内，多出来的部分留白，两张码的视觉高度与说明位置因此一致。
            **不要用 `object-cover`** —— 那会把二维码裁掉边缘，扫不出来。 */}
        <div className="grid grid-cols-2 gap-4">
          {DONATE_CHANNELS.map((channel) => (
            <figure key={channel.key} className="flex flex-col gap-2">
              <div className="flex h-[320px] items-center justify-center overflow-hidden rounded-lg border border-border bg-muted/30">
                <img
                  src={channel.image}
                  alt={t("wbSettings.donate.qrAlt", { name: t(channel.nameKey) })}
                  className="h-full w-full object-contain"
                />
              </div>
              <figcaption className="text-center text-sm font-medium">{t(channel.nameKey)}</figcaption>
            </figure>
          ))}
        </div>
        <DialogFooter className="sm:justify-between">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => void openReleaseUrl(GITHUB_REPOSITORY_URL)}
          >
            <ExternalLink className="size-4" aria-hidden="true" />
            {t("wbSettings.donate.repoHome")}
          </Button>
          <DialogClose asChild>
            <Button type="button" variant="outline" size="sm">
              {t("wbSettings.common.close")}
            </Button>
          </DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
