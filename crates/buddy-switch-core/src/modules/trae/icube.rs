//! Trae 客户端 icube 设备凭证（`tc` 信封解密 + DeviceProof 签名）。
//!
//! ## 这个模块为什么存在
//!
//! Trae 客户端把 OAuth 设备的 EC P-256 密钥对写进
//! `%APPDATA%\<产品名>\User\globalStorage\storage.json` 的
//! `iCubeAuthInfo://icube-dc:<deviceId>` 键。本项目旧笔记断言「客户端改用加密存储，
//! 该键无法离线解密」—— **该结论已被实测推翻**（2026-09-18）：
//!
//! - 该键值是客户端自研 `tc` 信封（magic `74 63 05 10 00 00`）；
//! - 其 pepper 是**随安装包分发的 4 张 64B 公开常量表**（混淆，不是加密）；
//! - 纯标准库即可解开，解出的 JSON **同时含 `privateKeyPEM` 与 `publicKeyPEM`**；
//! - 本机 `TRAE SOLO CN` / `TRAE SOLO` / `Trae CN` 三条产品线实测均可解。
//!
//! ## 两条交换路径的私钥分工（★ 红线级，改代码前必读）
//!
//! 参考实现 `icube_auth.rs` 只有一个 `DeviceCredential`（含私钥），本项目**有意拆成两个类型**：
//!
//! | 路径 | 发 DeviceProof | 持有私钥 | 类型 |
//! |:---|:---|:---|:---|
//! | **AuthCode 交换**（OAuth 登录主路径） | **否** | **否** | [`DeviceIdentity`]（**类型里没有私钥字段**） |
//! | **refreshToken 刷新**（自动续期） | **是** | 是（仅内存） | [`DeviceCredential`]（🔴 `private_key_pem`） |
//!
//! 依据：参考 `commands/oauth.rs:531` 明说「AuthCode 场景**不发 DeviceProof**（那是
//! refreshToken 刷新场景专属），设备身份通过 DeviceInfo（DeviceID+DevicePublicKey）声明」；
//! 全仓 `device_proof` 只有 2 个调用点，其中 AuthCode 那处（`:593`）是**兜底探测变体**，
//! 注释自述「保留以验证逆向结论」，本项目**不移植**（见架构 §10 #8）。
//!
//! ⇒ 拆分让「AuthCode 路径误用私钥」变成**编译错误**而不是代码评审项。
//! `p256` 依赖因此**只为 refresh 路径服务**，不得让 AuthCode 路径接触到私钥。
//!
//! ## 脱敏红线（两条路径都适用）
//!
//! 私钥**允许在内存中持有**（refresh 的签名原文必须用它），但**绝不外流**：
//!
//! - ❌ 不落盘、不进日志、不进错误文案、不进 Tauri 事件 / HTTP 响应 / 前端展示
//! - ❌ 不进 `Debug` 输出 —— [`DeviceCredential`] / [`CloudideAuthInfo`] **手写 `impl Debug`**
//! - ❌ 不进 [`IcubeError`] 的任何 `String` 载荷（只允许类型化的 [`IcubeError::kind`]）
//! - ✅ 允许：局部变量、[`device_proof`] 的入参、`SigningKey` 的临时值
//!
//! 设备**公钥**不属于机密（`DeviceInfo` 本来就发它），可以进日志与诊断视图。
//!
//! ## 与参考实现的差异（有意，不是遗漏）
//!
//! 1. **按变体限定数据目录**：参考 `icube_auth.rs:122` 的 `for app in [...]`
//!    是「跨变体取第一个命中」—— 那会让 `Trae Work` 读到 `Trae CN` 的凭证。
//!    本模块一律走 `platform::select_data_dir_for(variant)`，天然限定在该变体内。
//! 2. **公钥直取，不推导**：参考 `oauth.rs:546` 用私钥推导 SPKI 公钥；本模块**直接取信封里的
//!    `publicKeyPEM`**（实测 178B 标准 SPKI，DER 头 `3059301306072a8648ce3d020106082a`）。
//!    缺失时返回 [`IcubeError::PublicKeyMissing`] 而**不**回退推导 —— 否则「信封形态变了」
//!    会被掩盖成「推导成功」，问题在真机登录时才暴露。
//! 3. **不缓存**：参考用 `OnceLock` 进程内缓存。本场景需要「用户刚在客户端登录完、
//!    立刻点 OAuth」能读到新值，故每次调用重读（单文件 JSON 读，成本可忽略）。

use base64::Engine as _;
use serde_json::{json, Map, Value};
use sha2::Digest as _;

use crate::modules::trae::platform;
use crate::modules::trae::store;
use crate::modules::trae::variant::TraeVariant;

/// `tc` 信封 magic：hex `746305100000`。
///
/// 旧笔记曾把这个头误判成「Windows DPAPI，解不开」——结论已推翻（本机实测可解）。
pub const TC_HEADER: [u8; 6] = [116, 99, 5, 16, 0, 0];

/// 信封内嵌的随机盐长度。
const RANDOM_LEN: usize = 32;
/// magic 长度。
const HEADER_LEN: usize = 6;
/// 明文头部摘要长度（`SHA512(body)`）。
const SHA512_LEN: usize = 64;

/// byteCrypto 四张 64B 公开常量表（**随安装包分发，混淆非加密**）。
///
/// 出处：Trae CN `resources/app/out/main.js`，2026-09-16 实测提取；
/// `WOE_T ^ VOE_T` = AES 模式 pepper，`JOE_T ^ HOE_T` = AES_PRIVATE 模式 pepper。
/// 上游版本更新导致表变化时重新提取即可。
const WOE_T: [u8; 64] = [
    82, 9, 106, 213, 48, 54, 165, 56, 191, 64, 163, 158, 129, 243, 215, 251, 124, 227, 57, 130,
    155, 47, 255, 135, 52, 142, 67, 68, 196, 222, 233, 203, 84, 123, 148, 50, 166, 194, 35, 61,
    238, 76, 149, 11, 66, 250, 195, 78, 8, 46, 161, 102, 40, 217, 36, 178, 118, 91, 162, 73, 109,
    139, 209, 37,
];
const VOE_T: [u8; 64] = [
    31, 221, 168, 51, 136, 7, 199, 49, 177, 18, 16, 89, 39, 128, 236, 95, 96, 81, 127, 169, 25,
    181, 74, 13, 45, 229, 122, 159, 147, 201, 156, 239, 160, 224, 59, 77, 174, 42, 245, 176, 200,
    235, 187, 60, 131, 83, 153, 97, 23, 43, 4, 126, 186, 119, 214, 38, 225, 105, 20, 99, 85, 33,
    12, 125,
];
const JOE_T: [u8; 64] = [
    191, 192, 216, 250, 122, 246, 220, 97, 31, 254, 98, 27, 8, 72, 71, 176, 135, 99, 96, 18, 127,
    101, 203, 104, 211, 102, 191, 125, 37, 72, 150, 156, 51, 229, 121, 35, 17, 153, 141, 177, 110,
    131, 150, 128, 172, 255, 254, 6, 18, 140, 55, 62, 236, 249, 135, 64, 135, 12, 117, 4, 89, 149,
    168, 209,
];
const HOE_T: [u8; 64] = [
    246, 204, 26, 232, 232, 70, 129, 109, 223, 146, 169, 242, 23, 241, 105, 145, 50, 196, 165, 42,
    254, 120, 3, 54, 244, 207, 209, 85, 53, 6, 138, 106, 175, 148, 31, 204, 186, 186, 165, 182,
    87, 142, 49, 10, 39, 110, 26, 154, 86, 56, 173, 125, 18, 64, 198, 225, 99, 99, 83, 82, 191,
    134, 76, 170,
];

/// storage.json 里设备凭证键的前缀（键名内嵌 `<deviceId>`）。
///
/// **设备身份 ≠ 登录态**：这个键在客户端首次启动时就会写入，用户从未登录也照样存在。
/// 诊断文案若拿它当「登录发生过」的证据，就会把「没登录」说成「登录了」
/// （`profile::diagnose_missing_credential` 因此把它与登录态分开陈述）。
pub(crate) const ICUBE_DC_PREFIX: &str = "iCubeAuthInfo://icube-dc:";
/// storage.json 里 cloudide 登录态副本的键（**与设备凭证共用同一条解密实现**）。
///
/// `pub(crate)` 而非私有：`profile.rs` 的导入诊断要**只看键是否存在**（不解密）
/// 就能说清「凭据在不在」，共用同一个常量避免两处字面量漂移。
pub(crate) const CLOUDIDE_KEY: &str = "iCubeAuthInfo://icube.cloudide";
/// 同文件的机器标识（`DeviceInfo.MachineID` 取它）。
const TELEMETRY_MACHINE_ID: &str = "telemetry.machineId";

/// 信封模式。
///
/// **两个键（`icube-dc` / `icube.cloudide`）共用同一条 [`tc_decrypt`]**，
/// 仅 pepper 来源不同（本机实测两键均为 [`TcMode::Aes`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcMode {
    /// AES 模式：pepper = `WOE_T ^ VOE_T`。
    Aes,
    /// AES_PRIVATE 模式：pepper = `JOE_T ^ HOE_T`（本机数据未见使用，保留支持）。
    AesPrivate,
}

impl TcMode {
    /// 该模式对应的 pepper。
    fn pepper(self) -> [u8; 64] {
        let (base, other) = match self {
            TcMode::Aes => (&WOE_T, &VOE_T),
            TcMode::AesPrivate => (&JOE_T, &HOE_T),
        };
        let mut pepper = [0u8; 64];
        for i in 0..64 {
            pepper[i] = base[i] ^ other[i];
        }
        pepper
    }
}

/// icube 读取失败的结构化原因。
///
/// **`String` 载荷里绝不出现私钥 / 令牌 / 签名**（见模块头的脱敏红线）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IcubeError {
    /// 该变体没有存在的 userData 目录（客户端没装或从未启动过）。
    DataDirMissing,
    /// `storage.json` 读不出或不是合法 JSON。
    StorageUnreadable(String),
    /// `storage.json` 里没有目标键。
    KeyMissing,
    /// magic / 长度 / AES / SHA512 完整性校验失败。
    DecryptFailed(String),
    /// 信封缺 `publicKeyPEM`（**不推导、不静默**）。
    PublicKeyMissing,
    /// refresh 需要私钥但信封里没有。
    PrivateKeyMissing,
    /// PEM 解析失败（仅 refresh 路径可能遇到）。
    PrivateKeyInvalid(String),
    /// 该变体的候选目录里**没有任何** `icube-dc` 条目（设备身份未注册）。
    DeviceIdentityMissing,
    /// 候选目录里**有** `icube-dc` 条目，但没有一条绑定请求的 `deviceId`。
    ///
    /// 载荷是**已脱敏**的对比描述（`store::mask` 口径），**不得**塞入 deviceId 原文 ——
    /// 与其余变体同守模块头的脱敏红线。
    DeviceIdentityMismatch(String),
}

