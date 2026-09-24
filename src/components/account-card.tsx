import { ArrowRight, Check, CircleCheck, Clock3, Coins, Ellipsis, Loader2, Pencil, PlaneTakeoff, RefreshCw, Sparkles, Star, StickyNote, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { DemoAction } from "@/components/demo-action";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { CodeBuddyCnIdeMark, CodeBuddyMark, WorkBuddyMark } from "@/components/product-marks";
import { cn } from "@/lib/utils";
import { demoModeEnabled } from "@/lib/demo-mode";
import { displayText } from "@/lib/display-text";
import { useT, type Translate } from "@/lib/i18n";
import type { AccountMeta, CreditExpiry, CreditResource, TravelStatus } from "@/lib/types";

const AVATAR_TONES = [
  "bg-emerald-100 text-emerald-800",
  "bg-violet-100 text-violet-800",
  "bg-sky-100 text-sky-800",
  "bg-amber-100 text-amber-800",
  "bg-rose-100 text-rose-800",
  "bg-teal-100 text-teal-800",
] as const;

/**
 * 按账号名稳定地取一个头像底色。
 *
 * 与 `trae-account-card.tsx` 共用：两个产品分区的账号卡片必须是同一套视觉，
 * 各自维护一份色板迟早会漂移，因此这里导出而不是复制。
 */
export function avatarTone(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i += 1) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return AVATAR_TONES[hash % AVATAR_TONES.length];
}

function formatCredits(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(value);
}

function formatCreditExpiry(ts: number | null, t: Translate): string {
  if (!ts) return t("wbAccounts.card.forever");
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return t("wbAccounts.card.forever");
  return `${String(date.getMonth() + 1).padStart(2, "0")}/${String(date.getDate()).padStart(2, "0")}`;
}

function formatFullDate(ts: number | null): string {
  if (!ts) return "—";
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleDateString("zh-CN", { year: "numeric", month: "2-digit", day: "2-digit" });
}

function formatCreditUpdatedAt(ts: number | undefined): string {
  if (!ts) return "—";
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return "—";
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

function expiryClass(expired: boolean, expiringSoon: boolean): string {
  if (expired) return "text-destructive";
  if (expiringSoon) return "text-orange-600";
  return "text-muted-foreground";
}

function creditResources(credit?: CreditExpiry): CreditResource[] {
  return (credit?.resources ?? [])
    .filter((resource) => resource.remaining > 0)
    .map((resource, index) => ({ resource, index }))
    .sort((left, right) => {
      const leftExpiry = left.resource.expireAt ?? Number.POSITIVE_INFINITY;
      const rightExpiry = right.resource.expireAt ?? Number.POSITIVE_INFINITY;
      return leftExpiry === rightExpiry ? left.index - right.index : leftExpiry - rightExpiry;
    })
    .map(({ resource }) => resource);
}

function accountIdentity(account: AccountMeta): string {
  // 先归一再看：脏值（对象）会让下面的 `split` 直接抛 TypeError，
  // 而渲染期的 TypeError 会顺着错误边界外的路径把整棵树带走（issue #2）。
  const email = displayText(account.email);
  if (email) {
    const [local, domain] = email.split("@");
    if (!domain) return email;
    return `${local.slice(0, 1)}${"*".repeat(Math.max(3, local.length - 1))}@${domain}`;
  }
  const uid = displayText(account.uid);
  if (uid) return `UID · ${uid}`;
  return `ID · ${displayText(account.id) ?? "—"}`;
}

const chipClass = "rounded-md px-1.5 py-0 text-[11px] font-medium";

function travelIconChip({
  label,
  tooltip,
  variant,
}: {
  label: string;
  tooltip: string;
  variant: "secondary" | "success";
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant={variant} className={cn(chipClass, "px-1")} aria-label={label}>
          <PlaneTakeoff className="size-3.5" />
        </Badge>
      </TooltipTrigger>
      <TooltipContent side="top">{tooltip}</TooltipContent>
    </Tooltip>
  );
}

function formatTravelRemaining(arriveAt: number | null | undefined, t: Translate): string | null {
  if (!arriveAt || arriveAt <= 0) return null;
  const arriveMs = arriveAt > 1e12 ? arriveAt : arriveAt * 1000;
  const remainingMs = arriveMs - Date.now();
  if (remainingMs <= 0) return t("wbAccounts.card.arrivingSoon");
  const totalMinutes = Math.max(1, Math.ceil(remainingMs / 60_000));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours > 0 && minutes > 0) return t("wbAccounts.card.remainingHM", { hours, minutes });
  if (hours > 0) return t("wbAccounts.card.remainingH", { hours });
  return t("wbAccounts.card.remainingM", { minutes });
}

