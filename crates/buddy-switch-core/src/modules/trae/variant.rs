//! 产品线变体（variant）：把 Trae 各条产品线的全部差异收敛到一张表。
//!
//! ## 为什么需要这一层
//!
//! Trae 有**多条可以同机并存的产品线**（实测本机同时装着 `TRAE SOLO CN` 与
//! `Trae CN`，各有独立的安装目录、userData、进程名）。在引入本模块之前，
//! 这些差异散落在 [`super::platform`]（候选名列表）与 [`super`]（写死的 CN 端点常量）里，
//! 且**只有一个「全候选混在一起」的视角** —— 于是：
//!
//! - 界面上分不清「管理的是哪一条产品线」（自动探测按最近活跃挑，用户只能猜）；
//! - 想按产品线派生端点时没有可用的维度（端点只能写死成 CN 那套）。
//!
//! 本模块给出与 [`crate::modules::region`] 同形的表驱动设计：`TraeVariant` 枚举 +
//! `variant_spec()` 查表。**所有按产品线分家的取值都必须从这里派生**，
//! 不要再在别处散落 `if solo { … } else { … }`。
//!
//! ## 与 `Region` 的关系：正交，不要合并
//!
//! [`crate::modules::region::Region`] 描述 **WorkBuddy 这一产品的两个发行版本**
//! （`workbuddy-desktop.info` / `workbuddy-desktop-ai.info`），它们的认证文件格式、
//! 账号库结构、上游协议形状**完全一致**，差异只是域名/版本号。而 Trae 的产品线差异是
//! **产品级**的：凭据形态、账号库文件名、登录态载体、上游主机族都不同。
//! 把 Trae 塞进 `Region` 会让 `region_spec(All)`、`accounts_file_for(All)` 之类的既有约定
//! 失去定义，且所有 `match region { Cn | Global }` 的调用点都要补分支。
//! 因此两者**并列存在、互不转换**。
//!
//! ## 端点取值的权威来源（重要）
//!
//! 下表里的主机值**不是猜的**，来自客户端自带且随安装包一起分发的
//! `<安装根>\<产品名>\resources\app\product.json`：
//!
//! ```text
//! product.json → bootConfig → <能力>.trae.<regionKey>
//! ```
//!
//! 其中 `normal` 是 CN 构建的默认 region 键（同一份文件里另有 `SG` / `US` / `CN` / `USTTP`）。
//! 实测两条 CN 产品线（`TRAE SOLO CN` / `Trae CN`）的 `*.trae.normal` **逐字相同**，
//! 即：**产品线不改变端点，region 才改变端点**。这一点决定了下面这张表的形状 ——
//! 每个变体记录自己那套 region 键的取值。
//!
//! 同样来自 `product.json` 的身份事实（实测）：
//!
//! | 键 | `TRAE SOLO CN` | `Trae CN` |
//! |:---|:---|:---|
//! | `nameAlias` | **`TraeWork CN`** | `TraeCode CN` |
//! | `packageType` | `SOLO_CN` | `TRAE_CN` |
//! | `runMode` | `solo-lite` | （null） |
//! | `applicationName` | `trae-solo-cn` | `trae-cn` |
//! | `darwinBundleIdentifier` | `cn.trae.solo.app` | `cn.trae.app` |
//!
//! **⇒ `TRAE SOLO CN` 的官方别名就是 `TraeWork CN`**，即它是 Trae Work 产品线的 CN 版；
//! `Trae CN` 才是 IDE 那条线。这就是本模块把两个变体命名为 `TraeWork` / `Trae` 的依据，
//! 也是界面上「Trae Work」这一分区名的出处 —— **不是我们起的名字**。
//!
//! ## 国际化端点：取值来源已更正（2026-09-21）
//!
//! 本表曾把国际版端点记为 `api.trae.ai` / `api.trae.ai` / `grow-normal.trae.ai`，
//! 那三个值是**脱离结构的字符串误摘** —— 它们确实出现在**国内版**客户端的
//! `product.json` 里，但不是 `bootConfig.<能力>.trae.<regionKey>` 的取值，而是
//! 别的能力表（CDN / 市场域）的 `SG`/`US` 键。按结构读一遍就能发现：
//! 国内版本机的 `bootConfig.account.trae.*` 只有 `normal` 一个 CN 值，
//! 而 `grow-normal.trae.ai` 在国际版客户端里是 **account** 基址，被记成了 agent。
//!
//! 现在改用**国际版客户端自己声明的**值（本机已装：注册表 `TraeWork (User)`
//! → `%LOCALAPPDATA%\Programs\TRAE SOLO`，`packageType = SOLO_I18N`），
//! 取自它的 `<安装根>\resources\app\product.json` → `bootConfig.<能力>.trae.normal`：
//!
//! | 能力 | 国内版（`TRAE SOLO CN`） | 国际版（`TRAE SOLO`） |
//! |:---|:---|:---|
//! | `account` | `https://api.trae.cn` | `https://grow-normal.trae.ai` |
//! | `iCube` | `https://api.trae.com.cn` | `https://icube-normal.trae.ai` |
//! | `agent` | `https://trae-api-cn.mchost.guru` | `https://core-normal.trae.ai` |
//! | `ws` | `wss://trae-ws-cn.mchost.guru/custom_model` | `wss://wss-normal.trae.ai/custom_model` |
//! | `consoleHost`（授权页域） | `https://www.trae.cn` | `https://www.trae.ai` |
//!
//! `consoleHost` 那一行**不是 API**：它是「浏览器里打开的那一页」的域，
//! 授权页 = `${consoleHost}/authorization`。它与 `account`/`iCube` 不同主机，
//! 且**必须按区域分家** —— 拿 CN 域给国际版发登录，用户会在错的账号体系的
//! 授权页上登录，页面走完也不回调（见 [`EndpointSet::console_base`]）。
//!
//! ## 仍未验证的部分（勿凭推理"补全"）
//!
//! 上表**主机名**是客户端自述的权威值，但**接口路径与鉴权头是否与国内版同名**
//! 没有任何证据 —— 它从未对真实上游跑通过（本机没有可用的国际版凭据）。
//! 因此国际化联网能力必须**逐项实测**后才能宣称可用，失败方向见
//! [`VariantSpec::endpoints`]：缺失时返回 `None`，**绝不**用 CN 值冒充国际化值。
//! 本模块只负责「端点取哪个值」，不负责「上游是否接受」。