impl IcubeError {
    /// 面向诊断的结构化标签（写进日志 / 诊断视图的 `errorKind`）。
    pub fn kind(&self) -> &'static str {
        match self {
            IcubeError::DataDirMissing => "dataDirMissing",
            IcubeError::StorageUnreadable(_) => "storageUnreadable",
            IcubeError::KeyMissing => "keyMissing",
            IcubeError::DecryptFailed(_) => "decryptFailed",
            IcubeError::PublicKeyMissing => "publicKeyMissing",
            IcubeError::PrivateKeyMissing => "privateKeyMissing",
            IcubeError::PrivateKeyInvalid(_) => "privateKeyInvalid",
            IcubeError::DeviceIdentityMissing => "deviceIdentityMissing",
            IcubeError::DeviceIdentityMismatch(_) => "deviceIdentityMismatch",
        }
    }

    /// 面向用户的文案（含**下一步动作**，不含任何凭据片段）。
    ///
    /// 风格遵循架构 §9.2：说清哪条产品线、说清下一步、不出现裸错误码、
    /// 不用「登录失败」这类笼统措辞。
    pub fn user_message(&self, variant: TraeVariant) -> String {
        let line = variant.display_name();
        match self {
            IcubeError::DataDirMissing => {
                // ★ 必须区分「**没装**客户端」与「装了但**从没启动过**」——这两件事对用户是
                // **完全不同**的下一步，而原文案只说「未找到…的数据目录」，
                // 用户会读成「你没装客户端」。
                //
                // 现场（2026-09-24 用户报障原话）：「**已经安装了，为什么检查不到安装的客户端**」——
                // 客户端确实装着（`D:\Programs\TRAE SOLO CN\`），只是从未启动过，
                // 而设备凭证是**首次启动**才写的（见模块头「设备身份 ≠ 登录态」）。
                //
                // 文案与判据全在 [`platform::data_dir_missing_reason`] 一处
                // （它与「启动客户端」按钮同源），这里**不要**再自己拼一遍。
                platform::data_dir_missing_reason(variant)
            }
            IcubeError::StorageUnreadable(detail) => format!(
                "【{line}】的 storage.json 无法读取（{detail}）；\
                 若客户端正在运行，请关闭后重试。"
            ),
            IcubeError::KeyMissing => format!(
                "【{line}】的 storage.json 中没有设备凭证（iCubeAuthInfo）；\
                 请先启动一次该客户端并完成登录，再重试。"
            ),
            IcubeError::DecryptFailed(detail) => format!(
                "【{line}】的设备凭证解密失败（{detail}）；\
                 客户端刚升级过时请重新启动一次以刷新凭证。"
            ),
            IcubeError::PublicKeyMissing => format!(
                "【{line}】的设备凭证缺少公钥（信封形态可能已变化）；\
                 请先启动一次该客户端并完成登录，再重试。"
            ),
            IcubeError::PrivateKeyMissing => format!(
                "【{line}】的设备凭证缺少私钥，自动续期不可用；\
                 请先启动一次该客户端并完成登录，再重试。"
            ),
            IcubeError::PrivateKeyInvalid(detail) => format!(
                "【{line}】的设备私钥格式无效（{detail}），自动续期不可用；\
                 请重新启动一次该客户端以刷新凭证。"
            ),
            IcubeError::DeviceIdentityMissing => format!(
                "【{line}】的 storage.json 中没有设备身份（iCubeAuthInfo://icube-dc）；\
                 请先在客户端里完成一次登录，或改用「OAuth 网页登录」重新授权。"
            ),
            IcubeError::DeviceIdentityMismatch(detail) => format!(
                "【{line}】的设备身份与账号绑定的不一致（{detail}）；\
                 该账号绑定的设备已变更，请重新导入登录态，或改用「OAuth 网页登录」重新授权。"
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// tc 信封解密
// ---------------------------------------------------------------------------

fn sha512(buf: &[u8]) -> [u8; SHA512_LEN] {
    let mut hasher = sha2::Sha512::new();
    hasher.update(buf);
    hasher.finalize().into()
}

/// 密钥派生（byteCrypto `deriveKeys`）：`SHA512(random) ‖ pepper` → `SHA512` → 前 32B，
/// 切分为 `aesKey[0..16]` / `iv[16..32]`。
fn derive_keys(random: &[u8], pepper: &[u8; 64]) -> ([u8; 16], [u8; 16]) {
    let first = sha512(random);
    let mut joined = [0u8; SHA512_LEN + 64];
    joined[..SHA512_LEN].copy_from_slice(&first);
    joined[SHA512_LEN..].copy_from_slice(pepper);
    let digest = sha512(&joined);
    let mut key = [0u8; 16];
    let mut iv = [0u8; 16];
    key.copy_from_slice(&digest[..16]);
    iv.copy_from_slice(&digest[16..32]);
    (key, iv)
}

/// ★ **全仓唯一**一条 `tc` 信封解密实现。
///
/// `icube-dc`（设备凭证）与 `icube.cloudide`（登录态副本）**都调它**，
/// 只是入参 b64 与 [`TcMode`] 不同。后人若复制成第二份，「两个键的形态悄然分叉」
/// 会变成只有真机才暴露的缺陷 —— 见 [`decrypt_storage_key`] 与对应单测。
///
/// 失败**绝不 panic**，一律返回结构化 [`IcubeError`]。
pub fn tc_decrypt(b64: &str, mode: TcMode) -> Result<String, IcubeError> {
    use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

    #[cfg(test)]
    TC_DECRYPT_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| IcubeError::DecryptFailed(format!("信封 base64 解码失败: {e}")))?;
    if raw.len() < HEADER_LEN + RANDOM_LEN + 16 {
        return Err(IcubeError::DecryptFailed(format!(
            "信封长度异常: {}",
            raw.len()
        )));
    }
    if raw[..HEADER_LEN] != TC_HEADER {
        return Err(IcubeError::DecryptFailed(format!(
            "信封 magic 不匹配: {:02x?}（期望 746305100000）",
            &raw[..HEADER_LEN]
        )));
    }
    let random = &raw[HEADER_LEN..HEADER_LEN + RANDOM_LEN];
    let cipher = &raw[HEADER_LEN + RANDOM_LEN..];
    let (key, iv) = derive_keys(random, &mode.pepper());
    let padded = Aes128CbcDec::new((&key).into(), (&iv).into())
        .decrypt_padded_vec_mut::<Pkcs7>(cipher)
        .map_err(|e| IcubeError::DecryptFailed(format!("AES-128-CBC 解密失败: {e}")))?;
    if padded.len() <= SHA512_LEN {
        return Err(IcubeError::DecryptFailed("解密后明文过短".into()));
    }
    let (tag, body) = padded.split_at(SHA512_LEN);
    if sha512(body)[..] != tag[..] {
        return Err(IcubeError::DecryptFailed(
            "信封完整性校验失败（SHA512 不匹配）".into(),
        ));
    }
    String::from_utf8(body.to_vec())
        .map_err(|e| IcubeError::DecryptFailed(format!("明文非 UTF-8: {e}")))
}

/// `#[cfg(test)]` 调用计数：证明两个键走的是**同一条** [`tc_decrypt`]。
#[cfg(test)]
static TC_DECRYPT_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 从 storage.json 对象里取某个键并解密。
///
/// **两个键共用这一条**（因此也共用 [`tc_decrypt`]）。抽出来是为了让
/// 「两个键共用同一条解密实现」成为可执行断言而不是口头约定。
fn decrypt_storage_key(
    object: &Map<String, Value>,
    key: &str,
    mode: TcMode,
) -> Result<String, IcubeError> {
    let b64 = object
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or(IcubeError::KeyMissing)?;
    tc_decrypt(b64, mode)
}

// ---------------------------------------------------------------------------
// 数据结构
// ---------------------------------------------------------------------------

/// 设备**身份**（AuthCode 交换路径与全部诊断用）。
///
/// **永不包含私钥**：解析阶段就把 `privateKeyPEM` 丢弃，
/// 使「AuthCode 路径泄漏私钥」在类型层面不可能发生。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdentity {
    /// 所属产品线变体。
    pub variant: TraeVariant,
    /// `icube-dc` 键名内嵌的 `<deviceId>`。
    pub device_id: String,
    /// 同文件 `telemetry.machineId`（取不到为空串）。
    pub machine_id: String,
    /// 安装目录 `resources/app/package.json` 的 `version`（取不到为空串）。
    pub app_version: String,
    /// 🔓 信封里的 `publicKeyPEM`（SPKI）**直取，不推导**。
    pub public_key_pem: String,
    /// 来源 userData 目录名（诊断用，如 `TRAE SOLO CN`）。
    pub source_app: String,
}

/// 设备**凭证**（**仅 refreshToken 刷新路径**用：DeviceProof 需要私钥签名）。
///
/// `private_key_pem` 🔴 **允许在内存中持有**（refresh 的签名原文必须用它），
/// 但**绝不落盘 / 不进日志 / 不进错误文案 / 不进事件 / 不进任何序列化输出**。
///
/// 本类型**手写 `impl Debug`**（打印 `<redacted>`）：`#[derive(Debug)]` 会让
/// 一次 `dbg!` / `{:?}` 就把私钥打进日志 —— 见模块头的脱敏红线。
#[derive(Clone)]
pub struct DeviceCredential {
    /// 所属产品线变体。
    pub variant: TraeVariant,
    /// `icube-dc` 键名内嵌的 `<deviceId>`。
    pub device_id: String,
    /// 🔴 EC P-256 PKCS#8 私钥 PEM。
    pub private_key_pem: String,
    /// 来源 userData 目录名（诊断用）。
    pub source_app: String,
}

impl std::fmt::Debug for DeviceCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceCredential")
            .field("variant", &self.variant)
            .field("device_id", &self.device_id)
            .field("private_key_pem", &"<redacted>")
            .field("source_app", &self.source_app)
            .finish()
    }
}

/// `icube.cloudide` 键解出的用户级凭据。
///
/// **本轮仅用于诊断 / 导入兜底，不承担登录职责**（架构 N-3）：
/// 它是客户端自己的登录态副本，不是「从客户端文件提取 JWT 作为登录替代」那条被否掉的路径。
#[derive(Clone, PartialEq, Eq)]
pub struct CloudideAuthInfo {
    /// 所属产品线变体。
    pub variant: TraeVariant,
    /// 🔴 裸 JWT（无 `Cloud-IDE-JWT ` 前缀，实测 1004 字符三段）。
    pub token: String,
    /// 🔴 refresh token（实测 61 字符）。
    pub refresh_token: String,
    /// API 主机（实测 `https://api.trae.cn`，带 scheme、无尾斜杠）。
    pub host: String,
    /// 用户区域（可选）。
    pub user_region: Option<String>,
    /// 用户 ID（可选；实测 = JWT payload 的 `data.id`）。
    pub user_id: Option<String>,
    /// access token 到期时间（epoch 秒）。
    pub expired_at: Option<i64>,
    /// 账号标识（可选）。
    pub account: Option<String>,
}

impl std::fmt::Debug for CloudideAuthInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudideAuthInfo")
            .field("variant", &self.variant)
            .field("token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("host", &self.host)
            .field("user_region", &self.user_region)
            .field("user_id", &self.user_id)
            .field("expired_at", &self.expired_at)
            .field("account", &self.account)
            .finish()
    }
}

