//! WorkBuddy 5.6 的 **at-rest 加密信封**解密。
//!
//! 客户端从 5.6 起把 `accessToken` / `refreshToken` / `nickname` 存成
//! `{"$wbEncrypted":1,"envelope":"<base64>"}`。本模块把它们解回明文，好让签到 /
//! 积分 / Token 统计 / 刷新这些**需要账号身份**的链路能用「从本机导入」的账号。
//!
//! # 密钥从哪来（**不需要逆向二进制**）
//!
//! ```text
//! ELECTRON_RUN_AS_NODE=1 WorkBuddy.exe probe.js
//!   └─ process._linkedBinding("electron_browser_workbuddy_storage").loggerGet()
//!        └─ {version:1, atRestSecretKey:"<base64 32B>", atRestDeveloperPublicKey:"…"}
//! ```
//!
//! `ELECTRON_RUN_AS_NODE=1` 让官方 exe 以**纯 Node** 模式启动（不开窗、不写数据），
//! 原生模块已注册在二进制里 ⇒ `_linkedBinding` 照常可用。钥匙是**编译期 payload**，
//! 不以字符串形式躺在二进制里 ⇒「grep exe 找不到」不等于「拿不到」。
//!
//! # 密码学参数（逐字对照客户端 `app.asar` 的 `packages/at-rest-crypto`）
//!
//! | 项 | 值 |
//! | --- | --- |
//! | 套件 | `suite=1`，`scheme="sym-v1"`，AES-256-GCM，nonce 12B，tag 16B |
//! | 钥匙 | `protectorKey = sha256(utf8(atRestSecretKey))`（32B） |
//! | keyId | `sha256(protectorKey).hex[..16]`，**必须**等于信封声明的 `keyId` |
//! | AAD | 见 [`build_field_aad`]，`framing="field"`，固定 **54 字节** |
//!
//! 字段级加密**直接**用 `protectorKey`，不经过 `~/.workbuddy/keyblob` 里的 masterKey
//! （那是文件级加密用的，本模块用不到）。
//!
//! # 为什么敢把明文写回账号库
//!
//! 客户端的字段编解码器是 `decodeString(v) { if (typeof v === "string") return v; … }`
//! ⇒ **明文照收**；且它下次写回时会自己把明文字段重新封成信封。所以解锁后的账号与
//! 「OAuth 扫码添加」得到的账号**形态完全一致**，切换写回不会毁掉客户端登录态。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use aes_gcm::aead::AeadInPlace;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, Tag};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::modules::account::is_envelope;
use crate::modules::region::Region;

/// AAD 域前缀（`app.asar`：`AAD_DOMAIN = Buffer.from("WB-AAD\0", "ascii")`）。
const AAD_DOMAIN: &[u8] = b"WB-AAD\0";
/// `STANDARD_FORMAT_ID.field`。
const FORMAT_ID_FIELD: &str = "WBEV1";
/// `FRAMING_CODE.field`。
const FRAMING_CODE_FIELD: u8 = 2;
/// 对称套件名。
const SCHEME_SYM_V1: &str = "sym-v1";
/// 客户端当前只认 suite 1（`SUPPORTED_SUITES = {1}`）。
const SUPPORTED_SUITE: u32 = 1;
/// 字段信封的 AAD 固定长度。**写错就是 53 或 61**，见 [`build_field_aad`]。
pub const FIELD_AAD_LEN: usize = 54;
/// 取钥子进程的超时。正常约 1 秒；超时即 kill，绝不把界面卡死。
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// 取钥探针（写进临时文件，交给官方 exe 以 Node 模式执行）。
///
/// 只回一个 key，**不做任何解密** —— 密码学全在 Rust 侧，便于单测与复用。
const KEY_PROBE_JS: &str = r#"
try {
  var n = process._linkedBinding("electron_browser_workbuddy_storage");
  var p = JSON.parse(n.loggerGet());
  if (typeof p.atRestSecretKey !== "string" || p.atRestSecretKey.length === 0) {
    console.log(JSON.stringify({ ok: false, error: "missing-at-rest-secret-key" }));
  } else {
    console.log(JSON.stringify({ ok: true, atRestSecretKey: p.atRestSecretKey }));
  }
} catch (e) {
  console.log(JSON.stringify({ ok: false, error: String((e && e.message) || e) }));
}
"#;

