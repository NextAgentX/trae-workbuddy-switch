import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  CircleSlash,
  Columns3,
  Download,
  FileDown,
  FileUp,
  Loader2,
  QrCode,
  RefreshCw,
  Rocket,
  Rows3,
  UserPlus,
  XCircle,
} from "lucide-react";
import { toast } from "sonner";

import { DemoAction } from "@/components/demo-action";
import { TraeVariantBar } from "@/components/trae-variant-bar";
import { TraeAccountCard, type TraeProgram } from "@/components/trae-account-card";
import { TraeExportAccountsDialog } from "@/components/trae-export-accounts-dialog";
import { TraeImportAccountsDialog } from "@/components/trae-import-accounts-dialog";
import { TraeOAuthLoginDialog } from "@/components/trae-oauth-login-dialog";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
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
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import * as api from "@/lib/api";
import { useT } from "@/lib/i18n";
import type { TranslationKey } from "@/locales/zh";
import {
  loadScheduleConfig,
  saveSchedulePatch,
  SCHEDULE_CONFIG_KEY,
} from "@/lib/schedule-config";
import { isAutoDetected } from "@/lib/trae-client";
import { traeVariantLabel } from "@/lib/trae-types";
import {
  TRAE_VARIANT_FALLBACK,
  loadTraeVariantLogins,
  loadTraeVariantStatuses,
  type TraeVariantLogins,
} from "@/lib/trae-variant-status";
import type { ScheduleConfig } from "@/lib/types";
import type {
  TraeAccount,
  TraeAccountsOverview,
  TraeCheckinEvent,
  TraeCheckinReport,
  TraeCheckinStatus,
  TraeCreditsOverview,
  TraeEnvStatus,
  TraeSettings,
  TraeVariantId,
  TraeVariantStatus,
} from "@/lib/trae-types";
import { cn } from "@/lib/utils";
import { useCachedResource } from "@/lib/use-cached-resource";
import { useCompactMode } from "@/lib/use-compact-mode";
import { useTraeVariant } from "@/lib/use-trae-variant";

/** 未分组在过滤器里的哨兵值（`Select` 不接受空字符串作为 value）。 */
const UNGROUPED = "__ungrouped__";

/**
 * 账号页的快照。
 *
 * 这些数据**必须一起**缓存：卡片上的程序切换按钮要同时用到账号、程序位与各区域
 * 登录态，分开缓存会让「账号已是新的、按钮还是旧的」这种不一致有缝可钻。
 *
 * 缓存键里带 `variant`（产品线），所以切到另一条线绝不会渲染出上一条线的数据。
 */
interface AccountsSnapshot {
  overview: TraeAccountsOverview;
  env: TraeEnvStatus;
  credits: TraeCreditsOverview;
  checkin: TraeCheckinStatus;
  /** 本机全部产品线的环境状态（并排视角），账号卡片与状态条共用。 */
  variantStatuses: TraeVariantStatus[];
  /** 每条产品线各自的当前登录账号（`profiles.currentAccount` 的 uid + 展示名）。 */
  logins: TraeVariantLogins;
}

/**
 * 「账号管理」页（Trae 分区）。
 *
 * ## 与 WorkBuddy 页面的关系
 *
 * **骨架逐段照抄** WorkBuddy 的 `AccountsPage`：产品状态标记行 → 「添加与迁移账号」
 * 横幅 → 环境说明行 → 空态卡 → 账号工具栏（开关 / 分组筛选 / 紧凑 / 刷新）→
 * 卡片栅格 → 各确认 Dialog。用户在两个分区之间切换时不需要重新学习界面。
 *
 * **文案与数据源按 Trae 实情落地**，不搬 WorkBuddy 的：
 * - 版本切换的**语义与 WorkBuddy 完全对应**（国内版 / 国际版），差别只在**控件位置**：
 *   WorkBuddy 在页头右侧放一枚窄切换器，Trae 用页头下方的**全宽状态条**
 *   （`trae-variant-bar.tsx`）—— 它还要并排显示每个区域各自的登录账号与程序位状态，
 *   窄控件放不下。承载标识见 `useTraeVariant`（返回**区域**；旧值 `trae_work`/`trae_cn`
 *   一律回落国内版，否则老书签会去读空库）；
 * - 卡片头部的**程序切换按钮**（该区域的每个 Trae 程序各一枚）对应 WorkBuddy 卡片上的
 *   WorkBuddy / CodeBuddy IDE / CodeBuddy CLI 三枚按钮——都是「把这个账号挂到哪个
 *   客户端上」。这个维度 Trae 叫**程序位**（TraeWork / TraeCode，见 `TraeProgram`），
 *   与「区域」是两层：区域决定账号库与端点，程序位决定写进哪个客户端；
 * - **支持 OAuth 网页登录**（`trae-oauth-login-dialog.tsx` + OAuth 三命令），
 *   粘贴 `Cloud-IDE-JWT` 只是「优先用网页登录」的兜底方式；
 * - **有自动签到**（排程任务 `trae_checkin`，默认关闭）：与 WorkBuddy 工具栏的同名开关
 *   同位同义 —— 到点由后台调度器签一轮、应用启动时若当天还没签则补一轮，两个区域各签一遍；
 *   小时点在 Trae 设置页配置。它管**何时签**；
 * - 「跳过今日已签」是**策略**开关（管**怎么签**）：手动与自动两轮都受它约束，
 *   两者是不同层，因此并排出现并不重复；
 * - 没有自动旅行（Trae 侧不存在该客户端）。
 */
