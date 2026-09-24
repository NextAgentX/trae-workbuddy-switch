//! Trae 数据目录与文件路径。
//!
//! 全部路径由 [`crate::modules::config::store_dir`] 派生，因此自动继承
//! `BUDDY_SWITCH_HOME` 覆盖，测试里可用隔离目录重定向。
//!
//! ## ★ 按产品线变体分家（2026-09-18 起）
//!
//! Trae 有两条**可同机并存、账号互不相通**的产品线（见 [`super::variant`]）。
//! 它们各有独立的客户端、独立的 userData、**独立的登录凭据**，因此账号库也必须分家 ——
//! 否则会出现「Trae CN 的账号被写进 Trae Work 的账号库」，两个产品线互相污染。
//! 这与 WorkBuddy 侧 [`crate::modules::region::accounts_file_for`] 的分家动机完全同源。
//!
//! **命名规则（沿用 `region.rs` 的既有惯例）**：
//!
//! - **`TraeWork`（默认变体）沿用无后缀的旧文件名** —— 老用户既有数据零失效；
//! - **`Trae` 加 `.trae_cn` 中缀**，如 `checkin_accounts.trae_cn.json`。
//!
//! ⇒ 插入位置在**扩展名之前**，不是简单追加。这样文件名仍是 `.json` 结尾，
//! 与「同名不同目录」的旧约定不冲突，也让用户一眼能看出归属。
//!
//! 布局（`~/.buddy-switch/trae/`，括号内为 `Trae` 的额外后缀）：
//!
//! ```text
//! trae/
//! ├── settings.json                    # Trae 模块设置（端口、客户端路径、域名白名单…）※不分家
//! ├── checkin_accounts(.trae_cn).json  # 账号库（参考实现格式，UserID + jwt + refresh_token）
//! ├── groups(.trae_cn).json            # 分组与成员关系
//! ├── device_map(.trae_cn).json        # user_id -> 伪设备标识（与代理脚本共用）
//! ├── credits_history(.trae_cn).json   # 签到明细（按日期追加）
//! ├── credits_daily(.trae_cn).json     # 每日积分快照（趋势图数据源）
//! ├── remaining_credits(.trae_cn).json # 剩余积分与到期时间缓存
//! ├── account_cooldowns(.trae_cn).json # 签到错误冷却状态
//! ├── checkin_summary(.trae_cn).json   # 最近一次签到摘要
//! ├── checkin_ledger(.trae_cn).json    # 今日已签到台账（跨运行累积，按 userId）
//! ├── api_pool.json                    # API 网关账号池配置（按变体分目录，见下）
//! ├── profiles/<user_id>/              # 登录态快照（精准复制的 9 类核心文件）
//! └── logs/                            # app / checkin / switcher / proxy 日志
//! ```
//!
//! ## 哪些**刻意不分家**
//!
//! - **`settings.json`**：端口、域名白名单、`traePath` 这类**应用级**配置，
//!   描述的是「本工具怎么工作」，不是「哪条产品线的数据」。两条产品线共用一份，
//!   否则用户在设置页改一次要改两遍。
//! - **`api_gateway.json` / `api_gateway_logs.json` / `api_pool.json`**：
//!   网关是**单一进程、单一监听端口**（7864），一次只能服务一个账号池。
//!   ⇒ 网关配置保持全局单份（与「网关是平行第二套实现」的既有设计一致）。
//!   `api_pool.json` 里若含按变体区分的账号引用，由**内容**区分，不由文件区分。
//! - **`logs/`**：日志按**文件名前缀**区分而非目录，便于在同一个 `logs/` 里对照排查。

use std::path::PathBuf;

use crate::modules::config::store_dir;

use super::region::{TraeProgram, TraeRegion};
use super::variant::TraeVariant;

/// 拼出**该变体所属区域**的数据文件路径（区域级文件族的唯一入口）。
///
/// ## 为什么按 `variant.region()` 而不是按 `variant` 本身分文件
///
/// 改造前这族函数按「**产品线**」分文件；现在账号/签到/积分/冷却这些数据按
/// **区域**分家（两套互不相通的账号体系）。
///
/// 关键事实：改造前的两个产品线（`TraeWork` / `Trae`）**都属于国内区域**
/// ⇒ 它们必须落到**同一个文件**。合并迁移（[`super::region_migrate`]）负责把
/// `.trae_cn` 里的内容并进国内库，于是两个旧取值读写同一本库。
///
/// ⚠️ **不要**改回按 `variant` 分文件：合并后的库会被读成两半
/// （一边有账号、另一边空），写入还会互相覆盖 —— 这正是本次要消除的缺陷。
fn scoped_file(name_with_ext: &str, variant: TraeVariant) -> PathBuf {
    scoped_file_for_region(name_with_ext, variant.region())
}

// ---------------------------------------------------------------------------
// 区域轴（持久化轴）：账号库 / 签到 / 积分 / 冷却 / 快照 的**目标**命名
// ---------------------------------------------------------------------------
//
// 上面那族 `*_for(variant)` 是**改造前的产品线命名**，仍在线上运行；下面这族是
// 区域轴的目标命名。两族并存是**过渡态**：调用点逐段切换，切换完成后上面那族删除。
//
// ## 命名规则（与 WorkBuddy 的 region 后缀惯例一致）
//
// - **`Cn`（默认区域）沿用无后缀的旧文件名** —— 老用户既有数据零失效，且
//   账号主库（`checkin_accounts.json`）原地不动，不需要迁移；
// - **`Global` 加 `.global` 中缀**，如 `checkin_accounts.global.json`。
//   （与 `~/.buddy-switch/accounts.global.json` 同款约定，一眼能认出是国际版。）
//
// ## 为什么账号库按**区域**分家而不是按程序分家
//
// CN 与国际是两套**互不相通**的账号体系（JWT 不通用、端点不同），而同一区域内的
// 两条程序（TraeWork / TraeCode）共用同一套账号 —— 同一个 Trae 账号可以分别启用
// 在国内的两条程序上。因此账号库的归属维度是区域；「启用在哪条程序上」由**登录态
// 快照目录**表达（见下）。

