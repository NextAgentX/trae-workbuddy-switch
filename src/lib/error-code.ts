import { currentLocale, interpolate } from "@/lib/i18n";
import { asErrorCodeId, EN_ERROR_TEMPLATES, type ErrorCodeId } from "@/locales/errors.en";

/**
 * 错误字符串的**结构尾**编解码（前端侧）。
 *
 * 后端把「文本 + 码 + 参数」编成一个字符串（见
 * `crates/buddy-switch-core/src/modules/error_code.rs`）。选这个载体而不是新增 JSON 字段，
 * 是因为错误要走两条通道、两种位置：
 *
 * - Tauri：command 签名统一 `Result<_, String>`（70 处），reject 出来的就是字符串；
 * - webui：响应体 `{ok:false, error:"…"}`；
 * - 还有第三种：错误本身是**数据**（如 `TraeAccount.error` 字段），同样只放得下字符串。
 *
 * 三种位置用同一个解码器即可，不必改任何 JSON 形状或命令签名。
 *
 * ## 界面文案的裁决规则
 *
 * {@link localizeError} 是**唯一**的呈现出口：
 *
 * | 当前语言 | 结果 |
 * |:---|:---|
 * | 中文 | 后端原文（去掉结构尾）—— 后端是措辞权威，避免两份中文各自漂移 |
 * | 英文 + 认得的码 | 英文模板 + 参数插值 |
 * | 英文 + 不认得的码 / 无码 | 回落后端原文（**宁可显示中文，也不显示空白或乱码**） |
 *
 * 「英文界面里出现一条中文错误」是刻意接受的最坏情况：它只在后端新增了码而前端尚未补英文时
 * 出现，且一眼可辨、可运维；反过来（吞掉错误或显示 `undefined`）会让用户彻底失去线索。
 */
export const WIRE_SEP = "\u001f";

export interface DecodedError {
  /** 去掉结构尾之后的用户可见文本（中方原文）。 */
  text: string;
  /** 本版本认识的码；无尾部、码损坏或码未知时为 `null`。 */
  code: ErrorCodeId | null;
  /** 尾部携带的插值参数。 */
  params: Record<string, string>;
  /** 尾部有码但本版本不认识（新后端 → 旧前端），供排障使用。 */
  unrecognizedCode: string | null;
}

/**
 * 解析跨通道错误字符串。**永不抛错**。
 *
 * 与 Rust 侧 `decode_wire` 同一套切分规则：以**最后两个**分隔符为界，
 * 之前的全部算文本（因此文案里恰好含分隔符时不会把内容切碎）。
 */
export function decodeError(raw: string): DecodedError {
  const plain = (text: string): DecodedError => ({
    text,
    code: null,
    params: {},
    unrecognizedCode: null,
  });

  const segments = raw.split(WIRE_SEP);
  if (segments.length < 3) return plain(raw);

  const paramsJson = segments[segments.length - 1];
  const codeText = segments[segments.length - 2];
  const text = segments.slice(0, segments.length - 2).join(WIRE_SEP);
  if (!codeText) return plain(raw);

  const params: Record<string, string> = {};
  try {
    const parsed: unknown = JSON.parse(paramsJson);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
        params[key] = typeof value === "string" ? value : String(value);
      }
    }
  } catch {
    // 参数坏了不影响文本与码：错误信息比参数重要。
  }

  const code = asErrorCodeId(codeText);
  return code
    ? { text, code, params, unrecognizedCode: null }
    : { text, code: null, params, unrecognizedCode: codeText };
}

/** 去掉结构尾，只留用户该看到的文本。 */
export function stripErrorCode(raw: string): string {
  return decodeError(raw).text;
}

/**
 * 把任意错误字符串渲染成**当前语言**的文案。
 *
 * 这是全应用错误呈现的唯一裁决点 —— `api.asError()` 走它，因此所有已经用
 * `asError(e)` 的调用点**无需逐个改造**就同时获得中英两种文案。
 */
export function localizeError(raw: string): string {
  const decoded = decodeError(raw);
  // 中文界面（或任何非英文界面）：后端原文就是权威措辞。
  if (currentLocale() !== "en") return decoded.text;
  if (!decoded.code) return decoded.text;
  const template = EN_ERROR_TEMPLATES[decoded.code];
  if (!template) return decoded.text;
  // ★ 模板需要的参数必须**全部**到位，否则回落原文。
  //
  // 与 `interpolate` 的「保留占位符以便发现漏配」相反：那里漏配是**开发期错误**
  // （`t()` 的参数是编译期确定的），一眼可见是优点；这里漏配是**运行期数据缺失**
  // （后端版本不匹配、结构尾被截断），把 `{chain}` 端到用户面前毫无价值 ——
  // 中文原文至少还是一句能读的话。
  if (!hasAllPlaceholders(template, decoded.params)) return decoded.text;
  return interpolate(template, decoded.params);
}

/** 模板里的每个 `{name}` 都能在 `params` 里找到值。 */
function hasAllPlaceholders(template: string, params: Record<string, string>): boolean {
  for (const match of template.matchAll(/\{(\w+)\}/g)) {
    if (!Object.prototype.hasOwnProperty.call(params, match[1])) return false;
  }
  return true;
}

/**
 * **单一收口**：把响应体里所有带结构尾的字符串就地本地化。
 *
 * ## 为什么要在数据边界做，而不是逐个渲染点做
 *
 * 错误有两条到达界面的路径：
 *
 * 1. **被抛出**（`Err(String)` → `invoke` reject / HTTP 非 2xx）→ 由 `api.asError()` 收口；
 * 2. **当数据传回**（`TraeAccount.error`、OAuth 轮询结果的 `error`、迁移警告……）→
 *    由**几十个渲染点**各自直接 `{error}` 显示。
 *
 * 路径 2 是真正的风险：漏掉任何一处，用户界面上就会出现一个不可见的控制字符
 * （`U+001F`）以及它后面的码，复制粘贴还会带出去。这类「漏一个就出事」的清单无法维持 ——
 * 所以策略不是逐个补渲染点，而是在**数据进入应用的唯一入口**（`api.ts` 的 `call`/`httpCall`）
 * 统一处理，此后进入组件的字符串**保证不含结构尾**。
 *
 * ## 三个性质
 *
 * - **幂等**：已本地化的字符串不含分隔符，再次调用原样返回。
 * - **就近改写、零拷贝**：只有真的含分隔符的字符串才被替换；整个响应不含分隔符时
 *   **不产生任何新对象**（保持引用相等，不影响 React 的重渲染判断）。
 * - **不动原型链**：只遍历普通对象与数组；`Date`、`Map` 等一律跳过。
 */
export function localizeCodedStrings<T>(value: T): T {
  rewriteInPlace(value);
  return value;
}

function rewriteInPlace(value: unknown): void {
  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      const item: unknown = value[index];
      if (typeof item === "string") {
        if (item.includes(WIRE_SEP)) value[index] = localizeError(item);
      } else {
        rewriteInPlace(item);
      }
    }
    return;
  }
  if (!value || typeof value !== "object") return;
  const proto = Object.getPrototypeOf(value);
  if (proto !== Object.prototype && proto !== null) return;
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    const item = record[key];
    if (typeof item === "string") {
      if (item.includes(WIRE_SEP)) record[key] = localizeError(item);
    } else {
      rewriteInPlace(item);
    }
  }
}