function travelTooltip(status: TravelStatus, t: Translate): string {
  const place = status.locationName?.trim();
  const credit = status.rewardCredit;
  const points = credit != null ? `+${credit}` : null;
  const remaining = formatTravelRemaining(status.arriveAt, t);
  if (status.label === "traveling") {
    const parts = [place, points ? t("wbAccounts.card.travelEst", { points }) : t("wbAccounts.card.traveling"), remaining].filter(Boolean);
    return parts.length > 0 ? parts.join(" · ") : t("wbAccounts.card.traveling");
  }
  if (status.label === "finished") {
    if (place && points) return `${place} · ${points}`;
    if (place) return `${place} · ${t("wbAccounts.card.finished")}`;
    if (points) return `${t("wbAccounts.card.finished")} · ${points}`;
    return t("wbAccounts.card.finished");
  }
  if (status.label === "no-buddy") return t("wbAccounts.card.noBuddy");
  return t("wbAccounts.card.notTraveled");
}

/** 按旅行状态渲染标签：无 Buddy / 未旅行 / 旅行中 / 已结束。 */
function travelChip(status: TravelStatus | undefined) {
  const t = useT();
  if (!status) return null;
  switch (status.label) {
    case "no-buddy":
      return <Badge variant="secondary" className={cn(chipClass, "text-muted-foreground")}>{t("wbAccounts.card.noBuddy")}</Badge>;
    case "traveling":
      return travelIconChip({ label: travelTooltip(status, t), tooltip: travelTooltip(status, t), variant: "secondary" });
    case "finished":
      return travelIconChip({ label: travelTooltip(status, t), tooltip: travelTooltip(status, t), variant: "success" });
    case "untraveled":
    default:
      return <Badge variant="secondary" className={cn(chipClass, "text-muted-foreground")}>{t("wbAccounts.card.notTraveled")}</Badge>;
  }
}

interface Props {
  account: AccountMeta;
  onDelete: (a: AccountMeta) => void;
  onCheckin?: (a: AccountMeta) => void;
  onRefresh?: (a: AccountMeta) => void;
  onSwitch?: (a: AccountMeta) => void;
  /**
   * 保存备注；返回 `true` 表示已写回（编辑器才会收起）。
   *
   * 由页面提供而不由卡片自己调 `api`：写回后要刷新整个账号列表，
   * 而卡片手里没有列表状态。返回布尔值而不是抛异常，是为了失败时
   * **保持编辑态**让用户能改完重试，而不是把半截输入丢掉。
   */
  onSaveRemark?: (a: AccountMeta, remark: string) => Promise<boolean>;
  todayCheckedIn?: boolean;
  /** 今日旅行状态（undefined=查询中/未知，不渲染标签） */
  travelStatus?: TravelStatus;
  credit?: CreditExpiry;
  creditLoading?: boolean;
  /** 该账号积分最近一次查询完成时间（时间戳） */
  creditUpdatedAt?: number;
  creditPriority?: boolean;
  workbuddyActive?: boolean;
  codebuddyCliConfigured?: boolean;
  codebuddyCliActive?: boolean;
  /** 任一 CodeBuddy CLI 账号切换正在进行，用于阻止并发切换。 */
  codebuddyCliBusy?: boolean;
  onSwitchCodebuddyCli?: (a: AccountMeta) => void;
  /** 当前卡片是否为正在切换的目标账号。 */
  codebuddyCliLoading?: boolean;
  /** CodeBuddy CN IDE 是否已安装（可切换）。 */
  codebuddyCnIdeAvailable?: boolean;
  codebuddyCnIdeActive?: boolean;
  codebuddyCnIdeBusy?: boolean;
  codebuddyCnIdeLoading?: boolean;
  onSwitchCodebuddyCnIde?: (a: AccountMeta) => void;
  featuresDisabled?: boolean;
  /** 紧凑模式：头部缩成一条、按钮图标化、无 footer */
  compact?: boolean;
}

function ProductCurrentState({ product, compact = false }: { product: "workbuddy" | "codebuddy" | "codebuddy-cn"; compact?: boolean }) {
  const t = useT();
  const title =
    product === "workbuddy"
      ? t("wbAccounts.card.currentWorkbuddy")
      : product === "codebuddy-cn"
        ? t("wbAccounts.card.currentIde")
        : t("wbAccounts.card.currentCli");
  return (
    <span
      role="status"
      aria-label={title}
      title={title}
      className={cn(
        "inline-flex items-center gap-2 rounded-full border border-primary/25 bg-primary/10 px-2.5 text-primary shadow-[inset_0_1px_0_rgba(255,255,255,.8)]",
        compact ? "h-7 text-xs" : "h-9",
      )}
    >
      {product === "workbuddy" ? (
        <WorkBuddyMark size={compact ? 18 : 22} />
      ) : product === "codebuddy-cn" ? (
        <CodeBuddyCnIdeMark size={compact ? 18 : 22} />
      ) : (
        <CodeBuddyMark size={compact ? 18 : 22} />
      )}
      <Check className={compact ? "size-3.5" : "size-4"} strokeWidth={2.25} />
    </span>
  );
}