/// DeviceProof 签名编码（2026-09-16 实测排查）。
///
/// - [`ProofSigFormat::P1363`]：raw `r‖s` 固定 64 字节 —— WebCrypto（Electron 客户端 JS 侧）
///   标准输出，ring 参照实现同为此格式，**首选**；
/// - [`ProofSigFormat::Der`]：ASN.1 SEQUENCE（~70-72B）—— 早期逆向结论，实测报 20405 被拒，
///   **保留作 refresh 链的对照探测**（与参考 `oauth.rs:888-892` 一致）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofSigFormat {
    /// raw r‖s（64B）。
    P1363,
    /// ASN.1 DER。
    Der,
}

impl ProofSigFormat {
    /// 诊断标签后缀（`"/P1363"` / `"/DER"`）。
    pub fn suffix(self) -> &'static str {
        match self {
            ProofSigFormat::P1363 => "/P1363",
            ProofSigFormat::Der => "/DER",
        }
    }
}

// ---------------------------------------------------------------------------
// 明文 → 结构（纯函数，便于单测喂内联信封）
// ---------------------------------------------------------------------------

fn parse_plain(plain: &str) -> Result<Value, IcubeError> {
    serde_json::from_str::<Value>(plain)
        .map_err(|e| IcubeError::DecryptFailed(format!("明文不是合法 JSON: {e}")))
}

/// 取字符串字段；数字字段（如 `userId`）也接受。
fn opt_string(value: &Value, key: &str) -> Option<String> {
    let raw = value.get(key)?;
    if let Some(text) = raw.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }
    raw.as_i64().map(|number| number.to_string())
}

/// 明文 → [`DeviceIdentity`]（**解析后立即丢弃私钥，不构造、不拷贝**）。
fn identity_from_plain(
    plain: &str,
    variant: TraeVariant,
    device_id: &str,
    machine_id: &str,
    app_version: &str,
    source_app: &str,
) -> Result<DeviceIdentity, IcubeError> {
    let parsed = parse_plain(plain)?;
    // ⚠️ 这里**只读 publicKeyPEM**：privateKeyPEM 连读都不读，
    // 保证 AuthCode 路径结构上不可能持有私钥。
    let public_key_pem = parsed
        .get("publicKeyPEM")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|pem| !pem.is_empty())
        .ok_or(IcubeError::PublicKeyMissing)?
        .to_string();
    Ok(DeviceIdentity {
        variant,
        device_id: device_id.to_string(),
        machine_id: machine_id.to_string(),
        app_version: app_version.to_string(),
        public_key_pem,
        source_app: source_app.to_string(),
    })
}

/// 明文 → [`DeviceCredential`]（**只被 refresh 链路调用**）。
fn credential_from_plain(
    plain: &str,
    variant: TraeVariant,
    device_id: &str,
    source_app: &str,
) -> Result<DeviceCredential, IcubeError> {
    let parsed = parse_plain(plain)?;
    let private_key_pem = parsed
        .get("privateKeyPEM")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|pem| pem.contains("BEGIN"))
        .ok_or(IcubeError::PrivateKeyMissing)?
        .to_string();
    Ok(DeviceCredential {
        variant,
        device_id: device_id.to_string(),
        private_key_pem,
        source_app: source_app.to_string(),
    })
}

/// 明文 → [`CloudideAuthInfo`]。
fn cloudide_from_plain(plain: &str, variant: TraeVariant) -> Result<CloudideAuthInfo, IcubeError> {
    let parsed = parse_plain(plain)?;
    let token = opt_string(&parsed, "token").ok_or(IcubeError::KeyMissing)?;
    let refresh_token = opt_string(&parsed, "refreshToken").ok_or(IcubeError::KeyMissing)?;
    Ok(CloudideAuthInfo {
        variant,
        token,
        refresh_token,
        host: opt_string(&parsed, "host").unwrap_or_default(),
        user_region: opt_string(&parsed, "userRegion"),
        user_id: opt_string(&parsed, "userId"),
        expired_at: parsed.get("expiredAt").and_then(|value| value.as_i64()),
        account: opt_string(&parsed, "account"),
    })
}

// ---------------------------------------------------------------------------
// 磁盘加载（按变体限定）
// ---------------------------------------------------------------------------

/// 一次 `storage.json` 读取的快照。
struct StorageSnapshot {
    source_app: String,
    app_version: String,
    machine_id: String,
    object: Map<String, Value>,
}

/// 读取**该变体**的 storage.json（目录由 [`platform::select_data_dir_for`] 限定）。
///
/// 参考实现跨变体扫全部候选目录并取第一个命中 —— 本项目**不照抄**（见模块头差异 1）。
fn load_storage(variant: TraeVariant) -> Result<StorageSnapshot, IcubeError> {
    let dir = platform::select_data_dir_for(variant).ok_or(IcubeError::DataDirMissing)?;
    load_storage_from_dir(&dir, variant)
}