/// 解锁失败的原因。**必须分档**：每档的处置完全不同，混成一句「信封态」等于没诊断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtRestError {
    /// 本机没装客户端（或探测不到可执行文件）。
    ClientMissing,
    /// 子进程起不来（权限、路径、被杀软拦）。
    Spawn(String),
    /// 子进程超时（已被 kill）。
    Timeout,
    /// 探针回了 `ok:false`，或输出不是预期 JSON。
    Probe(String),
    /// 信封本身解析不了（base64 / JSON / 字段缺失 / nonce 长度不对）。
    Envelope(String),
    /// 信封声明的 keyId 与由密钥推出的 keyId 不符 ⇒ 客户端换实现或拿错钥匙。
    KeyIdMismatch { declared: String, derived: String },
    /// GCM 校验失败（AAD / tag / 密文任一不符）。
    Integrity,
}

impl AtRestError {
    /// 稳定的错误码，供日志与界面分档（**不要**让调用方去 match 中文文案）。
    pub fn code(&self) -> &'static str {
        match self {
            AtRestError::ClientMissing => "client-missing",
            AtRestError::Spawn(_) => "spawn-failed",
            AtRestError::Timeout => "probe-timeout",
            AtRestError::Probe(_) => "probe-failed",
            AtRestError::Envelope(_) => "envelope-invalid",
            AtRestError::KeyIdMismatch { .. } => "key-id-mismatch",
            AtRestError::Integrity => "integrity-error",
        }
    }
}

impl std::fmt::Display for AtRestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtRestError::ClientMissing => {
                write!(f, "未找到 WorkBuddy 桌面客户端，无法取得解密密钥")
            }
            AtRestError::Spawn(e) => write!(f, "启动 WorkBuddy 客户端取密钥失败：{e}"),
            AtRestError::Timeout => write!(f, "WorkBuddy 客户端取密钥超时"),
            AtRestError::Probe(e) => write!(f, "WorkBuddy 客户端未返回可用密钥：{e}"),
            AtRestError::Envelope(e) => write!(f, "加密信封格式无法解析：{e}"),
            AtRestError::KeyIdMismatch { declared, derived } => write!(
                f,
                "信封声明的 keyId({declared}) 与密钥推出的 keyId({derived}) 不符，\
                 客户端可能已升级加密实现"
            ),
            AtRestError::Integrity => write!(f, "信封认证失败（AAD / 认证标签不符）"),
        }
    }
}

impl std::error::Error for AtRestError {}

/// 解析后的信封。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub suite: u32,
    pub key_id: String,
    pub nonce: Vec<u8>,
    pub auth_tag: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// `u32BE` 长度前缀 + UTF-8 字节（`encodeLengthPrefixed`）。
///
/// ⚠️ 前缀是 **u32BE**，不是 u8 —— 写成 u8 会得到一个能编译、但永远解不开的 AAD。
fn encode_length_prefixed(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
    out
}

/// `encodeOptionalUint64(void 0)` ⇒ **1 字节 `0x00`**。
///
/// ⚠️ 不是空、也不是 8 个零字节。这是第一次解密失败的原因之一：写成空得 53 字节、
/// 写成 8 个零得 61 字节，两种都会 `INTEGRITY_ERROR`。
const OPTIONAL_UINT64_ABSENT: u8 = 0x00;

/// `final === undefined ⇒ 0`。
const FINAL_ABSENT: u8 = 0x00;

