//! Trae 模块：Trae Work 客户端的账号管理、签到、积分、登录态切换与本地代理。
//!
//! ## 为什么是独立命名空间，而不是往 `Region` 里加值
//!
//! 本仓库既有的 [`crate::modules::region::Region`] 描述 **同一产品（WorkBuddy）的两个
//! 发行版本**：共享认证文件格式、账号库结构、上游协议，差异只是域名/版本号/UA 形态。
//! Trae 与 WorkBuddy 的差异是**产品级**的：
//!
//! | 维度 | WorkBuddy | Trae |
//! |:---|:---|:---|
//! | 凭据形态 | `workbuddy-desktop.info`（JSON，含 domain/refresh） | `Cloud-IDE-JWT <jwt>`（VSCode 系） |
//! | 账号库 | `accounts[.global].json`（本仓库自有格式） | `checkin_accounts.json`（参考实现格式） |
//! | 上游 | `copilot.tencent.com` / `workbuddy.ai` | `api.trae.cn` / `api.trae.com.cn` |
//! | 登录态载体 | 单个认证文件 | 客户端 userData 目录下 9 类文件 |
//! | 客户端 | `WorkBuddy.exe`（单一） | **多条产品线**：`TRAE SOLO CN.exe` / `TRAE SOLO.exe` / `Trae CN.exe` / `Trae.exe` |
//!
//! 客户端那一行要特别注意：Trae 侧**不是**「一个客户端」，而是可以同机并存的
//! 多条产品线，安装目录、userData 目录、进程名各有一套。因此本模块内**不存在**
//! 「Trae 客户端叫什么」这样的单值常量 —— 一律以 [`variant`] 的变体表为准
//! （[`platform`] 的候选列表由那张表摊平而来，见该模块的
//! `exe_names()` / `data_dir_names()` 与护栏测试）。
//!
//! 若把 Trae 塞进 `Region`，`region_spec(All)`、`accounts_file_for(All)` 之类的既有约定
//! 会失去定义，且所有 `match region { Cn | Global }` 的调用点都要补分支——这与
//! 「同一逻辑 API 的两条通道必须形状一致」的仓库铁律一起，会把改动面从「新增」变成
//! 「改造」。因此 Trae 用自己的枚举与目录，绝不污染 `Region`。
//!
//! ## 变体（variant）—— 产品线的唯一真相
//!
//! [`variant`] 把各条产品线的全部差异收敛成一张表（同 [`crate::modules::region`] 的
//! 表驱动风格）：userData 目录名、exe 名、进程名、端点主机。
//! **所有按产品线分家的取值都必须从这张表派生**，不要在调用点写
//! `if variant == …`，也不要在别处维护第二份产品名清单。
//!
//! 变体命名与展示名**不是我们起的**：客户端 `product.json` 里
//! `TRAE SOLO CN` 的 `nameAlias` 就写着 `TraeWork CN`，故 Trae Work 是那条
//! `TRAE SOLO` 产品线的官方名称。依据与完整身份对照表见 [`variant`] 模块文档。
//!
//! ## 已知边界（未验证，勿凭推理改）
//!
//! 用「多版本隔离审计」过了一遍本模块，结论是**两处不对称属于已知且有意**，
//! 但**没有任何证据支持改动**，故维持现状并在此登记，避免后来者"顺手修正"：
//!
//! 1. **产品线有 4 条，但两条 CN 产品线的上游端点逐字相同（已实测）**。
//!    [`platform`] 能探测 `TRAE SOLO CN` / `TRAE SOLO` / `Trae CN` / `Trae`，
//!    而 `credits.rs` 的 `user-agent` / `package-type` / `x-user-region` 仍是
//!    **国内版单值**（常量在 `credits.rs` 顶部，均带出处注释并被回归测试钉住）。
//!    **重要区分**：[`variant`] 里的**端点**已按变体登记（含国际版一套），取值来自
//!    客户端 `product.json`；但**请求头**没有对应的按变体取值 ——
//!    `product.json` 里没有 `x-market-client-id` 这类头的任何字面量，
//!    即**无法从客户端侧核对**，只能以线上实测为准。
//!    两者别混为一谈：端点可以查表派生，请求头**未经实测不许改**
//!    （改了会让签到/积分直接失效，且 UA 里嵌着产品名）。
//! 2. **数据文件是单套，不按产品线分名**（`accounts_file()` / `groups_file()` /
//!    `credits_*` / `cooldowns_file()` 等，见 [`paths`]）。
//!    这是**刻意**的：本模块的**账号库本身**不按产品线分家，Trae 只有一套账号库。
//!    但**读客户端数据的那一侧是分变体的**：导入本机账号 / OAuth 采集会按
//!    传入变体限定候选目录（见 [`profile::extract_local_jwt_for`] 与
//!    [`platform::select_data_dir_for`]），即变体信息参与「读哪条产品线的 userData」，
//!    不再只是展示与端点派生。别把这两件事混为一谈——数据文件仍单套，
//!    只有那条**导入来源目录**是随变体走的。
//!    注意目标是会**漂移**的——本机同时装着 `TRAE SOLO CN` 与 `Trae CN`，
//!    谁最近被用过谁就是目标（实测本机最近是 `Trae CN`）。跨产品线的登录态
//!    （`profiles/`）是否可互相套用**未验证**；两条 CN 产品线共用同一上游与 JWT 形态，
//!    理论上可行，但没有实测。
//!
//! ## 数据目录
//!
//! 全部落在 `~/.buddy-switch/trae/`（见 [`paths`]），与 WorkBuddy 数据同库不同名：
//! 复用 [`crate::modules::config::store_dir`] 的 home/兼容回落逻辑，但文件名一律带
//! `trae` 语义或置于 `trae/` 子目录，避免与既有的 `accounts.json` / `checkin_*.json` 撞名。
//!
//! ## ★ 旧笔记更正：iCube 设备凭证**可以**离线解密（2026-09-18 实测）
//!
//! 本模块此前的文档与 `profile.rs` 的诊断文案都断言「客户端改用加密存储，
//! `iCubeAuthInfo://icube-dc:*` 无法离线解密」。**该结论已被推翻**：
//! `storage.json` 里的该键值是客户端自研 `tc` 信封（magic `746305100000`），
//! 其 pepper 是随安装包分发的 4 张 64B 公开常量表（混淆非加密），
//! 纯标准库即可解开，解出的 JSON 同时含 `privateKeyPEM` 与 `publicKeyPEM`。
//! 本机三条产品线（`TRAE SOLO CN` / `TRAE SOLO` / `Trae CN`）实测均可解。
//!
//! 解密实现见 [`icube`]；**它只读本机自有凭证，不做任何网络、不落盘、不写日志**。
//! 注意边界：这不等于「从客户端文件提取 JWT 作为登录替代」（那条路仍被否），
//! `icube.cloudide` 解出的 token/refreshToken 只用于**诊断与导入兜底**。
//!
//! ## 跨平台
//!
//! 参考实现是 Windows-only（注册表、PowerShell 5.1、`schtasks`、`certutil`、系统代理
//! 注册表项）。本模块按「能力优先、平台兜底」实现：能在三平台都做的做等价实现，
//! 做不到的返回结构化 [`platform::Unsupported`]，**绝不 panic、绝不静默假装成功**。