/// 读取**指定目录**下的 storage.json（显式入参）。
///
/// 与 [`load_storage`] 的唯一区别是「目录从哪来」：后者按 [`platform::select_data_dir_for`]
/// 挑最近活跃的那个。当调用方需要让**校验的对象**与**操作的对象**落在同一个目录时，
/// 必须先自己定目录、再走这个函数 —— 两个选择器给出的目录**可能不同**
/// （实测 Trae Work：`select` 给 `TRAE SOLO`、`detect` 给 `TRAE SOLO CN`，
/// 而登录态只在其一），否则会出现「校验读了 A、操作改了 B」。
fn load_storage_from_dir(
    dir: &std::path::Path,
    variant: TraeVariant,
) -> Result<StorageSnapshot, IcubeError> {
    let source_app = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let path = dir
        .join("User")
        .join("globalStorage")
        .join("storage.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| IcubeError::StorageUnreadable(format!("{}: {e}", path.display())))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| IcubeError::StorageUnreadable(format!("{}: {e}", path.display())))?;
    let object = value
        .as_object()
        .cloned()
        .ok_or_else(|| IcubeError::StorageUnreadable("storage.json 顶层不是对象".into()))?;
    let machine_id = object
        .get(TELEMETRY_MACHINE_ID)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    // `DeviceInfo.ClientVersion` 用：安装目录 package.json 的 version（真实客户端上报的
    // 就是 appVersion，与服务端对设备注册记录的校验相关）。取不到时为空串。
    let app_version = platform::detect_install_for(variant)
        .version
        .unwrap_or_default();
    Ok(StorageSnapshot {
        source_app,
        app_version,
        machine_id,
        object,
    })
}

/// 在快照里定位 `iCubeAuthInfo://icube-dc:<deviceId>` 条目。
fn find_icube_dc_entry(object: &Map<String, Value>) -> Result<(String, String), IcubeError> {
    for (key, value) in object {
        let Some(device_id) = key.strip_prefix(ICUBE_DC_PREFIX) else {
            continue;
        };
        if let Some(b64) = value.as_str() {
            return Ok((device_id.to_string(), b64.to_string()));
        }
    }
    Err(IcubeError::KeyMissing)
}

/// 按**变体**取设备身份（AuthCode 路径 / 诊断）。
///
/// 目录取 [`platform::select_data_dir_for`]（最近活跃）。需要「与某个具体操作指向
/// **同一个目录**」时用 [`device_identity_from_dir`]。
pub fn device_identity_for(variant: TraeVariant) -> Result<DeviceIdentity, IcubeError> {
    let dir = platform::select_data_dir_for(variant).ok_or(IcubeError::DataDirMissing)?;
    device_identity_from_dir(&dir, variant)
}

/// 从**指定目录**取设备身份（显式入参）。
///
/// 存在的理由与 [`cloudide_auth_info_from_dir`] 相同：同一变体的多个候选目录
/// **可能装着不同设备 / 不同账号的凭据**，而两个选择器给出的目录**可能不同**。
/// 调用方需要让「校验的对象」与「操作的对象」落在同一目录时，先自己定目录再走这里。
///
/// 目录里有**多个** `icube-dc` 条目时取第一个（[`find_icube_dc_entry`] 的既有语义）；
/// 要按 `deviceId` **精确取某一条**用 [`device_credential_by_device_id`]。
pub(crate) fn device_identity_from_dir(
    dir: &std::path::Path,
    variant: TraeVariant,
) -> Result<DeviceIdentity, IcubeError> {
    let snapshot = load_storage_from_dir(dir, variant)?;
    let (device_id, b64) = find_icube_dc_entry(&snapshot.object)?;
    let plain = tc_decrypt(&b64, TcMode::Aes)?;
    identity_from_plain(
        &plain,
        variant,
        &device_id,
        &snapshot.machine_id,
        &snapshot.app_version,
        &snapshot.source_app,
    )
}

/// 按**变体**取设备凭证（refresh 路径）。**只被 refresh 链路调用**。
pub fn device_credential_for(variant: TraeVariant) -> Result<DeviceCredential, IcubeError> {
    let snapshot = load_storage(variant)?;
    let (device_id, b64) = find_icube_dc_entry(&snapshot.object)?;
    let plain = tc_decrypt(&b64, TcMode::Aes)?;
    credential_from_plain(&plain, variant, &device_id, &snapshot.source_app)
}

/// 按**变体**取 cloudide 凭据（诊断 / 导入兜底，**不承担登录职责**）。
///
/// 目录由 [`platform::select_data_dir_for`]（最近活跃）决定。需要「与某个具体操作
/// 指向同一个目录」时，用 [`cloudide_auth_info_from_dir`]。
pub fn cloudide_auth_info_for(variant: TraeVariant) -> Result<CloudideAuthInfo, IcubeError> {
    let dir = platform::select_data_dir_for(variant).ok_or(IcubeError::DataDirMissing)?;
    cloudide_auth_info_from_dir(&dir, variant)
}

/// 从**指定目录**取 cloudide 凭据（显式入参）。
///
/// 存在的理由：同一变体的多个候选目录**可能装着不同账号的登录态**
/// （实测 Trae Work：`TRAE SOLO CN` 有登录态、`TRAE SOLO` 没有，且后者更活跃）。
/// 调用方若用「最近活跃」去**校验**、却用「首位候选」去**操作**，就会
/// 校验通过而操作作用在另一个目录上。此函数让两件事能锁定同一个目录。
pub(crate) fn cloudide_auth_info_from_dir(
    dir: &std::path::Path,
    variant: TraeVariant,
) -> Result<CloudideAuthInfo, IcubeError> {
    let snapshot = load_storage_from_dir(dir, variant)?;
    let plain = decrypt_storage_key(&snapshot.object, CLOUDIDE_KEY, TcMode::Aes)?;
    cloudide_from_plain(&plain, variant)
}

/// 按**指定 `deviceId`** 取设备凭证（refresh 路径）。
///
/// ## 为什么需要它（而不是复用 [`device_credential_for`]）
///
/// [`device_credential_for`] 取的是「该变体当前目录里的那一条」—— 客户端换过设备
/// 或换过账号后，那一条**未必**是账号绑定的那台设备的。续期必须用**账号绑定的
/// 那台设备的私钥**签名，用错私钥会被服务端拒绝（或更糟：静默失败）。
///
/// 故本函数按 `deviceId` **精确取**，并且**必须**在拿到条目后确认它确实绑定了
/// 请求的 `deviceId` —— 这是本函数存在的全部意义。
///
/// ## ★ 断言为什么不是同义反复
///
/// `deviceId` 是从 **key 名**（`iCubeAuthInfo://icube-dc:<deviceId>`）里**解析**出来的
/// （见 [`find_icube_dc_entry`] / 下面的遍历），**不是**用 `format!` 拼进 key 再查。
/// 若按拼接 key 精确查表，这条断言确实恒真、等于没有 —— 那种写法会漏掉
/// 「目录里只有别的设备的条目」这一情形，把 `DeviceIdentityMissing` 与
/// `DeviceIdentityMismatch` 混为一谈。
///
/// ## 遍历顺序与失败语义
///
/// 候选目录按**最近活跃降序**（[`platform::data_dirs_by_activity_for`]，只含存在的）；
/// 目录读不出 `storage.json` 时跳过（继续找下一个，而不是整体失败）。
///
/// - 所有候选目录里**一条 `icube-dc` 都没有** ⇒ [`IcubeError::DeviceIdentityMissing`]；
/// - 有若干条、但**没有一条**绑定请求的 `deviceId` ⇒ [`IcubeError::DeviceIdentityMismatch`]
///   （载荷列出本机有的那些 id 的**脱敏**值，便于用户判断是不是换过设备）。
pub(crate) fn device_credential_by_device_id(
    variant: TraeVariant,
    device_id: &str,
) -> Result<DeviceCredential, IcubeError> {
    let mut others: Vec<String> = Vec::new();
    for dir in platform::data_dirs_by_activity_for(variant) {
        let Ok(snapshot) = load_storage_from_dir(&dir, variant) else {
            continue;
        };
        // 遍历**全部** `icube-dc` 条目：不预设 key，`found_id` 一律从键名解析。
        for (key, value) in &snapshot.object {
            let Some(found_id) = key.strip_prefix(ICUBE_DC_PREFIX) else {
                continue;
            };
            if found_id != device_id {
                others.push(found_id.to_string());
                continue;
            }
            let Some(b64) = value.as_str() else {
                continue;
            };
            let plain = tc_decrypt(b64, TcMode::Aes)?;
            let credential =
                credential_from_plain(&plain, variant, found_id, &snapshot.source_app)?;
            // ★ 断言：解出的凭证必须确实绑定请求的 deviceId。不匹配**不得**返回 ——
            //   否则「绑定错了设备」会静默用错私钥（见函数文档）。
            if credential.device_id != device_id {
                return Err(IcubeError::DeviceIdentityMismatch(format!(
                    "请求 {}，条目绑定 {}",
                    store::mask(device_id),
                    store::mask(&credential.device_id)
                )));
            }
            return Ok(credential);
        }
    }
    if others.is_empty() {
        Err(IcubeError::DeviceIdentityMissing)
    } else {
        Err(IcubeError::DeviceIdentityMismatch(format!(
            "请求 {}，本机仅有 {}",
            store::mask(device_id),
            others
                .iter()
                .map(|id| store::mask(id))
                .collect::<Vec<_>>()
                .join(" / ")
        )))
    }
}

/// 该变体**哪个候选目录装着登录态**（凭据副本，供导入侧定位来源）。
///
/// ## 为什么需要它
///
/// 同一变体的多个候选目录**可能只有一个装着登录态**（实测 Trae Work：
/// `TRAE SOLO CN` 有登录态、`TRAE SOLO` 没有，而后者更**活跃**）。
/// 导入侧若只看「最近活跃」，在用户机器上会**必然读不到凭据** ——
/// 用户看到的症状是「明明登录了，导入却说找不到登录态」。
///
/// 判定口径：该目录能解出 [`CloudideAuthInfo`]（即 `iCubeAuthInfo://icube.cloudide`
/// 存在且可解密），与 [`crate::modules::trae::profile`] 的登录态来源**同一口径**。
///
/// 遍历顺序按**最近活跃降序**（多个目录都有登录态时取最活跃的那个）。
/// 返回 `None` = 该变体没有任何候选目录装着登录态。
///
/// ⚠️ **不判过期**：本函数只回答「凭据在哪个目录」，token 是否过期、是否可用
/// 是调用方的策略（`profile` 侧还要比对 `exp` / `userId`）。
///
/// 这**不是遗漏而是必需**（R6）：过期信封的目录**仍应被选中** —— 只有选中它，
/// 导入侧才能在那一个目录上判出「信封键存在但已过期」，从而**拒绝**回落到
/// 跨账号留存的明文日志（见 `profile::local_login_from_dir`）。
/// 若这里按过期过滤，那个目录会被跳过，导入转而选中别的候选目录并在那里扫明文，
/// 恰好绕过 R6 的收口。
///
/// ⚠️ 判定口径含「**可解密**」：解不开的信封在本函数看来「没有登录态」。
/// 于是「信封键存在但解不开」且本变体另有更活跃候选时，选中的可能**不是**
/// 那个坏信封所在的目录 —— 那时按选中目录判「无信封键」⇒ 允许回落。
/// 这是已知的、被接受的边界（R6 的收口是**按选中的那个目录**判定的）。
pub(crate) fn login_state_dir_for(variant: TraeVariant) -> Option<std::path::PathBuf> {
    platform::data_dirs_by_activity_for(variant)
        .into_iter()
        .find(|dir| cloudide_auth_info_from_dir(dir, variant).is_ok())
}

// ---------------------------------------------------------------------------
// DeviceProof（refreshToken 刷新场景专属）
// ---------------------------------------------------------------------------

/// 生成随机 hex 字符串。
///
/// 熵源：`uuid::Uuid::new_v4()`（走 OS CSPRNG），两段拼接够 64 字符。
/// **刻意不引 `rand`**：本仓库已有 `uuid`，且 `p256` 的 `rand_core` 与 `uuid` 的
/// `getrandom` 不同 trait 路径，再加一个随机源只会让依赖树更复杂。
///
/// 放在本模块是因为它是**密码学原语**（本模块的职责之一）；
/// `oauth.rs` 的 PKCE / trace_id / seed 也复用它，避免出现第二份实现。
pub(crate) fn random_hex(len: usize) -> String {
    let mut out = String::with_capacity(len + 32);
    while out.len() < len {
        out.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    out.truncate(len);
    out
}

/// 生成 DeviceProof JSON（PascalCase 字段；20405 实测小写会被拒；
/// `Timestamp` 必须为 JSON int，字符串会被服务端 schema 拒绝）。
///
/// 签名原文（5 个 `\n` 连接）：
/// `POST\n<sign_path>\n<ClientID>\n<RefreshToken>\n<Timestamp>\n<Nonce>`
///
/// ⚠️ **只有 refreshToken 刷新路径调用它**。AuthCode 交换路径不得调用
/// （那会让 AuthCode 路径持有私钥，与模块头的红线冲突）。
pub fn device_proof(
    cred: &DeviceCredential,
    sign_path: &str,
    client_id: &str,
    refresh_token: &str,
    format: ProofSigFormat,
) -> Result<Value, String> {
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::{Signature, SigningKey};
    use p256::pkcs8::DecodePrivateKey;

    // ⚠️ 错误文案**不含** `{e}`：`pkcs8` 的错误类型虽不携带输入内容，
    // 但把「解析失败」的具体细节暴露出去没有收益，而一旦上游实现变化就可能回显 PEM。
    let signing = SigningKey::from_pkcs8_pem(&cred.private_key_pem)
        .map_err(|_| "设备私钥解析失败（PKCS#8 PEM 无效或不受支持）".to_string())?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let nonce = random_hex(32);
    let message = format!("POST\n{sign_path}\n{client_id}\n{refresh_token}\n{timestamp}\n{nonce}");
    let signature: Signature = signing.sign(message.as_bytes());
    let encoded = match format {
        // r‖s 固定 64 字节（P1363 / WebCrypto / ring 兼容）
        ProofSigFormat::P1363 => {
            base64::engine::general_purpose::STANDARD.encode(signature.to_bytes())
        }
        ProofSigFormat::Der => base64::engine::general_purpose::STANDARD.encode(signature.to_der()),
    };
    Ok(json!({
        "Signature": encoded,
        "Timestamp": timestamp,
        "Nonce": nonce,
    }))
}

// ---------------------------------------------------------------------------
// 诊断视图
// ---------------------------------------------------------------------------

/// 把「取设备身份」的结果转成**按构造保证不含私钥**的诊断视图。
///
/// 白名单字段只有 `available` / `sourceApp` / `deviceId` / `machineId` /
/// `appVersion` / `errorKind` / `errorMessage` —— 私钥字段**根本不在白名单里**，
/// 因此不可能因为「忘了脱敏」而外流。
fn credential_status_value(
    result: &Result<DeviceIdentity, IcubeError>,
    variant: TraeVariant,
) -> Value {
    match result {
        Ok(identity) => json!({
            "available": true,
            "sourceApp": identity.source_app,
            "deviceId": identity.device_id,
            "machineId": identity.machine_id,
            "appVersion": identity.app_version,
            "errorKind": Value::Null,
            "errorMessage": Value::Null,
        }),
        Err(error) => json!({
            "available": false,
            "sourceApp": Value::Null,
            "deviceId": Value::Null,
            "machineId": Value::Null,
            "appVersion": Value::Null,
            "errorKind": error.kind(),
            "errorMessage": error.user_message(variant),
        }),
    }
}

/// 设备凭证诊断视图（**不含私钥**）。
///
/// 供 `login_start` 响应与 `profile::diagnose_missing_credential` 共用。
pub fn credential_status_for(variant: TraeVariant) -> Value {
    credential_status_value(&device_identity_for(variant), variant)
}

/// 跨模块测试专用的**合成**信封构造器（`cfg(test)` 下才存在）。
///
/// ## 为什么需要它
///
/// `oauth.rs` / `account.rs` 的链路测试需要一份**可复现的设备身份**，而真实身份来自
/// 本机 `%APPDATA%\<产品线>\User\globalStorage\storage.json`。直接读真机有两个问题：
/// 1. 测试会依赖「跑测试的机器恰好装过并登录过 Trae」——CI 上必然红；
/// 2. 真机信封里装着**用户的真实私钥**，绝不能进仓库当 fixture。
///
/// 因此这里用**测试专用密钥对**造一份合成信封。生产代码里没有「加密」这条路
/// （写信封是客户端的事），故本模块整体 `#[cfg(test)]`。
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// 测试用 EC P-256 密钥对（openssl 生成，**仅供测试**，不是任何真实凭证）。
    pub(crate) const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgfMhArVaVsHbRHQHS\n\
hLkrYGiVNhsErnjKIgS7/EIHdsihRANCAARznG0WLhenNiMW5jA3SwFpTNyet2zw\n\
1YDTNTiZI0L4egFQ0IzlUPgrZK/O5nwkmNJR0MukWZH7NgvdoDLsS0NB\n\
-----END PRIVATE KEY-----";
    pub(crate) const TEST_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----\n\
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEc5xtFi4XpzYjFuYwN0sBaUzcnrds\n\
8NWA0zU4mSNC+HoBUNCM5VD4K2SvzuZ8JJjSUdDLpFmR+zYL3aAy7EtDQQ==\n\
-----END PUBLIC KEY-----";
    /// 标准 EC P-256 SPKI 的 DER 头（`30 59` + `id-ecPublicKey` + `prime256v1`）。
    pub(crate) const SPKI_PREFIX: [u8; 15] = [
        0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08,
    ];

    /// 测试用固定信封盐（生产上由客户端随机生成；测试要确定性）。
    pub(crate) const TEST_RANDOM: [u8; RANDOM_LEN] = [7u8; RANDOM_LEN];

    /// 按 `tc` 信封格式**加密**一段明文（复刻 byteCrypto 的写侧）。
    pub(crate) fn seal_envelope(body: &[u8], tag: &[u8; SHA512_LEN]) -> String {
        use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

        let (key, iv) = derive_keys(&TEST_RANDOM, &TcMode::Aes.pepper());
        let mut plain = Vec::with_capacity(SHA512_LEN + body.len());
        plain.extend_from_slice(tag);
        plain.extend_from_slice(body);
        let cipher = Aes128CbcEnc::new((&key).into(), (&iv).into())
            .encrypt_padded_vec_mut::<Pkcs7>(&plain);
        let mut raw = Vec::with_capacity(HEADER_LEN + RANDOM_LEN + cipher.len());
        raw.extend_from_slice(&TC_HEADER);
        raw.extend_from_slice(&TEST_RANDOM);
        raw.extend_from_slice(&cipher);
        base64::engine::general_purpose::STANDARD.encode(raw)
    }

    /// 用**正确的**摘要封装一段明文。
    pub(crate) fn seal(body: &[u8]) -> String {
        seal_envelope(body, &sha512(body))
    }

    /// `{"privateKeyPEM":…,"publicKeyPEM":…}` 明文（键名与客户端写入的逐字一致）。
    pub(crate) fn device_plain_with_both_keys() -> String {
        format!(
            r#"{{"privateKeyPEM":"{}","publicKeyPEM":"{}"}}"#,
            TEST_PRIVATE_KEY_PEM.replace('\n', "\\n"),
            TEST_PUBLIC_KEY_PEM.replace('\n', "\\n")
        )
    }

    /// 合成本信封（base64）—— 直接塞进 fixture `storage.json` 的
    /// `iCubeAuthInfo://icube-dc:<deviceId>` 键。
    pub(crate) fn synthetic_device_envelope() -> String {
        seal(device_plain_with_both_keys().as_bytes())
    }

    /// 造一个**裸** JWT（三段；payload 含 `data.id` 与 `exp`）。
    ///
    /// 「裸」是刻意的：真机 `CloudideAuthInfo.token` 实测就是**不带**
    /// `Cloud-IDE-JWT ` 前缀的裸 token，而落库必须补前缀 —— 这里复刻该形态，
    /// 使「补前缀」这条护栏有真实输入可测。
    pub(crate) fn bare_jwt(user_id: &str, exp: i64) -> String {
        let b64 = |text: &str| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text.as_bytes())
        };
        format!(
            "{}.{}.{}",
            b64(r#"{"alg":"RS256","typ":"JWT"}"#),
            b64(&format!(r#"{{"exp":{exp},"data":{{"id":"{user_id}"}}}}"#)),
            "c2lnbmF0dXJl"
        )
    }

    /// 合成 `iCubeAuthInfo://icube.cloudide` 的**明文** JSON。
    ///
    /// 键名与客户端写入的逐字一致（即 [`cloudide_from_plain`] 的解析目标），
    /// 取值形态按真机实测：`token` 裸 JWT、`refreshToken` 短串、
    /// `host` 带 scheme 无尾斜杠、`expiredAt` 为 epoch 秒。
    pub(crate) fn cloudide_plain(user_id: &str, exp: i64) -> String {
        let token = bare_jwt(user_id, exp);
        format!(
            r#"{{"token":"{token}","refreshToken":"rt-{user_id}","host":"https://api.trae.cn","userRegion":"cn","userId":"{user_id}","expiredAt":{exp},"account":"acc-{user_id}"}}"#
        )
    }

    /// 合成 cloudide 信封（base64）—— 直接塞进 fixture `storage.json` 的
    /// [`CLOUDIDE_KEY`] 键。
    pub(crate) fn synthetic_cloudide_envelope(user_id: &str, exp: i64) -> String {
        seal(cloudide_plain(user_id, exp).as_bytes())
    }

    /// 铺一份**只有 tc 信封**的 userData：`storage.json` 里只有 [`CLOUDIDE_KEY`]，
    /// **没有** `icube-dc` 设备凭证、**没有**任何明文 `Cloud-IDE-JWT`、
    /// **连 `logs/` 目录都没有**。
    ///
    /// 这是「Trae Work 能导入本机账号」的护栏场景：`TRAE SOLO CN` 的真实形态就是
    /// 只有加密信封、没有任何明文来源（实测 355 个日志文件、0 个 `completion.log`）。
    ///
    /// 返回 `(userId, 目录名)`（覆盖全部候选目录，各给**不同** userId，
    /// 使「按变体限定」若退化成「跨变体取第一个命中」时断言能立刻发现）。
    pub(crate) fn write_cloudide_only_user_data(
        base: &std::path::Path,
        exp: i64,
    ) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (variant_index, variant) in TraeVariant::all().into_iter().enumerate() {
            let names = crate::modules::trae::platform::data_dir_names_for(variant);
            for (index, name) in names.iter().enumerate() {
                let user_id = format!("900000000000{:04}", variant_index * 10 + index);
                let dir = base.join(name).join("User").join("globalStorage");
                std::fs::create_dir_all(&dir).expect("fixture 目录应能创建");
                let storage = serde_json::json!({
                    CLOUDIDE_KEY: synthetic_cloudide_envelope(&user_id, exp),
                });
                let file = dir.join("storage.json");
                std::fs::write(&file, serde_json::to_vec_pretty(&storage).unwrap())
                    .expect("fixture storage.json 应能写入");
                pin_activity(&file, names.len() - 1 - index);
                out.push((user_id, (*name).to_string()));
            }
        }
        out
    }

    /// 「候选目录 × 登录态 × 活跃度」**四格**的构造结果（每个候选目录一格）。
    ///
    /// ## 为什么要有它
    ///
    /// R3 / R5 的护栏需要**显式**构造四种组合，而不是靠「写入先后」碰运气：
    ///
    /// | 护栏 | 需要的格子 |
    /// |:--|:--|
    /// | R5：目录存在 + 活跃 + **无**登录态 ⇒ 保存必须**拒绝** | `(true, false, true)` |
    /// | R3：只存在次位候选且**已登录** ⇒ 备份 / 恢复必须**可用** | 首位 `(false, _, _)` + 次位 `(true, true, true)` |
    ///
    /// ## 为什么把「是否分叉」在构造时就算好
    ///
    /// 凡把「两个选择器必须分叉」当**前置条件**的护栏，会在两者**碰巧一致**时
    /// **假绿** —— 而「碰巧一致」恰恰是常态：活跃度取的是 `storage.json` 的 mtime，
    /// Windows 文件时间粒度约 15.6ms，相邻两次 fixture 写入常落在同一刻度上，
    /// 于是稳定排序退回候选表原顺序、`select` 与 `detect` 相同（当初 P0-2 的成因）。
    /// 故由本结构体在**构造完成时**统一算一次，用例只断言这个字段，不各写一遍比较。
    pub(crate) struct SelectionGrid {
        /// 每个候选目录一格的构造记录，按**候选表顺序**。
        pub(crate) cells: Vec<SelectionCell>,
        /// 构造完成后 `select_data_dir_for(variant) != detect_data_dir_for(variant)`。
        pub(crate) selectors_diverge: bool,
    }

    /// 单格记录。
    pub(crate) struct SelectionCell {
        /// 候选目录名（如 `TRAE SOLO CN`）。
        pub(crate) name: &'static str,
        /// 该目录是否存在。
        pub(crate) exists: bool,
        /// 该目录是否有**可解登录态**（cloudide 信封）。
        pub(crate) logged_in: bool,
        /// 该目录是否被钉成「活跃」。
        pub(crate) active: bool,
        /// 登录态归属的 userId（`logged_in == false` 时为空串）。
        pub(crate) user_id: String,
        /// 该格对应的目录绝对路径。
        pub(crate) dir: std::path::PathBuf,
    }

    /// 按**显式网格**构造某个变体的候选 userData 目录。
    ///
    /// `cells[i]` 对应 `data_dir_names_for(variant)[i]`，每格是
    /// `(存在?, 有登录态?, 活跃?)`。
    ///
    /// 活跃度用 [`pin_activity`]（`File::set_modified`）**显式钉死**：
    /// 活跃 ⇒ `now`、不活跃 ⇒ `now - 24h`。**不依赖写入顺序、不用 `sleep`**。
    ///
    /// 「无登录态」的格子也会写一份**与登录无关**的 `storage.json`（`aha.account`），
    /// 而不是留空目录 —— R5 的护栏正是「目录存在、`backup_to_slot_for` 能拷到文件、
    /// 却没有登录态」；留空目录会以「未找到任何登录态文件」失败，走的就不是那条路径了。
    pub(crate) fn write_selection_grid(
        base: &std::path::Path,
        variant: TraeVariant,
        cells: &[(bool, bool, bool)],
        exp: i64,
    ) -> SelectionGrid {
        let names = crate::modules::trae::platform::data_dir_names_for(variant);
        assert_eq!(
            cells.len(),
            names.len(),
            "网格格数必须等于该变体的候选目录数"
        );

        let mut out = Vec::new();
        for (index, ((exists, logged_in, active), name)) in cells.iter().zip(names).enumerate() {
            let dir = base.join(name);
            if !*exists {
                let _ = std::fs::remove_dir_all(&dir);
                out.push(SelectionCell {
                    name,
                    exists: false,
                    logged_in: false,
                    active: false,
                    user_id: String::new(),
                    dir,
                });
                continue;
            }

            let storage_dir = dir.join("User").join("globalStorage");
            std::fs::create_dir_all(&storage_dir).expect("fixture 目录应能创建");
            let user_id = format!("700000000000{:04}", index);
            let storage = if *logged_in {
                serde_json::json!({
                    CLOUDIDE_KEY: synthetic_cloudide_envelope(&user_id, exp),
                })
            } else {
                serde_json::json!({ "aha": { "account": "no-login-state" } })
            };
            let file = storage_dir.join("storage.json");
            std::fs::write(&file, serde_json::to_vec_pretty(&storage).unwrap())
                .expect("fixture storage.json 应能写入");
            pin_activity(&file, if *active { 0 } else { 24 });

            out.push(SelectionCell {
                name,
                exists: true,
                logged_in: *logged_in,
                active: *active,
                user_id,
                dir,
            });
        }

        let selectors_diverge = crate::modules::trae::platform::select_data_dir_for(variant)
            != crate::modules::trae::platform::detect_data_dir_for(variant);
        SelectionGrid {
            cells: out,
            selectors_diverge,
        }
    }

    /// 把 fixture 文件的 mtime **钉死**在 `age_hours` 小时之前。
    ///
    /// ★ 为什么必须钉死、不能依赖「写入顺序」：`platform::data_dir_activity` 取的是
    /// `storage.json` 的 mtime，而 Windows 的文件时间粒度约 **15.6ms** —— 相邻两次
    /// fixture 写入经常落在**同一个刻度**上，于是两个候选目录的活跃时间**相等**，
    /// `select_data_dir_for` 的稳定排序退回候选表原顺序、返回 `names[0]`，
    /// 与 `detect_data_dir_for` **相同**。凡是把「两个选择器必须分叉」当**前置条件**
    /// 的用例（守卫类三条）就会随机红在那一行前置断言上：实测全量并行下约 20%
    /// （15 轮中 3 轮），单独跑 40 次一次不复现 —— 典型的「最难定位」形态。
    ///
    /// 这里给第 `i` 个候选钉 `now - (n-1-i)` 小时：**首位最旧、末位最新**，
    /// 与真机 Trae Work 同形（首位 `TRAE SOLO CN` 有登录态、末位 `TRAE SOLO` 更活跃）。
    /// 把 `storage.json` 的 mtime 钉到 `age_hours` 小时之前（= 活跃度）。
    ///
    /// `pub(crate)` 是给 `account.rs` 的「活跃目录**翻转**后绑定仍指向来源设备」用例用的：
    /// 那需要先铺好 fixture、再**事后**改活跃度，而网格构造器只在构造时钉一次。
    pub(crate) fn pin_activity(file: &std::path::Path, age_hours: usize) {
        let pinned = std::time::SystemTime::now()
            - std::time::Duration::from_secs(age_hours as u64 * 3_600);
        // 必须带 `write(true)`：Windows 上 `SetFileTime` 需要 `FILE_WRITE_ATTRIBUTES`，
        // 只读句柄会 `ERROR_ACCESS_DENIED`。
        std::fs::OpenOptions::new()
            .write(true)
            .open(file)
            .and_then(|handle| handle.set_modified(pinned))
            .expect("fixture 应能把 storage.json 的 mtime 钉死");
    }

    /// 按**显式网格**铺「设备凭证」userData（`icube-dc` 信封）。
    ///
    /// `cells[i]` 对应 `data_dir_names_for(variant)[i]`，每格是 `(存在?, 活跃?)`；
    /// 存在的格子各绑**不同** deviceId（`22929298067` + 6 位序号），活跃度用
    /// [`pin_activity`] **显式钉死**（活跃 ⇒ `now`、不活跃 ⇒ `now - 24h`）。
    ///
    /// ## 为什么不能复用 [`write_synthetic_user_data`]
    ///
    /// 那个 helper 覆盖**全部变体**且**不钉活跃度** —— 候选目录的先后取决于**写入顺序**
    /// （后写的 mtime 更新、更活跃），而 Windows 时间粒度约 15.6ms，同刻度时会退回
    /// 候选表原顺序。`device_credential_by_device_id` 的护栏要求「请求的 deviceId 落在
    /// **非首个、且不活跃**的候选里」，靠写入顺序会**随机假绿**。
    ///
    /// 返回 `(deviceId, 目录名, 目录路径)`，按候选表顺序（不存在的格子不返回）。
    pub(crate) fn write_device_entries_grid(
        base: &std::path::Path,
        variant: TraeVariant,
        cells: &[(bool, bool)],
    ) -> Vec<(String, &'static str, std::path::PathBuf)> {
        let names = crate::modules::trae::platform::data_dir_names_for(variant);
        assert_eq!(
            cells.len(),
            names.len(),
            "网格格数必须等于该变体的候选目录数"
        );

        let mut out = Vec::new();
        for (index, ((exists, active), name)) in cells.iter().zip(names).enumerate() {
            let dir = base.join(name);
            if !*exists {
                let _ = std::fs::remove_dir_all(&dir);
                continue;
            }
            // 每个候选给**不同** deviceId：请求的那个若落在次位，
            // 「只查首个候选」的实现会立刻暴露。
            let device_id = format!("22929298067{index:06}");
            let storage_dir = dir.join("User").join("globalStorage");
            std::fs::create_dir_all(&storage_dir).expect("fixture 目录应能创建");
            let storage = serde_json::json!({
                format!("{ICUBE_DC_PREFIX}{device_id}"): synthetic_device_envelope(),
                TELEMETRY_MACHINE_ID: format!("telemetry-{device_id}"),
            });
            let file = storage_dir.join("storage.json");
            std::fs::write(&file, serde_json::to_vec_pretty(&storage).unwrap())
                .expect("fixture storage.json 应能写入");
            pin_activity(&file, if *active { 0 } else { 24 });
            out.push((device_id, *name, dir));
        }
        out
    }

    /// 往**已存在的**候选目录的 `storage.json` 里追加一条 `icube-dc` 设备凭证。
    ///
    /// 用于构造「登录态在 A、设备身份在 B」这类**跨目录**形态：导入必须只读 A，
    /// 在 A 里取不到设备身份就是 `None` —— **不得**回头去 B 取（那会让「凭据来自 A、
    /// 设备身份来自 B」，正是本项目反复栽的不同源）。
    ///
    /// ⚠️ 写回会刷新 `storage.json` 的 mtime，故这里**把原 mtime 恢复回去** ——
    /// 否则会打乱 [`write_selection_grid`] 钉好的活跃度（那正是它要防的坑）。
    pub(crate) fn attach_device_entry(
        base: &std::path::Path,
        name: &str,
        device_id: &str,
    ) -> std::path::PathBuf {
        let file = base
            .join(name)
            .join("User")
            .join("globalStorage")
            .join("storage.json");
        let previous = std::fs::metadata(&file)
            .and_then(|meta| meta.modified())
            .ok();
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).expect("fixture storage.json 应存在"))
                .expect("fixture storage.json 应是合法 JSON");
        value
            .as_object_mut()
            .expect("fixture storage.json 顶层应是对象")
            .insert(
                format!("{ICUBE_DC_PREFIX}{device_id}"),
                serde_json::json!(synthetic_device_envelope()),
            );
        std::fs::write(&file, serde_json::to_vec_pretty(&value).unwrap())
            .expect("fixture storage.json 应能写回");
        if let Some(previous) = previous {
            std::fs::OpenOptions::new()
                .write(true)
                .open(&file)
                .and_then(|handle| handle.set_modified(previous))
                .expect("应能把 storage.json 的 mtime 恢复回去");
        }
        file
    }

    /// 在 `base` 下铺一份**合成**的 Trae userData 目录树，覆盖全部变体的候选目录名。
    ///
    /// 返回 `(deviceId, 目录名)` 列表，调用方可据此断言。
    /// 目录结构：`<base>/<产品线目录>/User/globalStorage/storage.json`。
    pub(crate) fn write_synthetic_user_data(base: &std::path::Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (variant_index, variant) in TraeVariant::all().into_iter().enumerate() {
            for (index, name) in crate::modules::trae::platform::data_dir_names_for(variant)
                .iter()
                .enumerate()
            {
                // 每个候选目录给**不同** deviceId：这样「按变体限定」若退化成
                // 「跨变体扫第一个命中」，断言会立刻发现读错了目录。
                let device_id = format!("22929298067{:06}", variant_index * 100 + index);
                let dir = base
                    .join(name)
                    .join("User")
                    .join("globalStorage");
                std::fs::create_dir_all(&dir).expect("fixture 目录应能创建");
                let storage = serde_json::json!({
                    format!("{ICUBE_DC_PREFIX}{device_id}"): synthetic_device_envelope(),
                    TELEMETRY_MACHINE_ID: format!("telemetry-{device_id}"),
                });
                std::fs::write(
                    dir.join("storage.json"),
                    serde_json::to_vec_pretty(&storage).unwrap(),
                )
                .expect("fixture storage.json 应能写入");
                out.push((device_id, (*name).to_string()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    #[cfg(windows)]
    use crate::modules::trae::test_support::TempEnv;
    use std::sync::atomic::Ordering;

    fn test_credential() -> DeviceCredential {
        DeviceCredential {
            variant: TraeVariant::TraeWork,
            device_id: "2292929806738024".into(),
            private_key_pem: TEST_PRIVATE_KEY_PEM.to_string(),
            source_app: "TRAE SOLO CN".into(),
        }
    }

    // -- 常量表 --------------------------------------------------------------

    /// 表数据完整性：AES 模式 pepper 前六字节 = `WOE_T ^ VOE_T`。
    #[test]
    fn pepper_tables_xor_matches_known_head() {
        let expected = [77u8, 212, 194, 230, 184, 49];
        let pepper = TcMode::Aes.pepper();
        assert_eq!(&pepper[..6], &expected[..]);
    }

    // -- tc_decrypt 的失败路径（绝不 panic） ---------------------------------

    #[test]
    fn tc_decrypt_rejects_bad_magic() {
        let bad = base64::engine::general_purpose::STANDARD.encode([1u8; 64]);
        let error = tc_decrypt(&bad, TcMode::Aes).expect_err("magic 不匹配必须报错");
        assert_eq!(error.kind(), "decryptFailed");
        assert!(
            error.user_message(TraeVariant::TraeWork).contains("Trae Work"),
            "错误文案必须点明产品线"
        );
    }

    #[test]
    fn tc_decrypt_rejects_bad_base64() {
        assert!(tc_decrypt("!!!not-base64!!!", TcMode::Aes).is_err());
    }

    #[test]
    fn tc_decrypt_rejects_short_envelope() {
        // magic 正确但总长不足。
        let short = base64::engine::general_purpose::STANDARD.encode([116u8, 99, 5, 16, 0, 0, 1, 2]);
        assert!(tc_decrypt(&short, TcMode::Aes).is_err());
    }

    /// 改一字节 ⇒ SHA512 完整性校验失败（**不 panic**）。
    ///
    /// 造法：用**错误的**摘要封装（正文与 tag 不匹配），这等价于「信封被篡改」，
    /// 且能稳定命中 SHA512 分支（直接翻密文字节会先撞上 PKCS7 填充校验，
    /// 那是另一条分支，另有用例覆盖）。
    #[test]
    fn tc_decrypt_rejects_tampered_body() {
        let body = br#"{"publicKeyPEM":"x"}"#;
        let mut wrong_tag = sha512(body);
        wrong_tag[0] ^= 0xff;
        let envelope = seal_envelope(body, &wrong_tag);
        let error = tc_decrypt(&envelope, TcMode::Aes).expect_err("摘要不匹配必须报错");
        assert!(
            error.user_message(TraeVariant::TraeWork).contains("解密失败"),
            "文案应说明是解密环节: {error:?}"
        );
    }

    /// 直接翻转密文字节也必须**返回 Err 而不是 panic**（可能是填充失败，也可能是摘要失败）。
    #[test]
    fn tc_decrypt_survives_flipped_ciphertext_byte() {
        let envelope = seal(br#"{"publicKeyPEM":"x"}"#);
        let mut raw = base64::engine::general_purpose::STANDARD
            .decode(&envelope)
            .expect("自造信封必须能解码");
        let last = raw.len() - 1;
        raw[last] ^= 0x01;
        let flipped = base64::engine::general_purpose::STANDARD.encode(raw);
        assert!(tc_decrypt(&flipped, TcMode::Aes).is_err());
    }

    /// 自造信封能被自己解开（证明测试信封构造与生产解密实现同构）。
    #[test]
    fn sealed_envelope_round_trips() {
        let plain = device_plain_with_both_keys();
        let envelope = seal(plain.as_bytes());
        assert_eq!(tc_decrypt(&envelope, TcMode::Aes).unwrap(), plain);
    }

    // -- 身份 / 凭证的解析语义 ----------------------------------------------

    /// ★ 公钥**直取**信封里的 `publicKeyPEM`（逐字相等），不是由私钥推导的。
    #[test]
    fn device_identity_takes_public_key_pem_verbatim_from_envelope() {
        let plain = device_plain_with_both_keys();
        let identity = identity_from_plain(
            &plain,
            TraeVariant::TraeWork,
            "2292929806738024",
            "mach-1",
            "1.107.1",
            "TRAE SOLO CN",
        )
        .expect("信封含 publicKeyPEM，必须解析成功");

        // 逐字直取：解析出来的 PEM 必须与信封里的原值（含真实换行）完全一致。
        assert_eq!(identity.public_key_pem, TEST_PUBLIC_KEY_PEM, "公钥必须逐字直取");

        // 直取的公钥必须是标准 EC P-256 SPKI（DER 头 + 总长 91B）。
        let body: String = identity
            .public_key_pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        let der = base64::engine::general_purpose::STANDARD
            .decode(body)
            .expect("SPKI 必须是合法 base64");
        assert_eq!(der.len(), 91, "EC P-256 SPKI 应为 91 字节");
        assert_eq!(&der[..SPKI_PREFIX.len()], &SPKI_PREFIX[..]);
        assert_eq!(identity.device_id, "2292929806738024");
        assert_eq!(identity.machine_id, "mach-1");
        assert_eq!(identity.app_version, "1.107.1");
        assert_eq!(identity.source_app, "TRAE SOLO CN");
    }

    /// ★ 结构保证：AuthCode 路径拿到的 [`DeviceIdentity`] 里**没有私钥**。
    #[test]
    fn device_identity_never_contains_private_key() {
        let plain = device_plain_with_both_keys();
        let identity = identity_from_plain(
            &plain,
            TraeVariant::TraeWork,
            "d",
            "m",
            "v",
            "TRAE SOLO CN",
        )
        .unwrap();
        let debug = format!("{identity:?}");
        assert!(!debug.contains("PRIVATE"), "Debug 里出现了私钥标记: {debug}");
        assert!(!debug.contains("privateKeyPEM"));
        assert!(!debug.contains("MIGHAgEAMBMGByqGSM49"), "Debug 里出现了私钥正文");
        // 序列化同样不得含私钥（诊断视图按白名单构造）。
        let json = serde_json::to_string(&credential_status_value(
            &Ok(identity),
            TraeVariant::TraeWork,
        ))
        .unwrap();
        assert!(!json.contains("BEGIN"));
        assert!(!json.contains("PRIVATE"));
        assert!(!json.contains("MIGHAgEAMBMGByqGSM49"));
    }

    /// ★ [`DeviceCredential`] 的 `Debug` 必须手写脱敏。
    #[test]
    fn device_credential_debug_is_redacted() {
        let credential = test_credential();
        let debug = format!("{credential:?}");
        assert!(debug.contains("<redacted>"), "应打印占位符: {debug}");
        assert!(!debug.contains("MIGHAgEAMBMGByqGSM49"), "私钥正文泄漏: {debug}");
        assert!(!debug.contains("BEGIN PRIVATE KEY"));
        // device_id / source_app 属于允许展示的诊断字段。
        assert!(debug.contains("2292929806738024"));
    }

    /// `CloudideAuthInfo` 的 `Debug` 同样脱敏（token / refresh_token 是红线字段）。
    #[test]
    fn cloudide_auth_info_debug_is_redacted() {
        let info = cloudide_from_plain(
            r#"{"token":"a.b.c","refreshToken":"rt","host":"https://api.trae.cn","userId":123,"userRegion":"cn","expiredAt":100,"account":"me"}"#,
            TraeVariant::Trae,
        )
        .unwrap();
        let debug = format!("{info:?}");
        assert!(!debug.contains("a.b.c"), "token 泄漏: {debug}");
        assert!(!debug.contains("\"rt\""), "refresh_token 泄漏: {debug}");
        assert!(debug.contains("https://api.trae.cn"));
        assert_eq!(info.user_id.as_deref(), Some("123"));
        assert_eq!(info.expired_at, Some(100));
    }

    /// 信封缺 `publicKeyPEM` ⇒ 结构化错误，**不推导、不静默**。
    #[test]
    fn envelope_without_public_key_errors_instead_of_deriving() {
        let plain = format!(
            r#"{{"privateKeyPEM":"{}"}}"#,
            TEST_PRIVATE_KEY_PEM.replace('\n', "\\n")
        );
        let error = identity_from_plain(&plain, TraeVariant::TraeWork, "d", "m", "v", "app")
            .expect_err("缺公钥必须报错，而不是由私钥推导");
        assert_eq!(error, IcubeError::PublicKeyMissing);
        assert_eq!(error.kind(), "publicKeyMissing");
    }

    /// 信封缺 `privateKeyPEM` ⇒ `PrivateKeyMissing`（refresh 路径专用）。
    #[test]
    fn credential_without_private_key_errors() {
        let error = credential_from_plain(r#"{"publicKeyPEM":"x"}"#, TraeVariant::TraeWork, "d", "app")
            .expect_err("缺私钥必须报错");
        assert_eq!(error, IcubeError::PrivateKeyMissing);
    }

    /// 明文不是 JSON ⇒ 结构化错误（不 panic）。
    #[test]
    fn non_json_plaintext_is_structured_error() {
        assert!(identity_from_plain("not-json", TraeVariant::TraeWork, "d", "m", "v", "a").is_err());
        assert!(cloudide_from_plain("not-json", TraeVariant::TraeWork).is_err());
    }

    /// ★ 两个键**共用同一条**解密实现。
    ///
    /// 断言方式：对同一段 b64，分别按 `icube-dc` 与 `icube.cloudide` 的键名走
    /// [`decrypt_storage_key`]，两者结果必须**逐字相同**，且 [`tc_decrypt`] 的调用计数
    /// 恰好增加 2（说明两者都真的经过了那唯一一条实现，而不是各有一份）。
    #[test]
    fn cloudide_and_icube_dc_share_one_decrypt_impl() {
        let plain = device_plain_with_both_keys();
        let envelope = seal(plain.as_bytes());
        let mut object = Map::new();
        object.insert(format!("{ICUBE_DC_PREFIX}2292929806738024"), json!(envelope));
        object.insert(CLOUDIDE_KEY.to_string(), json!(envelope));

        let before = TC_DECRYPT_CALLS.load(Ordering::SeqCst);
        let from_dc = decrypt_storage_key(&object, &format!("{ICUBE_DC_PREFIX}2292929806738024"), TcMode::Aes)
            .expect("icube-dc 键必须能解开");
        let from_cloudide =
            decrypt_storage_key(&object, CLOUDIDE_KEY, TcMode::Aes).expect("cloudide 键必须能解开");
        let after = TC_DECRYPT_CALLS.load(Ordering::SeqCst);

        assert_eq!(from_dc, from_cloudide, "两个键必须得到同一份明文");
        assert_eq!(after - before, 2, "两次读取都必须经过同一条 tc_decrypt");
    }

    /// 键缺失 ⇒ `KeyMissing`（而不是静默空值）。
    #[test]
    fn missing_storage_key_is_reported() {
        let object = Map::new();
        assert_eq!(
            decrypt_storage_key(&object, CLOUDIDE_KEY, TcMode::Aes),
            Err(IcubeError::KeyMissing)
        );
        assert_eq!(
            find_icube_dc_entry(&object),
            Err(IcubeError::KeyMissing)
        );
    }

    /// `find_icube_dc_entry` 必须从键名里取出 `<deviceId>`。
    #[test]
    fn icube_dc_entry_parses_device_id_from_key_name() {
        let mut object = Map::new();
        object.insert(format!("{ICUBE_DC_PREFIX}2292929806738024"), json!("x"));
        let (device_id, _) = find_icube_dc_entry(&object).unwrap();
        assert_eq!(device_id, "2292929806738024");
    }

    // -- DeviceProof ---------------------------------------------------------

    /// ★ P1363 变体：64 字节 r‖s，字段名与 `Timestamp` 类型逐字对齐。
    #[test]
    fn device_proof_signature_is_p1363_64_bytes() {
        let credential = test_credential();
        let proof = device_proof(
            &credential,
            "/trae/api/v3/oauth/ExchangeToken",
            "ono9krqynydwx5",
            "refresh-token-value",
            ProofSigFormat::P1363,
        )
        .expect("签名不应失败");

        let signature = proof.get("Signature").and_then(|v| v.as_str()).unwrap();
        let raw = base64::engine::general_purpose::STANDARD.decode(signature).unwrap();
        assert_eq!(raw.len(), 64, "P1363 必须是 raw r‖s 64 字节");

        assert!(
            proof.get("Timestamp").map(|v| v.is_i64() || v.is_u64()).unwrap_or(false),
            "Timestamp 必须是 JSON int（字符串会被服务端 schema 拒绝）: {proof}"
        );
        assert_eq!(
            proof.get("Nonce").and_then(|v| v.as_str()).map(str::len),
            Some(32)
        );
        // 字段名 PascalCase，逐字。
        let keys: Vec<&str> = proof.as_object().unwrap().keys().map(String::as_str).collect();
        assert!(keys.contains(&"Signature"));
        assert!(keys.contains(&"Timestamp"));
        assert!(keys.contains(&"Nonce"));
        assert_eq!(keys.len(), 3);
    }

    /// DER 变体仍可用（供 refresh 链步 2 的对照探测），且比 P1363 长。
    #[test]
    fn device_proof_der_variant_is_longer_than_p1363() {
        let credential = test_credential();
        let p1363 = device_proof(&credential, "/p", "cid", "rt", ProofSigFormat::P1363).unwrap();
        let der = device_proof(&credential, "/p", "cid", "rt", ProofSigFormat::Der).unwrap();
        let len_of = |value: &Value| {
            base64::engine::general_purpose::STANDARD
                .decode(value.get("Signature").and_then(|v| v.as_str()).unwrap())
                .unwrap()
                .len()
        };
        assert!(len_of(&der) > len_of(&p1363), "DER 应比 P1363 长");
        assert_eq!(ProofSigFormat::P1363.suffix(), "/P1363");
        assert_eq!(ProofSigFormat::Der.suffix(), "/DER");
    }

    /// 签名错误不得泄漏私钥正文。
    #[test]
    fn device_proof_error_never_leaks_private_key() {
        let mut credential = test_credential();
        credential.private_key_pem = "-----BEGIN PRIVATE KEY-----\nnot-a-key\n-----END PRIVATE KEY-----".into();
        let error = device_proof(&credential, "/p", "cid", "rt", ProofSigFormat::P1363)
            .expect_err("非法 PEM 必须报错");
        assert!(!error.contains("not-a-key"), "错误文案泄漏了私钥正文: {error}");
        assert!(!error.contains("BEGIN PRIVATE KEY"));
    }

    // -- 诊断视图 ------------------------------------------------------------

    /// ★ 诊断视图**按构造**不含私钥（成功与失败两条分支都要成立）。
    #[test]
    fn credential_status_for_never_exposes_private_key() {
        let ok = credential_status_value(
            &Ok(DeviceIdentity {
                variant: TraeVariant::TraeWork,
                device_id: "d".into(),
                machine_id: "m".into(),
                app_version: "v".into(),
                public_key_pem: TEST_PUBLIC_KEY_PEM.into(),
                source_app: "TRAE SOLO CN".into(),
            }),
            TraeVariant::TraeWork,
        );
        let failed = credential_status_value(&Err(IcubeError::PrivateKeyMissing), TraeVariant::Trae);
        for value in [&ok, &failed] {
            let text = serde_json::to_string(value).unwrap();
            let upper = text.to_ascii_uppercase();
            assert!(!upper.contains("BEGIN"), "{text}");
            // `errorKind` 里的 `"privateKeyMissing"` 是**类型标签**、不是凭据，
            // 因此按「PRIVATE KEY」这个带空格的 PEM 标记判定，而不是按裸 `privateKey`。
            assert!(!upper.contains("PRIVATE KEY"), "{text}");
            assert!(!upper.contains("PUBLIC KEY"), "诊断视图不展示 PEM 原文: {text}");
            assert!(!text.contains("MIGHAgEAMBMGByqGSM49"), "{text}");
        }
        assert_eq!(ok.get("available").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(ok.get("sourceApp").and_then(|v| v.as_str()), Some("TRAE SOLO CN"));
        assert_eq!(failed.get("available").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(
            failed.get("errorKind").and_then(|v| v.as_str()),
            Some("privateKeyMissing")
        );
        // 错误文案必须点名**该程序位**。⚠️ `Trae` 与 `Trae Work` 前缀相同，
        // `contains("Trae")` 单独用会被 TraeWork 的文案蒙混过关，故补一条反向断言。
        let message = failed
            .get("errorMessage")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert!(message.contains("Trae"), "{message}");
        assert!(!message.contains("Trae Work"), "{message}");
    }

    /// 每种错误都有稳定的 `kind()` 标签（前端与日志依赖它）。
    #[test]
    fn every_error_variant_has_a_stable_kind() {
        let cases = [
            (IcubeError::DataDirMissing, "dataDirMissing"),
            (IcubeError::StorageUnreadable("x".into()), "storageUnreadable"),
            (IcubeError::KeyMissing, "keyMissing"),
            (IcubeError::DecryptFailed("x".into()), "decryptFailed"),
            (IcubeError::PublicKeyMissing, "publicKeyMissing"),
            (IcubeError::PrivateKeyMissing, "privateKeyMissing"),
            (IcubeError::PrivateKeyInvalid("x".into()), "privateKeyInvalid"),
            (IcubeError::DeviceIdentityMissing, "deviceIdentityMissing"),
            (
                IcubeError::DeviceIdentityMismatch("请求 aaaa…bbbb，本机仅有 cccc…dddd".into()),
                "deviceIdentityMismatch",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.kind(), expected);
            // 每条文案都必须点明产品线，且不含凭据片段。
            let message = error.user_message(TraeVariant::TraeWork);
            assert!(message.contains("Trae Work"), "{message}");
            assert!(!message.contains("BEGIN"), "{message}");
        }
    }

    /// 随机 hex 的长度与字符集（PKCE verifier / trace_id / nonce 共用）。
    #[test]
    fn random_hex_has_exact_length_and_lowercase_hex() {
        for len in [1usize, 31, 32, 64] {
            let value = random_hex(len);
            assert_eq!(value.len(), len);
            assert!(
                value.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{value}"
            );
        }
        assert_ne!(random_hex(32), random_hex(32), "两次调用必须不同");
    }

    // ---------- T13-2：按 deviceId 精确取凭证 / 定位装着登录态的目录 ----------

    /// 从**指定目录**取设备身份（显式入参）：`source_app` 必须是被喂进去的那个目录。
    #[cfg(windows)]
    #[test]
    fn device_identity_from_dir_reads_the_given_dir() {
        let env = TempEnv::empty();
        let variant = TraeVariant::TraeWork;
        // 首位**不活跃**、次位**活跃** —— 断言必须落在「被喂进去的那个目录」上，
        // 而不是活跃目录。
        let cells = write_device_entries_grid(&env.appdata(), variant, &[(true, false), (true, true)]);
        let (device_id, name, dir) = &cells[1];

        let identity =
            device_identity_from_dir(dir, variant).expect("次位候选目录里应有可解的设备身份");
        assert_eq!(identity.device_id, *device_id);
        assert_eq!(
            identity.source_app, *name,
            "source_app 必须是**被喂进去的那个目录**，而不是活跃目录"
        );
    }

    /// ★【T13-2 核心】按 `deviceId` 精确取凭证必须**遍历该变体的全部候选目录**，
    /// 而不是只看「最近活跃」或「首个」那一个。
    ///
    /// fixture：首位**活跃**、次位**不活跃**，且请求的 deviceId 只落在**次位**。
    /// ⇒ 「只查首个候选」与「按活跃度挑一个候选」两种退化实现都会失败。
    /// 活跃度由 `write_device_entries_grid` 显式钉死，**不依赖写入顺序**。
    #[cfg(windows)]
    #[test]
    fn device_credential_by_device_id_finds_the_entry_in_a_non_first_candidate() {
        let env = TempEnv::empty();
        let variant = TraeVariant::TraeWork;
        let cells =
            write_device_entries_grid(&env.appdata(), variant, &[(true, true), (true, false)]);
        let (requested, name, _) = &cells[1];

        let credential = device_credential_by_device_id(variant, requested)
            .expect("请求的 deviceId 在**不活跃**的次位候选里，必须能找到");
        assert_eq!(credential.device_id, *requested);
        assert_eq!(credential.source_app, *name);
        assert!(
            credential.private_key_pem.contains("BEGIN"),
            "refresh 路径拿到的必须是可用私钥"
        );
    }

    /// ★【T13-2 核心】请求一个本机**不存在**的 deviceId ⇒ `DeviceIdentityMismatch`：
    /// 既不报「缺失」，更**不得静默返回别的设备的凭证**（那会拿错私钥去签名）。
    ///
    /// 同时断言错误文案**不含** deviceId 原文（模块头的脱敏红线）。
    #[cfg(windows)]
    #[test]
    fn device_credential_by_device_id_refuses_a_mismatch() {
        let env = TempEnv::empty();
        let variant = TraeVariant::TraeWork;
        write_device_entries_grid(&env.appdata(), variant, &[(true, true), (true, true)]);

        // 用与真机同形（17 位）的长 id：`store::mask` 只对 >8 字符生效，
        // 短串会被**原样返回**，那样「脱敏」这条断言就失去意义了。
        let requested = "99998888777766665";
        let error = device_credential_by_device_id(variant, requested)
            .expect_err("本机没有这个 deviceId，必须报错而不是返回别的设备的凭证");
        assert_eq!(error.kind(), "deviceIdentityMismatch");

        let message = error.user_message(variant);
        assert!(
            !message.contains(requested),
            "错误文案不得出现 deviceId 原文：{message}"
        );
        assert!(
            message.contains(&crate::modules::trae::store::mask(requested)),
            "文案应给出**脱敏**后的对比值：{message}"
        );
    }

    /// 候选目录里**一条 `icube-dc` 都没有** ⇒ `DeviceIdentityMissing`。
    ///
    /// 与上一条成对：这是「本机压根没注册设备」与「注册的是别的设备」的分界。
    /// 混为一谈会让用户拿不到正确的下一步（前者要重新登录，后者要重新导入）。
    #[cfg(windows)]
    #[test]
    fn device_credential_by_device_id_reports_missing_when_no_entry_exists() {
        let env = TempEnv::empty();
        let variant = TraeVariant::TraeWork;
        // 只有 cloudide 信封，没有任何 `icube-dc` 条目。
        write_selection_grid(
            &env.appdata(),
            variant,
            &[(true, true, true), (true, true, false)],
            chrono::Utc::now().timestamp() + 3600,
        );

        let error = device_credential_by_device_id(variant, "99998888777766665")
            .expect_err("没有任何 icube-dc 条目时必须报缺失");
        assert_eq!(error.kind(), "deviceIdentityMissing");
    }

    /// ★【T13-2 核心】`login_state_dir_for` 必须返回**装着登录态**的那个候选目录，
    /// 而不是「最近活跃」的那个 —— 这正是用户机器上「导入必然失败」的成因。
    ///
    /// fixture：首位**活跃但无登录态**、次位**不活跃但有登录态**（真机 Trae Work 同形）。
    #[cfg(windows)]
    #[test]
    fn login_state_dir_for_picks_the_dir_that_actually_holds_the_credential() {
        let env = TempEnv::empty();
        let variant = TraeVariant::TraeWork;
        let grid = write_selection_grid(
            &env.appdata(),
            variant,
            &[(true, false, true), (true, true, false)],
            chrono::Utc::now().timestamp() + 3600,
        );
        // 前置：活跃的那个**没有**登录态、有登录态的那个**不活跃**。
        // 若两者恰好一致，本用例证明不了任何事（假绿）。
        assert!(
            grid.cells[0].active && !grid.cells[0].logged_in,
            "前置：首位必须活跃且无登录态"
        );
        assert!(
            !grid.cells[1].active && grid.cells[1].logged_in,
            "前置：次位必须不活跃且有登录态"
        );

        let found = login_state_dir_for(variant).expect("次位候选装着登录态，必须能找到");
        assert_eq!(
            found.file_name().and_then(|n| n.to_str()),
            Some(grid.cells[1].name),
            "必须返回**装着登录态**的那个目录，而不是最近活跃的那个"
        );
        // 对照：活跃目录选择器给的确实是首位 —— 两个问题答案不同，别混用。
        assert_eq!(
            crate::modules::trae::platform::select_data_dir_for(variant)
                .and_then(|dir| dir.file_name().map(|n| n.to_string_lossy().to_string())),
            Some(grid.cells[0].name.to_string()),
            "对照：select_data_dir_for 仍取最近活跃的那个（首位）"
        );
    }

    /// 没有任何候选目录装着登录态 ⇒ `None`（调用方据此回落到别的来源）。
    #[cfg(windows)]
    #[test]
    fn login_state_dir_for_is_none_when_no_candidate_holds_a_credential() {
        let env = TempEnv::empty();
        let variant = TraeVariant::TraeWork;
        write_selection_grid(
            &env.appdata(),
            variant,
            &[(true, false, true), (true, false, false)],
            chrono::Utc::now().timestamp() + 3600,
        );

        assert!(
            login_state_dir_for(variant).is_none(),
            "两个候选都没有登录态时必须是 None"
        );
    }
}