/// 构造 `framing="field"` 的 AAD（逐字对照 `buildAuthenticatedContextAad`）。
///
/// ```text
/// "WB-AAD\0"                     7B
/// 0x01                           1B
/// u32BE(5)  + "WBEV1"            9B
/// u32BE(6)  + "sym-v1"          10B
/// u32BE(suite)                   4B
/// u32BE(16) + keyId             20B   ← keyId 是 ASCII 十六进制串
/// 0x02                           1B   FRAMING_CODE.field
/// 0x00                           1B   encodeOptionalUint64(undefined)
/// 0x00                           1B   final === undefined
/// ----------------------------------
/// 合计                          54B
/// ```
pub fn build_field_aad(key_id: &str, suite: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(FIELD_AAD_LEN);
    out.extend_from_slice(AAD_DOMAIN);
    out.push(0x01);
    out.extend_from_slice(&encode_length_prefixed(FORMAT_ID_FIELD));
    out.extend_from_slice(&encode_length_prefixed(SCHEME_SYM_V1));
    out.extend_from_slice(&suite.to_be_bytes());
    out.extend_from_slice(&encode_length_prefixed(key_id));
    out.push(FRAMING_CODE_FIELD);
    out.push(OPTIONAL_UINT64_ABSENT);
    out.push(FINAL_ABSENT);
    out
}

/// 由客户端给出的 `atRestSecretKey` 派生 protector key。
///
/// ⚠️ 是 **sha256(该字符串的 UTF-8 字节)**，不是「base64 解码后再 sha256」。
pub fn derive_protector_key(at_rest_secret_key: &str) -> [u8; 32] {
    let digest = Sha256::digest(at_rest_secret_key.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

/// `sha256(key).hex[..16]`（小写十六进制）。
pub fn key_id_for(key: &[u8]) -> String {
    let digest = Sha256::digest(key);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex[..16].to_string()
}

/// 解析信封（base64 → JSON → 五元组）。长度与套件都在这里拦住，别留到解密再报错。
pub fn parse_envelope(envelope_b64: &str) -> Result<Envelope, AtRestError> {
    let raw = B64
        .decode(envelope_b64)
        .map_err(|e| AtRestError::Envelope(format!("base64 解码失败：{e}")))?;
    let inner: Value = serde_json::from_slice(&raw)
        .map_err(|e| AtRestError::Envelope(format!("内层 JSON 解析失败：{e}")))?;
    let suite = inner
        .get("suite")
        .and_then(Value::as_u64)
        .ok_or_else(|| AtRestError::Envelope("缺少 suite".into()))? as u32;
    if suite != SUPPORTED_SUITE {
        return Err(AtRestError::Envelope(format!("不支持的套件 suite={suite}")));
    }
    let key_id = inner
        .get("keyId")
        .and_then(Value::as_str)
        .ok_or_else(|| AtRestError::Envelope("缺少 keyId".into()))?
        .to_string();
    if key_id.len() != 16 || !key_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AtRestError::Envelope(format!("keyId 形态不对：{key_id}")));
    }
    let decode = |field: &str| -> Result<Vec<u8>, AtRestError> {
        inner
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| AtRestError::Envelope(format!("缺少 {field}")))
            .and_then(|s| {
                B64.decode(s)
                    .map_err(|e| AtRestError::Envelope(format!("{field} base64 解码失败：{e}")))
            })
    };
    let nonce = decode("nonce")?;
    if nonce.len() != 12 {
        return Err(AtRestError::Envelope(format!(
            "nonce 长度应为 12，实为 {}",
            nonce.len()
        )));
    }
    let auth_tag = decode("authTag")?;
    if auth_tag.len() != 16 {
        return Err(AtRestError::Envelope(format!(
            "authTag 长度应为 16，实为 {}",
            auth_tag.len()
        )));
    }
    Ok(Envelope {
        suite,
        key_id,
        nonce,
        auth_tag,
        ciphertext: decode("ciphertext")?,
    })
}