pub mod account;
pub mod checkin;
pub mod credits;
pub mod device;
pub mod export_import;
pub mod handlers;
pub mod icube;
pub mod jwt;
pub mod logs;
pub mod oauth;
pub mod oauth_client;
/// OAuth 回调结果页（浏览器里那一页的 HTML 渲染，见模块头）。
pub mod oauth_result_page;
pub mod paths;
pub mod platform;
pub mod profile;
/// 区域 × 程序两根正交轴（区域=持久化轴、程序=执行轴）。
///
/// 见模块头说明；这是把「产品线」单轴升级为双轴的落地层。
pub mod region;
/// 旧「产品线」账号库 → 新「区域」账号库的一次性合并迁移（幂等、带备份、不动旧文件）。
pub mod region_migrate;
pub mod settings;
pub mod store;
/// 测试专用的环境隔离与 fixture 构造（`cfg(test)` 下才编译，生产代码零引用）。
#[cfg(test)]
pub(crate) mod test_support;
pub mod token_stats;
pub mod variant;

/// Trae 上游：签到与积分接口基址（**CN 默认值**）。
///
/// ⚠️ **不要在新代码里直接用它** —— 按变体分家的正确取法是
/// [`variant::variant_spec`]`(v).endpoints(international).account_base`。
/// 保留此常量有两处用途：
/// 1. [`variant`] 的 CN 端点表以它为基准做逐字对拍（回归测试钉死）；
/// 2. 老调用点尚未迁移完时的兼容默认。
pub const TRAE_API_BASE_CN: &str = "https://api.trae.cn";

/// Trae 上游：OAuth / 令牌交换基址（**CN 默认值**，iCube 族）。
///
/// ⚠️ 同 [`TRAE_API_BASE_CN`]：新代码请走 [`variant`] 查表。
pub const TRAE_OAUTH_BASE_CN: &str = "https://api.trae.com.cn";

