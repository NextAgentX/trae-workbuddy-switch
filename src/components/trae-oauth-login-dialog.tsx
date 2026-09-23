import { useEffect, useRef, useState } from "react";
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
import { traeVariantLabel, type TraeAccount } from "@/lib/trae-types";
import { useTraeVariant } from "@/lib/use-trae-variant";

/**
 * 前端独立超时（秒）。
 *
 * 与后端 `TRAE_OAUTH_LOGIN_TIMEOUT_SECONDS`（300s）对齐并留 10s 余量：
 * 后端是最终裁决（它会真正释放端口），前端这一步只是**让用户看得见进度**，
 * 并在后端回调前先给出可操作提示，避免弹窗「像死住了一样」地干等到最后。
 * 取略大于后端，是为了让后端先超时、前端再兜底，两者不会互相打架。
 */
const TRAE_OAUTH_FRONTEND_TIMEOUT_SECONDS = 310;

/** 把剩余秒数格式化为 `M:SS`（如 272 → `4:32`）。 */
function formatRemaining(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds));
  const minutes = Math.floor(safe / 60);
  const seconds = safe % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

/**
 * Trae OAuth 登录（浏览器授权 + 本地回调监听）。
 *
 * ## 与 WorkBuddy 的 `OAuthLoginDialog` 是什么关系
 *
 * **交互骨架逐段照抄**：发起 → 展示验证链接并自动打开浏览器 → 每 1.5s 轮询 →
 * 成功后回调父级刷新 → 关闭。用户在两个分区之间切换时不需要重新学习。
 *
 * 差异只有两处，都是 Trae 侧客观事实决定的，不是设计选择：
 *
 * 1. **多一步「打开浏览器」是我们自己做的**。WorkBuddy 的 `verificationUri` 由
 *    服务端签发，其登录流程本身就会引导用户；Trae 的授权页是普通网页，
 *    必须由客户端打开。所以这里 `start()` 后立刻 `openInBrowser`，
 *    同时把链接展示出来作为兜底（弹窗被拦 / webui 无系统浏览器都能点）。
 * 2. **会话是「本机起监听」而非「服务端有状态」**。因此关闭对话框时要显式
 *    `traeOAuthCancel` 释放端口——WorkBuddy 侧没这个动作，会话在服务端自然过期。
 */
interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 登录成功后的回调（父级据此刷新账号列表）。 */
  onSuccess?: (account: TraeAccount) => void;
}