/// 为文件名插入**区域**中缀。
///
/// `Cn` 是默认区域 → 原样返回；其余在**扩展名之前**插入 `.<region>`，
/// 无扩展名时直接追加（不产生悬空点），规则与 [`variant_scoped_file_name`] 同构。
fn region_scoped_file_name(stem_with_ext: &str, region: TraeRegion) -> String {
    if region == TraeRegion::default() {
        return stem_with_ext.to_string();
    }
    match stem_with_ext.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}.{}.{ext}", region.as_str()),
        None => format!("{stem_with_ext}.{}", region.as_str()),
    }
}

/// 拼出某一区域的数据文件路径。
fn scoped_file_for_region(name_with_ext: &str, region: TraeRegion) -> PathBuf {
    trae_dir().join(region_scoped_file_name(name_with_ext, region))
}

/// 账号库文件（按区域分家）。
pub fn accounts_file_for_region(region: TraeRegion) -> PathBuf {
    scoped_file_for_region("checkin_accounts.json", region)
}

/// 分组文件（按区域分家）。
pub fn groups_file_for_region(region: TraeRegion) -> PathBuf {
    scoped_file_for_region("groups.json", region)
}

/// 设备标识映射文件（按区域分家）。
pub fn device_map_file_for_region(region: TraeRegion) -> PathBuf {
    scoped_file_for_region("device_map.json", region)
}

/// OAuth 登录设备身份文件（按区域分家）。
pub fn oauth_device_file_for_region(region: TraeRegion) -> PathBuf {
    scoped_file_for_region("oauth_device.json", region)
}

/// 签到积分明细文件（按区域分家）。
pub fn credits_history_file_for_region(region: TraeRegion) -> PathBuf {
    scoped_file_for_region("credits_history.json", region)
}

/// 每日积分快照文件（按区域分家）。
pub fn credits_daily_file_for_region(region: TraeRegion) -> PathBuf {
    scoped_file_for_region("credits_daily.json", region)
}

/// 剩余积分缓存文件（按区域分家）。
pub fn remaining_credits_file_for_region(region: TraeRegion) -> PathBuf {
    scoped_file_for_region("remaining_credits.json", region)
}

/// 账号冷却状态文件（按区域分家）。
pub fn cooldowns_file_for_region(region: TraeRegion) -> PathBuf {
    scoped_file_for_region("account_cooldowns.json", region)
}

/// 最近一次签到摘要文件（按区域分家）。
pub fn checkin_summary_file_for_region(region: TraeRegion) -> PathBuf {
    scoped_file_for_region("checkin_summary.json", region)
}

/// 按区域取日志文件路径（默认区域不加中缀）。
///
/// 例：`Global` + `checkin.log` → `checkin.global.log`。
/// 历史上按产品线分家的 `checkin.trae_cn.log` **不再写新内容**，只作为历史保留
/// （诊断日志不改写旧文件是刻意的：翻旧账时原文比"整理过的"更有用）。
pub fn log_file_for_region(region: TraeRegion, name: &str) -> PathBuf {
    log_file(&region_scoped_file_name(name, region))
}

/// 登录态快照根目录（**按区域 × 程序**分家）。
///
/// ## 为什么快照比账号库多一个维度
///
/// 账号库的归属是区域（账号属于哪套账号体系），但快照是**某个客户端 userData 的
/// 文件副本** —— 它只能被恢复到「采集它的那条程序」里去。把 CN TraeWork 的快照灌进
/// TraeCode，等于把一个未知格式的登录态写进另一个客户端。因此快照必须按程序分家。
///
/// ## 目录名规则（**保住历史目录，零迁移**）
///
/// 改造前的两个目录名**原样保留**（它们各自恰好就是新模型里的一个程序位）：
///
/// | 区域 | 程序 | 目录名 | 来源 |
/// |:---|:---|:---|:---|
/// | `Cn` | `TraeWork` | `profiles` | 旧默认变体的目录名 |
/// | `Cn` | `TraeCode` | `profiles_trae_cn` | 旧 `Trae` 变体的目录名 |
/// | `Global` | `TraeWork` | `profiles_global_trae_work` | 新 |
/// | `Global` | `TraeCode` | `profiles_global_trae_code` | 新 |
///
/// 新增槽位一律用 `<region>_<program>` 规则；两个历史名当**兼容别名**保留，
/// 因为改名要搬动用户既有快照，而收益只是"整齐"，不值这个风险。
pub fn profiles_dir_for_program(region: TraeRegion, program: TraeProgram) -> PathBuf {
    let name = match (region, program) {
        // 历史名（零迁移）：与改造前 `profiles` / `profiles_trae_cn` 逐字相同。
        (TraeRegion::Cn, TraeProgram::TraeWork) => "profiles".to_string(),
        (TraeRegion::Cn, TraeProgram::TraeCode) => "profiles_trae_cn".to_string(),
        // 新槽位：`<region>_<program>`。
        (region, program) => format!("profiles_{}_{}", region.as_str(), program.as_str()),
    };
    ensured(trae_dir().join(name))
}

/// 单个账号在某个程序上的登录态快照目录。
///
/// `user_id` 直接拼进路径，必须先用 [`safe_slot_name`] 过滤掉路径分隔符与上跳片段。
pub fn profile_dir_for_program(
    region: TraeRegion,
    program: TraeProgram,
    user_id: &str,
) -> Option<PathBuf> {
    if !safe_slot_name(user_id) {
        return None;
    }
    Some(profiles_dir_for_program(region, program).join(user_id))
}

