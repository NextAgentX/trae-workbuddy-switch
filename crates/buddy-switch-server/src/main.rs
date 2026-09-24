//! buddy-switch CLI：npm 安装形态的入口（展示名 `Buddy Switch`）。
//!
//! ```bash
//! buddy-switch              # 启动本地服务 + 打开浏览器 webui
//! buddy-switch serve        # 只起服务不开浏览器（--port / --no-open）
//! buddy-switch status       # 终端输出当前账号
//! buddy-switch version      # 版本号
//! ```

mod api;
mod gateway_host;
mod trae_gateway_host;

use serde_json::json;

use buddy_switch_core::modules::{
    account, auth_file, config, process, region::Region, rotate, schedule, scheduler, update,
};

fn default_port() -> u16 {
    57890
}

/// 为某一类任务起一个独立排程循环。
///
/// 排程语义（重读配置 → 算 `next_fire` → sleep 到点 → 派发）全部在
/// [`scheduler::schedule_loop`] 里，**桌面端与服务端共用同一份**：此前它只写在本文件，
/// 桌面端另有一套写死周期的循环，两边行为漂移且桌面端的排程配置完全不生效。
fn spawn_scheduled_task(task: schedule::ScheduleTask) {
    tokio::spawn(scheduler::schedule_loop(task));
}

/// 后台任务的注册表条目：**「有哪些后台任务」的唯一事实来源**。
///
/// 设计意图：后台任务的**存在性**必须是**数据**，而不是散落在 [`spawn_background_loops`]
/// 里的 `tokio::spawn` 调用。原因——本工单修复的账号池余额接入缺陷，本质就是一次
/// 「**调用点静默消失**」：`Pool::set_credits` 存在、类型全对、构建全绿，但生产上没人
/// 调用它，于是最大的两个选号权重因子从未生效，且**无人发现**。把「有哪些后台任务」
/// 做成注册表数据后，就能用测试守住关键任务不被删除
/// （见 `tests::background_task_registry_includes_credits_refresh`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackgroundTask {
    /// 启动补跑：整理历史签到日志 + 签到核验 / 旅行派出领取 / 保活各跑一轮
    /// （受各自排程开关约束，关掉的任务一次都不跑）。
    StartupMaintenance,
    /// 自动轮换（按 `auto_rotate_config` 间隔；CodeBuddy CLI 为 CN 专有，保持 CN）。
    AutoRotate,
    /// 账号池余额刷新（真实余额回填选号权重）。
    CreditsRefresh,
    /// 六类积分定时任务之一（各自独立排程）。
    Scheduled(schedule::ScheduleTask),
}

/// 后台任务注册表：列出所有应启动的后台任务。
///
/// [`spawn_background_loops`] **严格**按本表启动；因此「增删后台任务」是数据变化，
/// 且有测试守住关键项。**登记即执行**——不要在本表之外直接 `tokio::spawn` 后台循环。
fn background_tasks() -> Vec<BackgroundTask> {
    let mut tasks = vec![
        BackgroundTask::StartupMaintenance,
        BackgroundTask::AutoRotate,
        BackgroundTask::CreditsRefresh,
    ];
    tasks.extend(
        schedule::ScheduleTask::all()
            .into_iter()
            .map(BackgroundTask::Scheduled),
    );
    tasks
}

/// 启动全部后台任务（以 [`background_tasks`] 为唯一事实来源）。
fn spawn_background_loops() {
    for task in background_tasks() {
        spawn_background_task(task);
    }
}