use std::path::PathBuf;

/// Trae 产品线变体。
///
/// 命名依据是客户端 `product.json` 的 `nameAlias`：`TRAE SOLO CN` 自称 `TraeWork CN`，
/// `Trae CN` 自称 `TraeCode CN`。对外展示用 [`TraeVariant::display_name`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraeVariant {
    /// Trae Work 产品线（对应客户端的 `TRAE SOLO` / `TRAE SOLO CN`）。
    ///
    /// ⚠️ 持久化轴翻到区域之后（2026-09-21），本取值在**读写数据**时表示「**国内区域**」
    /// （见 [`TraeVariant::region`]）：`TraeWork` 与 `Trae` 共用国内库，
    /// 差别只在**程序**维度（启动哪个客户端、快照落哪个目录）。
    TraeWork,
    /// Trae IDE 产品线（客户端 `product.json`：`nameShort = Trae CN`、`nameAlias = TraeCode CN`）。
    ///
    /// 语义同 `TraeWork`：**数据层面＝国内区域**，差别只在程序维度。
    Trae,
    /// **国际版区域**（客户端 `packageType = SOLO_I18N`，发布者 SPRING (SG) PTE. LTD）。
    ///
    /// 在持久化轴翻到区域之后新增：它让「区域之间互不污染」这条**真契约**继续可表达
    /// （此前那批断言用的是两个国内产品线，而它们现在共用一本库，断言前提已消失）。
    ///
    /// **刻意暂不列入 [`TraeVariant::all`]**：`all()` 是界面与网关遍历产品线的入口，
    /// 而国际版尚未接入探测与登录，进了 `all()` 会让界面上凭空多出一条「未检测到」的线。
    /// 等探测按程序位接好后（见 `.trellis/tasks/09-21-trae-region-program-model`）再纳入。
    Global,
}

impl TraeVariant {
    /// 稳定标识（用于文件名、序列化与前端传参）。
    ///
    /// ## `Trae` 的字符串**仍是 `trae_cn`**（刻意不改，别"顺手修"）
    ///
    /// 它已经写进用户的磁盘与既有集成：
    ///
    /// - `api_gateway_keys.json` 里每条 Key 的归属；
    /// - 网关请求日志的 `variant` 字段（Token 统计按它分档）；
    /// - 快照目录名 `profiles_trae_cn`（见 `paths::profiles_dir_for`）。
    ///
    /// 改这个字符串等于**断老数据**，而它本身只是个键名。所以本次只把**标识符**
    /// 正名为 `Trae`（与官方 `nameShort` 一致），字符串保持原样；`parse` 同时接受
    /// `trae_cn` 与 `trae`，两侧都可读。
    pub fn as_str(self) -> &'static str {
        match self {
            TraeVariant::TraeWork => "trae_work",
            TraeVariant::Trae => "trae_cn",
            TraeVariant::Global => "global",
        }
    }

    /// 本取值对应的**持久化区域**（账号库/签到/积分/冷却/日志按它分家）。
    ///
    /// `TraeWork` 与 `Trae` **都是国内区域** —— 持久化轴翻到区域后，
    /// 二者的差别只剩**程序**维度（启动哪个客户端、快照落哪个目录）。
    pub fn region(self) -> super::region::TraeRegion {
        match self {
            TraeVariant::TraeWork | TraeVariant::Trae => super::region::TraeRegion::Cn,
            TraeVariant::Global => super::region::TraeRegion::Global,
        }
    }

    /// 对外展示名（界面分区标题、日志、错误文案）。
    ///
    /// `TraeWork` 用官方 `nameAlias` 的写法 `Trae Work`（带空格）；
    /// `Trae` 用它自己的产品名（官方 `nameShort` 是 `Trae CN`，界面上取更短的 `Trae`）；
    /// `Global` 是区域而不是产品线，故用区域名。
    ///
    /// ⚠️ 这里是**程序位**级的名，不是区域名。界面顶部的区域切换用的是
    /// `region::TraeRegion::display_name()`（国内版 / 国际版）。
    pub fn display_name(self) -> &'static str {
        match self {
            TraeVariant::TraeWork => "Trae Work",
            TraeVariant::Trae => "Trae",
            TraeVariant::Global => "国际版",
        }
    }

    /// 从字符串解析变体。大小写不敏感，接受若干常见别名。
    ///
    /// 接受 `TRAE SOLO CN` / `TRAE SOLO` 这类**目录名**，因为用户可能直接从
    /// 探测结果（`TraeEnvStatus.dataDir` 的末段）把产品名传回来。
    pub fn parse(s: &str) -> Option<TraeVariant> {
        match s.trim().to_ascii_lowercase().as_str() {
            "trae_work" | "traework" | "trae work" | "work" | "solo" | "trae solo" | "trae solo cn" => {
                Some(TraeVariant::TraeWork)
            }
            // `cn` 是**区域标识**（国内版）⇒ 必须落到该区域的**主程序**（TraeWork）。
            // 它曾作为 Trae 的宽容别名；但区域标识指向主程序才安全 ——
            // 否则「保存登录态」这类**程序级**操作会去读 TraeCode 客户端的 userData。
            "cn" => Some(TraeVariant::TraeWork),
            "trae_cn" | "traecn" | "trae cn" | "ide" | "trae" => Some(TraeVariant::Trae),
            // 国际版区域。**不放宽成 `global`/`intl` 之外的词**：区域标识是要落到
            // 文件名与落盘数据上的，认得太宽会让一个拼错的参数静默读到另一套账号库
            // （症状是"账号凭空消失"，极难定位）。
            "global" | "trae_global" | "intl" | "international" => Some(TraeVariant::Global),
            _ => None,
        }
    }

    /// 全部已知变体，供遍历用。
    pub fn all() -> [TraeVariant; 2] {
        [TraeVariant::TraeWork, TraeVariant::Trae]
    }

    /// 本变体的**授权页产品线**（决定 `auth_from` / `client_id` / `hide_saas_login`）。
    ///
    /// ★ 判定依据**逐字照抄客户端自己的分派**（本机 2026-09-21 读国际版
    /// `out/main.js`）：客户端按 `packageType` 选产品线，而 `packageType`
    /// 正是本表里每个变体已经登记的字段：
    ///
    /// ```js
    /// function Su(t){ return t.packageType===SOLO_CN || t.packageType===SOLO_I18N
    ///                       || t.packageType===SOLO_CN_ENTERPRISE ? "SOLO_Lite" : "TRAE" }
    /// clientId = Pr(t) ? authConfig.SOLO[channel] : authConfig.TRAE[channel]
    /// authFrom = Su(t)==="SOLO_Lite" ? "solo" : "trae"     // 且 solo 时追加 hide_saas_login=true
    /// ```
    ///
    /// ⚠️ **授权页产品线 ≠ 区域**：`SOLO_CN` 与 `SOLO_I18N` 是**两条区域的同一个产品线**，
    /// 因此 `TraeWork`（CN）与 `Global`（国际版 TraeWork）落到**同一条授权页产品线** ——
    /// 本机实测两台客户端的 `iCubeApp.authConfig` 逐字相同，也印证了这一点。
    pub fn oauth_line(self) -> OAuthLine {
        OAuthLine::from_package_type(variant_spec(self).package_type)
    }
}