/// 取某变体的端点集（CN）。等价于
/// `variant::variant_spec(variant).endpoints(false)`，但直接返回而非 `Option`
/// —— CN 端点永远已登记，调用方不需要处理 `None`。
pub fn endpoints_for(variant: variant::TraeVariant) -> &'static variant::EndpointSet {
    &variant::variant_spec(variant).cn_endpoints
}

/// 签到接口路径。
pub const TRAE_CHECKIN_PATH: &str = "/trae/api/v2/ug/checkin_credits/claim";

/// 签到状态预检接口路径。
pub const TRAE_CHECKIN_STATUS_PATH: &str = "/trae/api/v2/ug/checkin_credits/status";

/// 剩余积分查询接口路径。
pub const TRAE_ENTITLEMENT_PATH: &str = "/trae/api/v2/pay/ide_user_ent_usage";

/// 令牌交换（ExchangeToken）接口路径 —— **主路径**（抓包固化 2026-09-16）。
///
/// 完整 URL 由**回调回传的 `host`** 拼接：`${host}/trae/api/v3/oauth/ExchangeToken`。
/// 旧的 cloudide 端点是另一条路径，见 [`TRAE_EXCHANGE_TOKEN_LEGACY_PATH`]。
pub const TRAE_EXCHANGE_TOKEN_PATH: &str = "/trae/api/v3/oauth/ExchangeToken";

/// 令牌交换的**旧端点**路径（保留作兜底链末位）。
///
/// 参考实现把它标注为「保留至固化协议验证期结束」；本项目按架构裁决保留为
/// 兜底链的最后一环（见 `account::exchange_token_for` 的四步变体链），
/// 但**不再作为主路径**。完整 URL = `${icube_base}${本常量}`。
pub const TRAE_EXCHANGE_TOKEN_LEGACY_PATH: &str =
    "/cloudide/api/v3/trae/oauth/ExchangeToken";

/// **TRAE / IDE 产品线**的公开 ClientID（抓包固化 2026-09-16，非机密）。
///
/// ⚠️ **这不是"唯一的 client_id"**：它只是 [`variant::OAuthLine::Trae`] 那条线的值。
/// 另一条线（SOLO / TraeWork）用的是 `en1oxy7wnw8j9n` —— 客户端 `product.json` 的
/// `iCubeApp.authConfig` 里两条线各有独立的一把钥匙，`auth_from` 也随线而变
/// （`trae` / `solo`）。**按线取值的正确入口是
/// [`oauth_client::OAuthClientConfig::client_id_for`]`(variant.oauth_line())`**。
///
/// 本常量是 [`variant::OAuthLine::default_client_id`] 的**具名别名**（单点定义在那边，
/// 这里不再写第二份字面量）。参考实现把它标为「真实 Trae **IDE** 登录 URL 实证值」，
/// 并记着「旧值 `en1oxy7wnw8j9n` 会让授权页停在 billing status 后不回跳」——
/// 那条观察是在 **IDE** 语境下的，**不能**推广成「SOLO 线也不能用 SOLO 的值」。
/// 出处：上游项目 `TraeWorkAssistant` 的 `src-tauri/src/commands/oauth.rs:20-23`（不在本仓库）。
pub const TRAE_OAUTH_CLIENT_ID: &str = variant::OAuthLine::Trae.default_client_id();

/// 授权页 `plugin_version` 参数（抓包固化 2026-09-16）。
///
/// 出处：参考 `oauth.rs:28`（`OAUTH_PAGE_PLUGIN_VERSION`）。**逐字对齐，勿按语义改写**。
pub const TRAE_PAGE_PLUGIN_VERSION: &str = "2.3.83560";

/// 授权页 `x_app_version` 参数（抓包固化 2026-09-16）。
///
/// 出处：参考 `oauth.rs:29`（`OAUTH_PAGE_APP_VERSION`）。
pub const TRAE_PAGE_APP_VERSION: &str = "3.3.100";

/// 交换请求的 `PlatformCode`（请求头与 `DeviceInfo` 共用）。
///
/// 出处：参考 `oauth.rs:30`（`OAUTH_PAGE_PLATFORM_CODE`）。
pub const TRAE_PAGE_PLATFORM_CODE: &str = "IDE_PC";

/// 云 IDE API 的 `x-app-id` 请求头取值。
///
/// 出处：参考 `oauth.rs:25`（`OAUTH_APP_ID`）。**照抄参考，未单独实测**：
/// 简报只强制 `x-cloudide-token`，此头多带无副作用（见架构 §10 #4）。
pub const TRAE_OAUTH_APP_ID: &str = "6eefa01c-1036-4c7e-9ca5-d891f63bfcd8";

