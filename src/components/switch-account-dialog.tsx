import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, ExternalLink, Folder, Loader2 } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";

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
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import * as api from "@/lib/api";
import { decodeError } from "@/lib/error-code";
import { REGIONS, regionLabel } from "@/lib/region";
import { useT, type Translate } from "@/lib/i18n";
import type { AccountMeta, MigrateResult, Region, Session } from "@/lib/types";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 目标账号 */
  account: AccountMeta | null;
  /** 切换完成后刷新列表 */
  onDone?: () => void;
  /** 目标版本；缺省为国内版。 */
  region?: Region;
  /**
   * 「复制会话常开」（设置页 → 账号切换）。
   *
   * 由调用方注入而不是本组件自己读配置：调用方已经为了置顶读过一次，
   * 再读一次会出现两个页面各自缓存的配置互相打架。
   */
  copySessionsByDefault?: boolean;
}

/**
 * 「复制会话常开」的一次性初始化状态。
 *
 * 会话加载 effect 同时被 `sourceRegion` 变化触发，因此必须把「打开时套用默认值」
 * 与「每次重取会话都重来」区分开：否则用户手动取消勾选之后，只要切一下
 * 数据来源版本，勾选就会被悄悄恢复成全选。
 */
interface SwitchDefaults {
  /** 本次打开是否已经套用过默认值。 */
  applied: boolean;
  /** 会话回来后是否还要补一次「全选」（消费后置 false）。 */
  selectAll: boolean;
}

