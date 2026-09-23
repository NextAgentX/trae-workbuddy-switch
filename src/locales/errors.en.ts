/**
 * 错误码 → 英文文案。
 *
 * ## 为什么这里**只有英文**
 *
 * 后端（Rust）是这些错误的**生产者**，它下发的文本本身就是中文，并且对中文用户是
 * 权威措辞（`crates/buddy-switch-core/src/modules/net.rs` 的 `transport_error_label`）。
 * 如果这里再维护一份中文，两处措辞会各自演化 —— 迟早出现「同一种故障两种说法」。
 * 因此中文路径**直接用后端原文**，本表只负责英文。
 *
 * ## 占位符
 *
 * `{chain}` 等占位符由后端随码一起下发（见 `lib/error-code.ts` 的 `params`）。
 * 后端漏传某个参数时占位符会原样留在文案里 —— 这是刻意的：漏配一眼可见，
 * 比静默产出半句话好排障。
 *
 * ## 新增一个码
 *
 * 后端 `ErrorCode` 加变体 → 这里的联合类型加字面量 → 补一条英文。
 * **键名（码）一旦发布不得改名**，它是跨版本的协议。
 */
export type ErrorCodeId =
  | "net.transport.timeout"
  | "net.transport.connect"
  | "net.transport.body"
  | "net.transport.other"
  | "permission.denied";

export const EN_ERROR_TEMPLATES: Partial<Record<ErrorCodeId, string>> = {
  "net.transport.timeout": "Connection timed out: {chain}",
  "net.transport.connect":
    "Cannot connect (DNS failure / network unreachable / TLS handshake failure): {chain}",
  "net.transport.body": "Failed to read the response body: {chain}",
  "net.transport.other": "Request failed: {chain}",
  "permission.denied":
    "No permission to write the credential file. Open System Settings → Privacy & Security → App Management and allow this app to control WorkBuddy's data (or grant it Full Disk Access), then retry.",
};

/** 运行期校验：把后端下发的任意字符串收紧为本版本认识的码。 */
export function asErrorCodeId(value: string): ErrorCodeId | null {
  return Object.prototype.hasOwnProperty.call(EN_ERROR_TEMPLATES, value)
    ? (value as ErrorCodeId)
    : null;
}