/// 解一个字段信封。**先比 keyId**：不匹配就别去试解密，直接说清是哪一档错。
pub fn decrypt_field(envelope_b64: &str, protector_key: &[u8; 32]) -> Result<String, AtRestError> {
    let envelope = parse_envelope(envelope_b64)?;
    let derived = key_id_for(protector_key);
    if derived != envelope.key_id {
        return Err(AtRestError::KeyIdMismatch {
            declared: envelope.key_id,
            derived,
        });
    }
    let cipher = Aes256Gcm::new_from_slice(protector_key)
        .map_err(|e| AtRestError::Envelope(format!("密钥长度不对：{e}")))?;
    let aad = build_field_aad(&envelope.key_id, envelope.suite);
    // ★ 用 **detached** 接口，不能用 `Aead::decrypt`：后者要求 `msg` 是
    //   「密文 ‖ 认证标签」拼接（它自己切末尾 16 字节当 tag）。本格式的 tag 是
    //   **独立字段**，直接喂 `decrypt` 会把密文尾巴当成 tag ⇒ 恒定 `INTEGRITY_ERROR`。
    let mut buffer = envelope.ciphertext.clone();
    cipher
        .decrypt_in_place_detached(
            Nonce::from_slice(&envelope.nonce),
            &aad,
            &mut buffer,
            Tag::from_slice(&envelope.auth_tag),
        )
        .map_err(|_| AtRestError::Integrity)?;
    String::from_utf8(buffer).map_err(|e| AtRestError::Envelope(format!("明文非 UTF-8：{e}")))
}

/// 客户端可执行文件（Windows 是 exe；macOS 是 `.app` 里的可执行体）。
///
/// `workbuddy_app_path_for` 在 macOS 返回的是 **`.app` 包路径**，而 `ELECTRON_RUN_AS_NODE`
/// 要的是包内的可执行体 ⇒ 这里补一层 `Contents/MacOS/<name>`（Electron 约定：
/// `CFBundleExecutable` 与包名同名）。名字猜错不会静默：子进程起不来就是 `Spawn`。
pub fn client_executable_for(region: Region) -> PathBuf {
    let path = crate::modules::auth_file::workbuddy_app_path_for(region);
    #[cfg(target_os = "macos")]
    {
        if path.extension().and_then(|e| e.to_str()) == Some("app") {
            if let Some(stem) = path.file_stem() {
                return path.join("Contents").join("MacOS").join(stem);
            }
        }
    }
    path
}

/// 探针脚本落盘路径（固定名，每次覆盖写；内容只有 1 KB）。
fn key_probe_path() -> PathBuf {
    std::env::temp_dir().join("buddy-switch-at-rest-key.js")
}

/// 起子进程取 `atRestSecretKey`。
///
/// 用 `cmd_builder`（Windows 带 `CREATE_NO_WINDOW`，否则每次都会闪一个黑窗）；
/// stdout/stderr **并发读**，超时 kill —— 先等退出再读会在管道写满时死锁。
pub fn fetch_build_key(exe: &Path) -> Result<String, AtRestError> {
    if !exe.exists() {
        return Err(AtRestError::ClientMissing);
    }
    let probe = key_probe_path();
    std::fs::write(&probe, KEY_PROBE_JS)
        .map_err(|e| AtRestError::Spawn(format!("写探针脚本失败：{e}")))?;

    let mut child = crate::modules::process::cmd_builder(exe)
        .arg(&probe)
        .env("ELECTRON_RUN_AS_NODE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AtRestError::Spawn(e.to_string()))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| AtRestError::Spawn("无法读取子进程 stdout".into()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| AtRestError::Spawn("无法读取子进程 stderr".into()))?;
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AtRestError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(AtRestError::Spawn(e.to_string())),
        }
    }
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();

    let text = String::from_utf8_lossy(&out);
    let line = text
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
        .ok_or_else(|| {
            AtRestError::Probe(format!(
                "探针无 JSON 输出；stderr={}",
                String::from_utf8_lossy(&err).trim()
            ))
        })?;
    let parsed: Value = serde_json::from_str(line.trim())
        .map_err(|e| AtRestError::Probe(format!("探针输出不是 JSON：{e}")))?;
    if parsed.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(AtRestError::Probe(
            parsed
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
        ));
    }
    parsed
        .get("atRestSecretKey")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AtRestError::Probe("探针未返回 atRestSecretKey".into()))
}