/** 切换账号弹窗：可勾选当前账号的会话复制到目标账号（路径 B）。 */
export function SwitchAccountDialog({ open, onOpenChange, account, onDone, region, copySessionsByDefault = false }: Props) {
  const t = useT();
  const [sessions, setSessions] = useState<Session[]>([]);
  const [loadingSessions, setLoadingSessions] = useState(false);
  const [copySessions, setCopySessions] = useState(false);
  /** 把当前账号的长期记忆合并进目标账号（带去重）。 */
  const [migrateMemory, setMigrateMemory] = useState(false);
  /**
   * 把当前账号的连接器配置合并进目标账号（带去重）。
   *
   * 刻意与 `migrateMemory` 做成两个**平级**独立开关，而**不是**「主开关 + 两个子开关」：
   * 嵌套结构会引入「主开关开着、两个子项都关」的退化状态，需要额外为其定义语义；
   * 两个平级独立开关天然不存在该状态。二者都关时见 `doSwitch` —— 不发起迁移请求。
   */
  const [migrateConnectors, setMigrateConnectors] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  /** 展开的节点：任务 / 空间 / 文件夹。默认全部收起。 */
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [currentUid, setCurrentUid] = useState<string | null>(null);
  const [progress, setProgress] = useState("");
  /** 降级 / 异常提示（索引库不可读、扫描结果不完整）。普通场景为 null。 */
  const [sessionWarning, setSessionWarning] = useState<string | null>(null);
  /**
   * 会话列表的来源：`db` / `scan` / `empty` / `no-dir`（见 `api.listSessions`）。
   *
   * `no-dir` 必须与 `empty` 分开显示 —— 前者是**数据目录空了**（客户端刚重装 / 重置），
   * 后者才是**账号没有会话**；混为一谈会把用户引向错误结论（2026-09-23 现场）。
   */
  const [sessionSource, setSessionSource] = useState<string | null>(null);
  /**
   * 数据**来源**版本（会话 / 记忆 / 连接器取自哪一版的当前登录账号）。
   * 缺省等于目标 `region` —— 即同版本内切换，行为与改造前完全一致。
   */
  const [sourceRegion, setSourceRegion] = useState<Region>(region ?? "cn");
  const defaultsRef = useRef<SwitchDefaults>({ applied: false, selectAll: false });

  // 关闭时清掉本次打开的一次性状态，下次打开重新按配置初始化。
  useEffect(() => {
    if (!open) defaultsRef.current = { applied: false, selectAll: false };
  }, [open]);

  // 监听后端切换进度：桌面端走 Tauri 事件，webui 走 HTTP 轮询
  useEffect(() => {
    if (api.isWebui()) {
      const timer = setInterval(() => {
        void api.switchProgress().then((p) => {
          if (p.progress) setProgress(p.progress);
        });
      }, 600);
      return () => clearInterval(timer);
    }
    let unlisten: (() => void) | undefined;
    listen<{ message: string }>("switch-progress", (e) => {
      setProgress(e.payload.message);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  /**
   * 打开弹窗时复位一次性状态 —— 其中包含「数据来源版本回到目标版本」。
   *
   * ★ **`sourceRegion` 绝不能出现在本 effect 的依赖里**：本 effect 会把来源版本重置成
   * 目标版本，一旦被 `sourceRegion` 触发，用户刚点的「国际版」会在下一帧被弹回
   * 「国内版」—— 症状是「点了没反应」，于是跨版本迁移整条路都走不通
   * （2026-09-24 用户报障现场）。复位只发生在「打开」这一刻，之后来源版本由用户掌控。
   */
  useEffect(() => {
    if (!open) return;
    setMigrateMemory(false);
    setMigrateConnectors(false);
    setSelected(new Set());
    setExpanded(new Set());
    setError("");
    setSourceRegion(region ?? "cn");
    setSessionSource(null);
  }, [open, account, region]);

  // 加载会话：打开时取一次；此后每次改「数据来源版本」都要重取。
  useEffect(() => {
    if (!open || !account) return;
    if (!defaultsRef.current.applied) {
      defaultsRef.current.applied = true;
      defaultsRef.current.selectAll = copySessionsByDefault;
      setCopySessions(copySessionsByDefault);
    }
    // 换了来源版本后，已勾选的 id 属于**另一个版本**的数据，必须清空
    // （否则会拿 A 版的会话 id 去 B 版复制）。
    setSelected(new Set());
    setLoadingSessions(true);
    // 首次进入时 `sourceRegion` 可能还是上一次的选择，复位后会再跑一次本 effect；
    // 用 cancelled 丢弃那次过期请求，避免短暂显示「另一个版本」的会话。
    let cancelled = false;
    api
      .listSessions(sourceRegion)
      .then((res) => {
        if (cancelled) return;
        setSessions(res.sessions);
        setCurrentUid(res.current);
        setSessionWarning(res.warning ?? null);
        setSessionSource(res.source ?? null);
        // 「复制会话常开」：默认全选，用户仍可逐条取消。
        if (defaultsRef.current.selectAll) {
          defaultsRef.current.selectAll = false;
          setSelected(new Set(res.sessions.map((session) => session.id)));
        }
      })
      .catch((e) => {
        if (!cancelled) setError(api.asError(e));
      })
      .finally(() => {
        if (!cancelled) setLoadingSessions(false);
      });
    return () => {
      cancelled = true;
    };
    // 刻意**不**把 `copySessionsByDefault` 放进依赖：它决定的是「打开瞬间的初值」，
    // 任何在弹窗开着时发生的值变化都不该反过来改动用户当前的选择。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, account, sourceRegion]);

  function toggleSession(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleFolder(ids: string[]) {
    setSelected((prev) => {
      const next = new Set(prev);
      const allOn = ids.length > 0 && ids.every((id) => next.has(id));
      if (allOn) ids.forEach((id) => next.delete(id));
      else ids.forEach((id) => next.add(id));
      return next;
    });
  }

  function toggleExpanded(key: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  async function doSwitch() {
    if (!account) return;
    setBusy(true);
    setProgress(t("wbAccounts.dialog.switchProgress"));
    setError("");
    const parts: string[] = [];
    const warnings: string[] = [];
    try {
      // 先迁移记忆与连接器：只碰普通文件，不需要关闭 WorkBuddy；
      // 且目标账号此刻不是登录账号，其文件不会被占用，故在切换前做最安全。
      // 至少勾选一个迁移范围时才发请求：后端对空范围返回 400「未指定任何迁移范围」，
      // 两者都关时**完全不要调用**，否则用户会看到一个莫名其妙的错误。
      if (migrateMemory || migrateConnectors) {
        setProgress(t("wbAccounts.dialog.migrateProgress"));
        // 只请求勾选的分区；未请求的分区不会出现在响应里（见 summarizeMigration）。
        const mig = await api.migrateAccountData(account.id, {
          region,
          sourceRegion,
          memory: migrateMemory,
          connectors: migrateConnectors,
        });
        parts.push(...summarizeMigration(mig, warnings, t));
      }

      setProgress(t("wbAccounts.dialog.switchProgress"));
      const res = await api.switchAccount({
        accountId: account.id,
        region,
        // 只有「跨版本」时才显式带来源版本：同版本时它由后端缺省为目标 region，
        // 少传一个字段就少一处与后端默认值漂移的机会。
        sourceRegion: sourceRegion === region ? undefined : sourceRegion,
        copySessionIds: copySessions ? [...selected] : undefined,
      });
      const nickname =
        account.nickname || account.email || account.uid || t("wbAccounts.common.unknownAccount");
      if (res.sessionCopy?.copied.length) {
        parts.push(t("wbAccounts.dialog.copiedSessions", { n: res.sessionCopy.copied.length }));
      }
      if (res.sessionCopy?.skipped?.length) {
        parts.push(t("wbAccounts.dialog.skippedSessions", { n: res.sessionCopy.skipped.length }));
      }
      // 收集复制阶段每条副本的降级警告（如「索引不可写，已复制正文」）。
      const sessionWarnings = (res.sessionCopy?.copied ?? [])
        .map((c) => c.warning)
        .filter((w): w is string => Boolean(w));
      if (sessionWarnings.length) {
        warnings.push(...sessionWarnings);
      }
      if (res.backup) parts.push(t("wbAccounts.dialog.backup", { path: res.backup }));

      const description = parts.length
        ? parts.join(t("wbAccounts.dialog.listSeparator"))
        : t("wbAccounts.dialog.switchedDefault");
      if (warnings.length) {
        // 部分迁移失败但未阻断切换：明确降级为警告，避免用户误以为全部成功。
        toast.warning(t("wbAccounts.dialog.switchedWarn", { name: nickname }), {
          description: `${warnings.join(t("wbAccounts.dialog.listSeparator"))}${t("shared.punct.period")}${description}`,
        });
      } else {
        toast.success(t("wbAccounts.dialog.switchedOk", { name: nickname }), { description });
      }
      onOpenChange(false);
      onDone?.();
    } catch (e) {
      setError(api.asError(e));
    } finally {
      setBusy(false);
      setProgress("");
    }
  }

  /** 打开系统设置授权面板（默认完全磁盘访问），供小白一键跳转。 */
  async function openPermissionSettings() {
    try {
      await api.openPermissionSettings("all_files");
    } catch (e) {
      // 打开失败时退化为提示
      setError(api.asError(e));
    }
  }

  /** 权限自检：确认完全磁盘访问是否生效。 */
  const [permCheck, setPermCheck] = useState<string | null>(null);
  async function runPermissionCheck() {
    setPermCheck(t("wbAccounts.dialog.permissionChecking"));
    try {
      const res = await api.checkAuthPermission();
      setPermCheck(
        res.ok
          ? `✓ ${res.message}`
          : t("wbAccounts.dialog.permissionFailDir", { error: res.error ?? "", dir: res.dir ?? "" }),
      );
    } catch (e) {
      setPermCheck(t("wbAccounts.dialog.permissionFail", { error: api.asError(e) }));
    }
  }

  /**
   * 判定「是否因缺少完全磁盘访问权限而失败」。
   *
   * ⚠️ **不能按当前界面语言去找中文关键词**：文案会随语言切换，一旦变成英文，
   * `error.includes("无权限")` 永远为假 ⇒ 授权引导整块**静默失效**。
   * 因此以**结构化错误码**（`permission.denied`，见 `lib/error-code.ts`）为准，
   * 中文子串只作为**旧版本后端**的兼容兜底。
   */
  function permissionDenied(raw: string): boolean {
    const decoded = decodeError(raw);
    return decoded.code === "permission.denied" || decoded.text.includes("无权限");
  }

  // 出现「无权限」错误时，自动每 2s 轮询一次授权状态；用户拖入 app 授权成功后自动恢复
  useEffect(() => {
    if (!permissionDenied(error)) return;
    let cancelled = false;
    let timer: number | undefined;
    const check = async () => {
      try {
        const res = await api.checkAuthPermission();
        if (res.ok) {
          if (!cancelled) {
            setPermCheck(t("wbAccounts.dialog.permissionOk"));
            setError("");
          }
          return;
        }
      } catch {
        /* 忽略中间态 */
      }
      if (!cancelled) timer = window.setTimeout(check, 2000);
    };
    check();
    return () => {
      cancelled = true;
      if (timer) window.clearTimeout(timer);
    };
  }, [error, t]);

  const copyCount = copySessions ? selected.size : 0;
  const targetRegion: Region = region ?? "cn";
  const crossRegion = sourceRegion !== targetRegion;
  const needsPermission = permissionDenied(error);
  const sessionsEmpty = !loadingSessions && sessions.length === 0;
  // 空列表的成因必须分开说，否则会把用户引向错误结论：
  //   no-dir → 该版本的数据目录空了（客户端刚重装 / 重置）；empty → 该版本账号确实没有会话。
  const sourceLabel = regionLabel(sourceRegion);
  const copyHint = loadingSessions
    ? t("wbAccounts.dialog.copyHintLoading")
    : error && sessionsEmpty
      ? t("wbAccounts.dialog.copyHintErrorEmpty")
      : sessionsEmpty
        ? currentUid
          ? sessionSource === "no-dir"
            ? t("wbAccounts.dialog.copyHintNoData", { region: sourceLabel })
            : t("wbAccounts.dialog.copyHintEmptyCurrent", { region: sourceLabel })
          : t("wbAccounts.dialog.copyHintNoCurrent")
        : crossRegion
          ? t("wbAccounts.dialog.copyHintCross", {
              from: regionLabel(sourceRegion),
              to: regionLabel(targetRegion),
            })
          : t("wbAccounts.dialog.copyHintSame");

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={!busy}
        className="flex max-h-[min(90vh,calc(100vh-2rem))] min-w-0 flex-col overflow-hidden"
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>{t("wbAccounts.dialog.switchTitle", { name: account?.nickname || account?.email || account?.uid || t("wbAccounts.common.unknownAccount") })}</DialogTitle>
          <DialogDescription>
            {t("wbAccounts.dialog.switchDesc")}
          </DialogDescription>
        </DialogHeader>

        {busy && (
          <div className="absolute inset-0 z-50 flex flex-col items-center justify-center gap-3 rounded-lg bg-background/85 backdrop-blur-sm">
            <Loader2 className="size-8 animate-spin text-primary" />
            <p className="text-sm font-medium">{progress || t("wbAccounts.dialog.switchProgress")}</p>
            <p className="max-w-xs text-center text-xs text-muted-foreground">
              {t("wbAccounts.dialog.switchProcessing")}
            </p>
          </div>
        )}

        <div className="min-h-0 space-y-3 overflow-x-hidden overflow-y-auto">
          <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium">{t("wbAccounts.dialog.sourceVersion")}</div>
              <div className="text-xs text-muted-foreground">
                {sourceRegion === targetRegion
                  ? t("wbAccounts.dialog.sourceSame", { region: regionLabel(targetRegion) })
                  : t("wbAccounts.dialog.sourceCross", {
                      from: regionLabel(sourceRegion),
                      to: regionLabel(targetRegion),
                    })}
              </div>
            </div>
            <Tabs
              value={sourceRegion}
              onValueChange={(next) => {
                if (next === "cn" || next === "global") setSourceRegion(next);
              }}
            >
              <TabsList className="h-auto" aria-label={t("wbAccounts.dialog.sourceVersionAria")}>
                {REGIONS.map((r) => (
                  <TabsTrigger key={r} value={r} className="whitespace-nowrap">
                    {regionLabel(r)}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          </div>

          <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium">{t("wbAccounts.dialog.copySessions")}</div>
              <div
                className={
                  sessionsEmpty
                    ? "text-xs text-amber-700 dark:text-amber-400"
                    : "text-xs text-muted-foreground"
                }
              >
                {copyHint}
              </div>
              {/* 降级警告：索引库不可读 / 不完整。仍允许勾选（降级扫描已列出
                   projects 下的 jsonl），但必须明确告诉用户数据可能缺失。 */}
              {sessionWarning && (
                <div className="mt-0.5 text-xs text-amber-700 dark:text-amber-400">
                  {sessionWarning}
                </div>
              )}
            </div>
            <Switch
              checked={copySessions}
              onCheckedChange={setCopySessions}
              disabled={loadingSessions || sessions.length === 0}
            />
          </div>

          <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium">{t("wbAccounts.dialog.migrateMemory")}</div>
              <div className="text-xs text-muted-foreground">
                {crossRegion
                  ? t("wbAccounts.dialog.migrateMemoryDescCross", {
                      from: regionLabel(sourceRegion),
                      to: regionLabel(targetRegion),
                    })
                  : t("wbAccounts.dialog.migrateMemoryDescSame")}
              </div>
            </div>
            <Switch checked={migrateMemory} onCheckedChange={setMigrateMemory} />
          </div>

          <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium">{t("wbAccounts.dialog.migrateConnectors")}</div>
              <div className="text-xs text-muted-foreground">
                {crossRegion
                  ? t("wbAccounts.dialog.migrateConnectorsDescCross", {
                      from: regionLabel(sourceRegion),
                      to: regionLabel(targetRegion),
                    })
                  : t("wbAccounts.dialog.migrateConnectorsDescSame")}
              </div>
            </div>
            <Switch checked={migrateConnectors} onCheckedChange={setMigrateConnectors} />
          </div>

          {copySessions && (
            <>
              <Separator />
              <div className="max-h-[min(20rem,45vh)] overflow-y-auto pr-1">
                {loadingSessions ? (
                  <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
                    <Loader2 className="animate-spin" /> {t("wbAccounts.dialog.loadingSessions")}
                  </div>
                ) : sessions.length === 0 ? (
                  <p className="py-4 text-center text-sm text-muted-foreground">
                    {currentUid
                      ? t("wbAccounts.dialog.noSessionsCurrent")
                      : t("wbAccounts.dialog.noSessionsNoCurrent")}
                  </p>
                ) : (
                  buildSessionTree(sessions, t).map((kind) => {
                    const kindOpen = expanded.has(kind.key);
                    const kindSel = selectionState(kind.sessions, selected);
                    return (
                      <div key={kind.key} className="mb-0.5">
                        <div className="sticky top-0 z-10 flex items-center gap-1.5 rounded-md bg-background px-1.5 py-1">
                          <TreeCheckbox
                            allOn={kindSel.allOn}
                            someOn={kindSel.someOn}
                            onChange={() => toggleFolder(kind.sessions.map((s) => s.id))}
                            ariaLabel={t("wbAccounts.dialog.selectKind", { label: kind.label })}
                          />
                          <button
                            type="button"
                            className="flex min-w-0 flex-1 items-center gap-1 rounded px-1 py-0.5 text-left hover:bg-accent/50"
                            onClick={() => toggleExpanded(kind.key)}
                            aria-expanded={kindOpen}
                            aria-label={t(
                              kindOpen ? "wbAccounts.dialog.collapse" : "wbAccounts.dialog.expand",
                              { label: kind.label },
                            )}
                          >
                            <span className="min-w-0 flex-1 truncate text-sm font-medium">
                              {kind.label}
                              <span className="ml-1 font-normal text-muted-foreground">
                                ({kind.count})
                              </span>
                            </span>
                            {kindOpen ? (
                              <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
                            ) : (
                              <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
                            )}
                          </button>
                        </div>
                        {kindOpen && kind.key === "task" &&
                          kind.sessions.map((s) => (
                            <SessionPickRow
                              key={s.id}
                              session={s}
                              checked={selected.has(s.id)}
                              indentClass="pl-7"
                              onToggle={() => toggleSession(s.id)}
                            />
                          ))}
                        {kindOpen &&
                          kind.folders?.map((folder) => {
                            const folderOpen = expanded.has(folder.key);
                            const folderSel = selectionState(folder.sessions, selected);
                            return (
                              <div key={folder.key}>
                                <div className="flex items-center gap-1.5 px-1.5 py-0.5 pl-7">
                                  <TreeCheckbox
                                    allOn={folderSel.allOn}
                                    someOn={folderSel.someOn}
                                    onChange={() => toggleFolder(folder.sessions.map((s) => s.id))}
                                    ariaLabel={t("wbAccounts.dialog.selectFolder", { label: folder.label })}
                                  />
                                  <button
                                    type="button"
                                    className="flex min-w-0 flex-1 items-center gap-1.5 rounded px-1 py-0.5 text-left hover:bg-accent/50"
                                    onClick={() => toggleExpanded(folder.key)}
                                    aria-expanded={folderOpen}
                                    aria-label={t(folderOpen ? "wbAccounts.dialog.collapseFolder" : "wbAccounts.dialog.expandFolder", { label: folder.label })}
                                  >
                                    <Folder className="size-3.5 shrink-0 text-muted-foreground" />
                                    <span className="min-w-0 flex-1 truncate text-sm">
                                      {folder.label}
                                    </span>
                                    {folderOpen ? (
                                      <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
                                    ) : (
                                      <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
                                    )}
                                  </button>
                                </div>
                                {folderOpen &&
                                  folder.sessions.map((s) => (
                                    <SessionPickRow
                                      key={s.id}
                                      session={s}
                                      checked={selected.has(s.id)}
                                      indentClass="pl-12"
                                      onToggle={() => toggleSession(s.id)}
                                    />
                                  ))}
                              </div>
                            );
                          })}
                      </div>
                    );
                  })
                )}
              </div>
            </>
          )}

          {error && (
            <Alert variant={needsPermission ? "warning" : "destructive"} className="min-w-0 break-all">
              <AlertDescription className="min-w-0 break-all">
                <div className="min-w-0 break-all">{error}</div>
                {needsPermission && (
                  <div className="mt-2 space-y-2">
                    <div className="rounded-md border bg-muted/60 p-3 text-xs text-muted-foreground">
                      <p className="mb-1 font-medium text-foreground">{t("wbAccounts.dialog.permissionTitle")}</p>
                      <ol className="list-decimal space-y-1 pl-4">
                        <li>{t("wbAccounts.dialog.permissionStep1")}</li>
                        <li>{t("wbAccounts.dialog.permissionStep2")}</li>
                        <li>{t("wbAccounts.dialog.permissionStep3")}</li>
                      </ol>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <Button variant="outline" size="sm" onClick={openPermissionSettings}>
                        <ExternalLink />
                        {t("wbAccounts.dialog.openFullDisk")}
                      </Button>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => void api.revealAppInFinder()}
                      >
                        {t("wbAccounts.dialog.showInFinder")}
                      </Button>
                      <Button variant="secondary" size="sm" onClick={runPermissionCheck}>
                        {t("wbAccounts.dialog.checkNow")}
                      </Button>
                    </div>
                  </div>
                )}
                {permCheck && <div className="mt-2 text-xs">{permCheck}</div>}
              </AlertDescription>
            </Alert>
          )}
        </div>

        <DialogFooter className="shrink-0">
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={busy}>
            {t("wbAccounts.dialog.cancel")}
          </Button>
          <Button onClick={doSwitch} disabled={busy || (copySessions && copyCount === 0)}>
            {busy ? t("wbAccounts.card.switching") : t("wbAccounts.dialog.confirmSwitch")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

type FolderGroup = { key: string; label: string; sessions: Session[] };
type KindGroup = {
  key: "task" | "space";
  label: string;
  count: number;
  sessions: Session[];
  folders?: FolderGroup[];
};

function selectionState(sessions: Session[], selected: Set<string>) {
  const ids = sessions.map((s) => s.id);
  const n = ids.filter((id) => selected.has(id)).length;
  return { allOn: n === ids.length && ids.length > 0, someOn: n > 0 && n < ids.length };
}

function TreeCheckbox({
  allOn,
  someOn,
  onChange,
  ariaLabel,
}: {
  allOn: boolean;
  someOn: boolean;
  onChange: () => void;
  ariaLabel: string;
}) {
  return (
    <input
      type="checkbox"
      className="size-3.5 shrink-0 accent-primary"
      checked={allOn}
      ref={(el) => {
        if (el) el.indeterminate = someOn;
      }}
      onChange={onChange}
      aria-label={ariaLabel}
    />
  );
}

function SessionPickRow({
  session,
  checked,
  indentClass,
  onToggle,
}: {
  session: Session;
  checked: boolean;
  indentClass: string;
  onToggle: () => void;
}) {
  const t = useT();
  return (
    <label
      className={`flex cursor-pointer items-center gap-2.5 rounded-md py-1.5 pr-2 hover:bg-accent/50 ${indentClass}`}
    >
      <input
        type="checkbox"
        className="size-3.5 shrink-0 accent-primary"
        checked={checked}
        onChange={onToggle}
      />
      <span className="min-w-0 flex-1 truncate text-sm" title={session.title}>
        {session.title}
      </span>
      {session.hasHistory && (
        <Badge variant="outline" className="shrink-0 text-[10px]">
          {t("wbAccounts.dialog.hasBody")}
        </Badge>
      )}
    </label>
  );
}

/** 区分「计数对象」与「`{ error }`」两种分区结果。 */
function hasError(
  section: { error: string } | object | undefined,
): section is { error: string } {
  return !!section && typeof section === "object" && "error" in section;
}

/** 无需迁移时的静默分区（源账号本来就没有这类数据）。 */
function isNoop(section: { changed?: boolean; skipped?: boolean } | undefined): boolean {
  if (!section) return true;
  return section.changed === false || section.skipped === true;
}

/**
 * 把迁移报告压成一行可读摘要，并把分区级失败追加进 `warnings`。
 *
 * 分区失败（如只读文件、JSON 损坏）不影响切换本身，因此在界面降级为警告而非错误。
 */
function summarizeMigration(mig: MigrateResult, warnings: string[], t: Translate): string[] {
  const parts: string[] = [];

  if (hasError(mig.memory)) {
    warnings.push(t("wbAccounts.dialog.memoryFail", { error: mig.memory.error }));
  } else if (!isNoop(mig.memory) && mig.memory) {
    const { appended, skippedDuplicate } = mig.memory;
    parts.push(
      skippedDuplicate > 0
        ? t("wbAccounts.dialog.migratedMemory", { n: appended, d: skippedDuplicate })
        : t("wbAccounts.dialog.migratedMemorySimple", { n: appended }),
    );
  }

  if (hasError(mig.connectors)) {
    warnings.push(t("wbAccounts.dialog.connectorFail", { error: mig.connectors.error }));
  } else if (!isNoop(mig.connectors) && mig.connectors) {
    const { addedKeys, droppedDuplicateElements } = mig.connectors;
    parts.push(
      droppedDuplicateElements > 0
        ? t("wbAccounts.dialog.migratedConnectors", { k: addedKeys, d: droppedDuplicateElements })
        : t("wbAccounts.dialog.migratedConnectorsSimple", { k: addedKeys }),
    );
  }

  // 备份路径较长，只在确实产生备份时给出，供用户回滚。
  const backups: string[] = [];
  for (const section of [mig.memory, mig.connectors]) {
    if (section && !hasError(section) && section.backup) backups.push(section.backup);
  }
  if (backups.length) {
    parts.push(t("wbAccounts.dialog.backup", { path: backups.join(" / ") }));
  }

  return parts;
}

/** WorkBuddy 侧栏文件夹名：cwd 最后一段。 */
function sessionFolderLabel(cwd: string, t: Translate): string {
  const normalized = cwd.trim().replace(/[\\/]+$/, "");
  if (!normalized) return t("wbAccounts.dialog.ungrouped");
  const parts = normalized.split(/[\\/]/);
  return parts[parts.length - 1] || normalized;
}

/** 按工作目录分组，文件夹顺序跟会话一样按最近活动排。 */
function groupSessionsByFolder(sessions: Session[], t: Translate): FolderGroup[] {
  const groups = new Map<string, Session[]>();
  const order: string[] = [];
  for (const session of sessions) {
    const key = session.cwd.trim() || "__none__";
    let list = groups.get(key);
    if (!list) {
      list = [];
      groups.set(key, list);
      order.push(key);
    }
    list.push(session);
  }
  return order.map((key) => ({
    key,
    label: key === "__none__" ? t("wbAccounts.dialog.ungrouped") : sessionFolderLabel(key, t),
    sessions: groups.get(key) ?? [],
  }));
}

/** 对齐 WorkBuddy 侧栏：任务（playground）平铺，空间按文件夹分组。 */
function buildSessionTree(sessions: Session[], t: Translate): KindGroup[] {
  const tasks = sessions.filter((s) => s.isPlayground);
  const spaces = sessions.filter((s) => !s.isPlayground);
  const groups: KindGroup[] = [];
  if (tasks.length > 0) {
    groups.push({
      key: "task",
      label: t("wbAccounts.dialog.task"),
      count: tasks.length,
      sessions: tasks,
    });
  }
  if (spaces.length > 0) {
    const folders = groupSessionsByFolder(spaces, t);
    groups.push({
      key: "space",
      label: t("wbAccounts.dialog.space"),
      count: folders.length,
      sessions: spaces,
      folders,
    });
  }
  return groups;
}