/// 签到与状态接口共用的客户端身份版本号。
pub const TRAE_APP_VERSION: &str = "0.1.45";

/// 历史记录保留天数（签到明细 / 每日快照）。
pub const TRAE_HISTORY_KEEP_DAYS: i64 = 90;

/// JWT 剩余有效期低于该值时告警（小时）。
pub const TRAE_JWT_WARN_HOURS: f64 = 24.0;

/// 账号池 / 网关默认端口（与 WorkBuddy 网关默认端口错开，避免同时监听冲突）。
pub const TRAE_DEFAULT_API_PORT: u16 = 7864;

/// 本地代理默认端口（与 WorkBuddy 侧无冲突；参考实现用 8899）。
pub const TRAE_DEFAULT_PROXY_PORT: u16 = 8899;

/// OAuth 登录会话的有效期（秒）。超时后监听任务自行收尾并释放端口。
///
/// 与 WorkBuddy 侧同量级：授权流程要走「打开浏览器 → 登录 Trae 账号 →
/// （新用户还要注册）→ 点授权」，实测 2 分钟内可完成，留足重试余地。
/// **不要调短**：超时后端口被释放，用户此时点授权会落到一个没人监听的端口，
/// 表现为「浏览器一直转圈」——比等久一点糟糕得多。
pub const TRAE_OAUTH_LOGIN_TIMEOUT_SECONDS: u64 = 300;

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 契约常量必须与抓包固化值**逐字节**一致。
    ///
    /// 本用例存在的理由（本轮缺陷的教训）：229 个单测全绿、协议却整体过时。
    /// 单测断言的是我们自己写下的期望，不是上游的真实行为 —— 因此凡是
    /// **从抓包/参考实现抄来的字面量**，都要有一条「钉死在固化值上」的断言，
    /// 且注明出处行号，防止后人「按语义顺手改写」。
    ///
    /// 出处：抓包固化 2026-09-16，
    /// 上游项目 `TraeWorkAssistant` 的 `src-tauri/src/commands/oauth.rs:22,28-30`（不在本仓库）。
    #[test]
    fn oauth_constants_match_captured_snapshot() {
        // client_id：旧值 en1oxy7wnw8j9n 会让授权页停在 billing status 后不回跳。
        assert_eq!(TRAE_OAUTH_CLIENT_ID, "ono9krqynydwx5");

        // 交换端点：主路径是 `/trae/api/v3/oauth/ExchangeToken`（host 由回调回传），
        // 旧 cloudide 端点降级为兜底链末位，两者必须都在。
        assert_eq!(TRAE_EXCHANGE_TOKEN_PATH, "/trae/api/v3/oauth/ExchangeToken");
        assert_eq!(
            TRAE_EXCHANGE_TOKEN_LEGACY_PATH,
            "/cloudide/api/v3/trae/oauth/ExchangeToken"
        );

        // 三个页面参数：逐字对齐参考 `OAUTH_PAGE_*`。
        assert_eq!(TRAE_PAGE_PLUGIN_VERSION, "2.3.83560");
        assert_eq!(TRAE_PAGE_APP_VERSION, "3.3.100");
        assert_eq!(TRAE_PAGE_PLATFORM_CODE, "IDE_PC");

        // x-app-id：照抄参考，未单独实测。
        assert_eq!(TRAE_OAUTH_APP_ID, "6eefa01c-1036-4c7e-9ca5-d891f63bfcd8");

        // 交换端点不得再是旧值（回归护栏：改回旧路径会让 W6 悄悄失效）。
        assert_ne!(
            TRAE_EXCHANGE_TOKEN_PATH, TRAE_EXCHANGE_TOKEN_LEGACY_PATH,
            "主路径与兜底路径必须是两条不同的路径"
        );
    }

    /// 签到侧常量与变体表**逐字相同**（本轮零改动的证据）。
    #[test]
    fn checkin_constants_stay_aligned_with_variant_table() {
        for variant in variant::TraeVariant::all() {
            assert_eq!(
                endpoints_for(variant).account_base,
                TRAE_API_BASE_CN,
                "{} 的 account_base 漂了",
                variant.display_name()
            );
            assert_eq!(
                endpoints_for(variant).icube_base,
                TRAE_OAUTH_BASE_CN,
                "{} 的 icube_base 漂了",
                variant.display_name()
            );
        }
        assert_eq!(TRAE_CHECKIN_PATH, "/trae/api/v2/ug/checkin_credits/claim");
        assert_eq!(
            TRAE_CHECKIN_STATUS_PATH,
            "/trae/api/v2/ug/checkin_credits/status"
        );
    }
}