/// 授权页的**产品线**（客户端 `iCubeApp.authConfig` 的两把钥匙）。
///
/// 它与 [`TraeRegion`](super::region::TraeRegion) **正交**，也与 [`TraeProgram`]
/// 同构但不是同一个东西：`TraeProgram` 是**本项目的执行轴**（写哪个目录、启动哪个 exe），
/// 本枚举是**上游授权页的入参选择器**。二者当前取值一一对应，但依据不同
/// （前者来自 userData/exe 名，后者来自客户端 `packageType` 的分派），
/// 所以**刻意不互相转换** —— 合并会让「授权页参数该按什么分」这个事实被藏起来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OAuthLine {
    /// SOLO 产品线（`packageType` ∈ {`SOLO_CN`, `SOLO_I18N`, `SOLO_CN_ENTERPRISE`}）。
    Solo,
    /// TRAE / IDE 产品线（其余 `packageType`）。
    Trae,
}

impl OAuthLine {
    /// 客户端的 `packageType → 产品线` 分派（**逐字照抄**，含企业版取值）。
    ///
    /// 未登记的取值一律落 [`OAuthLine::Trae`] —— 与客户端 `? :` 的 `else` 分支同向。
    /// 本项目三条变体的 `packageType` 都已登记，故这只是一条与上游对齐的兜底。
    ///
    /// ⚠️ **不是 `const fn`**：`match` 一个 `&str` 在 const 上下文里尚未稳定
    /// （`cannot match on str in constant functions`）。本函数不需要 const，
    /// 真正需要 const 的是 [`OAuthLine::default_client_id`]（`mod.rs` 的常量引用它）。
    pub fn from_package_type(package_type: &str) -> OAuthLine {
        match package_type {
            "SOLO_CN" | "SOLO_I18N" | "SOLO_CN_ENTERPRISE" => OAuthLine::Solo,
            _ => OAuthLine::Trae,
        }
    }

    /// 授权页 `auth_from` 参数（客户端同名字符串）。
    pub const fn auth_from(self) -> &'static str {
        match self {
            OAuthLine::Solo => "solo",
            OAuthLine::Trae => "trae",
        }
    }

    /// 内置 `client_id` 默认值 —— 即客户端 `product.json` 的
    /// `iCubeApp.authConfig.<本产品线>.stable`（本机 2026-09-21 实测；
    /// **两条区域的客户端取值逐字相同**，所以它只按产品线分，不按区域分）。
    ///
    /// ⚠️ 拿另一条线的值去发登录，授权页会停在 billing status 后**不回跳**
    /// （参考实现踩过：拿 SOLO 的值 `en1oxy7wnw8j9n` 去走 IDE 的流程）。
    pub const fn default_client_id(self) -> &'static str {
        match self {
            OAuthLine::Solo => "en1oxy7wnw8j9n",
            OAuthLine::Trae => "ono9krqynydwx5",
        }
    }

    /// 是否追加 `hide_saas_login=true`。
    ///
    /// 客户端原文：`auth_from === "solo" && (url += "&hide_saas_login=true")`
    /// —— 它是 `auth_from` 的**从属**参数，不是独立开关，因此不单独配置。
    pub const fn hide_saas_login(self) -> bool {
        matches!(self, OAuthLine::Solo)
    }

    /// `DeviceInfo.PlatformCode` —— 同样是**产品线级**的取值。
    ///
    /// 客户端原文（本机 2026-09-24 读 `out/main.js`，逐字）：
    /// ```js
    /// k(){ return gr(this.d) ? "SOLO_PC" : "IDE_PC" }   // gr = 是否 SOLO_Lite
    /// ```
    /// 与 `auth_from` / `client_id` 用的是**同一个**判定（`Su(t) === "SOLO_Lite"`），
    /// 因此必须与它们同源派生 —— 分成两个常量必然漂移。
    ///
    /// ## 为什么这条值得单独写一段注释（真机现场）
    ///
    /// 2026-09-24 用户报「TraeWork（SOLO 线）OAuth 登录失败，上游回
    /// `20403/040036 Token device not match`」。抓真机对照发现：本实现把
    /// `PlatformCode` 写死成 `IDE_PC`（从参考实现 `oauth.rs` 逐字抄来，而参考
    /// 固化的是 **TraeCode/IDE 线**），而真机客户端在 SOLO 线上报的是 **`SOLO_PC`**。
    /// 这正是「移植参考实现前先问这段抓自哪条产品线」那条教训的漏网之鱼。
    pub const fn platform_code(self) -> &'static str {
        match self {
            OAuthLine::Solo => "SOLO_PC",
            OAuthLine::Trae => "IDE_PC",
        }
    }
}