/// 进程级取钥缓存（按 region 分槽）。
///
/// # 为什么必须缓存（不是优化，是可用性问题）
///
/// 网关的 `AccountStrategy::Current` 会在**每个转发请求**上重新「导入本机账号」
/// ⇒ 不缓存的话每个请求都要「探测 exe + 起一次子进程」，**0.5~1 秒/请求**，
/// 直接把上游超时拖垮。
///
/// `atRestSecretKey` 是客户端二进制的**编译期 payload**，进程生命周期内不会变
/// （客户端升级后需重启本应用才生效）⇒ 按 region 记忆是安全的。
static KEY_CACHE: OnceLock<Mutex<[Option<Result<String, AtRestError>>; 2]>> = OnceLock::new();

fn key_cache() -> &'static Mutex<[Option<Result<String, AtRestError>>; 2]> {
    KEY_CACHE.get_or_init(|| Mutex::new([None, None]))
}

fn cache_slot(region: Region) -> usize {
    match region {
        Region::Cn => 0,
        Region::Global => 1,
    }
}

/// 取钥（带进程级缓存）。**解锁链路一律走这个**，别直接调 [`fetch_build_key`]。
///
/// 只缓存两类**确定性**结果：成功拿到钥匙、以及「本机没装客户端」。
/// 超时 / spawn 失败 / 探针报错都可能是**一次性抖动**，缓存它们会让一次偶发失败
/// 把功能永久锁死到下次重启 —— 这类不缓存，代价是偶尔多起一次子进程。
pub fn cached_build_key_for(region: Region) -> Result<String, AtRestError> {
    let slot = cache_slot(region);
    if let Some(entry) = key_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(slot)
        .cloned()
        .flatten()
    {
        match &entry {
            Ok(key) => return Ok(key.clone()),
            Err(AtRestError::ClientMissing) => return Err(AtRestError::ClientMissing),
            Err(_) => {}
        }
    }
    let result = fetch_build_key(&client_executable_for(region));
    if matches!(&result, Ok(_) | Err(AtRestError::ClientMissing)) {
        key_cache().lock().unwrap_or_else(|e| e.into_inner())[slot] = Some(result.clone());
    }
    result
}

/// 该账号有没有**值得解锁**的信封字段。
pub fn is_unlockable(account: &Value) -> bool {
    ["access_token", "refresh_token", "nickname"]
        .iter()
        .any(|key| is_envelope(account, key))
}