/// Trae 模块根目录：`~/.buddy-switch/trae`。
///
/// 与 WorkBuddy 数据同库不同名：复用 `store_dir()` 的 home 覆盖与兼容回落，
/// 但放在独立子目录，避免 `accounts.json` / `checkin_logs.json` 这类既有文件撞名。
pub fn trae_dir() -> PathBuf {
    store_dir().join("trae")
}

/// 确保目录存在后返回；`create_dir_all` 失败时仍返回路径（与仓库既有 `path()` 一致，
/// 由实际写入暴露错误，而不是在取路径阶段就失败）。
fn ensured(dir: PathBuf) -> PathBuf {
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Trae 模块设置文件。
///
/// **刻意不分变体**：描述的是「本工具怎么工作」（端口、白名单、客户端路径），
/// 不是某条产品线的数据。两条产品线共用一份，用户只需配置一次。
pub fn settings_file() -> PathBuf {
    trae_dir().join("settings.json")
}

/// 账号库文件（默认变体，兼容壳）。
pub fn accounts_file() -> PathBuf {
    accounts_file_for(TraeVariant::default())
}

/// 账号库文件（按变体分家）。
///
/// 这是「两条产品线账号互不污染」的**第一道闸门**：写错了这里，
/// 一个产品线的签到会把账号灌进另一个产品线的库。
pub fn accounts_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("checkin_accounts.json", variant)
}

/// 分组文件（默认变体，兼容壳）。
pub fn groups_file() -> PathBuf {
    groups_file_for(TraeVariant::default())
}

/// 分组文件（按变体分家）。
pub fn groups_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("groups.json", variant)
}

/// 设备标识映射文件（默认变体，兼容壳）。
pub fn device_map_file() -> PathBuf {
    device_map_file_for(TraeVariant::default())
}

/// 设备标识映射文件（按变体分家；签到与代理共用，结构必须一致）。
///
/// 分家的另一个理由：伪设备标识与账号绑定，两条产品线的 user_id 空间不同。
pub fn device_map_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("device_map.json", variant)
}

/// OAuth 登录设备身份文件（默认变体，兼容壳）。
///
/// 与 [`device_map_file`] 同构：落在 `trae/` 根、**不按变体分目录**，
/// 只按变体改文件名（`oauth_device(.trae_cn).json`）。
pub fn oauth_device_file() -> PathBuf {
    oauth_device_file_for(TraeVariant::default())
}

/// OAuth 登录设备身份文件（按变体分家）。
///
/// 存的是**授权 URL 用的身份 A2**：只有 `machine_id` 一个键（本机自造、变体级稳定）。
///
/// **`device_id` 不在这里**（曾经在，已移出）：授权 URL 的 `device_id` 是身份 A1，
/// 必须与 icube 设备凭证**同源**（= 签名私钥所属的那个 `icube-dc` deviceId），
/// 由 [`crate::modules::trae::icube::device_identity_for`] 提供 —— 否则服务端 20403/20405。
/// 自造 `device_id` 的能力在**类型层面**就不存在（见 `OAuthLoginMachine`）。
///
/// 与 `device_map.json`（身份 C，账号级）仍然不同源。
/// 分家的硬理由同 [`device_map_file_for`]：两条产品线的身份空间不同，
/// 混用不会报错、只会让上游把它们当成两个设备。
pub fn oauth_device_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("oauth_device.json", variant)
}

/// OAuth 客户端凭证外置配置文件：`trae/conf/oauth_client.json`。
///
/// **刻意不分变体**：`client_id` / `client_secret` / 交换路径是「客户端身份」，
/// 两条 CN 产品线实测共用同一套（见 `oauth_client` 模块文档）。
pub fn oauth_client_config_file() -> PathBuf {
    trae_dir().join("conf").join("oauth_client.json")
}

/// 某个快照槽位的**单代回滚目录**：`profiles[_<variant>]/<slot>.bak`。
///
/// 备份时先把旧槽位整体 rename 到这里，再拷新内容 —— 拷贝中断时上一份快照仍在，
/// 用户不会两头落空。返回 `None` 表示 `slot` 不是安全的目录名（含路径分隔符等）。
pub fn profile_bak_dir_for(variant: TraeVariant, slot: &str) -> Option<PathBuf> {
    if !safe_slot_name(slot) {
        return None;
    }
    Some(profiles_dir_for(variant).join(format!("{slot}.bak")))
}

/// 签到积分明细文件（默认变体，兼容壳）。
pub fn credits_history_file() -> PathBuf {
    credits_history_file_for(TraeVariant::default())
}

/// 签到积分明细文件（按变体分家）。
pub fn credits_history_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("credits_history.json", variant)
}

/// 每日积分快照文件（默认变体，兼容壳）。
pub fn credits_daily_file() -> PathBuf {
    credits_daily_file_for(TraeVariant::default())
}

/// 每日积分快照文件（按变体分家）。
pub fn credits_daily_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("credits_daily.json", variant)
}

/// 剩余积分缓存文件（默认变体，兼容壳）。
pub fn remaining_credits_file() -> PathBuf {
    remaining_credits_file_for(TraeVariant::default())
}

/// 剩余积分缓存文件（按变体分家）。
pub fn remaining_credits_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("remaining_credits.json", variant)
}

/// 账号冷却状态文件（默认变体，兼容壳）。
pub fn cooldowns_file() -> PathBuf {
    cooldowns_file_for(TraeVariant::default())
}

/// 账号冷却状态文件（按变体分家）。
///
/// 分家的硬理由：冷却表以 `user_id` 为键，两条产品线的 user_id 空间不重叠但**格式相同**，
/// 混在一起不会报错、只会让冷却状态错乱 —— 典型的静默缺陷。
pub fn cooldowns_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("account_cooldowns.json", variant)
}

/// 最近一次签到摘要文件（默认变体，兼容壳）。
pub fn checkin_summary_file() -> PathBuf {
    checkin_summary_file_for(TraeVariant::default())
}

