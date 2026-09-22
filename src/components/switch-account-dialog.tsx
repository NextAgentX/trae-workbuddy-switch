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
import { REGIONS, regionLabel } from "@/lib/region";
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

  // 打开时加载当前账号会话
  useEffect(() => {
    if (open && account) {
      if (!defaultsRef.current.applied) {
        defaultsRef.current.applied = true;
        defaultsRef.current.selectAll = copySessionsByDefault;
        setCopySessions(copySessionsByDefault);
      }
      setMigrateMemory(false);
      setMigrateConnectors(false);
      setSelected(new Set());
      setExpanded(new Set());
      setError("");
      setSourceRegion(region ?? "cn");
      setLoadingSessions(true);
      // 首次进入时 `sourceRegion` 可能还是上一次的选择，重置后会再跑一次本 effect；
      // 用 cancelled 丢弃那次过期请求，避免短暂显示「另一个版本」的会话。
      let cancelled = false;
      api
        .listSessions(sourceRegion)
        .then((res) => {
          if (cancelled) return;
          setSessions(res.sessions);
          setCurrentUid(res.current);
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
    }
    // 刻意**不**把 `copySessionsByDefault` 放进依赖：它决定的是「打开瞬间的初值」，
    // 任何在弹窗开着时发生的值变化都不该反过来改动用户当前的选择。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, account, region, sourceRegion]);

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
    setProgress("正在切换账号…");
    setError("");
    const parts: string[] = [];
    const warnings: string[] = [];
    try {
      // 先迁移记忆与连接器：只碰普通文件，不需要关闭 WorkBuddy；
      // 且目标账号此刻不是登录账号，其文件不会被占用，故在切换前做最安全。
      // 至少勾选一个迁移范围时才发请求：后端对空范围返回 400「未指定任何迁移范围」，
      // 两者都关时**完全不要调用**，否则用户会看到一个莫名其妙的错误。
      if (migrateMemory || migrateConnectors) {
        setProgress("正在迁移账号数据…");
        // 只请求勾选的分区；未请求的分区不会出现在响应里（见 summarizeMigration）。
        const mig = await api.migrateAccountData(account.id, {
          region,
          sourceRegion,
          memory: migrateMemory,
          connectors: migrateConnectors,
        });
        parts.push(...summarizeMigration(mig, warnings));
      }

      setProgress("正在切换账号…");
      const res = await api.switchAccount({
        accountId: account.id,
        region,
        // 只有「跨版本」时才显式带来源版本：同版本时它由后端缺省为目标 region，
        // 少传一个字段就少一处与后端默认值漂移的机会。
        sourceRegion: sourceRegion === region ? undefined : sourceRegion,
        copySessionIds: copySessions ? [...selected] : undefined,
      });
      const nickname = account.nickname || account.email || account.uid || "该账号";
      if (res.sessionCopy?.copied.length) {
        parts.push(`已复制 ${res.sessionCopy.copied.length} 个会话`);
      }
      if (res.sessionCopy?.skipped?.length) {
        parts.push(`跳过 ${res.sessionCopy.skipped.length} 个已存在副本`);
      }
      if (res.backup) parts.push(`备份: ${res.backup}`);

      const description = parts.length ? parts.join("；") : "WorkBuddy 已重启为目标账号。";
      if (warnings.length) {
        // 部分迁移失败但未阻断切换：明确降级为警告，避免用户误以为全部成功。
        toast.warning(`已切换至「${nickname}」，但部分迁移未完成`, {
          description: `${warnings.join("；")}。${description}`,
        });
      } else {
        toast.success(`已切换至「${nickname}」`, { description });
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
    setPermCheck("检测中…");
    try {
      const res = await api.checkAuthPermission();
      setPermCheck(res.ok ? `✓ ${res.message}` : `✗ ${res.error}（${res.dir}）`);
    } catch (e) {
      setPermCheck(`✗ ${api.asError(e)}`);
    }
  }

  // 出现「无权限」错误时，自动每 2s 轮询一次授权状态；用户拖入 app 授权成功后自动恢复
  useEffect(() => {
    if (!error.includes("无权限")) return;
    let cancelled = false;
    let timer: number | undefined;
    const check = async () => {
      try {
        const res = await api.checkAuthPermission();
        if (res.ok) {
          if (!cancelled) {
            setPermCheck("✓ 授权成功，可以重新切换了");
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
  }, [error]);

  const copyCount = copySessions ? selected.size : 0;
  const targetRegion: Region = region ?? "cn";
  const crossRegion = sourceRegion !== targetRegion;
  const needsPermission = error.includes("无权限");
  const sessionsEmpty = !loadingSessions && sessions.length === 0;
  const copyHint = loadingSessions
    ? "正在加载会话…"
    : error && sessionsEmpty
      ? "无法加载会话列表，暂不能复制"
      : sessionsEmpty
        ? currentUid
          ? "当前账号暂无会话，无法复制"
          : "未检测到当前登录账号，无法列出会话"
        : crossRegion
          ? `把${regionLabel(sourceRegion)}当前账号勾选的会话以新 id 复制到${regionLabel(targetRegion)}目标账号`
          : "将当前账号勾选的会话以新 id 复制给目标账号（云端归属目标）";

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={!busy}
        className="flex max-h-[min(90vh,calc(100vh-2rem))] min-w-0 flex-col overflow-hidden"
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>切换到「{account?.nickname || account?.email || account?.uid || "该账号"}」</DialogTitle>
          <DialogDescription>
            切换会关闭并重启 WorkBuddy，认证文件将写入目标账号。
          </DialogDescription>
        </DialogHeader>

        {busy && (
          <div className="absolute inset-0 z-50 flex flex-col items-center justify-center gap-3 rounded-lg bg-background/85 backdrop-blur-sm">
            <Loader2 className="size-8 animate-spin text-primary" />
            <p className="text-sm font-medium">{progress || "正在切换账号…"}</p>
            <p className="max-w-xs text-center text-xs text-muted-foreground">
              正在处理中，请勿关闭窗口
            </p>
          </div>
        )}

        <div className="min-h-0 space-y-3 overflow-x-hidden overflow-y-auto">
          <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium">数据来源版本</div>
              <div className="text-xs text-muted-foreground">
                {sourceRegion === targetRegion
                  ? `与目标一致（${regionLabel(targetRegion)}），同版本内搬运`
                  : `从${regionLabel(sourceRegion)}当前登录账号搬到${regionLabel(targetRegion)}`}
              </div>
            </div>
            <Tabs
              value={sourceRegion}
              onValueChange={(next) => {
                if (next === "cn" || next === "global") setSourceRegion(next);
              }}
            >
              <TabsList className="h-auto" aria-label="数据来源版本">
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
              <div className="text-sm font-medium">复制会话到目标账号</div>
              <div
                className={
                  sessionsEmpty
                    ? "text-xs text-amber-700 dark:text-amber-400"
                    : "text-xs text-muted-foreground"
                }
              >
                {copyHint}
              </div>
            </div>
            <Switch
              checked={copySessions}
              onCheckedChange={setCopySessions}
              disabled={loadingSessions || sessions.length === 0}
            />
          </div>

          <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium">迁移长期记忆</div>
              <div className="text-xs text-muted-foreground">
                {crossRegion
                  ? `把${regionLabel(sourceRegion)}当前账号的长期记忆合并到${regionLabel(targetRegion)}目标账号；按内容去重，只补差集，改写前自动备份原文`
                  : "合并当前账号的长期记忆到目标账号；按内容去重，只补差集，改写前自动备份原文"}
              </div>
            </div>
            <Switch checked={migrateMemory} onCheckedChange={setMigrateMemory} />
          </div>

          <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium">迁移连接器配置</div>
              <div className="text-xs text-muted-foreground">
                {crossRegion
                  ? `把${regionLabel(sourceRegion)}当前账号的连接器配置合并到${regionLabel(targetRegion)}目标账号；同名条目递归合并、重复项去重，改写前自动备份原文`
                  : "合并当前账号的连接器配置到目标账号；同名条目递归合并、重复项去重，改写前自动备份原文"}
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
                    <Loader2 className="animate-spin" /> 加载会话…
                  </div>
                ) : sessions.length === 0 ? (
                  <p className="py-4 text-center text-sm text-muted-foreground">
                    {currentUid ? "当前账号暂无会话" : "未检测到当前登录账号，无法列出会话"}
                  </p>
                ) : (
                  buildSessionTree(sessions).map((kind) => {
                    const kindOpen = expanded.has(kind.key);
                    const kindSel = selectionState(kind.sessions, selected);
                    return (
                      <div key={kind.key} className="mb-0.5">
                        <div className="sticky top-0 z-10 flex items-center gap-1.5 rounded-md bg-background px-1.5 py-1">
                          <TreeCheckbox
                            allOn={kindSel.allOn}
                            someOn={kindSel.someOn}
                            onChange={() => toggleFolder(kind.sessions.map((s) => s.id))}
                            ariaLabel={`选择${kind.label}`}
                          />
                          <button
                            type="button"
                            className="flex min-w-0 flex-1 items-center gap-1 rounded px-1 py-0.5 text-left hover:bg-accent/50"
                            onClick={() => toggleExpanded(kind.key)}
                            aria-expanded={kindOpen}
                            aria-label={`${kindOpen ? "折叠" : "展开"}${kind.label}`}
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
                                    ariaLabel={`选择文件夹 ${folder.label}`}
                                  />
                                  <button
                                    type="button"
                                    className="flex min-w-0 flex-1 items-center gap-1.5 rounded px-1 py-0.5 text-left hover:bg-accent/50"
                                    onClick={() => toggleExpanded(folder.key)}
                                    aria-expanded={folderOpen}
                                    aria-label={`${folderOpen ? "折叠" : "展开"}文件夹 ${folder.label}`}
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
                      <p className="mb-1 font-medium text-foreground">如何授权（只需 3 步）：</p>
                      <ol className="list-decimal space-y-1 pl-4">
                        <li>点击下方「打开完全磁盘访问」</li>
                        <li>
                          把 <b>BuddySwitch.app</b> 从 Finder 拖进面板列表（即使没提示框也直接拖），
                          打开它的开关
                        </li>
                        <li>授权后这里会自动检测到，无需其他操作</li>
                      </ol>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <Button variant="outline" size="sm" onClick={openPermissionSettings}>
                        <ExternalLink />
                        打开完全磁盘访问
                      </Button>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => void api.revealAppInFinder()}
                      >
                        在 Finder 中显示
                      </Button>
                      <Button variant="secondary" size="sm" onClick={runPermissionCheck}>
                        立即检测
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
            取消
          </Button>
          <Button onClick={doSwitch} disabled={busy || (copySessions && copyCount === 0)}>
            {busy ? "切换中…" : "确认切换"}
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
          有正文
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
function summarizeMigration(mig: MigrateResult, warnings: string[]): string[] {
  const parts: string[] = [];

  if (hasError(mig.memory)) {
    warnings.push(`记忆迁移失败（${mig.memory.error}）`);
  } else if (!isNoop(mig.memory) && mig.memory) {
    const { appended, skippedDuplicate } = mig.memory;
    parts.push(
      skippedDuplicate > 0
        ? `迁移记忆 +${appended} 行（去重 ${skippedDuplicate} 行）`
        : `迁移记忆 +${appended} 行`,
    );
  }

  if (hasError(mig.connectors)) {
    warnings.push(`连接器迁移失败（${mig.connectors.error}）`);
  } else if (!isNoop(mig.connectors) && mig.connectors) {
    const { addedKeys, droppedDuplicateElements } = mig.connectors;
    parts.push(
      droppedDuplicateElements > 0
        ? `迁移连接器 +${addedKeys} 键（去重 ${droppedDuplicateElements} 项）`
        : `迁移连接器 +${addedKeys} 键`,
    );
  }

  // 备份路径较长，只在确实产生备份时给出，供用户回滚。
  const backups: string[] = [];
  for (const section of [mig.memory, mig.connectors]) {
    if (section && !hasError(section) && section.backup) backups.push(section.backup);
  }
  if (backups.length) parts.push(`备份: ${backups.join(" / ")}`);

  return parts;
}

/** WorkBuddy 侧栏文件夹名：cwd 最后一段。 */
function sessionFolderLabel(cwd: string): string {
  const normalized = cwd.trim().replace(/[\\/]+$/, "");
  if (!normalized) return "未分组";
  const parts = normalized.split(/[\\/]/);
  return parts[parts.length - 1] || normalized;
}

/** 按工作目录分组，文件夹顺序跟会话一样按最近活动排。 */
function groupSessionsByFolder(sessions: Session[]): FolderGroup[] {
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
    label: key === "__none__" ? "未分组" : sessionFolderLabel(key),
    sessions: groups.get(key) ?? [],
  }));
}

/** 对齐 WorkBuddy 侧栏：任务（playground）平铺，空间按文件夹分组。 */
function buildSessionTree(sessions: Session[]): KindGroup[] {
  const tasks = sessions.filter((s) => s.isPlayground);
  const spaces = sessions.filter((s) => !s.isPlayground);
  const groups: KindGroup[] = [];
  if (tasks.length > 0) {
    groups.push({ key: "task", label: "任务", count: tasks.length, sessions: tasks });
  }
  if (spaces.length > 0) {
    const folders = groupSessionsByFolder(spaces);
    groups.push({
      key: "space",
      label: "空间",
      count: folders.length,
      sessions: spaces,
      folders,
    });
  }
  return groups;
}