/// 账号级解锁：把 `access_token` / `refresh_token` / `nickname` 的信封换成明文，返回新记录。
///
/// # ⚠️ 本函数**不落盘**，由调用方决定
///
/// 网关的 `AccountStrategy::Current` 会在**每个转发请求**上调用导入链路，而它导入出来的
/// 记录带的是**每次新生成的 `id`**。如果在这里 `upsert_account_for`：
/// 按 id 找不到 ⇒ 追加一条 ⇒ **每个请求往账号库塞一个新账号**（还会反复写盘）。
/// 所以「持久化」必须留在调用方手里 —— 导入侧由 `account::import_local_for` 保存，
/// 刷新侧由 `refresh_account_token_for` 的既有落盘点保存。
///
/// # 为什么可以覆盖信封（安全性说明）
///
/// 客户端的字段解码器是 `decodeString(v){ if (typeof v === "string") return v; … }`
/// ⇒ 明文照收，且下次写回时它会自己重新封成信封。解锁后的记录与「OAuth 扫码添加」
/// 得到的记录形态一致 ⇒ 切换写回不会毁掉客户端登录态。
///
/// 返回 `Ok(None)` 表示「没有需要解锁的字段」（不启子进程）；`Ok(Some(acc))` 是解锁后的账号。
pub fn unlock_account_for(
    region: Region,
    account: &Value,
) -> Result<Option<Value>, AtRestError> {
    // ① 先把要解的信封**全部收集并解析**（纯计算、无副作用），再考虑起子进程。
    //    这样「信封本身是坏的」会在**起子进程之前**就响亮失败，而不是白花一次
    //    0.5 秒 + 一次外部进程调用。
    let mut pending: Vec<(&str, String, String)> = Vec::new();
    for key in ["access_token", "refresh_token", "nickname"] {
        if !is_envelope(account, key) {
            continue;
        }
        let envelope_b64 = account
            .get(key)
            .and_then(|v| v.get("envelope"))
            .and_then(Value::as_str)
            .ok_or_else(|| AtRestError::Envelope(format!("{key} 的信封字段缺失")))?;
        // 解析只在这里做一次；`keyId` 提前取出，好在解密前就比对上。
        let key_id = parse_envelope(envelope_b64)?.key_id;
        pending.push((key, key_id, envelope_b64.to_string()));
    }
    if pending.is_empty() {
        return Ok(None);
    }

    // ② 取钥（走进程级缓存，见 `cached_build_key_for`），再逐个解密。
    let protector_key = derive_protector_key(&cached_build_key_for(region)?);
    let derived = key_id_for(&protector_key);
    let mut unlocked = account.clone();
    for (key, declared, envelope_b64) in pending {
        if derived != declared {
            return Err(AtRestError::KeyIdMismatch { declared, derived });
        }
        unlocked[key] = json!(decrypt_field(&envelope_b64, &protector_key)?);
    }
    Ok(Some(unlocked))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AAD 必须**正好 54 字节**，且逐段布局与客户端一致。
    ///
    /// 可证伪：`encode_length_prefixed` 改成 u8 前缀 ⇒ 长度 44；`OPTIONAL_UINT64_ABSENT`
    /// 改成空 ⇒ 53；改成 8 个零字节 ⇒ 61。三种都会让真机用例 `INTEGRITY_ERROR`。
    #[test]
    fn field_aad_layout_is_exactly_54_bytes() {
        let key_id = "9127dea1b44020a7";
        let aad = build_field_aad(key_id, 1);
        assert_eq!(aad.len(), FIELD_AAD_LEN, "AAD 长度必须是 54");

        // 逐段核对（用切片而不是「大概包含」，位置错了也要红）。
        assert_eq!(&aad[0..7], b"WB-AAD\0", "域前缀");
        assert_eq!(aad[7], 0x01, "版本字节");
        assert_eq!(&aad[8..12], &5u32.to_be_bytes(), "WBEV1 的 u32BE 长度前缀");
        assert_eq!(&aad[12..17], b"WBEV1", "STANDARD_FORMAT_ID.field");
        assert_eq!(&aad[17..21], &6u32.to_be_bytes(), "sym-v1 的 u32BE 长度前缀");
        assert_eq!(&aad[21..27], b"sym-v1", "scheme");
        assert_eq!(&aad[27..31], &1u32.to_be_bytes(), "suite（u32BE）");
        assert_eq!(&aad[31..35], &16u32.to_be_bytes(), "keyId 的 u32BE 长度前缀");
        assert_eq!(&aad[35..51], key_id.as_bytes(), "keyId");
        assert_eq!(aad[51], FRAMING_CODE_FIELD, "FRAMING_CODE.field = 2");
        assert_eq!(aad[52], 0x00, "encodeOptionalUint64(undefined) = 单字节 0");
        assert_eq!(aad[53], 0x00, "final === undefined = 0");
    }

    /// 密钥派生与 keyId 的黄金向量（固定输入 ⇒ 固定输出，防止「顺手改成 base64 先解码」）。
    #[test]
    fn protector_key_derivation_matches_client() {
        let key = derive_protector_key("test-secret-key");
        assert_eq!(
            key.as_slice(),
            hex_to_bytes("2ceac6f36363c6246a64cca805cd43ca7a01b14eb2fcc532ceec3f60f2f7df1c").as_slice(),
            "protectorKey = sha256(utf8(atRestSecretKey))"
        );
        assert_eq!(key_id_for(&key), "ff09780817b2c04a", "keyId = sha256(key).hex[..16]");
    }

    /// 自造信封跑一遍 seal → decrypt：钉住「AAD 用法 + AES-GCM 装配」是对的。
    ///
    /// ⚠️ 这只证明**自洽**，不证明与客户端一致 —— 与真客户端的一致性由
    /// `unlock_real_local_account`（`#[ignore]`，需本机装客户端）负责。
    #[test]
    fn sealed_envelope_round_trips() {
        let protector_key = derive_protector_key("round-trip-secret");
        let key_id = key_id_for(&protector_key);
        let nonce = [7u8; 12];
        let plaintext = "eyJhbGciOiJIUzI1NiJ9.payload.sig";

        // 与客户端同款姿势：`encrypt_in_place_detached` 拿到独立的 tag（`getAuthTag`）。
        let cipher = Aes256Gcm::new_from_slice(&protector_key).expect("32B key");
        let mut ciphertext = plaintext.as_bytes().to_vec();
        let auth_tag = cipher
            .encrypt_in_place_detached(
                Nonce::from_slice(&nonce),
                &build_field_aad(&key_id, 1),
                &mut ciphertext,
            )
            .expect("seal");

        let envelope_b64 = B64.encode(
            serde_json::to_vec(&json!({
                "suite": 1,
                "keyId": key_id,
                "nonce": B64.encode(nonce),
                "authTag": B64.encode(auth_tag.as_slice()),
                "ciphertext": B64.encode(&ciphertext),
            }))
            .expect("json"),
        );
        assert_eq!(
            decrypt_field(&envelope_b64, &protector_key).expect("decrypt"),
            plaintext
        );

        // 阳性对照：换一把钥匙 ⇒ 必须在 keyId 这一档就响亮失败，不是静默解出垃圾。
        let other = derive_protector_key("another-secret");
        assert!(matches!(
            decrypt_field(&envelope_b64, &other),
            Err(AtRestError::KeyIdMismatch { .. })
        ));
    }

    /// 畸形信封必须**分档报错**，不许 panic、也不许当成「解出来是空」。
    #[test]
    fn malformed_envelopes_are_rejected_loudly() {
        assert!(matches!(
            parse_envelope("not-base64!!"),
            Err(AtRestError::Envelope(_))
        ));
        assert!(matches!(
            parse_envelope(&B64.encode(b"not json")),
            Err(AtRestError::Envelope(_))
        ));

        let ok = |extra: Value| {
            let mut base = json!({
                "suite": 1,
                "keyId": "9127dea1b44020a7",
                "nonce": B64.encode([0u8; 12]),
                "authTag": B64.encode([0u8; 16]),
                "ciphertext": B64.encode([0u8; 4]),
            });
            for (k, v) in extra.as_object().cloned().unwrap_or_default() {
                if v.is_null() {
                    base.as_object_mut().expect("obj").remove(&k);
                } else {
                    base[k] = v;
                }
            }
            B64.encode(serde_json::to_vec(&base).expect("json"))
        };

        // 缺字段 / 套件不支持 / nonce 长度不对 —— 三种都必须拦住。
        assert!(matches!(
            parse_envelope(&ok(json!({"keyId": null}))),
            Err(AtRestError::Envelope(_))
        ));
        assert!(matches!(
            parse_envelope(&ok(json!({"suite": 9}))),
            Err(AtRestError::Envelope(_))
        ));
        assert!(matches!(
            parse_envelope(&ok(json!({"nonce": B64.encode([0u8; 8])}))),
            Err(AtRestError::Envelope(_))
        ));
        assert!(matches!(
            parse_envelope(&ok(json!({"authTag": B64.encode([0u8; 8])}))),
            Err(AtRestError::Envelope(_))
        ));
        assert!(matches!(
            parse_envelope(&ok(json!({"keyId": "XYZ"}))),
            Err(AtRestError::Envelope(_))
        ));

        // 阳性对照：这份合法信封必须解析成功（否则上面的红是「全都解不开」的假红）。
        assert!(parse_envelope(&ok(json!({}))).is_ok());
    }

    /// 只有信封字段才算「值得解锁」；纯明文账号不该去起子进程。
    #[test]
    fn is_unlockable_only_for_envelope_fields() {
        assert!(is_unlockable(&json!({
            "access_token": {"$wbEncrypted": 1, "envelope": "x"}
        })));
        assert!(is_unlockable(&json!({"nickname": {"$wbEncrypted": 1}})));
        assert!(!is_unlockable(&json!({
            "access_token": "PLAIN", "refresh_token": "PLAIN", "nickname": "Andev"
        })));
        assert!(!is_unlockable(&json!({"id": "a1"})));
    }

    /// ★ 真机用例：**默认忽略**（依赖本机装有客户端 + 账号库里有信封账号）。
    ///
    /// ```bash
    /// cargo test -p buddy-switch-core --lib -- --ignored unlock_real_local_account
    /// ```
    ///
    /// 这是唯一能证明「AAD / 派生 / 套件与客户端**逐字节一致**」的用例：
    /// 自造信封只能证明自洽，证明不了兼容。
    ///
    /// ⚠️ 全程在**隔离 home** 里跑（真实账号库只读一次、原样拷进临时 home）——
    /// `unlock_account_for` 会落盘，绝不能让一次测试改写用户真实账号库。
    #[tokio::test]
    #[ignore = "需本机安装 WorkBuddy 客户端"]
    async fn unlock_real_local_account() {
        // ① 隔离之前：拿到真实账号与真实 exe 路径。
        let accounts = crate::modules::account::load_accounts_for(Region::Cn);
        let Some(account) = accounts.iter().find(|a| is_unlockable(a)).cloned() else {
            panic!("账号库里没有信封账号，无法验证");
        };
        let exe = client_executable_for(Region::Cn);
        assert!(exe.exists(), "客户端不存在：{}", exe.display());

        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-at-rest-real-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");
        let guard = crate::modules::config::HomeOverrideGuard::set(&dir);
        // 临时 home 里没有 exe 缓存 ⇒ 显式种进去，否则要依赖注册表/盘符扫描。
        crate::modules::config::save_workbuddy_exe_cache_for(Region::Cn, &exe).expect("cache exe");

        // ② 走**完整**的账号级解锁（含取钥子进程、解密、落盘）。
        let unlocked = unlock_account_for(Region::Cn, &account)
            .expect("真机解锁应成功")
            .expect("信封账号应被判为需要解锁");
        for field in ["access_token", "refresh_token"] {
            let value = unlocked[field].as_str().unwrap_or_default();
            assert!(
                value.starts_with("eyJ") && value.len() > 200,
                "{field} 应解成 JWT，实为：{}",
                &value[..value.len().min(16)]
            );
        }
        assert!(
            unlocked["nickname"].as_str().is_some_and(|n| !n.is_empty()),
            "nickname 应解成非空明文：{}",
            unlocked["nickname"]
        );

        // ③ 进程级缓存必须回同一把钥匙（网关每个请求都走这条路，取不到就是全站 503）。
        //    两次调用分别覆盖「冷缓存」与「命中缓存」两条分支。
        let first = cached_build_key_for(Region::Cn).expect("冷缓存应能取到钥匙");
        let second = cached_build_key_for(Region::Cn).expect("命中缓存应能取到钥匙");
        assert_eq!(first, second, "缓存前后必须一致");
        assert_eq!(first, fetch_build_key(&exe).expect("直取"), "缓存值与直取必须一致");

        // ④ ★ 本函数**不落盘**：隔离 home 的账号库必须仍是空的。
        //    这条是「网关每请求导入」那条路径的护栏 —— 一旦有人在 `unlock_account_for`
        //    里加回 `upsert_account_for`，网关就会**每个请求追加一个新账号**。
        assert!(
            crate::modules::account::load_accounts_for(Region::Cn).is_empty(),
            "unlock_account_for 不得写账号库（持久化是调用方的事）"
        );

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hex"))
            .collect()
    }
}