impl Default for TraeVariant {
    /// 默认 `TraeWork`。
    ///
    /// 选它而非 `Trae` 的理由与参考实现一致：`TraeWorkAssistant` 的
    /// `TargetApp::parse` 对未知值**回退 `TraeWork`**，且 `TRAE SOLO CN` 是本机
    /// 最近活跃的产品线。**注意这只是一处"缺省"**，不是"另一条线不支持"。
    fn default() -> Self {
        TraeVariant::TraeWork
    }
}

/// 一套端点取值（对应 `product.json` 里某个 region 键下的全部主机）。
///
/// 字段名对齐 [`super`] 里既有的常量语义，便于逐字对拍。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointSet {
    /// 账号中心 / 签到 / 积分基址（`bootConfig.account.trae.*` / `ug.trae.*`）。
    pub account_base: &'static str,
    /// iCube / 市场 / ASR 基址（`bootConfig.iCube.*` / `market.trae.*` / `asr.domain.*`）。
    pub icube_base: &'static str,
    /// 对话（agent）网关基址（`bootConfig.agent.trae.*`，与 `icube_base` **不同主机**）。
    pub agent_host: &'static str,
    /// WebSocket 基址（`bootConfig.ws.trae.*`）。
    ///
    /// **未验证**：CN 值是实测读到的，国际化值同样读到了但**从未连通过**。
    pub ws_base: Option<&'static str>,
    /// 网页控制台 / **授权页**基址（`bootConfig.consoleHost`）。
    ///
    /// ⚠️ 这是**网页域，不是 API 域**：它与 `account_base` / `icube_base` 不同主机，
    /// 只用于「浏览器里打开的那一页」。授权页完整地址 = `${console_base}/authorization`，
    /// 拼法照抄客户端自身（`loginHost = bootConfig.consoleHost`，
    /// 再拼 `/authorization?login_version=1&…`，见 `out/main.js` 的 OAuth 构造段）。
    ///
    /// ⚠️ **必须按区域分家**：CN 是 `https://www.trae.cn`，国际版是 `https://www.trae.ai`。
    /// 拿 CN 域给国际版发登录 ⇒ 用户在**错的账号体系**的授权页上登录，
    /// 页面即使走完也不会把凭据回调回本机（域与客户端不匹配），
    /// 症状与「回调端口没人监听」几乎一样（停在「认证中」），极易误判成端口问题。
    pub console_base: &'static str,
}

/// 一个变体的全部差异。
#[derive(Debug, Clone, Copy)]
pub struct VariantSpec {
    /// 所属变体。
    pub variant: TraeVariant,
    /// 稳定的展示名（界面分区标题）。
    pub display_name: &'static str,
    /// 客户端 `product.json` 的 `nameAlias`（官方自称，用于诊断文案与核对）。
    pub name_alias: &'static str,
    /// 客户端 `product.json` 的 `packageType`。
    pub package_type: &'static str,
    /// userData 目录名候选（`%APPDATA%\<名字>`，按优先级）。
    ///
    /// 同一变体可能有多个名字：客户端 `dataFolderName` 都是 `.trae-cn`，
    /// 但 userData 用的是 `nameShort`。
    pub data_dir_names: &'static [&'static str],
    /// 可执行文件名候选（按优先级，与 [`VariantSpec::data_dir_names`] 一一对应）。
    pub exe_names: &'static [&'static str],
    /// 进程名（Windows 精简名，不带 `.exe`）候选。
    pub proc_names: &'static [&'static str],
    /// CN 端点集（有实测依据）。
    pub cn_endpoints: EndpointSet,
    /// 国际版端点集。
    ///
    /// **从未对真实上游跑通过**（本机未装国际版客户端、无可用凭据），
    /// 值取自 CN 客户端 `product.json` 里的 `SG`/`US` 键 —— 那是**客户端自己声明的**，
    /// 比我们的任何推断都可信，但仍不等于「服务端接受」。
    pub global_endpoints: Option<EndpointSet>,
}

impl VariantSpec {
    /// 取该变体的端点集。
    ///
    /// `international = false` 一律返回 CN 端点（有实测依据）；
    /// `true` 返回国际版端点 —— **可能为 `None`**，调用方必须显式处理，
    /// 不要用 CN 值兜底冒充国际化值（那会把请求打到错的域，且错误很难定位）。
    pub fn endpoints(&self, international: bool) -> Option<&EndpointSet> {
        if international {
            self.global_endpoints.as_ref()
        } else {
            Some(&self.cn_endpoints)
        }
    }

