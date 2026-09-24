/**
 * 展示文本归一 —— 把「类型声明说是字符串、运行时却可能是任意 JSON」的字段
 * 收敛成 `string | null`。
 *
 * ## 为什么需要它（issue #2：Win11 打开后一片白）
 *
 * 账号相关的展示字段（`nickname` / `email` / `uid` / `remark` …）在
 * `lib/types.ts` 里都声明为 `string | null`，**但 TypeScript 的类型在运行时不成立**
 * —— 数据来自认证文件与账号库，那里可能出现对象形态（AI 生成的展示名带 emoji /
 * 特殊 unicode 时，客户端会把它存成 `{"zh": "…"}` 这类结构）。
 *
 * 于是同一份脏数据在两个地方发作：
 *
 * 1. **显示错**：`t("…", { name })` 的插值走 `String(params[name])` ⇒ 界面上出现
 *    `已登录: [object Object]`（用户截图里逐字可见）；
 * 2. **整页崩**：`<h3>{name}</h3>` 把对象当 **React 子节点**渲染 ⇒ React 抛
 *    `Objects are not valid as a React child` ⇒ 未捕获 ⇒ React **卸载整棵树**
 *    ⇒ 窗口一片白。同理 `account.email.split("@")`、`account.remark?.trim()`
 *    会直接抛 TypeError。
 *
 * ## 归一的规则（与 Rust 侧 `account::display_str` 保持一致）
 *
 * - `string`：原样保留（含空串 —— 语义与历史一致，调用方的 `||` 回落链会接管）；
 * - `number`：转成文本 —— **纯数字昵称是合法数据**（有人昵称就是数字），
 *   不能当脏值丢掉；
 * - 其余（对象 / 数组 / 布尔 / `null` / `undefined`）：`null`，交给调用方的回落链。
 *
 * 布尔刻意**不**转成 `"true"`：那会让界面把「字段坏了」显示成一个人名，
 * 属于静默降级；落 `null` 才能触发正常的兜底展示。
 *
 * ## 用在哪儿
 *
 * - `lib/api.ts`：在 API 边界收口（`getStatus` / `getAccounts`）—— 这是**主闸**，
 *   一次覆盖账号页全部消费点；
 * - 崩溃现场（`components/account-card.tsx`、`pages/AccountsPage.tsx` 的
 *   `presenceText`）：**副闸**。这些组件是纯展示的，也可能被 demo / 测试数据
 *   喂进来，而它们的输出会直接变成 React 子节点。
 *
 * ⚠️ **时间戳字段（`expiresAt` 一族）不要过这道归一**：它们在 `types.ts` 里是
 * `number | null`，且 `account-card.tsx` 用 `typeof account.expiresAt === "number"`
 * 判定是否过期 —— 一旦被字符串化，过期提示会**静默消失**（不报错，只是不再出现）。
 */
export function displayText(value: unknown): string | null {
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  return null;
}