export function AccountCard({ account, onDelete, onCheckin, onRefresh, onSwitch, onSaveRemark, todayCheckedIn, travelStatus, credit, creditLoading, creditUpdatedAt, creditPriority, workbuddyActive, codebuddyCliConfigured, codebuddyCliActive, codebuddyCliBusy, onSwitchCodebuddyCli, codebuddyCliLoading, codebuddyCnIdeAvailable, codebuddyCnIdeActive, codebuddyCnIdeBusy, codebuddyCnIdeLoading, onSwitchCodebuddyCnIde, featuresDisabled = true, compact = false }: Props) {
  const t = useT();
  const [resourcesOpen, setResourcesOpen] = useState(false);
  const [remarkEditing, setRemarkEditing] = useState(false);
  const [remarkDraft, setRemarkDraft] = useState("");
  const [remarkSaving, setRemarkSaving] = useState(false);
  const remarkInputRef = useRef<HTMLInputElement | null>(null);
  /**
   * 取消编辑时置位，用来抵掉紧随其后的那次 blur。
   *
   * 编辑态结束时输入框会被卸载，浏览器/React 可能仍补发一次 blur；
   * 不拦的话「按 Esc 取消」会被那次 blur 反向提交成一次保存。
   */
  const skipCommitRef = useRef(false);
  /**
   * 从「更多操作」菜单进入备注编辑时置位，用来**取消这一次菜单关闭的焦点归还**。
   *
   * Radix 菜单关闭时会把焦点还给触发按钮（`onCloseAutoFocus` 的默认行为，且发生在
   * 关闭动画之后 —— 实测约 150ms）。而编辑框那时**已经拿到焦点**，焦点被顶掉会补发
   * 一次 `onBlur`，被当成「用户点了别处」提交一次（草稿与已保存值相同 ⇒ 直接退出编辑态），
   * 表现为**编辑框闪一下就没了**。
   *
   * 只在「本次关闭确实是为了进入编辑态」时拦截，其他菜单项（刷新 Token / 删除账号）
   * 仍按 Radix 默认行为把焦点还给触发按钮。
   */
  const keepRemarkFocusRef = useRef(false);
  /**
   * 兜底归一（第三道闸）。
   *
   * 后端与 `lib/api.ts` 都已归一，但本组件是**纯展示**的，也可能被 demo 数据、
   * 测试或未来的新调用方直接喂进来。而 `name` / `remark` 会**直接当 React 子节点
   * 渲染** —— 对象会让 React 抛错并卸载整棵树 ⇒ 窗口一片白（issue #2）。
   * 宁可显示「未命名账号」，也不能崩。
   */
  const name = displayText(account.nickname) || displayText(account.uid) || t("wbAccounts.card.unnamed");
  const expired = typeof account.expiresAt === "number" && account.expiresAt < Date.now();
  const remark = displayText(account.remark)?.trim() || "";
  const identity = accountIdentity(account);

  useEffect(() => {
    if (!remarkEditing) return;
    // 等 DOM 提交后再聚焦。从菜单进入时菜单关闭还会再跑一次焦点归还，
    // 那一次已由 `onCloseAutoFocus` 拦掉（见 keepRemarkFocusRef）。
    const timer = window.setTimeout(() => remarkInputRef.current?.focus(), 0);
    return () => window.clearTimeout(timer);
  }, [remarkEditing]);

  /** 进入备注编辑态；返回 `true` 表示确实进入了（调用方据此决定要不要拦焦点）。 */
  function beginRemarkEdit(): boolean {
    if (featuresDisabled || !onSaveRemark || remarkSaving) return false;
    skipCommitRef.current = false;
    setRemarkDraft(remark);
    setRemarkEditing(true);
    return true;
  }

  function cancelRemarkEdit() {
    // 取消即丢弃草稿：下次进入编辑态重新从已保存值起算，不留半截输入。
    skipCommitRef.current = true;
    setRemarkEditing(false);
    setRemarkDraft("");
  }

  async function commitRemark() {
    if (skipCommitRef.current) {
      skipCommitRef.current = false;
      return;
    }
    if (!onSaveRemark || remarkSaving) return;
    if (remarkDraft.trim() === remark) {
      cancelRemarkEdit();
      return;
    }
    setRemarkSaving(true);
    const saved = await onSaveRemark(account, remarkDraft);
    setRemarkSaving(false);
    if (saved) setRemarkEditing(false);
  }
  const avatarClass = avatarTone(name);
  const resources = creditResources(credit);
  const visibleResources = resources.slice(0, 2);
  const expiringAmount = credit?.ok ? credit.expiringSoonRemaining ?? 0 : 0;
  /** 弹窗内展示还有剩余的资源包（已用完的隐藏），按到期时间升序 */
  const allResources = (credit?.resources ?? [])
    .filter((resource) => resource.remaining > 0)
    .map((resource, index) => ({ resource, index }))
    .sort((left, right) => {
      const leftExpiry = left.resource.expireAt ?? Number.POSITIVE_INFINITY;
      const rightExpiry = right.resource.expireAt ?? Number.POSITIVE_INFINITY;
      return leftExpiry === rightExpiry ? left.index - right.index : leftExpiry - rightExpiry;
    })
    .map(({ resource }) => resource);

  const activeProductCount = [workbuddyActive, codebuddyCliActive, codebuddyCnIdeActive].filter(Boolean).length;

  const statusChips = (
    <>
      {todayCheckedIn !== undefined && (
        <Badge variant={todayCheckedIn ? "success" : "secondary"} className={cn(chipClass, !todayCheckedIn && "text-muted-foreground")}><CircleCheck /> {todayCheckedIn ? t("wbAccounts.card.checkedIn") : t("wbAccounts.card.notCheckedIn")}</Badge>
      )}
      {travelChip(travelStatus)}
      {(account.needsRelogin || expired) && <Badge variant="warning" className={chipClass}>{account.needsRelogin ? t("wbAccounts.card.needRelogin") : t("wbAccounts.card.tokenExpired")}</Badge>}
      {creditPriority && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Badge variant="warning" className={cn(chipClass, "px-1")} aria-label={t("wbAccounts.card.priorityAria")}>
              <Star className="size-3.5" />
            </Badge>
          </TooltipTrigger>
          <TooltipContent side="top">{t("wbAccounts.card.priorityTip")}</TooltipContent>
        </Tooltip>
      )}
      {!compact && activeProductCount >= 2 && <Badge variant="secondary" className={cn(chipClass, "text-muted-foreground")}>{t("wbAccounts.card.toolsInUse", { n: activeProductCount })}</Badge>}
    </>
  );

  return (
    <TooltipProvider>
      <article className="flex min-w-0 flex-col overflow-hidden rounded-2xl border border-border bg-card shadow-[0_1px_2px_rgba(15,23,42,.025),0_10px_28px_rgba(15,23,42,.035)] transition-shadow hover:shadow-[0_2px_4px_rgba(15,23,42,.04),0_14px_34px_rgba(15,23,42,.055)]">
      <header
        className={cn(
          "relative flex items-center border-b border-border",
          compact ? "min-h-[52px] px-3.5 py-1.5" : "min-h-[104px] px-5 py-3",
          workbuddyActive ? "bg-primary/5" : codebuddyCliActive ? "bg-muted/60" : "bg-muted/30",
        )}
      >
        <div className="pointer-events-none absolute inset-0 overflow-hidden">
          <div
            className={cn(
              "absolute -right-10 -top-16 rounded-full blur-2xl",
              compact ? "size-20" : "size-24",
              workbuddyActive ? "bg-primary/15" : codebuddyCliActive ? "bg-muted/50" : "bg-muted/30",
            )}
          />
          {workbuddyActive && (
            <div className={cn("absolute top-[64%] -translate-y-1/2 opacity-[0.075] saturate-50 grayscale-[10%]", codebuddyCliActive ? "right-[68px] rotate-[8deg]" : "right-5 rotate-[7deg]")}>
              <WorkBuddyMark size={compact ? 40 : 56} />
            </div>
          )}
          {codebuddyCliActive && (
            <div className={cn("absolute top-[63%] -translate-y-1/2 opacity-[0.065] saturate-50 grayscale-[18%]", workbuddyActive ? "right-1 -rotate-[8deg]" : "right-5 -rotate-[7deg]")}>
              <CodeBuddyMark size={compact ? 38 : 54} />
            </div>
          )}
        </div>

        <div className={cn("absolute z-20", compact ? "right-2.5 top-1/2 -translate-y-1/2" : "right-3.5 top-3.5")}>
          {demoModeEnabled ? (
            <DemoAction>
              <Button variant="ghost" size="icon" className={cn("rounded-lg text-muted-foreground hover:text-foreground", compact ? "size-7" : "size-8")} aria-label={t("wbAccounts.card.manageAria", { name })} title={t("wbAccounts.card.moreActions")}>
                <Ellipsis />
              </Button>
            </DemoAction>
          ) : (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon" className={cn("rounded-lg text-muted-foreground hover:text-foreground", compact ? "size-7" : "size-8")} aria-label={t("wbAccounts.card.manageAria", { name })} title={t("wbAccounts.card.moreActions")}>
                  <Ellipsis />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                align="end"
                className="w-40"
                onCloseAutoFocus={(event) => {
                  if (!keepRemarkFocusRef.current) return;
                  keepRemarkFocusRef.current = false;
                  // 焦点留给刚渲染出来的备注输入框，别还给触发按钮（见 keepRemarkFocusRef）。
                  event.preventDefault();
                }}
              >
                <DropdownMenuItem disabled={featuresDisabled || !onRefresh} onSelect={() => onRefresh?.(account)}>
                  <RefreshCw />{t("wbAccounts.card.refreshToken")}
                </DropdownMenuItem>
                {todayCheckedIn === false && (
                  <DropdownMenuItem disabled={featuresDisabled || !onCheckin} onSelect={() => onCheckin?.(account)}>
                    <CircleCheck />{t("wbAccounts.card.manualCheckin")}
                  </DropdownMenuItem>
                )}
                <DropdownMenuItem
                  disabled={featuresDisabled || !onSaveRemark}
                  onSelect={() => {
                    // 见 keepRemarkFocusRef：这一次菜单关闭不要把焦点抢回去。
                    keepRemarkFocusRef.current = beginRemarkEdit();
                  }}
                >
                  <StickyNote />{t("wbAccounts.card.editRemark")}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem className="text-destructive focus:bg-destructive/5 focus:text-destructive" onSelect={() => onDelete(account)}>
                  <Trash2 />{t("wbAccounts.card.deleteAccount")}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>

        {compact ? (
          <div className="relative z-10 flex w-full min-w-0 items-center gap-2 pr-10">
            <h3 className="min-w-0 flex-1 truncate text-[13px] font-semibold leading-5" title={name}>{name}</h3>
            <div className="hidden shrink-0 items-center gap-1 min-[420px]:flex">{statusChips}</div>
            <div className="ml-auto flex shrink-0 items-center gap-1">
              {workbuddyActive ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span className="relative inline-flex size-7 items-center justify-center rounded-lg border border-primary/25 bg-primary/10 text-primary">
                      <WorkBuddyMark size={15} />
                      <span className="absolute -right-1 -top-1 flex size-3.5 items-center justify-center rounded-full bg-primary text-primary-foreground">
                        <Check className="size-2.5" strokeWidth={3} />
                      </span>
                    </span>
                  </TooltipTrigger>
                  <TooltipContent side="top">{t("wbAccounts.card.currentWorkbuddy")}</TooltipContent>
                </Tooltip>
              ) : demoModeEnabled ? (
                <DemoAction>
                  <Button variant="outline" size="icon" className="size-7 rounded-lg" aria-label={t("wbAccounts.card.setCurrentWorkbuddy")}>
                    <WorkBuddyMark size={15} />
                  </Button>
                </DemoAction>
              ) : (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="icon" className="size-7 rounded-lg" disabled={featuresDisabled || !onSwitch} onClick={() => onSwitch?.(account)} aria-label={t("wbAccounts.card.setCurrentWorkbuddy")}>
                      <WorkBuddyMark size={15} />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="top">{t("wbAccounts.card.setCurrentWorkbuddyRestart")}</TooltipContent>
                </Tooltip>
              )}
              {codebuddyCnIdeActive ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span className="relative inline-flex size-7 items-center justify-center rounded-lg border border-primary/25 bg-primary/10 text-primary">
                      <CodeBuddyCnIdeMark size={15} />
                      <span className="absolute -right-1 -top-1 flex size-3.5 items-center justify-center rounded-full bg-primary text-primary-foreground">
                        <Check className="size-2.5" strokeWidth={3} />
                      </span>
                    </span>
                  </TooltipTrigger>
                  <TooltipContent side="top">{t("wbAccounts.card.currentIde")}</TooltipContent>
                </Tooltip>
              ) : (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="icon" className="relative size-7 rounded-lg" disabled={featuresDisabled || !codebuddyCnIdeAvailable || !onSwitchCodebuddyCnIde || codebuddyCnIdeBusy} onClick={() => onSwitchCodebuddyCnIde?.(account)} aria-label={codebuddyCnIdeLoading ? t("wbAccounts.card.switchingIde") : t("wbAccounts.card.switchIde")} aria-busy={codebuddyCnIdeLoading}>
                      {codebuddyCnIdeLoading ? <Loader2 className="size-3.5 animate-spin" /> : <CodeBuddyCnIdeMark size={15} />}
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="top">{codebuddyCnIdeAvailable ? t("wbAccounts.card.switchIdeRestart") : t("wbAccounts.card.ideNotDetected")}</TooltipContent>
                </Tooltip>
              )}
              {codebuddyCliActive ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span className="relative inline-flex size-7 items-center justify-center rounded-lg border border-primary/25 bg-primary/10 text-primary">
                      <CodeBuddyMark size={15} />
                      <span className="absolute -right-1 -top-1 flex size-3.5 items-center justify-center rounded-full bg-primary text-primary-foreground">
                        <Check className="size-2.5" strokeWidth={3} />
                      </span>
                    </span>
                  </TooltipTrigger>
                  <TooltipContent side="top">{t("wbAccounts.card.currentCli")}</TooltipContent>
                </Tooltip>
              ) : (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="outline" size="icon" className="size-7 rounded-lg" disabled={featuresDisabled || !codebuddyCliConfigured || !onSwitchCodebuddyCli || codebuddyCliBusy} onClick={() => onSwitchCodebuddyCli?.(account)} aria-label={codebuddyCliLoading ? t("wbAccounts.card.switchingCli") : t("wbAccounts.card.setCurrentCli")} aria-busy={codebuddyCliLoading}>
                      {codebuddyCliLoading ? <Loader2 className="size-3.5 animate-spin" /> : <CodeBuddyMark size={15} />}
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="top">{codebuddyCliConfigured ? t("wbAccounts.card.setCurrentCli") : t("wbAccounts.card.connectCliFirst")}</TooltipContent>
                </Tooltip>
              )}
            </div>
          </div>
        ) : (
          <div className={cn("relative z-10 flex w-full min-w-0 items-center gap-3", workbuddyActive || codebuddyCliActive ? "pr-[112px]" : "pr-10")}>
            <div className={cn("flex size-12 shrink-0 items-center justify-center rounded-full text-base font-semibold ring-4 ring-white/65", avatarClass)}>{name.charAt(0).toUpperCase()}</div>
            <div className="min-w-0 flex-1">
              <h3 className="truncate text-sm font-semibold leading-5" title={name}>{name}</h3>
              <p className="mt-0.5 truncate text-xs leading-5 text-muted-foreground" title={identity}>{identity}</p>
              <div className="mt-1.5 flex min-w-0 flex-wrap items-center gap-1.5">{statusChips}</div>
            </div>
          </div>
        )}
      </header>

      <section className={cn("flex min-w-0 flex-1 flex-col", compact ? "px-3.5 pb-3 pt-3" : "px-5 pb-4 pt-4")}>
        {/* 备注就地编辑。
            紧凑模式**只在有备注或正在编辑时**渲染这一行 —— 那里本来就是为了多塞几张卡，
            给每张卡都加一条空占位行等于白送纵向空间。
            普通模式的空态常显（弱化色）而不是藏进 hover：备注的价值就在于「我记得要看它」，
            藏起来等于没人会用。 */}
        {remarkEditing ? (
          <Input
            ref={remarkInputRef}
            value={remarkDraft}
            disabled={remarkSaving}
            onChange={(event) => setRemarkDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void commitRemark();
              } else if (event.key === "Escape") {
                event.preventDefault();
                cancelRemarkEdit();
              }
            }}
            onBlur={() => void commitRemark()}
            maxLength={80}
            placeholder={t("wbAccounts.card.remarkPlaceholder")}
            spellCheck={false}
            autoComplete="off"
            aria-label={t("wbAccounts.card.remarkAria", { name })}
            className={cn("mb-3 h-7 w-full text-xs", compact && "mb-2")}
          />
        ) : remark ? (
          <button
            type="button"
            onClick={beginRemarkEdit}
            disabled={featuresDisabled || !onSaveRemark}
            title={remark}
            aria-label={t("wbAccounts.card.editRemarkAria", { name })}
            className={cn(
              "-mx-1 mb-3 flex w-[calc(100%+0.5rem)] min-w-0 items-center gap-1.5 rounded-md px-1 text-left text-[11px] leading-4 text-foreground/80 transition-colors hover:bg-foreground/[0.04] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40",
              compact && "mb-2",
            )}
          >
            <StickyNote className="size-3.5 shrink-0 stroke-[1.75] text-muted-foreground" aria-hidden="true" />
            <span className="min-w-0 flex-1 truncate">{remark}</span>
          </button>
        ) : compact ? null : (
          <button
            type="button"
            onClick={beginRemarkEdit}
            disabled={featuresDisabled || !onSaveRemark}
            className="-mx-1 mb-3 flex w-[calc(100%+0.5rem)] items-center gap-1.5 rounded-md px-1 text-left text-[11px] leading-4 text-muted-foreground/70 transition-colors hover:bg-foreground/[0.04] hover:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
          >
            <Pencil className="size-3 shrink-0" aria-hidden="true" />
            {t("wbAccounts.card.addRemark")}
          </button>
        )}

        {creditLoading ? (
          <div className="flex items-center gap-2 py-3 text-sm text-muted-foreground"><Loader2 className="size-4 animate-spin" />{t("wbAccounts.card.loadingCredits")}</div>
        ) : !credit ? (
          <div className="py-3 text-sm text-muted-foreground">{t("wbAccounts.card.waitingCredits")}</div>
        ) : !credit.ok ? (
          <div className="flex min-w-0 items-center gap-2 py-3 text-sm text-destructive" title={credit.error}>
            <Coins className="size-4 shrink-0" />
            <span className="min-w-0 truncate">{credit.error || t("wbAccounts.card.creditFailed")}</span>
          </div>
        ) : (
          <>
            <div className="flex items-baseline gap-x-3 gap-y-1">
              <span className="flex items-center gap-1.5">
                <Sparkles className="size-4 shrink-0 stroke-[1.75] text-muted-foreground" aria-hidden="true" />
                <strong className={cn("font-semibold leading-none tabular-nums tracking-[-0.025em]", compact ? "text-[20px]" : "text-[22px]")} style={{ fontFamily: '"Bricolage Grotesque Variable", "SF Pro Display", ui-sans-serif, sans-serif' }}>{formatCredits(credit.totalRemaining ?? 0)}</strong>
              </span>
              <span className={cn("text-muted-foreground", compact ? "text-[11px]" : "text-xs")}>{t("wbAccounts.card.creditPacks", { n: resources.length })}</span>
              <div className={cn("ml-auto flex items-center gap-1.5 text-muted-foreground", compact ? "text-[11px]" : "text-xs")} title={expiringAmount > 0 ? t("wbAccounts.card.expireSoon", { amount: formatCredits(expiringAmount) }) : resources[0]?.expireAt ? t("wbAccounts.card.nextExpiry", { date: formatCreditExpiry(resources[0].expireAt, t) }) : t("wbAccounts.card.creditsForever")}>
                <Clock3 className="size-3.5 shrink-0" />
                <span className="whitespace-nowrap tabular-nums">{creditUpdatedAt ? `${formatCreditUpdatedAt(creditUpdatedAt)} ${t("wbAccounts.card.updated")}` : "—"}</span>
              </div>
            </div>

            <div className={cn("text-[11px] font-medium text-muted-foreground", compact ? "mt-3" : "mt-4")}>{t("wbAccounts.card.expiringSoonTitle")}</div>
            <div className={cn(compact ? "mt-1.5 space-y-2" : "mt-2 space-y-2.5")}>
              {visibleResources.length > 0 ? visibleResources.map((resource, index) => {
                const resourceName = resource.packageName || resource.packageCode || t("wbAccounts.card.creditPackFallback");
                const ratio = resource.total > 0 ? Math.min(100, Math.max(0, (resource.remaining / resource.total) * 100)) : 0;
                return (
                  <div key={`${resource.packageCode ?? "resource"}-${resource.expireAt ?? "none"}-${index}`} className="min-w-0" title={t("wbAccounts.card.resourceTitle", { name: resourceName, remaining: formatCredits(resource.remaining), total: formatCredits(resource.total), expiry: formatCreditExpiry(resource.expireAt, t) })}>
                    <div className={cn("grid min-w-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3", compact ? "text-[11px]" : "text-xs")}>
                      <span className={cn("rounded-lg bg-muted/80 font-medium tabular-nums text-foreground", compact ? "px-1.5 py-0.5" : "px-2 py-1")}>{t("wbAccounts.card.creditsUnit", { n: formatCredits(resource.remaining) })}</span>
                      <span className="truncate text-muted-foreground">{resourceName}</span>
                      <span className={cn("whitespace-nowrap tabular-nums", expiryClass(resource.expired, resource.expiringSoon))}>{formatCreditExpiry(resource.expireAt, t)}</span>
                    </div>
                    <div className={cn("h-1 overflow-hidden rounded-full bg-muted", compact ? "mt-1" : "mt-1.5")} aria-hidden="true">
                      <div className={cn("h-full rounded-full", resource.expiringSoon || resource.expired ? "bg-orange-500" : "bg-primary")} style={{ width: `${ratio}%` }} />
                    </div>
                  </div>
                );
              }) : <div className="py-1 text-[11px] text-muted-foreground">{t("wbAccounts.card.noCredits")}</div>}
            </div>

            {resources.length > 2 && (
              <button type="button" className={cn("inline-flex w-fit items-center gap-1.5 font-medium text-primary transition-colors hover:text-primary/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30", compact ? "mt-2 text-[11px]" : "mt-3 text-xs")} onClick={() => setResourcesOpen(true)}>
                {t("wbAccounts.card.viewAllPacks")}
                <ArrowRight className="size-3.5" />
              </button>
            )}
          </>
        )}
      </section>

      {!compact && (
        <footer className="flex flex-wrap items-center gap-2.5 border-t px-5 py-2.5">
          {workbuddyActive ? <ProductCurrentState product="workbuddy" compact /> : demoModeEnabled ? (
            <DemoAction>
              <Button variant="outline" size="sm" className="h-7 rounded-full px-2.5 pr-3.5 text-xs" aria-label={t("wbAccounts.card.setCurrentWorkbuddy")}>
                <WorkBuddyMark size={18} /><span>{t("wbAccounts.card.setCurrent")}</span>
              </Button>
            </DemoAction>
          ) : (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="outline" size="sm" className="h-7 rounded-full px-2.5 pr-3.5 text-xs" disabled={featuresDisabled || !onSwitch} onClick={() => onSwitch?.(account)} aria-label={t("wbAccounts.card.setCurrentWorkbuddy")}>
                  <WorkBuddyMark size={18} /><span>{t("wbAccounts.card.setCurrent")}</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent side="top">{t("wbAccounts.card.setCurrentWorkbuddyRestart")}</TooltipContent>
            </Tooltip>
          )}
          {codebuddyCnIdeActive ? <ProductCurrentState product="codebuddy-cn" compact /> : (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="outline" size="sm" className="h-7 rounded-full px-2.5 pr-3.5 text-xs" disabled={featuresDisabled || !codebuddyCnIdeAvailable || !onSwitchCodebuddyCnIde || codebuddyCnIdeBusy} onClick={() => onSwitchCodebuddyCnIde?.(account)} aria-label={codebuddyCnIdeLoading ? t("wbAccounts.card.switchingIde") : t("wbAccounts.card.switchIde")} aria-busy={codebuddyCnIdeLoading}>
                  {codebuddyCnIdeLoading ? <Loader2 className="size-4 animate-spin" /> : <CodeBuddyCnIdeMark size={18} />}<span>{codebuddyCnIdeLoading ? t("wbAccounts.card.switching") : "IDE"}</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent side="top">{codebuddyCnIdeAvailable ? t("wbAccounts.card.switchIdeRestart") : t("wbAccounts.card.ideNotDetected")}</TooltipContent>
            </Tooltip>
          )}
          {codebuddyCliActive ? <ProductCurrentState product="codebuddy" compact /> : (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="outline" size="sm" className="h-7 rounded-full px-2.5 pr-3.5 text-xs" disabled={featuresDisabled || !codebuddyCliConfigured || !onSwitchCodebuddyCli || codebuddyCliBusy} onClick={() => onSwitchCodebuddyCli?.(account)} aria-label={codebuddyCliLoading ? t("wbAccounts.card.switchingCli") : t("wbAccounts.card.setCurrentCli")} aria-busy={codebuddyCliLoading}>
                  {codebuddyCliLoading ? <Loader2 className="size-4 animate-spin" /> : <CodeBuddyMark size={18} />}<span>{codebuddyCliLoading ? t("wbAccounts.card.switching") : t("wbAccounts.card.cliCurrent")}</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent side="top">{codebuddyCliConfigured ? t("wbAccounts.card.setCurrentCli") : t("wbAccounts.card.connectCliFirst")}</TooltipContent>
            </Tooltip>
          )}
        </footer>
      )}
      </article>

      <Dialog open={resourcesOpen} onOpenChange={setResourcesOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("wbAccounts.card.allPacksTitle")}</DialogTitle>
            <DialogDescription>{t("wbAccounts.card.allPacksDesc", { name, n: allResources.length })}</DialogDescription>
          </DialogHeader>
          {allResources.length === 0 ? (
            <div className="px-1 py-6 text-center text-sm text-muted-foreground">{t("wbAccounts.card.noResourcePacks")}</div>
          ) : (
            <div className="max-h-[60vh] min-w-0 overflow-y-auto divide-y divide-border/60">
              {allResources.map((resource, index) => {
                const ratio = resource.total > 0 ? Math.min(100, Math.max(0, (resource.remaining / resource.total) * 100)) : 0;
                return (
                  <div key={`${resource.packageCode || resource.packageName || "resource"}-${index}`} className="min-w-0 py-3 first:pt-0 last:pb-0">
                    <div className="flex min-w-0 items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="truncate text-sm font-medium">{resource.packageName || resource.packageCode || t("wbAccounts.card.unnamedPack")}</div>
                        <div className="mt-1 text-[11px] text-muted-foreground">
                          {resource.expired
                            ? t("wbAccounts.card.expired")
                            : resource.expiringSoon
                              ? t("wbAccounts.card.expiresIn7")
                              : resource.expireAt
                                ? t("wbAccounts.card.expiresOn", { date: formatFullDate(resource.expireAt) })
                                : t("wbAccounts.card.forever")}
                        </div>
                      </div>
                      <div className="shrink-0 text-right text-xs">
                        <div className="font-medium">{formatCredits(resource.remaining)} / {formatCredits(resource.total)}</div>
                        <div className="mt-1 text-[11px] text-muted-foreground">{t("wbAccounts.card.used", { n: formatCredits(resource.used) })}</div>
                      </div>
                    </div>
                    <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted" aria-hidden="true">
                      <div className={cn("h-full rounded-full", resource.expired ? "bg-destructive/60" : resource.expiringSoon ? "bg-orange-500/80" : "bg-primary/75")} style={{ width: `${ratio}%` }} />
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </TooltipProvider>
  );
}