    /// 该变体在 userData 根目录下的候选路径（未做存在性过滤）。
    pub fn data_dir_candidates(&self, base: &std::path::Path) -> Vec<PathBuf> {
        self.data_dir_names
            .iter()
            .map(|name| base.join(name))
            .collect()
    }
}

/// Trae Work 变体（`TRAE SOLO` / `TRAE SOLO CN`）。
///
/// **CN 端点取值逐字等于改造前的 `modules::trae` 常量**，保证既有行为零变化：
/// 改造前 `TRAE_API_BASE = "https://api.trae.cn"`、`TRAE_OAUTH_BASE = "https://api.trae.com.cn"`、
/// `TRAE_AGENT_HOST = "https://trae-api-cn.mchost.guru"`。
///
/// ## ⚠️ 候选名与 `GLOBAL_SPEC` **重叠**（`TRAE SOLO`）—— 已知缺陷，勿在此处单独摘除
///
/// 本表的 `TRAE SOLO` 属于**国际版**客户端（`packageType = SOLO_I18N`，见 [`GLOBAL_SPEC`]），
/// 与 `TRAE SOLO CN`（国内版）不是同一个客户端。两者重叠的后果不是报错而是
/// **静默读错客户端**：`select_data_dir_for(TraeWork)`（按活跃度）在本机返回 `TRAE SOLO`
/// ⇒ 拿到国际版的设备凭证 / 目录。
///
/// **但"顺手摘掉它"会连带作废约 20 条护栏**：本表是目前**唯一**有"一个变体多个候选目录"
/// 的表，而 `select_data_dir_for` 与 `detect_data_dir_for` 的分叉、以及 R3/R5/R6 那批
/// 「登录态不在首个候选里」的用例**全部**靠它构造现场（2026-09-24 实测：摘掉后
/// `cargo test -p buddy-switch-core --lib` 30 条红）。`super::region` 的目标表
/// （`CN_TRAE_WORK.data_dir_names = ["TRAE SOLO CN"]` 等四个程序位**各一个名字**）
/// 一旦成为唯一来源，两个选择器就恒等 ⇒ 分叉机制整体失去意义。
///
/// ⇒ 正确做法是 `.trellis/tasks/09-21-trae-region-program-model` 的「仍未做 #1」
/// （`TraeVariant` → `(TraeRegion, TraeProgram)` 的类型收尾），**必须单独一轮**、
/// 连同那 20 条用例的去留一起决定。**不要**在别处顺手改这张表。
const TRAE_WORK_SPEC: VariantSpec = VariantSpec {
    variant: TraeVariant::TraeWork,
    display_name: "Trae Work",
    name_alias: "TraeWork CN",
    package_type: "SOLO_CN",
    data_dir_names: &["TRAE SOLO CN", "TRAE SOLO"],
    exe_names: &["TRAE SOLO CN.exe", "TRAE SOLO.exe"],
    proc_names: &["TRAE SOLO CN", "TRAE SOLO"],
    cn_endpoints: EndpointSet {
        account_base: "https://api.trae.cn",
        icube_base: "https://api.trae.com.cn",
        agent_host: "https://trae-api-cn.mchost.guru",
        ws_base: Some("wss://trae-ws-cn.mchost.guru/custom_model"),
        console_base: "https://www.trae.cn",
    },
    // 国际化取值来自**国际版客户端自己声明的** `bootConfig.<能力>.trae.normal`
    // （本机装在 `%LOCALAPPDATA%\Programs\TRAE SOLO`，`packageType = SOLO_I18N`）。
    // 主机名是权威值；接口路径与鉴权**从未对上游跑通**，见模块头「仍未验证的部分」。
    global_endpoints: Some(GLOBAL_ENDPOINTS),
};

/// Trae CN 变体（`Trae` / `Trae CN`）。
const TRAE_CN_SPEC: VariantSpec = VariantSpec {
    variant: TraeVariant::Trae,
    display_name: "Trae CN",
    name_alias: "TraeCode CN",
    package_type: "TRAE_CN",
    data_dir_names: &["Trae CN", "Trae"],
    exe_names: &["Trae CN.exe", "Trae.exe"],
    proc_names: &["Trae CN", "Trae"],
    cn_endpoints: EndpointSet {
        // 实测：与 Trae Work 变体**逐字相同** —— 产品线不改变端点，region 才改变。
        account_base: "https://api.trae.cn",
        icube_base: "https://api.trae.com.cn",
        agent_host: "https://trae-api-cn.mchost.guru",
        ws_base: Some("wss://trae-ws-cn.mchost.guru/custom_model"),
        console_base: "https://www.trae.cn",
    },
    // 同上：国际版客户端自述值。**产品线不改变端点，region 才改变端点** ——
    // 所以本变体的国际化端点与 `TRAE_WORK_SPEC` 逐字相同（有单测钉住）。
    global_endpoints: Some(GLOBAL_ENDPOINTS),
};

/// 国际版端点（**国际版客户端自述值**）—— 本文件的唯一来源，三处 spec 都引用它。
///
/// 取值来自国际版客户端 `<安装根>\resources\app\product.json` 的
/// `bootConfig.<能力>.trae.normal`（本机 `%LOCALAPPDATA%\Programs\TRAE SOLO`）。
const GLOBAL_ENDPOINTS: EndpointSet = EndpointSet {
    account_base: "https://grow-normal.trae.ai",
    icube_base: "https://icube-normal.trae.ai",
    agent_host: "https://core-normal.trae.ai",
    ws_base: Some("wss://wss-normal.trae.ai/custom_model"),
    // 授权页域：国际版客户端 `product.json` 的 `bootConfig.consoleHost` **实测值**
    // （本机 2026-09-21 读取，与 `homeUrl` 同值）。客户端自己的 OAuth 构造把它当
    // `loginHost` 用，见 `out/main.js`：`${loginHost}/authorization?login_version=1…`。
    console_base: "https://www.trae.ai",
};