/// 最近一次签到摘要文件（按变体分家）。
pub fn checkin_summary_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("checkin_summary.json", variant)
}

/// 今日已签到台账文件（默认变体，兼容壳）。
pub fn checkin_ledger_file() -> PathBuf {
    checkin_ledger_file_for(TraeVariant::default())
}

/// 今日已签到台账文件（按变体分家）。
///
/// **与 [`checkin_summary_file_for`] 是两份不同的文件，不要合并**：
/// - `checkin_summary` 是「**最近一次运行**的结果」，每轮整体覆盖，供界面展示；
/// - `checkin_ledger` 是「**今天哪些账号已经签到了**」，跨运行累积、按 `userId` 记，
///   供 `skip_checked_in` 与账号卡的「已签到」徽章判定。
///
/// 曾经用前者兼作后者的数据源（按 `name` 匹配），后果是：一轮只处理部分账号时
/// 摘要被覆盖 ⇒ 上一轮签过的账号**丢掉**已签到标记，徽章显示错、下一轮还会重复
/// 探测同一个账号。见 [`super::credits::CheckinLedger`]。
pub fn checkin_ledger_file_for(variant: TraeVariant) -> PathBuf {
    scoped_file("checkin_ledger.json", variant)
}

/// API 网关账号池配置文件。
///
/// **刻意不分变体**：网关是单一进程、单一监听端口（7864），一次只能服务一个账号池。
/// 池内若同时含两条产品线的账号，由**条目内容**区分归属，不由文件区分。
pub fn api_pool_file() -> PathBuf {
    trae_dir().join("api_pool.json")
}

/// API 网关运行配置（开关、监听地址/端口、日志保留、默认模型）。
pub fn api_gateway_file() -> PathBuf {
    trae_dir().join("api_gateway.json")
}

/// API 网关请求日志（JSON 数组，与 `logs/api.log` 的纯文本诊断日志分开）。
pub fn api_gateway_log_file() -> PathBuf {
    trae_dir().join("api_gateway_logs.json")
}

/// API 网关的多 Key 库（数组；**只存 sha256 哈希 + 前缀**，明文仅创建时返回一次）。
///
/// **刻意不分变体**：网关是单一进程、单一监听端口（7864），
/// 与 [`api_gateway_file`] / [`api_gateway_log_file`] 同理。
/// 单条 Key **自带** `variant`（归属产品线）字段决定它使用哪个账号池，
/// 因此归属由**记录内容**表达，不由文件名表达。
pub fn api_gateway_keys_file() -> PathBuf {
    trae_dir().join("api_gateway_keys.json")
}

/// 登录态快照根目录（默认变体，兼容壳）。
pub fn profiles_dir() -> PathBuf {
    profiles_dir_for(TraeVariant::default())
}

/// 登录态快照根目录（按变体分家）。
///
/// **必须分家**：快照目录名就是 `user_id`，而两条产品线的 `user_id` 是两套空间。
/// 更关键的是——快照恢复会把文件写回**该变体客户端**的 userData，
/// 若把 Trae CN 账号的快照误用于 Trae Work，等于把一个未知格式的登录态灌进另一个客户端。
pub fn profiles_dir_for(variant: TraeVariant) -> PathBuf {
    // 默认变体沿用旧目录名 `profiles`（老用户的既有快照仍可用）；
    // 其余变体用 `profiles_trae_cn` 这样的独立目录。
    let name = if variant == TraeVariant::default() {
        "profiles".to_string()
    } else {
        format!("profiles_{}", variant.as_str())
    };
    ensured(trae_dir().join(name))
}

/// 单个账号的登录态快照目录（默认变体，兼容壳）。
pub fn profile_dir(user_id: &str) -> Option<PathBuf> {
    profile_dir_for(TraeVariant::default(), user_id)
}

/// 单个账号的登录态快照目录（按变体分家）。
///
/// `user_id` 直接拼进路径，必须先过滤掉路径分隔符与上跳片段，否则一个形如
/// `../../` 的 userId 会把快照写到数据目录之外。
pub fn profile_dir_for(variant: TraeVariant, user_id: &str) -> Option<PathBuf> {
    if !safe_slot_name(user_id) {
        return None;
    }
    Some(profiles_dir_for(variant).join(user_id))
}

/// 校验快照槽位名（userId）是否可作为单个目录名使用。
///
/// 拒绝：空、`.`、`..`、含 `/` 或 `\`、含 Windows 保留字符、含 NUL。
pub fn safe_slot_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    !name
        .chars()
        .any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c == '\0')
}

/// 日志目录。
///
/// **刻意不分变体**：日志按**文件名前缀**区分（见 [`log_file_for`]），
/// 让两条产品线的日志并排放在同一个 `logs/` 里，排查跨产品线问题时不必切目录。
pub fn logs_dir() -> PathBuf {
    ensured(trae_dir().join("logs"))
}

/// 单一日志文件路径。
pub fn log_file(name: &str) -> PathBuf {
    logs_dir().join(name)
}

/// 按变体取日志文件路径：在文件名前加变体前缀（默认变体不加）。
///
/// 例：`Trae` + `checkin.log` → `checkin.trae_cn.log`。
/// **未在文件名里出现 `.log` 时退化为直接追加**，不会丢扩展名。
pub fn log_file_for(variant: TraeVariant, name: &str) -> PathBuf {
    // 与 `scoped_file` 同源：按**区域**分日志。国内两条程序共用同一套账号与
    // 同一份签到/冷却，日志分开写只会让排查一个账号的问题要在两份日志里对照，
    // 没有信息增益；国际版则必须单独一份（两套账号体系）。
    log_file_for_region(variant.region(), name)
}

/// 应用级日志（切换、托盘、启动等关键路径）。
///
/// **不分变体**：这是「本应用自己」的日志，不归属任何产品线。
pub fn app_log_file() -> PathBuf {
    log_file("app.log")
}