export function TraeOAuthLoginDialog({ open, onOpenChange, onSuccess }: Props) {
  /**
   * 当前产品线 —— **与页面同源**（`?line=` 承载，见 `useTraeVariant`）。
   *
   * 刻意在这里自己取、而不是新增一个 prop：
   *
   * 1. 本弹窗被父级**无条件挂载**、只切 `open`，prop 需要父级每次都记得传对，
   *    漏传就会静默退回默认变体（这正是本次要修的缺陷形态）；
   * 2. 变体的唯一事实来源是 URL 查询串，这里再取一次**不可能**与页面不一致；
   * 3. 后端「按变体分家」是全链路的：授权页、账号库、数据目录都跟着它走，
   *    所以它必须由本组件直接持有，而不是靠调用方转述。
   */
  const [variant] = useTraeVariant();
  const variantLabel = traeVariantLabel(variant);
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [loginId, setLoginId] = useState<string | null>(null);
  const [uri, setUri] = useState("");
  const [port, setPort] = useState<number | null>(null);
  const [error, setError] = useState("");
  const [result, setResult] = useState<TraeAccount | null>(null);
  /** 剩余等待秒数（`null` 表示尚未进入等待态）。 */
  const [remaining, setRemaining] = useState<number | null>(null);
  /**
   * 是否已到前端超时。
   *
   * 用 `ref` 而非 state：它要在**定时器回调里立即读到最新值**（同一轮里既 stop
   * 轮询又置 error），用 state 会读到闭包里的旧值。它同时驱动倒计时与轮询的停摆。
   */
  const timedOutRef = useRef(false);

  /**
   * 是否已经落定（成功 / 失败 / 超时）。
   *
   * ## 为什么必须有它：终态是**一次性上报**的
   *
   * 后端 [`login_poll`] 在返回终态的**同一次调用**里就把会话从内存摘掉
   * （Rust 侧「轮询到终态后把会话摘掉」）。因此**再轮询一次**只会拿到
   * `{done:true, error:"登录请求不存在或已过期"}`。
   *
   * 前端只要多轮询一次，界面就会**同时**出现「已添加账号：X」与那句红字。
   * 实测触发路径：父级的 `onSuccess` 是内联箭头函数（每次渲染都是新引用），
   * 而它原先在轮询 effect 的依赖数组里 —— 登录成功后 `onSuccess` 里的
   * `loadAll()` 触发父级重渲染 → effect 重跑 → 立刻又轮询一次 → 撞上已被摘除的会话。
   *
   * 修法是两条一起上：① 回调进 ref、依赖里不放函数（见 `onSuccessRef`）；
   * ② 用本 ref 记「已落定」，**跨 effect 重跑也绝不再轮询**。
   * 只用其中一条也能挡住当前这条路径，但两条都留着，未来任何重跑都不会复现。
   */
  const settledRef = useRef(false);

  /**
   * `onSuccess` 的 ref 镜像：轮询 effect 的依赖里**不允许出现函数 prop**。
   *
   * 父级传的是内联箭头函数，引用每次渲染都变；放进依赖会让 effect 无谓重跑
   * （重跑的副作用见 `settledRef` 的说明）。用 ref 承接后，依赖只剩
   * `[open, loginId]` 这两个**真正决定会话身份**的值。
   */
  const onSuccessRef = useRef(onSuccess);
  useEffect(() => {
    onSuccessRef.current = onSuccess;
  }, [onSuccess]);

  // 打开时重置。**同时取消上一次遗留的会话**：用户在轮询中途关掉再打开时，
  // 上一个监听端口还开着；不取消就会泄漏端口，且旧会话超时后可能把
  // 用户后续的授权请求接走（用户会看到「授权成功但应用没反应」）。
  useEffect(() => {
    if (open) {
      setBusy(false);
      setLoginId(null);
      setUri("");
      setPort(null);
      setError("");
      setResult(null);
      setRemaining(null);
      timedOutRef.current = false;
    }
  }, [open]);

  // 轮询登录结果
  useEffect(() => {
    // 父组件是**无条件挂载**本弹窗、只切 `open`；`loginId` 只在 `open` 变 true
    // 时才重置。若此处不检查 `open`，用户点「关闭」后 `loginId` 仍非空，
    // 轮询会继续每 1.5s 打后端（后端此时返回 `done:true, error:"已取消"`，
    // 于是关闭状态下还在 `setError`）。因此关闭即停。
    if (!open || !loginId) return;
    // 已落定过就不再轮询。**这条判断正是缺陷入口的封堵**：effect 因依赖变化重跑时
    // 会立刻再发一次请求，而终态会话早已被后端摘掉，于是「成功」被一句
    // 「登录请求不存在或已过期」染红。见 `settledRef` 的注释。
    if (settledRef.current) return;
    let timer: number | undefined;
    let cancelled = false;

    const poll = async () => {
      // 已到前端超时就停止轮询：后端那条会话由它自己的超时兜底，
      // 这里继续轮询只会让界面在「已超时」的文案下偷偷转圈。
      if (timedOutRef.current || settledRef.current || cancelled) return;
      try {
        const res = await api.traeOAuthStatus(loginId);
        if (res.done) {
          // **先落定、再改界面**：此后任何 effect 重跑都会在入口处直接返回，
          // 不会再发第二次轮询（终端态只能被读一次）。
          settledRef.current = true;
          if (res.account) {
            // 同时校 `!timedOutRef.current`：若后端耗时超过前端 310s 才成功
            // （慢换 token / 时钟偏移），此时界面**已经**因超时置了 error。
            // 渲染层 `{result && …}` 与 `{error && …}` 是两块独立渲染，
            // 若在此 `setResult` 覆盖，会同时显示「已添加账号」与红色超时提示。
            // **刻意**忽略一个迟到的成功结果：用户已被告知失败，
            // 静默接受会让状态与提示自相矛盾。
            if (!cancelled && !timedOutRef.current) {
              setResult(res.account);
              onSuccessRef.current?.(res.account);
            }
          } else if (!cancelled && !timedOutRef.current) {
            setError(res.error || t("trae.comp.oauth.error.fallback"));
          }
          return;
        }
        timer = window.setTimeout(poll, 1500);
      } catch (e) {
        // 轮询本身不抛错（后端永不返 Err），能走到这里说明是传输层问题。
        if (!cancelled && !timedOutRef.current) {
          settledRef.current = true;
          setError(api.asError(e));
        }
      }
    };
    void poll();

    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
    // 依赖里**只有**决定会话身份的两个值。`onSuccess` 走 ref（见 `onSuccessRef`），
    // 放进依赖会让父级每次重渲染都重跑本 effect —— 那正是本次修掉的缺陷。
  }, [open, loginId]);

  // 前端独立倒计时与超时：会话开始时起算，超时后给明确文案并停轮询。
  useEffect(() => {
    // 同轮询 effect：关闭弹窗后 `loginId` 仍非空，若不校 `open`，
    // 倒计时会一直 tick 到组件真正卸载。关闭即停。
    if (!open || !loginId || result) return undefined;
    // 起点：会话刚创建时。`timedOutRef` 保证超时只触发一次。
    const startedAt = Date.now();
    const deadline = startedAt + TRAE_OAUTH_FRONTEND_TIMEOUT_SECONDS * 1000;
    setRemaining(TRAE_OAUTH_FRONTEND_TIMEOUT_SECONDS);

    const tick = () => {
      const left = Math.ceil((deadline - Date.now()) / 1000);
      if (left <= 0) {
        setRemaining(0);
        if (!timedOutRef.current) {
          timedOutRef.current = true;
          // 超时同样是终态：一并落下"已落定"，让轮询的语义只有一个判据。
          settledRef.current = true;
          setError(t("trae.comp.oauth.error.timeout", { seconds: TRAE_OAUTH_FRONTEND_TIMEOUT_SECONDS - 10 }));
        }
        return;
      }
      setRemaining(left);
    };

    tick();
    const interval = window.setInterval(tick, 1000);
    return () => window.clearInterval(interval);
  }, [open, loginId, result]);

  /**
   * 关闭对话框：先取消会话再交给父级。
   *
   * 顺序不能反——先 `onOpenChange(false)` 会把组件卸载、`loginId` 随之丢失，
   * 那个监听端口就再也没人负责回收了。
   */
  async function close() {
    if (loginId && !result) {
      try {
        await api.traeOAuthCancel(loginId);
      } catch {
        // 取消失败不阻断关闭：后端会话有超时兜底，最坏情况是等它自己过期。
      }
    }
    onOpenChange(false);
  }

  async function start() {
    setBusy(true);
    setError("");
    // 重置超时标志与剩余时间：这是「重新发起」与「首次发起」共用的入口。
    // `settledRef` 必须一起重置 —— 否则「失败后重新发起」会因残留的"已落定"
    // 直接跳过轮询，新会话永远等不到结果。
    timedOutRef.current = false;
    settledRef.current = false;
    setRemaining(null);
    try {
      const res = await api.traeOAuthStart(variant);
      setLoginId(res.loginId);
      setUri(res.verificationUri);
      setPort(res.port);
      // 按当前宿主能力打开验证页
      await openInBrowser(res.verificationUri);
    } catch (e) {
      setError(api.asError(e));
    } finally {
      setBusy(false);
    }
  }

  /**
   * 出错后重新发起：先取消旧会话（释放它占的端口），再走一次 [`start`]。
   *
   * 不能只调 `start`：旧会话的监听任务还在等回调，不取消会让端口泄漏，
   * 且旧会话超时后可能把新会话的回调请求接走。
   */
  async function retry() {
    if (loginId) {
      try {
        await api.traeOAuthCancel(loginId);
      } catch {
        // 取消失败不阻断重试：后端会话有超时兜底。
      }
    }
    setLoginId(null);
    setUri("");
    setPort(null);
    setResult(null);
    await start();
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) void close();
        else onOpenChange(true);
      }}
    >
      <DialogContent>
        <DialogHeader>
          {/*
            标题里**必须**带产品线名：两条产品线的授权页、账号库、数据目录全都不同，
            而本弹窗的骨架与 WorkBuddy 的一致，用户很容易以为「点哪个都一样」。
            说清「正在为哪条线登录」是本次变体透传修复的**用户可见那一半** ——
            只把参数传对、却不告诉用户，用户仍然无法预期账号会落到哪里。
          */}
          <DialogTitle>{t("trae.comp.oauth.title", { variant: variantLabel })}</DialogTitle>
          <DialogDescription>
            {t("trae.comp.oauth.desc.lead")}{" "}
            <span className="font-medium text-foreground">{variantLabel}</span>{" "}
            {t("trae.comp.oauth.desc.mid")}{" "}
            <span className="font-medium text-foreground">{variantLabel}</span>
            {t("trae.comp.oauth.desc.tail")}
          </DialogDescription>
        </DialogHeader>

        {!loginId && !result && (
          <div className="space-y-3">
            <Button onClick={start} disabled={busy} className="w-full">
              {busy ? t("trae.comp.oauth.startBusy", { variant: variantLabel }) : t("trae.comp.oauth.start", { variant: variantLabel })}
            </Button>
          </div>
        )}

        {loginId && !result && !error && (
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
              {t("trae.comp.oauth.waiting")}
              {remaining !== null && (
                <span className="ml-1 text-xs">{t("trae.comp.oauth.remaining", { time: formatRemaining(remaining) })}</span>
              )}
            </p>
            {/* 显式展示回调地址与端口。
                端口是**固定的 17388**（见 Rust 侧 `oauth::CALLBACK_PORT`）：Trae 授权页
                在「认证中」阶段会探测这个固定端口判断客户端在线，随机端口会让它永远探不到。
                把地址摆出来是因为「授权页探得到、但没把回调打回来」这种情况只能靠它排查。 */}
            {port !== null && (
              <p className="text-xs text-muted-foreground break-all">
                {t("trae.comp.oauth.callback")}
                <code>http://127.0.0.1:{port}/authorize</code>
              </p>
            )}
          </div>
        )}

        {result && (
          <Alert>
            <AlertDescription>{t("trae.comp.oauth.result", { name: result.name || result.userId })}</AlertDescription>
          </Alert>
        )}

        {/*
          错误块**必须**与结果块互斥：`{error && …}` 单独渲染时，一次多余的轮询
          就能让界面同时出现「已添加账号：X」与红色报错（本次修掉的缺陷形态）。
          轮询侧已落定封堵，这里再互斥一次 —— 两块提示自相矛盾是最坏的用户体验，
          值得两道保险。重试按钮本来就用的 `error && !result`，这里与之对齐。
        */}
        {error && !result && (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        {/* 出错/超时后仍把回调地址摆出来：这是排查「回调没打回本机」的唯一抓手。 */}
        {error && !result && port !== null && (
          <p className="text-xs text-muted-foreground break-all">
            {t("trae.comp.oauth.callback")}
            <code>http://127.0.0.1:{port}/authorize</code>
          </p>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => void close()}>
            {t("trae.comp.oauth.close")}
          </Button>
          {result && <Button onClick={() => void close()}>{t("trae.comp.oauth.done")}</Button>}
          {error && !result && (
            <Button onClick={() => void retry()} disabled={busy}>
              {busy ? t("trae.comp.oauth.retryBusy") : t("trae.comp.oauth.retry")}
            </Button>
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