/// 按注册表条目派生对应的后台循环。
fn spawn_background_task(task: BackgroundTask) {
    match task {
        // 启动：整理历史签到日志 + 补跑一轮（签到核验 / 旅行派出领取 / 保活），
        // 各类是否补跑受自己的排程开关约束。与桌面端共用 core::scheduler 的同一份语义。
        BackgroundTask::StartupMaintenance => {
            tokio::spawn(async move {
                scheduler::run_startup_maintenance().await;
            });
        }
        // 自动轮换：按配置间隔执行。
        BackgroundTask::AutoRotate => {
            tokio::spawn(async move {
                let mut last_cycle_at: i64 = 0;
                loop {
                    let cfg = config::load_auto_rotate_config();
                    if cfg.get("enabled").and_then(|v| v.as_bool()) == Some(true) {
                        let interval_minutes = cfg
                            .get("check_interval_minutes")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(5)
                            .max(1);
                        let now = config::now_ms();
                        if now - last_cycle_at >= interval_minutes * 60_000 {
                            last_cycle_at = now;
                            let _ = rotate::run_rotate_cycle().await;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
            });
        }
        // 账号池余额刷新：**独立**循环（不与其它循环合并成一个 tick），首次启动先跑一次，
        // 之后按池配置 `credits_refresh_interval_ms`（默认 30 分钟）周期刷新。
        //
        // 为什么必须是独立后台循环：余额是慢变数据，在请求路径上同步拉余额会直接拉高
        // 每次请求的延迟。刷新语义见 `buddy_switch_gateway::credits_refresh`。
        BackgroundTask::CreditsRefresh => {
            tokio::spawn(async move {
                let state = gateway_host::shared_state();
                // 启动即刷一次：补齐上次进程遗留的「从未取过余额」账号。
                let _ = buddy_switch_gateway::credits_refresh::refresh_once(&state).await;
                loop {
                    let interval_ms = state
                        .pool
                        .read()
                        .await
                        .config()
                        .credits_refresh_interval_ms
                        .max(1);
                    tokio::time::sleep(std::time::Duration::from_millis(interval_ms as u64)).await;
                    let _ = buddy_switch_gateway::credits_refresh::refresh_once(&state).await;
                }
            });
        }
        // 六类积分任务各自独立排程。
        BackgroundTask::Scheduled(task) => spawn_scheduled_task(task),
    }
}

/// 状态快照的 `current` 段：**纯函数形态**（显式入参，测试用，不碰磁盘）。
///
/// 与 core `account::library_display_name_in` 同款拆分理由：**单测不该依赖进程级
/// `BUDDY_SWITCH_HOME`**。进程级 env 是全局状态，同一测试二进制内并行跑用例时，
/// 一个改 home 的用例会把其它用例一起带红（本仓已踩过多次）。
///
/// ⚠️ 展示字段一律归一（[`account::display_str`] / [`account::current_nickname_for`]）：
/// 认证文件里 `nickname` 可能是对象（脏值）或 WorkBuddy 5.6 的加密信封，裸透传会把
/// 对象当 React 子节点渲染。这是上游 PR #75 的**第 3 处同构透传点** ——
/// 本仓 `api.rs` / `commands.rs` 早已归一，CLI 这一处此前漏了
/// （PR #75 里对应的提交 `4b000ac` 就是「补上漏掉的那一处」）。
fn snapshot_from_root(root: &serde_json::Value) -> serde_json::Value {
    let acct = root.get("account").cloned().unwrap_or_else(|| json!({}));
    json!({
        "uid": account::display_str(&acct, "uid"),
        "nickname": account::current_nickname_for(Region::Cn, &acct),
        "email": account::display_str(&acct, "email"),
    })
}

/// 状态快照的 `current` 段（从认证文件读；无认证文件返回 `None`）。
fn current_account_snapshot() -> Option<serde_json::Value> {
    Some(snapshot_from_root(&auth_file::read_auth_file()?))
}

fn print_status() {
    let current = current_account_snapshot();
    let running = process::is_workbuddy_running();
    println!("Buddy Switch v{}", update::APP_VERSION);
    println!("WorkBuddy 运行中: {}", if running { "是" } else { "否" });
    match current {
        Some(c) => {
            let name = c
                .get("nickname")
                .and_then(|v| v.as_str())
                .or_else(|| c.get("email").and_then(|v| v.as_str()))
                .unwrap_or("未知");
            println!("当前账号: {name}");
        }
        None => println!("当前账号: 未登录"),
    }
    println!("账号数: {}", account::load_accounts().len());
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("serve");
    match cmd {
        "status" => print_status(),
        "version" | "--version" | "-V" => {
            println!("Buddy Switch {}", env!("CARGO_PKG_VERSION"));
        }
        "serve" | _ => serve(&args).await,
    }
}

async fn serve(args: &[String]) {
    let mut port = default_port();
    if let Some(i) = args.iter().position(|a| a == "--port") {
        if let Some(p) = args.get(i + 1).and_then(|p| p.parse::<u16>().ok()) {
            port = p;
        }
    }

    let app = api::router();
    let addr = format!("127.0.0.1:{port}");
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("启动失败: 端口 {port} 被占用或不可用（{e}）。可用 --port 指定其他端口。");
            std::process::exit(1);
        }
    };

    println!("Buddy Switch v{}", update::APP_VERSION);
    println!("webui: http://{addr}");
    println!("按 Ctrl+C 停止服务。");

    let no_open = args.iter().any(|a| a == "--no-open");
    if !no_open {
        open_browser(&addr);
    }

    spawn_background_loops();

    // 按配置启动 API 网关独立监听（默认关闭；默认 127.0.0.1:57891）。
    match gateway_host::apply().await {
        Ok(Some(addr)) => println!("API 网关: http://{addr}"),
        Ok(None) => {}
        Err(error) => eprintln!("[gateway] 启动失败: {error}"),
    }

    // 按配置启动 Trae 网关独立监听（默认关闭；默认 127.0.0.1:7864）。
    match trae_gateway_host::apply().await {
        Ok(Some(addr)) => println!("Trae API 网关: http://{addr}"),
        Ok(None) => {}
        Err(error) => eprintln!("[trae-gateway] 启动失败: {error}"),
    }

    axum::serve(listener, app).await.unwrap();
}

fn open_browser(addr: &str) {
    let url = format!("http://{addr}");
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let mut c = std::process::Command::new("cmd");
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW：开浏览器不闪 cmd 窗
        }
        let _ = c.args(["/C", "start", &url]).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 护栏：余额刷新任务**必须**登记在后台任务注册表里。
    ///
    /// 回归背景：本次修复的缺陷正是「调用点静默消失」——`Pool::set_credits` 存在、
    /// 类型全对、构建全绿，但生产上无人调用，于是四因子加权静默退化为两因子。
    /// 把「有哪些后台任务」做成注册表数据后，本用例守住关键任务不被删除：
    /// 从 [`background_tasks`] 移除 [`BackgroundTask::CreditsRefresh`] 会让本用例变红。
    #[test]
    fn background_task_registry_includes_credits_refresh() {
        let tasks = background_tasks();
        assert!(
            tasks.contains(&BackgroundTask::CreditsRefresh),
            "后台任务注册表必须登记 CreditsRefresh（账号池余额刷新）；缺了它生产上余额永远为 0，\
             四因子加权会静默退化为两因子。当前注册表：{tasks:?}"
        );
    }

    /// 注册表还须覆盖既有的启动维护 / 自动轮换 / 六类定时任务，防止被误删。
    #[test]
    fn background_task_registry_covers_all_existing_tasks() {
        let tasks = background_tasks();
        assert!(
            tasks.contains(&BackgroundTask::StartupMaintenance),
            "注册表缺少启动维护任务"
        );
        assert!(
            tasks.contains(&BackgroundTask::AutoRotate),
            "注册表缺少自动轮换任务"
        );
        for task in schedule::ScheduleTask::all() {
            assert!(
                tasks.contains(&BackgroundTask::Scheduled(task)),
                "注册表缺少定时任务：{}",
                task.as_str()
            );
        }
    }

    // ⚠️ 本模块**刻意不设**「改 `BUDDY_SWITCH_HOME` 的用例」：`api.rs` 的测试模块
    // 有它自己的 `TEST_LOCK`，两把锁互不相识 ⇒ 同一测试二进制内并行跑时照样互相踩
    // （2026-09-24 实测：本模块一个改 home 的用例把 `api.rs` 的 7 条用例带红）。
    // 凡需要隔离的取值逻辑，一律抽成**纯函数形态**（见 [`snapshot_from_root`]）。

    /// CLI `status` 的展示字段必须**归一**：对象 / 加密信封一律落 `null`。
    ///
    /// 这是上游 PR #75 的**第 3 处同构透传点**（本仓 `api.rs` / `commands.rs` 早已
    /// 归一，只有 CLI 漏了）。回归形态：裸透传会让 `nickname` 变成
    /// `{"$wbEncrypted":1,…}` 对象，终端 / 界面上就是 `[object Object]`。
    ///
    /// 可证伪：把 [`snapshot_from_root`] 改回 `acct.get("nickname")` 裸透传，
    /// 第一条断言（`is_null() || is_string()`）即红。
    #[test]
    fn status_snapshot_normalizes_display_fields() {
        // 脏值 + 加密信封：一律落 null（信封我方解不开，交给终端 / 前端回落）。
        let encrypted = snapshot_from_root(&json!({
            "account": {
                "uid": "uid-cli-probe",
                "nickname": {"$wbEncrypted": 1, "envelope": "x"},
                "email": {"zh": "对象脏值"},
            }
        }));
        for key in ["uid", "nickname", "email"] {
            let value = &encrypted[key];
            assert!(
                value.is_null() || value.is_string(),
                "{key} 必须是字符串或 null，不得是对象：{value}"
            );
        }
        assert_eq!(encrypted["uid"], "uid-cli-probe", "字符串 uid 必须原样保留");

        // 阳性对照：干净数据必须**逐字保留**，数字 uid 转文本。
        // （明文昵称走 `current_nickname_for` 的**早退**分支，不会去读账号库。）
        let clean = snapshot_from_root(&json!({
            "account": {"uid": 12345, "nickname": "小明", "email": "a@b.c"}
        }));
        assert_eq!(clean["uid"], "12345");
        assert_eq!(clean["nickname"], "小明");
        assert_eq!(clean["email"], "a@b.c");

        // 缺 `account` 段也不得 panic（历史 / 半截认证文件）。
        let empty = snapshot_from_root(&json!({}));
        assert!(empty["uid"].is_null());
        assert!(empty["nickname"].is_null());
    }
}