/// 签到日志（默认变体，兼容壳）。
pub fn checkin_log_file() -> PathBuf {
    checkin_log_file_for(TraeVariant::default())
}

/// 签到日志（按变体分家）。
pub fn checkin_log_file_for(variant: TraeVariant) -> PathBuf {
    log_file_for(variant, "checkin.log")
}

/// 登录态切换日志（默认变体，兼容壳）。
pub fn switcher_log_file() -> PathBuf {
    switcher_log_file_for(TraeVariant::default())
}

/// 登录态切换日志（按变体分家）。
pub fn switcher_log_file_for(variant: TraeVariant) -> PathBuf {
    log_file_for(variant, "switcher.log")
}

/// 代理日志（默认变体，兼容壳）。
pub fn proxy_log_file() -> PathBuf {
    proxy_log_file_for(TraeVariant::default())
}

/// 代理日志（按变体分家）。
pub fn proxy_log_file_for(variant: TraeVariant) -> PathBuf {
    log_file_for(variant, "proxy.log")
}

/// 网关请求日志。
///
/// **不分变体**：网关只有一套（单一进程、单一端口）。
pub fn api_log_file() -> PathBuf {
    log_file("api.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trae_paths_live_under_store_dir_trae() {
        // 相对断言：不依赖真实 HOME，只验证层级关系与命名空间隔离。
        let base = store_dir();
        let dir = trae_dir();
        assert!(dir.starts_with(&base), "{dir:?} 应位于 {base:?} 之下");
        assert_eq!(dir.file_name().and_then(|s| s.to_str()), Some("trae"));
    }

    #[test]
    fn trae_files_do_not_collide_with_workbuddy_files() {
        // 本用例要断言「同一函数族返回的路径彼此一致」，而它们各自**独立**读取
        // 进程级的 `BUDDY_SWITCH_HOME`。若有并发测试在中途改掉该变量，
        // 就会出现「左边临时目录、右边真实目录」的假失败 —— 所以先取 env 锁，
        // 让本用例与所有改 home 的测试互斥（见 `config::env_lock`）。
        let _lock = crate::modules::config::env_lock();

        // 关键护栏：Trae 与 WorkBuddy 的账号/签到文件同名会互相覆盖。
        let wb_accounts = crate::modules::config::accounts_file();
        assert_ne!(accounts_file(), wb_accounts);
        assert_ne!(trae_dir(), store_dir());
        // 都在 trae/ 子目录内，而非 store_dir 根。
        for path in [
            accounts_file(),
            groups_file(),
            device_map_file(),
            credits_history_file(),
            credits_daily_file(),
            remaining_credits_file(),
            cooldowns_file(),
            checkin_summary_file(),
            api_pool_file(),
            api_gateway_file(),
            api_gateway_keys_file(),
            api_gateway_log_file(),
            settings_file(),
        ] {
            assert_eq!(path.parent(), Some(trae_dir().as_path()), "{path:?}");
        }
    }

    #[test]
    fn safe_slot_name_rejects_traversal_and_separators() {
        assert!(safe_slot_name("1234567890123456"));
        assert!(safe_slot_name("user_abc-1"));
        assert!(!safe_slot_name(""));
        assert!(!safe_slot_name("."));
        assert!(!safe_slot_name(".."));
        assert!(!safe_slot_name("../../etc/passwd"));
        assert!(!safe_slot_name("a/b"));
        assert!(!safe_slot_name("a\\b"));
        assert!(!safe_slot_name("C:evil"));
        assert!(!safe_slot_name("a*b"));
    }

    #[test]
    fn profile_dir_rejects_unsafe_user_id() {
        assert!(profile_dir("1234567890").is_some());
        assert!(profile_dir("../escape").is_none());
        assert!(profile_dir("").is_none());
        assert!(profile_dir_for(TraeVariant::Global, "1234567890").is_some());
        assert!(profile_dir_for(TraeVariant::Global, "../escape").is_none());
    }

    /// 默认变体**必须沿用旧文件名**，否则老用户数据白失效。
    ///
    /// 这条断言是本次分家改造的「零回归」承诺：TraeWork 是 `Default`，
    /// 它的路径必须与改造前**逐字相同**。改这里等于改兼容性契约。
    ///
    /// ## 为什么这里与 `trae_files_do_not_collide_with_workbuddy_files` 一样要取 env 锁
    ///
    /// 这些函数是**无参全局路径**，每次调用都重新读进程级 `BUDDY_SWITCH_HOME`。
    /// lib 单测在**同一进程内并行**跑，只要有别的用例（如 `handlers.rs` 里那两条
    /// 各自设不同临时目录的签到选项用例）中途改掉该变量并短暂恢复，
    /// 同一条 `assert_eq!` 的左右两侧就会取到不同的值 —— 症状是
    /// 「左右目录名差一个后缀」这种看起来像竞态、实则是**跨用例状态泄漏**的失败。
    ///
    /// 修法不是"给断言加容错"，而是让本用例与所有改 home 的用例互斥（取 env 锁）。
    #[test]
    fn 默认变体沿用旧文件名() {
        let _lock = crate::modules::config::env_lock();

        assert_eq!(accounts_file(), trae_dir().join("checkin_accounts.json"));
        assert_eq!(groups_file(), trae_dir().join("groups.json"));
        assert_eq!(device_map_file(), trae_dir().join("device_map.json"));
        assert_eq!(
            credits_history_file(),
            trae_dir().join("credits_history.json")
        );
        assert_eq!(profiles_dir(), trae_dir().join("profiles"));

        // 兼容壳 == 显式传默认变体。
        assert_eq!(accounts_file(), accounts_file_for(TraeVariant::default()));
        assert_eq!(settings_file(), trae_dir().join("settings.json"));
    }

    /// ★ 同区域的两条程序**刻意共用**同一套数据文件与日志，但**快照目录必须按程序分家**。
    ///
    /// ## 为什么「共用」才是对的（这一条极容易改反）
    ///
    /// 账号库 / 签到 / 积分 / 冷却 / 日志的归属维度是**区域**，不是程序：CN 与国际是两套
    /// **互不相通**的账号体系，而同一区域内 TraeWork / TraeCode **共用同一套账号**
    /// （同一个 Trae 账号可以分别启用在国内的两条程序上）。`scoped_file(_, variant)`
    /// 因此直接转发到 `variant.region()`（见本文件顶部「为什么账号库按区域分家而不是
    /// 按程序分家」）。⇒ **同区域两条程序指向同一个文件是设计，不是缺陷。**
    ///
    /// 反过来，**快照必须按程序分家**：快照是某个客户端 userData 的文件副本，只能恢复到
    /// 采集它的那条程序里去（见 [`profiles_dir_for`] 那节）。
    ///
    /// ## 本用例为什么改过名
    ///
    /// 原名 `两条产品线的数据文件互不相同` 断言的是**改造前**的产品线语义。轴翻到区域之后
    /// 那条断言不再成立，原用例却靠把 `cn` 换成 `Global` 一直假绿 —— 于是名字、文档与代码
    /// 三处互相矛盾，而它声称要防的那种污染**在设计上不可能发生**（详见 2026-09-21 日志）。
    /// 现在改成断言**真实不变式**，并显式覆盖「同区域共用」这个此前无人断言的方向。
    #[test]
    fn 同区域两条程序共用数据文件而快照按程序分家() {
        // 同 [`Self::两个区域的数据文件互不相同`]：下面每一对都各自**独立**读取进程级
        // `BUDDY_SWITCH_HOME`，并发用例中途换 home 会让两侧取到不同根目录 ⇒ 整段持 env 锁。
        let _lock = crate::modules::config::env_lock();

        let work = TraeVariant::TraeWork; // 国内 · 默认变体
        let code = TraeVariant::Trae; // 国内 · 另一条程序（Trae CN）
        let global = TraeVariant::Global; // 国际版（另一个区域）

        // 1) 数据文件与日志：同区域的两条程序**必须相同**（刻意共用同一套账号）。
        let shared: [(&str, PathBuf, PathBuf); 9] = [
            ("账号库", accounts_file_for(work), accounts_file_for(code)),
            ("分组", groups_file_for(work), groups_file_for(code)),
            ("设备映射", device_map_file_for(work), device_map_file_for(code)),
            (
                "签到明细",
                credits_history_file_for(work),
                credits_history_file_for(code),
            ),
            (
                "每日积分",
                credits_daily_file_for(work),
                credits_daily_file_for(code),
            ),
            (
                "剩余积分",
                remaining_credits_file_for(work),
                remaining_credits_file_for(code),
            ),
            ("冷却状态", cooldowns_file_for(work), cooldowns_file_for(code)),
            (
                "签到摘要",
                checkin_summary_file_for(work),
                checkin_summary_file_for(code),
            ),
            (
                "签到日志",
                checkin_log_file_for(work),
                checkin_log_file_for(code),
            ),
        ];
        for (label, work_path, code_path) in shared {
            assert_eq!(
                work_path, code_path,
                "{label} 应被同区域两条程序共用，却分家了: {work_path:?} vs {code_path:?}"
            );
        }

        // 2) 但**跨区域**必须不同 —— 否则两套互不相通的账号体系会混库。
        //    （与 `两个区域的数据文件互不相同` 互补：那条走 `*_for_region` 入口，这条走 `*_for` 入口。）
        assert_ne!(
            accounts_file_for(work),
            accounts_file_for(global),
            "账号库在跨区域时不得共用"
        );

        // 3) 快照目录按**程序**分家 —— 把 CN TraeWork 的快照灌进 TraeCode，等于把一个
        //    未知格式的登录态写进另一个客户端。
        assert_ne!(
            profiles_dir_for(work),
            profiles_dir_for(code),
            "快照目录被两条程序共用"
        );
    }

    /// 变体后缀必须插在**扩展名之前**，而不是简单追加。
    ///
    /// 若写反成 `checkin_accounts.json.trae_cn`，文件不再是 `.json` 结尾，
    /// 任何按扩展名过滤的逻辑（备份、清理、用户肉眼辨认）都会失效。
    ///
    /// 本用例只断言 **basename**（`file_name()`），不碰绝对路径 ——
    /// 这样它天然不受「别的用例改了 HOME」影响，**不需要** env 锁。
    /// 这是更可取的写法：能只看文件名就别看全路径。
    #[test]
    fn 变体后缀插在扩展名之前() {
        let cn = accounts_file_for(TraeVariant::Global);
        let name = cn.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        assert_eq!(name, "checkin_accounts.global.json");

        let log = checkin_log_file_for(TraeVariant::Global);
        let log_name = log.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        assert_eq!(log_name, "checkin.global.log");

        // 分家文件与默认变体同目录（只改文件名，不改目录层级）。
        // 用 basename 比较父级层级数，而不是比较绝对路径。
        assert_eq!(
            cn.parent().map(|p| p.file_name()),
            accounts_file_for(TraeVariant::TraeWork)
                .parent()
                .map(|p| p.file_name()),
            "两条产品线的数据文件必须落在同一个 trae/ 目录下"
        );
    }

    /// 刻意**不分家**的文件：应用级设置与网关配置。
    ///
    /// 这些是「本工具怎么工作」的配置，不是某条产品线的数据。
    /// 若有人"顺手"给它们也加了变体后缀，这条会红 —— 那是提醒他先想清楚。
    ///
    /// 同样要取 env 锁：断言里同时出现"无参全局路径"与"基于 `trae_dir()` 的期望值"，
    /// 二者若在两次读之间被别的用例改走 HOME 就会假失败。
    #[test]
    fn 应用级配置与网关配置刻意不分家() {
        let _lock = crate::modules::config::env_lock();

        // 只有无参版本，没有 `_for` 变体 —— 这是刻意的。
        assert_eq!(settings_file(), trae_dir().join("settings.json"));
        assert_eq!(api_pool_file(), trae_dir().join("api_pool.json"));
        assert_eq!(api_gateway_file(), trae_dir().join("api_gateway.json"));
        assert_eq!(
            api_gateway_keys_file(),
            trae_dir().join("api_gateway_keys.json")
        );
        assert_eq!(
            api_gateway_log_file(),
            trae_dir().join("api_gateway_logs.json")
        );
        assert_eq!(app_log_file(), logs_dir().join("app.log"));
        assert_eq!(api_log_file(), logs_dir().join("api.log"));
    }

    /// 无扩展名的输入不能产生悬空点（`foo.global.`）—— 区域族同样要守这条。
    #[test]
    fn 无扩展名输入不产生悬空点() {
        assert_eq!(
            region_scoped_file_name("bare", TraeRegion::Global),
            "bare.global"
        );
        assert_eq!(
            region_scoped_file_name("a.b.json", TraeRegion::Global),
            "a.b.global.json"
        );
        // 默认区域原样返回。
        assert_eq!(
            region_scoped_file_name("checkin_accounts.json", TraeRegion::Cn),
            "checkin_accounts.json"
        );
    }

    /// ★ OAuth 设备身份文件：默认变体沿用无后缀名，另一变体加中缀；两者必须不同。
    ///
    /// 大多数断言只比 basename（不受「别的用例改了 HOME」影响），但**最后一条**
    /// `oauth_device_file() == oauth_device_file_for(TraeWork)` 比较的是**绝对路径**：
    /// 无参壳与显式变体两次读取之间若被别的用例改走 HOME，就会拿到真机 home 与
    /// 临时 home 两条不同路径而**假失败**（实测 25 轮全量并行中 1 轮，
    /// `left: …\.buddy-switch\trae\oauth_device.json` /
    /// `right: …\Temp\buddy-switch-trae-test-…\home\.buddy-switch\…`）。
    /// 故与 [`Self::应用级配置与网关配置刻意不分家`] 同款处置：整段持 `env_lock()`。
    #[test]
    fn oauth设备身份文件按变体分家且默认沿用旧名() {
        let _lock = crate::modules::config::env_lock();

        let work = oauth_device_file_for(TraeVariant::TraeWork);
        let cn = oauth_device_file_for(TraeVariant::Global);
        assert_eq!(
            work.file_name().and_then(|s| s.to_str()),
            Some("oauth_device.json")
        );
        assert_eq!(
            cn.file_name().and_then(|s| s.to_str()),
            Some("oauth_device.global.json")
        );
        assert_ne!(work, cn, "两条产品线的 OAuth 设备身份必须分家");
        // 与 device_map 同目录（同构语义：只改文件名，不改目录层级）。
        assert_eq!(
            work.parent().map(|p| p.file_name()),
            device_map_file().parent().map(|p| p.file_name())
        );
        // 兼容壳 == 显式默认变体。
        assert_eq!(oauth_device_file(), work);
    }

    /// OAuth 客户端配置**刻意不分变体**（只有无参版本）。
    #[test]
    fn oauth客户端配置刻意不分变体() {
        let path = oauth_client_config_file();
        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some("oauth_client.json")
        );
        // 落在 trae/conf/ 下（不是 trae/ 根，避免与数据文件混在一起）。
        assert_eq!(
            path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()),
            Some("conf")
        );
    }

    /// 槽位回滚目录：`<slot>.bak`，且拒绝不安全的槽位名。
    ///
    /// 用 [`crate::modules::config::HomeOverrideGuard`] 隔离 home：
    /// `profiles_dir_for` 会 `create_dir_all`，不隔离就会在用户真实
    /// `~/.buddy-switch` 下建目录（测试纪律：不得写用户真实数据目录）。
    /// 该 guard 内部已取 `env_lock()`，因此本用例与其它改 home 的用例互斥。
    #[test]
    fn 槽位回滚目录为同目录单代备份() {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-profile-bak-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("临时 home 应能创建");
        let guard = crate::modules::config::HomeOverrideGuard::set(&dir);

        let slot = profile_dir("1234567890123456").expect("安全槽位名应有快照目录");
        let bak = profile_bak_dir_for(TraeVariant::TraeWork, "1234567890123456")
            .expect("安全槽位名应有 .bak 路径");
        assert_eq!(
            bak.file_name().and_then(|s| s.to_str()),
            Some("1234567890123456.bak")
        );
        // 与正式槽位同目录（单代轮转靠 rename，必须同目录同卷）。
        assert_eq!(bak.parent(), slot.parent(), ".bak 必须与槽位同目录");
        assert_eq!(
            bak.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()),
            Some("profiles"),
            "默认变体的槽位目录名必须沿用旧名 profiles"
        );
        // 另一变体落在自己的 profiles_<variant>/ 下，互不干扰。
        let cn_bak = profile_bak_dir_for(TraeVariant::Global, "1234567890123456").unwrap();
        assert_eq!(
            cn_bak.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()),
            Some("profiles_global")
        );
        assert_ne!(cn_bak.parent(), bak.parent());

        // 不安全的名字一律拒绝，绝不拼出越界路径。
        assert!(profile_bak_dir_for(TraeVariant::TraeWork, "../escape").is_none());
        assert!(profile_bak_dir_for(TraeVariant::TraeWork, "").is_none());
        assert!(profile_bak_dir_for(TraeVariant::Global, "a/b").is_none());

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 区域文件名规则：`Cn` 沿用旧名（零迁移），`Global` 加 `.global` 中缀。
    ///
    /// 反例：若把中缀插在扩展名之后（`checkin_accounts.json.global`），文件就不再是
    /// `.json` 结尾，备份/清理/用户肉眼辨认全部失效 —— 与产品线那族同款护栏。
    #[test]
    fn 区域文件后缀插在扩展名之前() {
        assert_eq!(
            region_scoped_file_name("checkin_accounts.json", TraeRegion::Cn),
            "checkin_accounts.json"
        );
        assert_eq!(
            region_scoped_file_name("checkin_accounts.json", TraeRegion::Global),
            "checkin_accounts.global.json"
        );
        // 无扩展名不产生悬空点。
        assert_eq!(
            region_scoped_file_name("bare", TraeRegion::Global),
            "bare.global"
        );
        // 日志同理：`checkin.log` → `checkin.global.log`。
        assert_eq!(
            region_scoped_file_name("checkin.log", TraeRegion::Global),
            "checkin.global.log"
        );
    }

    /// ★ 两个区域的**每一个**数据文件都不能同路径。
    ///
    /// 若这条红了，说明国际版与国内版共用了一个文件 —— 后果是「国际版的签到把账号
    /// 灌进国内版的库」这类静默污染（两套账号体系互不相通，混库后必然对不上）。
    #[test]
    fn 两个区域的数据文件互不相同() {
        // 本用例要断言「同一函数族返回的路径彼此一致」，而它们各自**独立**读取
        // 进程级的 `BUDDY_SWITCH_HOME`。lib 单测在**同一进程里并行跑**，若有并发用例
        // 在中途换掉 home，同一条 `assert_eq!` 的两侧就会取到不同根目录
        // （现场：左侧真实 home、右侧 Temp home ⇒ 单跑必绿、全量才红的假失败）。
        // 与 `trae_files_do_not_collide_with_workbuddy_files` 同法：持 env 锁，
        // 让本用例与所有改 home 的测试互斥。
        let _lock = crate::modules::config::env_lock();

        let cn = TraeRegion::Cn;
        let global = TraeRegion::Global;
        let pairs: [(&str, PathBuf, PathBuf); 9] = [
            ("账号库", accounts_file_for_region(cn), accounts_file_for_region(global)),
            ("分组", groups_file_for_region(cn), groups_file_for_region(global)),
            ("设备映射", device_map_file_for_region(cn), device_map_file_for_region(global)),
            ("OAuth 设备身份", oauth_device_file_for_region(cn), oauth_device_file_for_region(global)),
            ("签到明细", credits_history_file_for_region(cn), credits_history_file_for_region(global)),
            ("每日积分", credits_daily_file_for_region(cn), credits_daily_file_for_region(global)),
            ("剩余积分", remaining_credits_file_for_region(cn), remaining_credits_file_for_region(global)),
            ("冷却状态", cooldowns_file_for_region(cn), cooldowns_file_for_region(global)),
            ("签到摘要", checkin_summary_file_for_region(cn), checkin_summary_file_for_region(global)),
        ];
        for (label, cn_path, global_path) in pairs {
            assert_ne!(cn_path, global_path, "{label} 被两个区域共用: {cn_path:?}");
        }
        // 国内版必须与**改造前的旧路径**逐字相同（否则老数据白丢）。
        assert_eq!(accounts_file_for_region(cn), accounts_file());
        assert_eq!(groups_file_for_region(cn), groups_file());
        assert_eq!(cooldowns_file_for_region(cn), cooldowns_file());
        // 国际版必须与国内版不同名（basename 级断言，不受 HOME 影响）。
        assert_eq!(
            accounts_file_for_region(global)
                .file_name()
                .and_then(|s| s.to_str()),
            Some("checkin_accounts.global.json")
        );
    }

    /// 快照目录按**区域 × 程序**分家，且两个历史目录名必须原样保住（零迁移）。
    ///
    /// 用 `HomeOverrideGuard` 隔离 home：`profiles_dir_for_program` 会 `create_dir_all`，
    /// 不隔离就会在用户真实 `~/.buddy-switch` 下建目录（测试纪律）。
    #[test]
    fn 快照目录按程序分家且保住历史目录名() {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-profile-program-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).expect("临时 home 应能创建");
        let guard = crate::modules::config::HomeOverrideGuard::set(&dir);

        let name_of = |region: TraeRegion, program: TraeProgram| {
            profiles_dir_for_program(region, program)
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        };

        // 历史名：与改造前 `profiles` / `profiles_global` 逐字相同。
        assert_eq!(
            name_of(TraeRegion::Cn, TraeProgram::TraeWork).as_deref(),
            Some("profiles")
        );
        assert_eq!(
            name_of(TraeRegion::Cn, TraeProgram::TraeCode).as_deref(),
            Some("profiles_trae_cn")
        );
        // 新槽位按 `<region>_<program>` 规则。
        assert_eq!(
            name_of(TraeRegion::Global, TraeProgram::TraeWork).as_deref(),
            Some("profiles_global_trae_work")
        );
        assert_eq!(
            name_of(TraeRegion::Global, TraeProgram::TraeCode).as_deref(),
            Some("profiles_global_trae_code")
        );

        // 四个程序位**两两不同目录**：混用会把一条程序的登录态灌进另一条。
        let all = [
            profiles_dir_for_program(TraeRegion::Cn, TraeProgram::TraeWork),
            profiles_dir_for_program(TraeRegion::Cn, TraeProgram::TraeCode),
            profiles_dir_for_program(TraeRegion::Global, TraeProgram::TraeWork),
            profiles_dir_for_program(TraeRegion::Global, TraeProgram::TraeCode),
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "两个程序位的快照目录相同: {a:?}");
            }
        }

        // 不安全槽位名一律拒绝，绝不拼出越界路径。
        assert!(profile_dir_for_program(TraeRegion::Global, TraeProgram::TraeWork, "1234").is_some());
        assert!(profile_dir_for_program(TraeRegion::Global, TraeProgram::TraeWork, "../escape").is_none());
        assert!(profile_dir_for_program(TraeRegion::Cn, TraeProgram::TraeCode, "a/b").is_none());

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