export default function TraeAccountsPage() {
  /**
   * 当前正在管理的**产品线变体**，由侧栏分区决定（URL `?line=`）。
   *
   * 为什么**不再**用「自动探测挑中的那一条」：探测是无入参、横跨全部变体的
   * 全局行为，它回答的是「本机哪条线最近活跃」，而不是「用户此刻想管哪条线」。
   * 用户点了侧栏的「Trae CN」，页面就必须是 Trae CN —— 哪怕 Trae Work 更活跃。
   *
   * 探测结果仍然有用：它作为**兜底**（URL 没有显式指定时）以及用于展示
   * 「该线的安装/运行状态」。两者分工不同，不要混为一谈。
   */
  const [variant] = useTraeVariant();
  const t = useT();

  /**
   * 一次取齐本页快照。四份分家数据全部按**当前选中的产品线**读取，与侧栏分区同源。
   * `env` 仍然单独取：它是「单一视角」的环境快照，用于展示该线的安装/运行状态。
   *
   * `statuses` 与「各线当前账号」则是**跨产品线**的：卡片上的程序切换按钮条
   * （对应 WorkBuddy 卡片的 WorkBuddy / CodeBuddy IDE / CLI 三枚按钮）必须知道
   * 「本机装了哪几条线、每条线当前挂的是哪个账号」，缺一条就渲染不出那一枚按钮。
   */
  const loadSnapshot = useCallback(async (): Promise<AccountsSnapshot> => {
    const [accountData, envData, creditData, checkinData, statuses] = await Promise.all([
      api.getTraeAccounts(variant),
      api.getTraeEnv(),
      api.getTraeCredits(variant),
      api.getTraeCheckinStatus(variant),
      loadTraeVariantStatuses(),
    ]);
    // 登录态读取依赖刚拿到的产品线清单（要遍历它逐条读），故串在探测之后。
    // 它自己逐条容错：某条线读不到只记 `null`，不会把整页拖成错误态。
    const currentLogins = await loadTraeVariantLogins(statuses);
    return {
      overview: accountData,
      env: envData,
      credits: creditData,
      checkin: checkinData,
      variantStatuses: statuses,
      logins: currentLogins,
    };
  }, [variant]);

  /**
   * 快照缓存（`stores/resources.ts`）。
   *
   * 数据持有权从组件搬到 store，是为了**跨挂载存活**：侧栏切到 WorkBuddy 再切回来时
   * 页面组件会卸载重建，数据留在 store 里 ⇒ 重挂载立刻渲染真数据、不再闪骨架。
   * `loadAll` 是「强制重取并等待」，变更操作（签到 / 切换 / 删除 / 导入）之后必须调它。
   */
  const {
    data: snapshot,
    loading,
    error,
    refresh: loadAll,
  } = useCachedResource<AccountsSnapshot>(`trae:accounts:${variant}`, loadSnapshot);

  /**
   * 签到跳过策略存在设置里，与设置页**共用同一把键** `trae:settings`：
   * 两个页面本来就都读同一份 `get_trae_settings`，共用键之后「设置页改完、账号页
   * 立刻是新的」在结构上成立，而不是靠各自重取去撞。
   */
  const { data: settings, patch: patchSettings } = useCachedResource<TraeSettings>(
    "trae:settings",
    api.getTraeSettings,
  );

  /**
   * 自动签到的**排程**（何时跑），与 Trae 设置页共用同一把键（`SCHEDULE_CONFIG_KEY`）。
   *
   * 它和上面那份 `trae:settings` 是**两份不同的数据**：
   * 这里回答「到点要不要跑、几点跑」，`trae:settings` 回答「跑的时候怎么跑」
   * （跳过已签 / 跳过过期 / 重试次数）。排程是**全局单份**、按**任务**分家 ——
   * WorkBuddy 的六类任务与它读写同一个 `schedule_config.json`。
   */
  const { data: schedule, patch: patchSchedule } = useCachedResource<ScheduleConfig>(
    SCHEDULE_CONFIG_KEY,
    loadScheduleConfig,
  );

  /**
   * 本机全部 Trae 产品线的环境状态（并排视角），与状态条、卡片共用同一份。
   *
   * 账号卡片上的「程序切换按钮」要按**每条线**渲染，因此这里必须拿全量，
   * 而不是 `env`（后者只是「自动挑中的那一条」的单一视角）。
   *
   * 初值是**兜底的两条线占位**而不是空数组：状态条与卡片都直接按它渲染，
   * 空数组会让首次加载期间出现「一枚 Tab 都没有」的空条（模板占了位却什么都没画）。
   * 用占位起步、加载完成后替换，形态与 WorkBuddy 的 region Tabs 一致
   * （后者也是先渲染两枚 Tab，再逐区填状态）。
   */
  const overview = snapshot?.overview ?? null;
  const env = snapshot?.env ?? null;
  const credits = snapshot?.credits ?? null;
  const checkin = snapshot?.checkin ?? null;
  const variantStatuses = snapshot?.variantStatuses ?? TRAE_VARIANT_FALLBACK;
  /**
   * 每条产品线各自的当前登录账号（`profiles.currentAccount` 的 uid + 展示名）。
   *
   * **不能**只读当前这条线：卡片上每条程序按钮的「是否当前账号」是**各判各的**，
   * 同一个 Trae 账号完全可以同时是 Trae Work 的当前账号、却不是 Trae CN 的。
   * 只拿当前线的值会让另一枚按钮永远显示成「未启用」——那是谎报，不是简化。
   *
   * 身份与展示名同源（同一次 `get_trae_profiles`）：卡片用 `userId` 比身份，
   * 状态条用 `name` 显示 —— 两者绑在一个对象里，不会出现半更新。
   */
  const logins = snapshot?.logins ?? {};

  const [busy, setBusy] = useState<string | null>(null);
  /** 「跳过已签到」（签到策略，写 `trae:settings`）的保存中标志。 */
  const [autoCheckinSaving, setAutoCheckinSaving] = useState(false);
  /**
   * 「自动签到」（排程，写 `schedule_config.json`）的保存中标志。
   *
   * 与上面那个**刻意分开**：两者写的是两份不同的文件、属于两个不同的概念层
   * （何时签 vs 怎么签）。共用一个标志会让「切 A 时 B 的转圈亮起」，把操作归因到错的地方。
   */
  const [scheduleSaving, setScheduleSaving] = useState(false);
  const [oauthOpen, setOauthOpen] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [importing, setImporting] = useState(false);
  const [groupFilter, setGroupFilter] = useState<string>("all");
  const [progress, setProgress] = useState<TraeCheckinEvent | null>(null);
  const [report, setReport] = useState<TraeCheckinReport | null>(null);
  const [compact, toggleCompact] = useCompactMode();
  /** 待确认删除的账号。用 shadcn Dialog 承载确认，而非 `window.confirm`——
   *  Tauri WebView 不支持原生 confirm，且原生弹窗无法保持主题与无障碍契约。 */
  const [pendingDelete, setPendingDelete] = useState<TraeAccount | null>(null);

  /**
   * 状态条第二行（每条产品线各自已登录哪个账号）的数据由**本页持有并注入**，
   * 状态条不再自己读一遍。
   *
   * ## 为什么不再用「修订号」触发状态条重读
   *
   * 状态条原先自己读各线登录态，因此需要一个 `variantBarRevision`：否则用户执行
   * 「切换账号 / 保存登录态 / 删除账号 / OAuth 新增」之后，页面主体已更新、
   * 状态条第二行却仍写着旧账号名。
   *
   * 现在卡片上的程序切换按钮**本来就要**每条线的当前账号（否则画不出「那条线上
   * 有没有挂着这个账号」），本页于是成为唯一数据源，状态条改为接收 `logins`。
   * 一次性取齐、同一次渲染下发，不一致在结构上就不可能出现 —— 修订号随之取消，
   * 它要解决的问题已经不存在了。
   */

  /**
   * 一次性把旧「产品线」账号库并入国内版区域账号库（幂等）。
   *
   * ## 为什么挂在页面挂载时
   *
   * 后端在没有旧库、或已经并完时立刻返回 `changed: false`（成本只是一次文件
   * 存在性判断），所以不需要前端再维护「跑过没有」的状态 —— 那反而会引入
   * 「换了台机器/清了缓存就不跑了」这类新缺陷。依赖 `loadAll` 会让切换产品线时
   * 再调一次：**这是有意的**，代价是一次廉价请求，换来的是「用户切过去时数据已就绪」。
   *
   * ## 为什么只在真并了东西时提示
   *
   * 改写用户账号库这件事，用户有权知道改了什么、旧数据备份在哪 ——
   * 因此提示里带上账号数、分组数与**备份路径**；备份失败单独警告（凭据仍在旧文件里，
   * 未被删除，所以不必阻断）。
   *
   * 失败**不阻断页面**：读侧此刻仍按旧库工作，账号一个都没少，用户照常用。
   */
  useEffect(() => {
    if (api.isDemoMode()) return;
    let disposed = false;
    void (async () => {
      try {
        const report = await api.traeMergeLegacyRegions();
        if (disposed || !report.changed) return;
        toast.success(t("trae.page.accounts.mergeDone"), {
          description: [
            t("trae.page.accounts.mergeSummary", {
              added: report.accountsAdded,
              kept: report.accountsKept,
            }),
            report.groupsAdded > 0
              ? t("trae.page.accounts.mergeGroups", { count: report.groupsAdded })
              : null,
            report.backup ? t("trae.page.accounts.mergeBackup", { path: report.backup }) : null,
            report.backupFailed ? t("trae.page.accounts.mergeBackupFailed") : null,
          ]
            .filter(Boolean)
            .join(" · "),
        });
        await loadAll();
      } catch (e) {
        toast.error(t("trae.page.accounts.mergeFailed"), { description: api.asError(e) });
      }
    })();
    return () => {
      disposed = true;
    };
  }, [loadAll]);

  // 签到进度：仅桌面端有事件通道；webui 只在结束时拿到完整报告。
  useEffect(() => {
    if (!api.isDesktop()) return;
    let dispose: (() => void) | undefined;
    void (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        dispose = await listen<TraeCheckinEvent>("trae-checkin-progress", (event) => {
          setProgress(event.payload);
        });
      } catch {
        // 事件通道不可用不影响主流程（仍可用返回的完整报告渲染结果）。
      }
    })();
    return () => dispose?.();
  }, []);

  /** 统一的动作执行：加忙标记、成功提示、失败提示、随后刷新聚合数据。 */
  async function run<T>(
    key: string,
    /** 动作的文案键；渲染时才取词，切换语言后同一枚按钮读到的就是新语言。 */
    labelKey: TranslationKey,
    action: () => Promise<T>,
    labelParams?: Record<string, string | number>,
    after?: (result: T) => void,
    /**
     * 自定义成功文案；返回 `null` 表示用默认的「{label}完成」。
     *
     * 用途：有些操作「成功」与「什么都没做」都算成功（如全部签到遇到
     * 「今日已全部签过」），统一报「完成」会让用户以为刚签了一遍。
     */
    successMessage?: (result: T) => string | null,
  ) {
    setBusy(key);
    const label = t(labelKey, labelParams);
    try {
      const result = await action();
      after?.(result);
      const custom = successMessage?.(result);
      toast.success(custom ?? t("trae.page.accounts.actionDone", { label }));
      await loadAll();
    } catch (e) {
      toast.error(t("trae.page.accounts.actionFailed", { label }), { description: api.asError(e) });
    } finally {
      setBusy(null);
    }
  }

  /**
   * 「自动签到」开关 —— **排程层**的开关（到点由后台调度器签一轮、应用启动时补一轮）。
   *
   * 与 WorkBuddy 工具栏的同名开关**同位同义**（都读写 `schedule_config.json` 里各自那一类
   * 任务），差别只在 Trae 是**独立任务**：关掉 WorkBuddy 的自动签到不会连带关掉 Trae 的，
   * 两边也可以设不同的小时点。
   *
   * 保存走**整份提交**（见 `saveSchedulePatch`）：后端按整份解析，只发一个字段会让
   * 其余任务的配置被默认值覆盖 —— 那是静默的配置丢失。
   */
  async function onAutoCheckinChange(enabled: boolean) {
    if (!schedule || scheduleSaving) return;
    const previous = schedule;
    // 乐观更新写进**快照缓存**：这个值归 `SCHEDULE_CONFIG_KEY` 那把键所有，
    // 写回组件 state 会让「设置页/账号页读到同一份」在结构上不再成立。
    patchSchedule((prev) => ({ ...prev, trae_checkin_enabled: enabled }));
    setScheduleSaving(true);
    try {
      const saved = await saveSchedulePatch(previous, { trae_checkin_enabled: enabled });
      patchSchedule(() => saved);
    } catch (e) {
      patchSchedule(() => previous);
      toast.error(t("trae.page.accounts.autoCheckinSaveFailed"), { description: api.asError(e) });
    } finally {
      setScheduleSaving(false);
    }
  }

  /**
   * 「跳过今日已签到」开关。
   *
   * 与 WorkBuddy 工具栏的第二个开关**同位**（那边是「自动旅行」；Trae 没有该功能，
   * 按用户决定「Trae 无自动旅行则隐藏」处理，不塞假开关占位）。
   *
   * 它属于**签到策略**（怎么签），与上面的自动签到（何时签）是两件事：
   * 后端从设置读（`handlers::parse_checkin_options`），因此手动与自动两轮**都**受它约束。
   */
  async function onSkipCheckedChange(enabled: boolean) {
    if (!settings || autoCheckinSaving) return;
    const previous = settings;
    // 乐观更新写进**快照缓存**而不是组件 state：这个值现在归 `trae:settings` 那把键
    // 所有，写回本地 state 会让「设置页/账号页读到同一份」在结构上不再成立。
    patchSettings(() => ({ ...previous, checkinSkipChecked: enabled }));
    setAutoCheckinSaving(true);
    try {
      const saved = await api.saveTraeSettings({ checkinSkipChecked: enabled });
      patchSettings(() => saved);
    } catch (e) {
      patchSettings(() => previous);
      toast.error(t("trae.page.accounts.settingsSaveFailed"), { description: api.asError(e) });
    } finally {
      setAutoCheckinSaving(false);
    }
  }

  /**
   * 工具栏唯一图标按钮：**签到并刷新全部账号积分**（与 WorkBuddy 同名同形）。
   *
   * Trae 侧这是两个接口（签到、积分快照），WorkBuddy 是一个后端动作，
   * 但对用户的语义完全一致——「点一下把我所有账号都签一遍并把积分刷新出来」。
   * 因此这里必须**两步都做完**再收工：只签到会让卡片上的积分停留在旧值，
   * 用户点完看到的数字没动，会以为签到没生效。
   *
   * 跳过策略由 `settings.checkinSkipChecked` 决定（后端读，不在这里传）。
   */
  async function checkinAll() {
    await run(
      "checkin",
      "trae.page.accounts.actionCheckinAll",
      async () => {
        // 变体决定「签哪条产品线的账号」以及「冷却/摘要落哪份文件」，
        // 不传会让后端回落默认变体 —— 在 Trae CN 页面上点签到却签了 Trae Work。
        const result = await api.traeCheckin({ scope: "all", variant });
        // 积分刷新失败不应让整次操作报错：签到本身可能已经成功，
        // 把「积分没刷出来」当成签到失败会让用户误以为一分没拿到。
        try {
          await api.traeRefreshCredits(undefined, variant);
        } catch (e) {
          toast.warning(t("trae.page.accounts.creditsRefreshFailed"), { description: api.asError(e) });
        }
        return result;
      },
      undefined,
      (result) => setReport(result),
      // 「跳过今日已签到」打开时，一轮只处理**本轮还没签过的**账号：全都签过时
      // `total` 为 0，报「签到并刷新积分完成」会让人以为刚签了一遍。
      (result) => (result.total === 0 ? t("trae.page.accounts.noNeedCheckin") : null),
    );
  }

  /**
   * 单账号签到（卡片菜单里的「手动签到」，与 WorkBuddy 卡片的同名菜单项对齐）。
   *
   * 与 `checkinAll` 的差别只有范围：走 `scope: "selected"` + 当个 `userId`，
   * 与「全部签到」共用同一条后端路径、同一套跳过与冷却规则 ——
   * **不为单账号另开旁路**，否则「整批签到会跳过它、单独点却签了」这类不一致
   * 迟早出现，且两个入口的说辞会互相矛盾。
   *
   * 不走 `run()`：批量动作统一报「××完成」够用，但单账号签到的结果有三种真实形态
   * （签到成功 / 今天已签 / 失败），用户点一下就该直接看到是哪一种，
   * 而不是先收到一句通用的「签到完成」、再自己到底下那张结果卡里找答案。
   */
  async function checkinOne(account: TraeAccount) {
    const label = account.name || account.userId;
    setBusy(`checkin-${account.userId}`);
    try {
      const result = await api.traeCheckin({
        scope: "selected",
        userIds: [account.userId],
        variant,
      });
      setReport(result);
      const outcome = result.results[0];
      if (!outcome) {
        // 一条结果都没有 = 该账号在计划阶段就被跳过（今日已签 / 冷却中 / 凭据过期）。
        // 如实说「没有执行」，不要为了好看谎报成功。
        toast.info(t("trae.page.accounts.checkinNotRun"), {
          description:
            result.warnings[0] ??
            t("trae.page.accounts.checkinSkippedDesc", { name: label }),
        });
      } else if (outcome.action === "skip_already") {
        toast.success(t("trae.page.accounts.alreadyCheckedToday"), { description: label });
      } else if (!outcome.ok) {
        toast.error(t("trae.page.accounts.checkinFailed"), {
          description: t("trae.page.accounts.checkinFailedDesc", {
            name: label,
            reason: outcome.message || outcome.action,
          }),
        });
      } else {
        toast.success(t("trae.page.accounts.checkinSuccess"), {
          description:
            outcome.delta > 0
              ? t("trae.page.accounts.checkinSuccessDesc", { name: label, delta: outcome.delta })
              : label,
        });
      }
      await loadAll();
    } catch (e) {
      toast.error(t("trae.page.accounts.checkinFailed"), { description: api.asError(e) });
    } finally {
      setBusy(null);
    }
  }

  /** 导入本机账号：读客户端登录态里的 `Cloud-IDE-JWT`，已存在的账号就地覆盖刷新。 */
  async function importLocal() {
    if (importing) return;
    setImporting(true);
    try {
      // 把当前产品线传下去：Trae 多条产品线可同机并存，不传会让后端按「最近活跃」
      // 全局挑一条 —— 用户在 Trae Work 分区导入却可能读到 Trae CN 的目录，
      // 连报错文案都会说错产品线。
      //
      // 用已解析好的 `variant` 状态而不是 `env?.variant`：后者在 env 还没加载完时
      // 是 `null`，会让「用户抢在加载完成前点导入」落到默认变体上；
      // `variant` 初值就是默认变体、加载完成后被修正，语义更稳。
      const result = await api.traeImportLocalAccount(variant);
      const who = result.name || result.userId;
      toast.success(t("trae.page.accounts.importLocalDone"), {
        description: result.userId
          ? t("trae.page.accounts.importLocalUid", { name: who, uid: result.userId.slice(-6) })
          : who,
      });
      await loadAll();
    } catch (e) {
      toast.error(t("trae.page.accounts.importLocalFailed"), { description: api.asError(e) });
    } finally {
      setImporting(false);
    }
  }

  const accounts = overview?.accounts ?? [];
  const groups = overview?.groups ?? [];

  const visible = useMemo(() => {
    if (groupFilter === "all") return accounts;
    if (groupFilter === UNGROUPED) return accounts.filter((account) => !account.groupId);
    return accounts.filter((account) => account.groupId === groupFilter);
  }, [accounts, groupFilter]);

  const coolingCount = overview?.cooling ?? 0;
  const expiringSoon = accounts.filter((account) => account.jwtStatus === "warn").length;
  /**
   * **当前正在管理的这条产品线**的当前登录账号（由该线登录态快照的 `currentAccount` 判定）。
   *
   * 它只用于卡片本体（头部高亮、幽灵 logo）：卡片上每枚程序按钮的「是否当前账号」
   * 各判各的，见 {@link programsFor}。
   *
   * ⚠️ 取的是 `userId`（身份），**不是** `name`：卡片高亮与「当前账号」角标都靠
   * 「这个账号 uid == 客户端此刻登录的 uid」，拿展示名比较会在用户改名后立刻失配。
   */
  const currentUserId = logins[variant]?.userId ?? null;
  const switchBusy = busy?.startsWith("switch-") ?? false;

  /**
   * 该账号在**每条 Trae 程序**上的状态 —— 卡片上那排切换按钮的数据源。
   *
   * 与 WorkBuddy 卡片的 `workbuddyActive` / `codebuddyCnIdeActive` / `codebuddyCliActive`
   * 是同一层语义：先在本页把「有哪些程序、各自装没装、这个账号是不是它当前的账号」
   * 算清楚，卡片只负责画。**判断不下沉到卡片里** —— 否则同一账号会被各卡片各算一遍，
   * 迟早与状态条、详情弹窗的口径出现分歧。
   */
  function programsFor(account: TraeAccount): TraeProgram[] {
    // 程序位来自**当前区域**的条目：区域决定账号体系（读哪本库），
    // 程序位决定客户端（登录态写进谁、启动谁）。
    const entry = variantStatuses.find((item) => item.variant === variant);
    return (entry?.programs ?? []).map((program) => ({
      // 尚未建模的程序位（如国际版 TraeCode）没有可回传的标识 ⇒ 用程序位标识占位；
      // 它的 `installed` 必为 false，卡片会渲染成禁用按钮，不会被误点。
      variant: program.variant ?? program.program,
      label: program.label,
      installed: program.installed,
      // 登录态是**客户端级**的：只有拿到程序位标识才能比较，
      // 且两边都非空（`logins` 读不到时是 `null`，不能让 `null` 与空 userId 相互匹配）。
      // 比的是 `userId`（身份）而不是 `name`（展示名）—— 后者改名即失配。
      current:
        program.variant !== null &&
        Boolean(account.userId) &&
        logins[program.variant]?.userId === account.userId,
    }));
  }
  /**
   * **当前区域**的探测结果（含该区域的 `dataDir` / `dataDirExists`）。
   *
   * 与 `env`（`get_trae_env`）的区别是本质的：`env` 走后端 `detect_data_dir()`，
   * 那是**横跨全部产品线**的全局探测，只适合「环境自检」。拿它在区域页里显示目录，
   * 会出现「国际版页签写着 `TRAE SOLO CN`」，而且本机一个候选都不存在时它会回落到
   * 候选表首项 ⇒ 把一个**不存在**的路径说成「已探测的登录态目录」。
   * 按区域取的是后端 `platform::select_data_dir_for(variant)`，不存在就是 `null`。
   */
  const regionStatus = variantStatuses.find((item) => item.variant === variant);

  /**
   * 「客户端环境」那一行要用的**本区域**客户端状态。
   *
   * ## ★ 为什么不能再用 `env`（`get_trae_env`）—— 用户报障现场
   *
   * `env` 是后端 `detect_data_dir()` / `detect_install()` 的**跨变体**全局探测
   * （按活跃度、首个命中挑一条线），只适合「环境自检」。放在区域页上它会**报错产品线**：
   * 本机实测国内版页签显示的是**国际版**客户端 —— `v1.107.1` +
   * `C:\...\AppData\Roaming\TRAE SOLO`，而该目录属 `packageType = SOLO_I18N`
   * （国际版）；真正装着登录态的是 `TRAE SOLO CN`（`SOLO_CN`）。
   *
   * ## 取「该区域的主程序」而不是区域级汇总
   *
   * 与状态条的登录态同源（`programs[0]`）。区域级汇总里的 `dataDir` 是**最近活跃**目录，
   * 而本行要核对的是「登录态在哪」⇒ 用**写侧**目录 `writeDataDir`
   * （与后端 `overview_for().dataDir` 同源）。本机两者不同值：
   * `writeDataDir` = `TRAE SOLO CN`（有登录态）、`dataDir` = `TRAE SOLO`（国际版、更活跃）。
   */
  const clientStatus = regionStatus?.programs?.[0] ?? null;
  const clientInstalled = clientStatus?.installed ?? regionStatus?.installed ?? false;
  const clientVersion = clientStatus?.version ?? regionStatus?.version ?? null;
  const clientPath = clientStatus?.path ?? regionStatus?.path ?? null;
  const clientDataDir = clientStatus?.writeDataDir ?? regionStatus?.writeDataDir ?? null;
  const clientDataDirExists =
    clientStatus?.writeDataDirExists ?? regionStatus?.writeDataDirExists ?? false;
  const clientRunning = clientStatus?.running ?? regionStatus?.running ?? false;

  /**
   * 探测到的是哪条产品线。同机装多个 Trae 时，用户靠这个确认切换器管的是哪一个。
   *
   * 取自**该区域主程序**的展示名（`TraeWork` / `TraeCode` / `TraeWork AI` / `Trae AI`），
   * 不再从 `env` 推 —— 那是全局探测的结果，在国际版页签上会写着国内版的产品名。
   */
  const productLabel = clientStatus?.label ?? regionStatus?.variantLabel ?? null;
  /**
   * 「手工指定」标记：取自**应用级**设置 `settings.traePath`（`env.configuredPath`），
   * 与区域无关，故仍看 `env`。
   */
  const autoDetected = isAutoDetected(env);
  /** 当前产品线的展示名；拿不到探测结果时回落 `variant` 状态的展示名。 */
  const variantLabel = productLabel ?? traeVariantLabel(variant);

  const checkedToday = accounts.filter((account) => account.checkedToday).length;
  const totalCredits = accounts.reduce(
    (sum, account) => sum + (account.remainingCredits ?? account.credits ?? 0),
    0,
  );

  function groupNameOf(account: TraeAccount): string | null {
    if (!account.groupId) return null;
    return groups.find((group) => group.id === account.groupId)?.name ?? t("trae.page.accounts.fallbackGroup");
  }

  return (
    <div className="mx-auto w-full max-w-[1180px] px-6 py-8 sm:px-8 sm:py-9">
      {/* 页头与 WorkBuddy 逐字同构：**只有标题与说明，不放任何动作按钮**。
          动作一律下沉到「添加与迁移账号」横幅与账号工具栏，
          这样两个分区的动作位置、视觉重量完全一致。
          曾经这里堆了 4 个按钮（刷新 / 刷新积分 / 全部签到 / 添加账号），
          与 WorkBuddy 的骨架冲突，也让同一功能出现两个入口。 */}
      <header className="mb-6">
        <div className="min-w-0">
          <h1 className="text-[28px] font-semibold tracking-tight">{t("trae.page.accounts.title")}</h1>
          <p className="mt-2 text-sm leading-6 text-muted-foreground">
            {t("trae.page.accounts.subtitle", { line: variantLabel })}
          </p>
        </div>
      </header>

      {/* 全宽产品线状态条（替代页头右侧的 `TraeVariantSwitch`）。
          `value` / `onValueChange` 映射到 URL `?line=`（`useTraeVariant`）；切换**只改 URL**，
          由下方既有 `loadAll` 重取四份数据——**不**用 `TabsContent` 为每个变体各放一个面板，
          那会让两个面板各自挂载一次取数、每次切换都触发重复请求。 */}
      {/* 两条线的状态与登录态**由本页注入**（本页为了卡片上的程序切换按钮本来就要取全）。
          这比让状态条自己再读一遍更不易错：同一次渲染只有一份数据，
          「主体已是新账号、顶部还写着旧账号」在结构上不可能出现。 */}
      <TraeVariantBar className="mb-6" statuses={variantStatuses} logins={logins} />

      {error && (
        <Alert variant="destructive" className="mb-4">
          <AlertTriangle />
          <AlertTitle>{t("trae.page.accounts.loadFailed")}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {loading && !overview ? (
        <Skeleton className="mb-6 h-24 w-full" />
      ) : accounts.length === 0 ? (
        /* 空态：与 WorkBuddy 的 `EmptyRegionCard` 同构（可能原因 + 期望产物 + 两个动作）。 */
        <EmptyTraeCard
          // ⚠️ 三个字段都必须取**当前区域**那份（见 `EmptyTraeCard` 的 prop 文档）：
          // `env`（`get_trae_env`）是横跨全部产品线的全局探测，在区域页里会张冠李戴
          // —— 「装了」会取自另一条产品线，`dataDir` 会写成另一个客户端的目录。
          installed={regionStatus?.installed ?? false}
          dataDir={regionStatus?.dataDir ?? null}
          dataDirExists={regionStatus?.dataDirExists ?? false}
          onRecheck={() => void loadAll()}
          onImport={() => void importLocal()}
          onOAuth={() => setOauthOpen(true)}
          onLaunch={() =>
            void run("launch-client", "trae.page.accounts.emptyLaunchClient", () =>
              api.launchTraeClient(variant),
            )
          }
          importing={importing}
          launching={busy === "launch-client"}
        />
      ) : (
        <>
          {/* 「添加与迁移账号」横幅：位置、结构与 WorkBuddy 完全一致。 */}
          <div className="relative mb-6 overflow-visible rounded-2xl border border-border bg-muted/30 px-5 py-5 shadow-[0_6px_20px_rgba(15,23,42,.025)]">
            <div className="pointer-events-none absolute inset-0 overflow-hidden rounded-2xl">
              <div className="absolute -right-12 -top-20 size-44 rounded-full border-[28px] border-slate-400/[0.035]" />
            </div>
            <div className="relative flex flex-wrap items-center gap-x-5 gap-y-4">
              <div className="min-w-[190px] flex-1">
                <h2 className="text-sm font-semibold text-foreground">{t("trae.page.accounts.addTitle")}</h2>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                  {t("trae.page.accounts.addSubtitle")}
                </p>
              </div>
              <div className="flex flex-wrap items-center gap-2.5">
                {/* 主入口与 WorkBuddy 同形同位：OAuth。Trae 的授权页是普通网页
                    （不是二维码），因此文案是「网页登录」而不是「扫码添加」。 */}
                <DemoAction>
                  <Button
                    className="h-10 bg-primary px-4 text-primary-foreground shadow-sm hover:bg-primary/90"
                    onClick={() => setOauthOpen(true)}
                  >
                    <QrCode />{t("trae.page.accounts.oauthLogin")}
                  </Button>
                </DemoAction>
                <DemoAction>
                  <Button className="h-10 px-4" onClick={() => void importLocal()} disabled={importing} variant="outline">
                    {importing ? <Loader2 className="animate-spin" /> : <Download />}{t("trae.page.accounts.importLocal")}
                  </Button>
                </DemoAction>
              </div>
              <div className="flex items-center gap-1">
                {/* 粘贴 JWT 是 Trae 独有且**已降级**的兜底路径（OAuth 才是主路径），
                    按「Trae 独有项下沉」放进 ghost 组，与备份导入导出同级。 */}
                <DemoAction>
                  <Button variant="ghost" size="sm" className="h-9 px-2.5" onClick={() => setAddOpen(true)} title={t("trae.page.accounts.pasteJwtTitle")}>
                    <UserPlus />{t("trae.page.accounts.pasteJwt")}
                  </Button>
                </DemoAction>
                <DemoAction>
                  <Button variant="ghost" size="sm" className="h-9 px-2.5" onClick={() => setImportOpen(true)} title={t("trae.page.accounts.importBackupTitle")}>
                    <FileUp />{t("trae.page.accounts.importBackup")}
                  </Button>
                </DemoAction>
                <DemoAction>
                  <Button variant="ghost" size="sm" className="h-9 px-2.5" onClick={() => setExportOpen(true)} disabled={accounts.length === 0} title={t("trae.page.accounts.exportTitle")}>
                    <FileDown />{t("trae.page.accounts.export")}
                  </Button>
                </DemoAction>
              </div>
            </div>
          </div>

          {/* 客户端环境说明行：Trae 独有（WorkBuddy 的环境信息在卡片里），下沉为一条细说明。 */}
          <Card className="mb-6 gap-0 py-0">
            <div className="flex flex-wrap items-center gap-x-8 gap-y-3 px-5 py-3.5 text-sm">
              <span className="flex items-center gap-2">
                {t("trae.page.accounts.client")}
                <span className={cn("font-medium", clientInstalled ? "text-emerald-600" : "text-muted-foreground")}>
                  {clientInstalled
                    ? clientVersion
                      ? t("trae.page.accounts.installedVersion", { version: clientVersion })
                      : t("trae.page.accounts.installed")
                    : t("trae.page.accounts.notInstalled")}
                </span>
                {clientInstalled && productLabel && (
                  <TooltipProvider delayDuration={400}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Badge variant="secondary" className="cursor-default">
                          {productLabel}
                          {!autoDetected && <span className="ml-1 text-muted-foreground">{t("trae.page.accounts.manualPinned")}</span>}
                        </Badge>
                      </TooltipTrigger>
                      <TooltipContent className="max-w-md">
                        <div className="space-y-1 font-mono text-xs break-all">
                          <div>{clientPath ?? "—"}</div>
                          {clientDataDir && <div className="text-muted-foreground">{clientDataDir}</div>}
                        </div>
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                )}
              </span>
              <span className="flex items-center gap-2">
                {t("trae.page.accounts.runtime")}
                <span className={cn("inline-flex items-center gap-1.5 font-medium", clientRunning ? "text-emerald-600" : "text-muted-foreground")}>
                  <span className={cn("size-2 rounded-full", clientRunning ? "bg-emerald-500" : "bg-muted-foreground/50")} />
                  {clientRunning ? t("trae.page.accounts.running") : t("trae.page.accounts.notRunning")}
                </span>
              </span>
              <span className="text-muted-foreground">
                {t("trae.page.accounts.checkedTodayLabel")} <span className="font-medium text-foreground">{checkedToday}</span>
              </span>
              <span className="text-muted-foreground">
                {t("trae.page.accounts.availableCredits")} <span className="font-medium text-foreground">{totalCredits.toLocaleString("zh-CN")}</span>
              </span>
              {credits && (
                <span className="text-muted-foreground">
                  {t("trae.page.accounts.todayEarned")} <span className="font-medium text-foreground">{credits.todayEarned}</span>
                </span>
              )}
              {expiringSoon > 0 && (
                <span className="text-amber-600 dark:text-amber-400">
                  {t("trae.page.accounts.jwtExpiring")} <span className="font-medium">{expiringSoon}</span>
                </span>
              )}
              {coolingCount > 0 && (
                <span className="flex items-center gap-2 text-muted-foreground">
                  <CircleSlash className="size-3.5" />
                  {t("trae.page.accounts.cooling")} <span className="font-medium text-foreground">{coolingCount}</span>
                  <DemoAction>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy === "clear-cooldown"}
                      onClick={() => void run("clear-cooldown", "trae.page.accounts.actionClearCooldown", () => api.traeClearCooldown(undefined, variant))}
                    >
                      {t("trae.page.accounts.clearCooldownAll")}
                    </Button>
                  </DemoAction>
                </span>
              )}
            </div>
            {clientDataDir && (
              <div className="border-t border-border/60 px-5 py-2.5 text-xs text-muted-foreground">
                {t("trae.page.accounts.dataDirLabel")}<code className="font-mono">{clientDataDir}</code>
                {!clientDataDirExists && <span className="ml-2 text-amber-600 dark:text-amber-400">{t("trae.page.accounts.dataDirMissing")}</span>}
              </div>
            )}
          </Card>

          <section className="mt-7 min-w-0" aria-labelledby="trae-accounts-list-title">
            <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <h2 id="trae-accounts-list-title" className="text-base font-semibold tracking-tight">
                  {t("trae.page.accounts.sectionTitle")}
                </h2>
                <Badge
                  variant="secondary"
                  className="h-6 min-w-6 rounded-full border-0 px-1.5 text-[11px] tabular-nums text-muted-foreground shadow-none"
                  aria-label={t("trae.page.accounts.countAria", { count: accounts.length })}
                >
                  {accounts.length}
                </Badge>
                {/* 分组筛选是 Trae 独有项（WorkBuddy 没有分组维度），
                    按「独有项下沉」放在标题侧，不占用右侧工具栏的固定槽位——
                    右侧必须与 WorkBuddy 保持「开关 → 分隔符 → 两个图标」的一致节奏。 */}
                <Select value={groupFilter} onValueChange={setGroupFilter}>
                  <SelectTrigger size="sm" className="ml-1 w-40" aria-label={t("trae.page.accounts.groupFilterAria")}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">{t("trae.page.accounts.allAccounts", { count: accounts.length })}</SelectItem>
                    <SelectItem value={UNGROUPED}>{t("trae.page.accounts.ungrouped", { count: overview?.ungrouped ?? 0 })}</SelectItem>
                    {groups.map((group) => (
                      <SelectItem key={group.id} value={group.id}>
                        {t("trae.page.accounts.groupItem", { name: group.name, count: group.count })}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <TooltipProvider delayDuration={400}>
                <div className="ml-auto flex items-center gap-1">
                  {/* 自动签到：与 WorkBuddy 工具栏的**同名开关同位同义** —— 都写排程任务
                      （何时签）。Trae 是**独立任务**（`trae_checkin`），关掉另一方不受影响。
                      小时点在 Trae 设置页配（这里只放开/关）。 */}
                  <div className="mr-1 flex items-center gap-2.5">
                    <label
                      htmlFor="trae-auto-checkin"
                      className="cursor-pointer text-xs font-medium text-muted-foreground"
                    >
                      {t("trae.page.accounts.autoCheckin")}
                    </label>
                    <DemoAction>
                      <Switch
                        id="trae-auto-checkin"
                        checked={schedule?.trae_checkin_enabled ?? false}
                        disabled={!schedule || scheduleSaving}
                        onCheckedChange={(enabled) => void onAutoCheckinChange(enabled)}
                        aria-label={t("trae.page.accounts.autoCheckin")}
                      />
                    </DemoAction>
                    {scheduleSaving && (
                      <Loader2
                        className="size-3.5 animate-spin text-muted-foreground"
                        aria-label={t("trae.page.accounts.autoCheckinSaving")}
                      />
                    )}
                  </div>
                  {/* 签到策略：与 WorkBuddy 工具栏的第二个开关同位 —— 那边是「自动旅行」，
                      Trae 没有该功能（按用户决定隐藏，不塞假开关占位）。
                      这里放的是「批量签到是否跳过今日已签账号」：它管**怎么签**，
                      与上面的「何时签」是两个层，因此两枚开关并不重复。 */}
                  <div className="mr-1 flex items-center gap-2.5">
                    <label
                      htmlFor="trae-skip-checked"
                      className="cursor-pointer text-xs font-medium text-muted-foreground"
                    >
                      {t("trae.page.accounts.skipChecked")}
                    </label>
                    <DemoAction>
                      <Switch
                        id="trae-skip-checked"
                        checked={settings?.checkinSkipChecked ?? true}
                        disabled={!settings || autoCheckinSaving}
                        onCheckedChange={(enabled) => void onSkipCheckedChange(enabled)}
                        aria-label={t("trae.page.accounts.skipCheckedAria")}
                      />
                    </DemoAction>
                    {autoCheckinSaving && (
                      <Loader2
                        className="size-3.5 animate-spin text-muted-foreground"
                        aria-label={t("trae.page.accounts.checkinSettingsSaving")}
                      />
                    )}
                  </div>
                  <Separator orientation="vertical" className="mx-2 h-5" />
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className={cn("size-9 rounded-lg", compact && "bg-accent text-accent-foreground")}
                        onClick={toggleCompact}
                        aria-label={compact ? t("trae.page.accounts.compactToLoose") : t("trae.page.accounts.compactToCompact")}
                      >
                        {compact ? <Rows3 /> : <Columns3 />}
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side="top">
                      {compact ? t("trae.page.accounts.compactToLoose") : t("trae.page.accounts.compactToCompact")}
                    </TooltipContent>
                  </Tooltip>
                  {/* 与 WorkBuddy 同名同形：一个图标同时承担「批量签到」与「刷新积分」。
                      Trae 侧这两步是两个接口，`checkinAll` 里按顺序调完再统一刷新。 */}
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span>
                        <DemoAction>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="size-9 rounded-lg"
                            disabled={busy === "checkin" || busy === "refresh-credits" || accounts.length === 0}
                            onClick={() => void checkinAll()}
                            aria-label={t("trae.page.accounts.checkinRefreshAll")}
                          >
                            <RefreshCw className={busy === "checkin" || busy === "refresh-credits" ? "animate-spin" : undefined} />
                          </Button>
                        </DemoAction>
                      </span>
                    </TooltipTrigger>
                    <TooltipContent side="top">
                      {api.isDemoMode() ? t("trae.page.accounts.demoDisabled") : t("trae.page.accounts.checkinRefreshAll")}
                    </TooltipContent>
                  </Tooltip>
                </div>
              </TooltipProvider>
            </div>

            {visible.length === 0 ? (
              <Card className="px-5 py-10 text-center text-sm text-muted-foreground">
                {t("trae.page.accounts.groupEmpty")}
              </Card>
            ) : (
              <div className={cn("grid min-w-0 items-start gap-5", compact ? "grid-cols-[repeat(auto-fit,minmax(min(100%,300px),1fr))]" : "grid-cols-[repeat(auto-fit,minmax(min(100%,340px),1fr))]")}>
                {visible.map((account) => (
                  <TraeAccountCard
                    key={account.userId}
                    account={{
                      ...account,
                      // 逐包明细来自 `get_trae_credits` 的 `packages[uid]`：账号视图（`list_account_views_for`）
                      // 只带聚合积分，包粒度明细在积分总览里，按 uid 合并后交给卡片渲染「近期到期」进度条。
                      creditPackages: credits?.packages?.[account.userId] ?? account.creditPackages ?? null,
                    }}
                    groupName={groupNameOf(account)}
                    current={account.userId === currentUserId}
                    programs={programsFor(account)}
                    compact={compact}
                    busy={busy}
                    switchBusy={switchBusy}
                    featuresDisabled={api.isDemoMode()}
                    /* 「把这个账号挂到哪条 Trae 线上」——对应 WorkBuddy 卡片上的
                       WorkBuddy / CodeBuddy IDE / CodeBuddy CLI 三枚按钮。
                       `variant` 取按钮自己那条线（不是当前页面那条）：用户就是要
                       在当前页面上给另一条线挂账号，用页面的 `variant` 会挂错线。 */
                    onSwitchTo={(target, programVariant) =>
                      void run(
                        `switch-${target.userId}@${programVariant}`,
                        "trae.page.accounts.actionSwitch",
                        () =>
                          api.traeSwitchAccount({
                            userId: target.userId,
                            launch: true,
                            variant: programVariant,
                          }),
                        { line: traeVariantLabel(programVariant) },
                        (outcome) => {
                          if (!outcome.success) {
                            const last = outcome.steps[outcome.steps.length - 1];
                            toast.error(t("trae.page.accounts.switchIncomplete"), { description: outcome.error ?? last?.message });
                          }
                        },
                      )
                    }
                    onCheckin={(target) => void checkinOne(target)}
                    onSaveLogin={(target) =>
                      void run(`save-${target.userId}`, "trae.page.accounts.actionSaveLogin", () => api.traeSaveLogin(target.userId, variant))
                    }
                    onRefreshJwt={(target) =>
                      void run(`jwt-${target.userId}`, "trae.page.accounts.actionRefreshJwt", () => api.traeRefreshJwt(target.userId, variant))
                    }
                    onClearCooldown={(target) =>
                      void run(`thaw-${target.userId}`, "trae.page.accounts.actionThaw", () => api.traeClearCooldown(target.userId, variant))
                    }
                    onDelete={(target) => setPendingDelete(target)}
                  />
                ))}
              </div>
            )}
          </section>
        </>
      )}

      {/* 签到进度 / 结果：与「全部签到」同源，就地展开不做成独立页。 */}
      {(progress || report) && (
        <Card className="mt-6 gap-0 py-0">
          <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
            <span className="text-sm font-semibold">{t("trae.page.accounts.resultTitle")}</span>
            {progress?.type === "account" && (
              <span className="text-xs text-muted-foreground">
                {t("trae.page.accounts.resultProgress", { index: progress.index, total: report?.total ?? "…" })}
              </span>
            )}
          </div>
          <div className="divide-y divide-border/60">
            {(report?.results ?? []).map((item) => (
              <div key={item.userId + item.name} className="flex items-center justify-between gap-3 px-5 py-2.5 text-sm">
                <span className="min-w-0 truncate">{item.name}</span>
                <span className="flex shrink-0 items-center gap-3">
                  {item.delta > 0 && <span className="text-emerald-600 dark:text-emerald-400">+{item.delta}</span>}
                  <span className="text-xs text-muted-foreground">{item.message || item.action}</span>
                  <Badge variant={item.ok ? "secondary" : "destructive"}>
                    {item.action === "skip_already"
                      ? t("trae.page.accounts.resultSkipped")
                      : item.ok
                        ? t("trae.page.accounts.resultSuccess")
                        : t("trae.page.accounts.resultFailed")}
                  </Badge>
                </span>
              </div>
            ))}
            {!report && progress?.type === "account" && (
              <div className="px-5 py-2.5 text-sm text-muted-foreground">
                {t("trae.page.accounts.resultLine", {
                  name: progress.name,
                  status:
                    progress.status === "success"
                      ? t("trae.page.accounts.resultSuccess")
                      : progress.status === "already"
                        ? t("trae.page.accounts.resultSkipped")
                        : t("trae.page.accounts.resultFailed"),
                })}
              </div>
            )}
            {/* 空队列要说明白「为什么一条都没有」，否则看着像功能坏了 ——
                打开「跳过今日已签到」后这是最常见的一种正常结果。 */}
            {report && report.results.length === 0 && (
              <div className="px-5 py-2.5 text-sm text-muted-foreground">
                {t("trae.page.accounts.resultEmpty")}
              </div>
            )}
          </div>
          {report && report.warnings.length > 0 && (
            <div className="border-t border-border/60 px-5 py-3 text-xs text-amber-600 dark:text-amber-400">
              {report.warnings.map((warning) => (
                <div key={warning}>{warning}</div>
              ))}
            </div>
          )}
        </Card>
      )}

      {/* 上次签到明细（含冷却）：与 WorkBuddy 一样就地展开，不占导航。 */}
      {(checkin?.summary.results.length ?? 0) > 0 && (
        <Card className="mt-6 gap-0 py-0">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
            <span className="text-sm font-semibold">{t("trae.page.accounts.lastTitle")}</span>
            <span className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
              <span>{checkin?.summary.time ?? t("trae.page.accounts.lastUnknownTime")}</span>
              {checkin && !checkin.summaryIsToday && (
                <span className="text-amber-600 dark:text-amber-400">{t("trae.page.accounts.lastNotToday")}</span>
              )}
              <span className="flex items-center gap-1.5">
                <CheckCircle2 className="size-3.5 text-emerald-600 dark:text-emerald-400" />
                {t("trae.page.accounts.resultSuccess")} <span className="font-medium text-foreground">{checkin?.summary.totalOk ?? 0}</span>
                <XCircle className="ml-1.5 size-3.5 text-destructive" />
                {t("trae.page.accounts.resultFailed")} <span className="font-medium text-foreground">{checkin?.summary.failed ?? 0}</span>
              </span>
            </span>
          </div>
          <div className="divide-y divide-border/60">
            {checkin?.summary.results.slice(0, 10).map((item, index) => (
              <div
                key={`${item.userId}-${index}`}
                className="flex items-center justify-between gap-3 px-5 py-2.5 text-sm"
              >
                <span className="min-w-0 truncate">{item.name || item.userId}</span>
                <span className="flex shrink-0 items-center gap-3">
                  {item.delta > 0 && (
                    <span className="text-emerald-600 dark:text-emerald-400">+{item.delta}</span>
                  )}
                  <span className="max-w-[280px] truncate text-xs text-muted-foreground">
                    {item.message || item.action}
                  </span>
                </span>
              </div>
            ))}
          </div>
          {(checkin?.summary.results.length ?? 0) > 10 && (
            <div className="border-t border-border/60 px-5 py-2.5 text-xs text-muted-foreground">
              {t("trae.page.accounts.lastTruncated", {
                limit: 10,
                total: checkin?.summary.results.length ?? 0,
              })}
            </div>
          )}
          {checkin && checkin.cooldownCount > 0 && (
            <div className="border-t border-border/60 px-5 py-2.5 text-xs text-muted-foreground">
              {t("trae.page.accounts.lastCooldownDetail", {
                list: checkin.cooldowns
                  .map((item) =>
                    t("trae.page.accounts.lastCooldownItem", {
                      uid: item.userId,
                      type: item.type,
                      permanent: item.permanent ? t("trae.page.accounts.lastCooldownPermanent") : "",
                    }),
                  )
                  .join(t("trae.page.accounts.lastCooldownSep")),
              })}
            </div>
          )}
        </Card>
      )}

      <TraeOAuthLoginDialog
        open={oauthOpen}
        onOpenChange={setOauthOpen}
        onSuccess={(account) => {
          toast.success(t("trae.page.accounts.accountAdded"), {
            description: `${account.name || account.userId}${account.hasRefreshToken ? t("trae.page.accounts.autoRenewSuffix") : ""}`,
          });
          void loadAll();
        }}
      />

      <AddAccountDialog
        open={addOpen}
        onOpenChange={setAddOpen}
        groups={groups}
        variant={variant}
        onAdded={() => void loadAll()}
      />

      <TraeExportAccountsDialog
        open={exportOpen}
        onOpenChange={setExportOpen}
        accounts={accounts}
        variant={variant}
        onExported={(count) => toast.success(t("trae.page.accounts.exportedCount", { count }))}
      />

      <TraeImportAccountsDialog
        open={importOpen}
        onOpenChange={setImportOpen}
        variant={variant}
        onImported={(result) => {
          toast.success(t("trae.page.accounts.importDone"), {
            description: t("trae.page.accounts.importSummary", {
              added: result.imported,
              overwritten: result.overwritten,
              skipped: result.skipped,
            }),
          });
          void loadAll();
        }}
      />

      {/* 删除确认 */}
      <Dialog open={pendingDelete !== null} onOpenChange={(open) => !open && setPendingDelete(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("trae.page.accounts.deleteTitle")}</DialogTitle>
            <DialogDescription>
              {t("trae.page.accounts.deleteBody", {
                name: pendingDelete?.name || pendingDelete?.userId || t("trae.page.accounts.unknownAccount"),
              })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPendingDelete(null)}>
              {t("trae.page.accounts.cancel")}
            </Button>
            <Button
              variant="destructive"
              disabled={busy?.startsWith("delete-")}
              onClick={() => {
                const target = pendingDelete;
                if (!target) return;
                setPendingDelete(null);
                void run(`delete-${target.userId}`, "trae.page.accounts.actionDelete", () =>
                  api.traeDeleteAccount(target.userId, false, variant),
                );
              }}
            >
              {t("trae.page.accounts.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/**
 * 空态卡：与 WorkBuddy 的 `EmptyRegionCard` 同构（可能原因 + 期望产物 + 动作）。
 *
 * ## 文案必须说实话：旧版说「登录一次即可自动识别」，那是错的
 *
 * 这张卡原先写着「Cloud-IDE-JWT 存放在该目录下，登录一次 Trae 后即可自动识别」，
 * 并把「从本机导入」作为**主**按钮。但实测 Trae 1.107.1 已改为**加密存储**凭据
 * （`storage.json` / `state.vscdb` / `Local Storage\leveldb` 里都搜不到明文
 * `Cloud-IDE-JWT`），本机导入在这条产品线上**注定失败**。
 *
 * 于是用户看到的正是那句文案：明明已经登录，却怎么都识别不到——这不是探测 bug，
 * 是我们给了一条走不通的路还写了包票。现在把 OAuth 登录提为主入口，
 * 并把本机导入的真实适用范围讲清楚。
 */
function EmptyTraeCard({
  installed,
  dataDir,
  dataDirExists,
  onRecheck,
  onImport,
  onOAuth,
  onLaunch,
  importing,
  launching,
}: {
  installed: boolean;
  /**
   * **当前区域**的客户端数据目录（后端 `platform::select_data_dir_for`）。
   *
   * 不能传 `get_trae_env` 那份 —— 它走 `detect_data_dir()`（**横跨全部产品线**
   * 的全局探测），在区域页里会显示**另一条产品线**的目录名
   * （实测：国际版页签显示 `TRAE SOLO CN`），且本机一个候选都不存在时它会
   * 回落到候选表首项，把一个**根本不存在**的路径说成「已探测」。
   */
  dataDir: string | null;
  dataDirExists: boolean;
  onRecheck: () => void;
  onImport: () => void;
  onOAuth: () => void;
  onLaunch: () => void;
  importing: boolean;
  launching: boolean;
}) {
  const t = useT();
  return (
    <Card className="gap-0 py-0">
      <div className="flex items-start gap-3 px-5 py-5">
        <AlertTriangle className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-medium">{t("trae.page.accounts.emptyTitle")}</h2>

          <div className="mt-3 text-sm text-muted-foreground">
            <p className="font-medium text-foreground/80">{t("trae.page.accounts.emptyReasons")}</p>
            <ul className="mt-1 list-disc space-y-1 pl-5">
              <li>{t("trae.page.accounts.emptyReasonNotInstalled")}</li>
              {installed && <li>{t("trae.page.accounts.emptyReasonNoSession")}</li>}
              <li>
                <span className="text-foreground/80">{t("trae.page.accounts.emptyReasonEncryptedLead")}</span>
                {t("trae.page.accounts.emptyReasonEncryptedMid")}
                <code className="mx-1 rounded bg-muted/60 px-1 py-0.5 font-mono text-[11px]">storage.json</code>
                {t("trae.page.accounts.emptyReasonEncryptedTail")}
              </li>
              <li>{t("trae.page.accounts.emptyReasonEmpty")}</li>
            </ul>
          </div>

          <div className="mt-4 rounded-lg border border-border bg-muted/30 px-3.5 py-3">
            <p className="text-sm text-foreground/80">
              <span className="font-medium">{t("trae.page.accounts.emptyRecommendLabel")}</span>
              {t("trae.page.accounts.emptyRecommendBody")}
            </p>
          </div>

          <div className="mt-4 flex flex-wrap gap-2">
            <Button size="sm" onClick={onOAuth}>
              <QrCode />
              {t("trae.page.accounts.oauthLogin")}
            </Button>
            {/* 启动客户端是「客户端从没启动过 ⇒ 没有设备凭证」的唯一解。
                放在这里而不是只放在 OAuth 弹窗里：本空态的「尝试从本机导入」
                同样依赖客户端数据目录，两个入口的前置条件是一样的。 */}
            <Button size="sm" variant="outline" onClick={onLaunch} disabled={launching}>
              {launching ? <Loader2 className="animate-spin" /> : <Rocket />}
              {t("trae.page.accounts.emptyLaunchClient")}
            </Button>
            <Button size="sm" variant="outline" onClick={onRecheck}>
              <RefreshCw />
              {t("trae.page.accounts.emptyRecheck")}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={onImport}
              disabled={importing}
              title={t("trae.page.accounts.emptyImportHint")}
            >
              {importing ? <Loader2 className="animate-spin" /> : <Download />}
              {t("trae.page.accounts.emptyImportTry")}
            </Button>
          </div>

          <div className="mt-4">
            {/* 目录不存在时不许写「已探测」——拿一个并不存在的路径当「探测结果」
                会把用户引向「文件在、只是读不出」，而真相是客户端从没启动过。 */}
            <p className="text-sm font-medium text-foreground/80">
              {dataDirExists ? t("trae.page.accounts.emptyDetectedDir") : t("trae.page.accounts.emptyExpectedDir")}
            </p>
            <div className="mt-1.5 flex flex-wrap items-center gap-2">
              <code className="min-w-0 break-all rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-[11px] text-muted-foreground">
                {dataDir ? `${dataDir}\\User\\globalStorage\\storage.json` : t("trae.page.accounts.emptyNoDataDir")}
              </code>
              {!dataDirExists && (
                <span className="text-xs text-amber-600 dark:text-amber-400">
                  {t("trae.page.accounts.dataDirMissing")}
                </span>
              )}
            </div>
            {dataDirExists && (
              <p className="mt-1.5 text-xs text-muted-foreground">
                {t("trae.page.accounts.emptyDirNote")}
              </p>
            )}
          </div>
        </div>
      </div>
    </Card>
  );
}

/** 添加账号对话框：粘贴 JWT + 可选分组。 */
function AddAccountDialog({
  open,
  onOpenChange,
  groups,
  variant,
  onAdded,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  groups: TraeAccountsOverview["groups"];
  /** 写进哪个产品线的账号库。 */
  variant: TraeVariantId;
  onAdded: () => void;
}) {
  const t = useT();
  const [name, setName] = useState("");
  const [jwt, setJwt] = useState("");
  const [groupId, setGroupId] = useState<string>("");
  const [saving, setSaving] = useState(false);

  async function submit() {
    if (!jwt.trim()) {
      toast.error(t("trae.page.accounts.needJwt"));
      return;
    }
    setSaving(true);
    try {
      await api.traeAddAccount(name.trim(), jwt.trim(), groupId || null, variant);
      toast.success(t("trae.page.accounts.accountAdded"));
      setName("");
      setJwt("");
      setGroupId("");
      onOpenChange(false);
      onAdded();
    } catch (e) {
      toast.error(t("trae.page.accounts.addFailed"), { description: api.asError(e) });
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("trae.page.accounts.addDialogTitle")}</DialogTitle>
          <DialogDescription>
            {t("trae.page.accounts.addDialogDesc")}
            <br />
            <span className="text-foreground/80">
              {t("trae.page.accounts.addDialogNote")}
            </span>
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="trae-jwt">JWT</Label>
            <Input
              id="trae-jwt"
              value={jwt}
              onChange={(event) => setJwt(event.target.value)}
              placeholder="Cloud-IDE-JWT eyJ…"
              autoComplete="off"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="trae-name">{t("trae.page.accounts.addDialogName")}</Label>
            <Input id="trae-name" value={name} onChange={(event) => setName(event.target.value)} placeholder={t("trae.page.accounts.addDialogNamePlaceholder")} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="trae-group">{t("trae.page.accounts.addDialogGroup")}</Label>
            <Select value={groupId} onValueChange={setGroupId}>
              <SelectTrigger id="trae-group" className="w-full">
                <SelectValue placeholder={t("trae.page.accounts.ungroupedPlaceholder")} />
              </SelectTrigger>
              <SelectContent>
                {groups.map((group) => (
                  <SelectItem key={group.id} value={group.id}>
                    {group.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            {t("trae.page.accounts.cancel")}
          </Button>
          <Button onClick={() => void submit()} disabled={saving}>
            {saving && <Loader2 className="animate-spin" />}
            {t("trae.page.accounts.add")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