/// **国际版区域**（`TraeWork` 国际构建，`packageType = SOLO_I18N`）。
///
/// 与两条 CN 产品线的区别：
/// - 候选名是**国际版客户端自己的**名字（`TRAE SOLO` / `TRAE SOLO.exe`），
///   不是 CN 那条的（`TRAE SOLO CN`）—— 混在一起用"按序取第一个存在的"会在
///   同机装两条线时永远命中 CN 那个，国际版被静默吞掉；
/// - 端点用国际版那套（本表里 `cn_endpoints` 这一格放的就是**它实际使用的端点**：
///   `EndpointSet` 的字段名是历史包袱，`Global` 没有"另一套"可切）。
const GLOBAL_SPEC: VariantSpec = VariantSpec {
    variant: TraeVariant::Global,
    display_name: "国际版",
    name_alias: "TraeWork",
    package_type: "SOLO_I18N",
    data_dir_names: &["TRAE SOLO"],
    exe_names: &["TRAE SOLO.exe"],
    proc_names: &["TRAE SOLO"],
    cn_endpoints: GLOBAL_ENDPOINTS,
    global_endpoints: None,
};

/// 取变体的描述符。
pub fn variant_spec(variant: TraeVariant) -> &'static VariantSpec {
    match variant {
        TraeVariant::TraeWork => &TRAE_WORK_SPEC,
        TraeVariant::Trae => &TRAE_CN_SPEC,
        TraeVariant::Global => &GLOBAL_SPEC,
    }
}

/// 全部变体的描述符（遍历用）。
pub fn all_specs() -> [&'static VariantSpec; 2] {
    [&TRAE_WORK_SPEC, &TRAE_CN_SPEC]
}

/// 从 userData 目录名 / 安装目录名 / exe 名反查变体。
///
/// 这是「探测到的是哪条产品线」的唯一判定入口：先用目录名精确匹配，
/// 再退化到「包含关系」（容忍 `TRAE SOLO CN` 之类的完整名与 `solo` 之类的片段）。
///
/// 返回 `None` 表示**不认识**这个名字 —— 调用方应保持"跨变体"的宽容行为
/// （例如仍然把它当候选目录），而不是硬判成某一个变体。
pub fn variant_of_name(name: &str) -> Option<TraeVariant> {
    let lowered = name.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return None;
    }
    // 去 `.exe` 后缀：调用方可能传 exe 文件名。
    let lowered = lowered.strip_suffix(".exe").unwrap_or(&lowered).to_string();

    // 先精确匹配全部候选名（含大小写差异）。
    for spec in all_specs() {
        for candidate in spec.data_dir_names.iter().chain(spec.exe_names.iter()) {
            if candidate.eq_ignore_ascii_case(&lowered) {
                return Some(spec.variant);
            }
        }
    }

    // 再退化到包含关系。`solo` 是 Trae Work 的独有词根（`TRAE SOLO`），
    // 放在 `trae` 之前判定，否则 `TRAE SOLO CN` 会被 `trae` 抢先命中。
    //
    // ⚠️ 已知后果（与 `TRAE_WORK_SPEC` 的候选名重叠同源）：`TRAE SOLO` 会落到
    //   `TraeWork`，而它其实是**国际版**客户端的 userData 名。修它等于摘掉那条重叠，
    //   见 `TRAE_WORK_SPEC` 的说明 —— 必须与那 20 条护栏一起单独一轮处理。
    if lowered.contains("solo") {
        return Some(TraeVariant::TraeWork);
    }
    if lowered.contains("trae") {
        return Some(TraeVariant::Trae);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 变体标识与展示名稳定() {
        assert_eq!(TraeVariant::TraeWork.as_str(), "trae_work");
        // ★ `Trae` 的字符串**是** `trae_cn`（持久化契约，见 `as_str` 的说明）：
        // 它已写进用户的 Key 记录、网关日志与快照目录名，改名会断老数据。
        assert_eq!(TraeVariant::Trae.as_str(), "trae_cn");
        assert_eq!(TraeVariant::Global.as_str(), "global");
        assert_eq!(TraeVariant::TraeWork.display_name(), "Trae Work");
        assert_eq!(TraeVariant::Trae.display_name(), "Trae");
        assert_eq!(TraeVariant::Global.display_name(), "国际版");
        assert_eq!(TraeVariant::default(), TraeVariant::TraeWork);
        // 标识符已正名为 `Trae`，但历史字符串仍可解析（两侧都可读）。
        assert_eq!(TraeVariant::parse("trae"), Some(TraeVariant::Trae));
        assert_eq!(TraeVariant::parse("trae_cn"), Some(TraeVariant::Trae));
    }

    #[test]
    fn 解析接受目录名与常见别名() {
        for s in ["trae_work", "TraeWork", "Trae Work", "solo", "TRAE SOLO CN"] {
            assert_eq!(TraeVariant::parse(s), Some(TraeVariant::TraeWork), "解析失败: {s}");
        }
        for s in ["trae_cn", "Trae", "Trae CN", "ide", "trae"] {
            assert_eq!(TraeVariant::parse(s), Some(TraeVariant::Trae), "解析失败: {s}");
        }
        assert_eq!(TraeVariant::parse("doubao"), None);
        assert_eq!(TraeVariant::parse(""), None);
    }

    /// 反查的核心判别式：`solo` 必须先于 `trae` 判定。
    /// 若顺序写反，`TRAE SOLO CN` 会命中 `trae` 分支被误判成 Trae CN。
    #[test]
    fn 反查时solo优先于trae() {
        assert_eq!(variant_of_name("TRAE SOLO CN"), Some(TraeVariant::TraeWork));
        assert_eq!(variant_of_name("TRAE SOLO"), Some(TraeVariant::TraeWork));
        assert_eq!(variant_of_name("TRAE SOLO CN.exe"), Some(TraeVariant::TraeWork));
        assert_eq!(variant_of_name("Trae CN"), Some(TraeVariant::Trae));
        assert_eq!(variant_of_name("Trae CN.exe"), Some(TraeVariant::Trae));
        // 目录名不区分大小写。
        assert_eq!(variant_of_name("trae solo cn"), Some(TraeVariant::TraeWork));
        // 不认识的平台（豆包）不能被硬判成某个 Trae 变体。
        assert_eq!(variant_of_name("Doubao"), None);
    }

    /// 变体之间的候选名**必须不重叠**：重叠会导致探测时互相抢，
    /// 出现「选中 Trae Work 的安装、读 Trae CN 的 userData」。
    ///
    /// ⚠️ **只比 `TraeWork` vs `Trae`**（历史范围）。`TraeWork` 与 `Global` 之间
    /// 在 `TRAE SOLO` 上**确实重叠**，而修它必须连同约 20 条依赖「多变体多候选目录」
    /// 的护栏一起做 —— 见 `TRAE_WORK_SPEC` 的说明。在那之前，把本用例扩到 `Global`
    /// 只会得到一条**恒红**的断言，不如把事实写在这里。
    #[test]
    fn 变体候选名互不重叠() {
        let work = variant_spec(TraeVariant::TraeWork);
        let cn = variant_spec(TraeVariant::Trae);
        for a in work.data_dir_names {
            assert!(
                !cn.data_dir_names.contains(a),
                "userData 目录名重叠: {a}"
            );
        }
        for a in work.exe_names {
            assert!(!cn.exe_names.contains(a), "exe 名重叠: {a}");
        }
    }

    /// ★★ 授权页**产品线**必须由 `packageType` 派生（客户端自己的分派），
    /// 且**只按产品线分、不按区域分**。
    ///
    /// 依据（本机 2026-09-21 读客户端 `out/main.js`，逐字）：
    /// ```js
    /// function Su(t){ return t.packageType===SOLO_CN || t.packageType===SOLO_I18N
    ///                       || t.packageType===SOLO_CN_ENTERPRISE ? "SOLO_Lite" : "TRAE" }
    /// clientId = Pr(t) ? authConfig.SOLO[channel] : authConfig.TRAE[channel]
    /// authFrom = Su(t)==="SOLO_Lite" ? "solo" : "trae"
    /// ```
    /// 而本机三台客户端的 `iCubeApp.authConfig` **逐字相同**：
    /// `SOLO.stable = en1oxy7wnw8j9n`、`TRAE.stable = ono9krqynydwx5`
    /// ⇒ 钥匙是**产品线级**的，不是区域级的（否则 CN 与国际版会各有一套）。
    ///
    /// 缺陷形态（本轮修掉的那个）：三处全用 IDE 那把钥匙 + `auth_from=trae`，
    /// 于是两条 SOLO 线的登录都拿错了钥匙 ⇒ 授权页停在 billing status 后不回跳。
    #[test]
    fn 授权页产品线由package_type派生且不按区域分() {
        // 三条变体各自的产品线。
        assert_eq!(
            variant_spec(TraeVariant::TraeWork).variant.oauth_line(),
            OAuthLine::Solo,
            "TraeWork（SOLO_CN）必须落 SOLO 线"
        );
        assert_eq!(
            variant_spec(TraeVariant::Global).variant.oauth_line(),
            OAuthLine::Solo,
            "国际版 TraeWork（SOLO_I18N）必须落 SOLO 线 —— 它与 CN 是同一条产品线"
        );
        assert_eq!(
            variant_spec(TraeVariant::Trae).variant.oauth_line(),
            OAuthLine::Trae,
            "Trae（TRAE_CN）必须落 TRAE 线"
        );

        // 分派是 `packageType` 的函数，与区域无关：同一 `packageType` 必然同一产品线。
        for spec in all_specs() {
            assert_eq!(
                OAuthLine::from_package_type(spec.package_type),
                spec.variant.oauth_line(),
                "{} 的产品线没有从 package_type 派生",
                spec.name_alias
            );
        }
        // 企业版取值按客户端原样识别（本项目暂无该变体，但规则必须完整）。
        assert_eq!(OAuthLine::from_package_type("SOLO_CN_ENTERPRISE"), OAuthLine::Solo);
        assert_eq!(OAuthLine::from_package_type("TRAE_CN_ENTERPRISE"), OAuthLine::Trae);

        // 两条线的三件取值必须**两两不同**（否则「按线分家」等于没分）。
        assert_ne!(OAuthLine::Solo.auth_from(), OAuthLine::Trae.auth_from());
        assert_ne!(
            OAuthLine::Solo.default_client_id(),
            OAuthLine::Trae.default_client_id()
        );
        assert_ne!(OAuthLine::Solo.hide_saas_login(), OAuthLine::Trae.hide_saas_login());
        assert_ne!(OAuthLine::Solo.platform_code(), OAuthLine::Trae.platform_code());
        // 逐字值（与客户端 `authConfig` / `auth_from` / `k()` 分派对拍）。
        assert_eq!(OAuthLine::Solo.auth_from(), "solo");
        assert_eq!(OAuthLine::Trae.auth_from(), "trae");
        assert_eq!(OAuthLine::Solo.default_client_id(), "en1oxy7wnw8j9n");
        assert_eq!(OAuthLine::Trae.default_client_id(), "ono9krqynydwx5");
        assert!(OAuthLine::Solo.hide_saas_login());
        assert!(!OAuthLine::Trae.hide_saas_login());
        // `PlatformCode` 与 `auth_from` **同源判定**：SOLO 线是 SOLO_PC。
        // 反例就是 2026-09-24 那个真实缺陷（写死 IDE_PC ⇒ 上游 20403）。
        assert_eq!(OAuthLine::Solo.platform_code(), "SOLO_PC");
        assert_eq!(OAuthLine::Trae.platform_code(), "IDE_PC");
        for spec in all_specs() {
            assert_eq!(
                OAuthLine::from_package_type(spec.package_type).platform_code(),
                spec.variant.oauth_line().platform_code(),
                "{} 的 PlatformCode 没有从 package_type 派生",
                spec.name_alias
            );
        }
    }

    /// 两条产品线的 CN 端点**逐字相同**（实测结论）。
    /// 这个断言是"产品线不改变端点、region 才改变端点"的机器可读证据；
    /// 若有人给某条产品线单独改了端点，这里会红。
    #[test]
    fn 两条产品线的cn端点逐字相同() {
        let work = variant_spec(TraeVariant::TraeWork).cn_endpoints;
        let cn = variant_spec(TraeVariant::Trae).cn_endpoints;
        assert_eq!(work, cn);
        assert_eq!(work.account_base, "https://api.trae.cn");
        assert_eq!(work.icube_base, "https://api.trae.com.cn");
        assert_eq!(work.agent_host, "https://trae-api-cn.mchost.guru");
    }

    /// 国际化端点必须是**国际版客户端自述**的那一组（2026-09-21 更正真实缺陷）。
    ///
    /// 反例就是本次改掉的那三个值：`api.trae.ai` / `api.trae.ai` /
    /// `grow-normal.trae.ai`。它们确实出现在**国内版**客户端的 `product.json` 里，
    /// 但属于别的能力表（CDN / 市场域）的 `SG`/`US` 键，不是
    /// `bootConfig.<能力>.trae.<regionKey>` 的取值；其中 `grow-normal.trae.ai`
    /// 在国际版客户端里是 **account** 基址，却被记成了 agent。
    /// 谁再按字符串搜国内版文件把它抄回去，这条会红。
    #[test]
    fn 国际化端点取自国际版客户端自述值() {
        let expected = EndpointSet {
            account_base: "https://grow-normal.trae.ai",
            icube_base: "https://icube-normal.trae.ai",
            agent_host: "https://core-normal.trae.ai",
            ws_base: Some("wss://wss-normal.trae.ai/custom_model"),
            console_base: "https://www.trae.ai",
        };
        for spec in all_specs() {
            assert_eq!(
                spec.global_endpoints.expect("国际化端点应已登记"),
                expected,
                "{} 的国际化端点漂了",
                spec.name_alias
            );
        }
    }

    /// 国际化端点在**每个主机**上都必须与 CN 不同：混用会把请求打到错的域，
    /// 而且这种错误在日志里只表现为「401 / 超时」，极难定位。
    ///
    /// 另钉住 `ws_base` **必须已登记**：它此前是 `None`（"未验证"占位），
    /// 这条断言在本次更正前是**红的** —— 正因如此它才值得留在这里。
    #[test]
    fn 国际化端点与cn端点主机全不同() {
        for spec in all_specs() {
            let global = spec.global_endpoints.expect("国际化端点应已登记");
            assert_ne!(global.account_base, spec.cn_endpoints.account_base, "account 撞了");
            assert_ne!(global.icube_base, spec.cn_endpoints.icube_base, "iCube 撞了");
            assert_ne!(global.agent_host, spec.cn_endpoints.agent_host, "agent 撞了");
            assert_ne!(global.ws_base, spec.cn_endpoints.ws_base, "ws 撞了");
            // 授权页域同样必须分家：共用 CN 域 = 国际版用户在错的账号体系上登录。
            assert_ne!(
                global.console_base, spec.cn_endpoints.console_base,
                "授权页域（consoleHost）撞了"
            );
            assert!(
                global.ws_base.is_some(),
                "{} 的国际版 ws 端点缺失（应为国际版客户端自述的 wss 值）",
                spec.name_alias
            );
        }
    }

    /// CN 端点必须等于改造前的硬编码常量原值 —— 保证既有行为零变化。
    #[test]
    fn cn端点等于改造前的常量原值() {        let spec = variant_spec(TraeVariant::TraeWork);
        assert_eq!(spec.cn_endpoints.account_base, super::super::TRAE_API_BASE_CN);
        assert_eq!(spec.cn_endpoints.icube_base, super::super::TRAE_OAUTH_BASE_CN);
    }

    /// `international = true` 而国际版端点缺失时必须返回 `None`，
    /// **绝不能**回落到 CN 值 —— 那会把国际化请求打到国内域。
    #[test]
    fn 国际版端点缺失时不回落cn() {
        let spec = variant_spec(TraeVariant::TraeWork);
        assert!(spec.endpoints(false).is_some());
        let global = spec.endpoints(true).expect("国际版端点应已登记");
        assert_ne!(global.account_base, spec.cn_endpoints.account_base);
        // 用一个刻意缺国际版端点的 spec 验证 None 语义。
        let mut stripped = *spec;
        stripped.global_endpoints = None;
        assert!(stripped.endpoints(true).is_none());
    }
}
