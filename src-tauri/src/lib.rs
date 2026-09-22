// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod commands;
mod gateway;
mod trae_gateway;
#[cfg(desktop)]
mod tray;

use std::time::Duration;
use tauri::Manager;
use buddy_switch_core::modules;

const SCREENSHOT_DEMO_ENV: &str = "BUDDY_SWITCH_SCREENSHOT_DEMO";

pub(crate) fn is_screenshot_demo() -> bool {
    std::env::var(SCREENSHOT_DEMO_ENV).as_deref() == Ok("1")
}

/// 桌面端后台任务：**唯一事实来源**。
///
/// ⚠️ 回归背景（本表存在的原因）：此前 `spawn_background_loops` 直接写死四个循环
/// （签到 30 分钟 / 旅行 30 分钟 / 领取 15 分钟 / 保活每天一次），**完全没有排程**，
/// 于是设置页「定时任务排程」的六类开关与六份小时表在桌面端全部空转——「活跃地图」
/// 从未执行过一次。改成注册表后，「增删后台任务」是数据变化，且有测试守住
/// （见 `tests::background_tasks_cover_all_scheduled_tasks`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackgroundTask {
    /// 启动补跑：整理历史签到日志 + 签到核验 / 旅行派出领取 / 保活各跑一轮
    /// （**受各自排程开关约束**，关掉的任务一次都不跑）。
    StartupMaintenance,
    /// 自动轮换（按 `auto_rotate_config` 间隔；CodeBuddy CLI 为 CN 专有）。
    AutoRotate,
    /// 六类积分定时任务之一（按 `schedule_config` 的小时表，各自独立排程）。
    Scheduled(modules::schedule::ScheduleTask),
}

/// 后台任务注册表：**登记即执行**——不要在本表之外直接 `spawn` 后台循环。
fn background_tasks() -> Vec<BackgroundTask> {
    let mut tasks = vec![BackgroundTask::StartupMaintenance, BackgroundTask::AutoRotate];
    tasks.extend(
        modules::schedule::ScheduleTask::all()
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
        // 启动补跑：排程小时表之外的「今天该做但还没做」的一次性动作。
        // 语义与服务端 `buddy-switch-server` 完全一致（共用 core::scheduler）。
        BackgroundTask::StartupMaintenance => {
            tauri::async_runtime::spawn(async move {
                modules::scheduler::run_startup_maintenance().await;
            });
        }
        // 自动轮换（CodeBuddy CLI）：按配置间隔执行。
        BackgroundTask::AutoRotate => {
            tauri::async_runtime::spawn(async move {
                let mut last_rotate_at: i64 = 0;
                loop {
                    let rotate_cfg = modules::config::load_auto_rotate_config();
                    if rotate_cfg.get("enabled").and_then(|v| v.as_bool()) == Some(true) {
                        let interval_minutes = rotate_cfg
                            .get("check_interval_minutes")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(5)
                            .max(1);
                        let now = modules::config::now_ms();
                        if now - last_rotate_at >= interval_minutes * 60_000 {
                            last_rotate_at = now;
                            let _ = modules::rotate::run_rotate_cycle().await;
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            });
        }
        // 六类定时任务各自独立排程：每类一个循环，按自己的小时表 sleep 到点。
        // 排程语义在 core::scheduler 里，桌面端与服务端共用，**不在此处另写一份**。
        BackgroundTask::Scheduled(task) => {
            tauri::async_runtime::spawn(modules::scheduler::schedule_loop(task));
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![tray::SILENT_STARTUP_ARG]),
        ));
        builder = builder.on_window_event(tray::on_window_event);
    }

    let app = builder
        .setup(|app| {
            #[cfg(desktop)]
            {
                tray::setup(app)?;
                // 主窗口由配置创建为不可见；在事件循环呈现前决定本次启动是否静默。
                // 仅系统自启（精确 `--hidden` 参数）进入静默托盘，普通启动立即显示主窗口。
                tray::setup_startup_visibility(
                    app.handle(),
                    tray::is_silent_startup(std::env::args()),
                );
            }
            // 网关运行时句柄（管理命令依赖它查询/切换独立监听）。
            app.manage(gateway::GatewayRuntime::new());
            // Trae 网关运行时句柄（与上面那份**互不相干**：配置 / Key / 日志全独立）。
            app.manage(trae_gateway::TraeGatewayRuntime::new());
            // README 截图模式只渲染前端虚构数据，禁止读取账号后执行签到、轮换或保活。
            if !is_screenshot_demo() {
                spawn_background_loops();
                // 按配置启动 API 网关独立监听（默认关闭；默认 127.0.0.1:57891）。
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let runtime = handle.state::<gateway::GatewayRuntime>();
                    match runtime.apply().await {
                        Ok(Some(addr)) => eprintln!("[gateway] 已启动: http://{addr}"),
                        Ok(None) => {}
                        Err(error) => eprintln!("[gateway] 启动失败: {error}"),
                    }
                });
                // 按配置启动 Trae 网关独立监听（默认关闭；默认 127.0.0.1:7864）。
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let runtime = handle.state::<trae_gateway::TraeGatewayRuntime>();
                    match runtime.apply().await {
                        Ok(Some(addr)) => eprintln!("[trae-gateway] 已启动: http://{addr}"),
                        Ok(None) => {}
                        Err(error) => eprintln!("[trae-gateway] 启动失败: {error}"),
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_accounts,
            commands::get_codebuddy_cli_status,
            commands::install_codebuddy_cli_helper,
            commands::switch_codebuddy_cli_account,
            commands::get_codebuddy_cn_ide_status,
            commands::switch_codebuddy_cn_ide_account,
            commands::detect_codebuddy_cn_ide_account,
            commands::delete_account,
            commands::oauth_start,
            commands::oauth_status,
            commands::import_local,
            commands::export_accounts,
            commands::export_accounts_to_path,
            commands::preview_import_accounts,
            commands::import_accounts,
            commands::switch_account,
            commands::switch_progress,
            commands::list_sessions,
            commands::copy_sessions,
            commands::migrate_account_data,
            commands::set_account_remark,
            commands::get_switch_config,
            commands::save_switch_config,
            commands::open_permission_settings,
            commands::check_auth_permission,
            commands::reveal_app_in_finder,
            commands::open_accounts_dir,
            commands::get_checkin_status,
            commands::get_credit_expiry,
            commands::get_credit_statistics,
            commands::get_token_statistics,
            commands::checkin,
            commands::checkin_all,
            commands::get_auto_checkin_config,
            commands::save_auto_checkin_config,
            commands::get_checkin_logs,
            commands::get_travel_status,
            commands::get_auto_travel_config,
            commands::save_auto_travel_config,
            commands::get_schedule_config,
            commands::save_schedule_config,
            commands::run_schedule_task,
            commands::refresh_account_token,
            commands::get_auto_rotate_config,
            commands::save_auto_rotate_config,
            commands::rotate_status,
            commands::run_rotate,
            commands::get_rotate_logs,
            commands::get_github_config,
            commands::save_github_config,
            commands::check_update,
            commands::relaunch_app,
            commands::get_launch_at_login_enabled,
            commands::set_launch_at_login_enabled,
            commands::get_gateway_config,
            commands::save_gateway_config,
            commands::gateway_status,
            commands::list_api_keys,
            commands::create_api_key,
            commands::revoke_api_key,
            commands::delete_api_key,
            commands::get_gateway_models,
            commands::refresh_gateway_models,
            commands::get_account_strategy,
            commands::save_account_strategy,
            commands::get_gateway_logs,
            commands::clear_gateway_logs,
            // ---- Trae 模块（与 server 的 /api/trae/* 路由一一对应）----
            commands::get_trae_env,
            commands::get_trae_variants,
            commands::get_trae_capabilities,
            commands::get_trae_accounts,
            commands::get_trae_checkin_status,
            commands::get_trae_credits,
            commands::get_trae_token_statistics,
            commands::get_trae_logs,
            commands::get_trae_profiles,
            commands::get_trae_settings,
            commands::save_trae_settings,
            commands::trae_add_account,
            commands::trae_update_account,
            commands::trae_delete_account,
            commands::trae_import_local_account,
            commands::trae_oauth_start,
            commands::trae_oauth_status,
            commands::trae_oauth_cancel,
            commands::trae_export_accounts,
            commands::trae_export_accounts_to_path,
            commands::trae_preview_import_accounts,
            commands::trae_import_accounts,
            commands::trae_group_op,
            commands::trae_checkin,
            commands::trae_refresh_credits,
            commands::trae_refresh_jwt,
            commands::trae_clear_cooldown,
            commands::trae_switch_account,
            commands::trae_merge_legacy_regions,
            commands::trae_save_login,
            commands::trae_backup_profile,
            commands::trae_restore_profile,
            commands::trae_delete_profile,
            commands::trae_reset_device,
            // Trae API 网关（管理面）——与上面 WorkBuddy 网关的一组命令平行。
            commands::get_trae_gateway_config,
            commands::save_trae_gateway_config,
            commands::trae_gateway_status,
            commands::get_trae_gateway_models,
            // 多 Key 管理（含归属产品线）+ 打开数据目录（替代旧的单 Key regenerate）。
            commands::list_trae_api_keys,
            commands::create_trae_api_key,
            commands::revoke_trae_api_key,
            commands::delete_trae_api_key,
            commands::open_trae_data_dir,
            commands::get_trae_gateway_logs,
            commands::clear_trae_gateway_logs,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app_handle, event| {
        #[cfg(desktop)]
        tray::on_run_event(event);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 护栏：六类定时任务**必须**全部登记在桌面端后台任务表里。
    ///
    /// 回归背景：排程循环此前只存在于 `buddy-switch-server` 二进制，桌面端只有四个写死
    /// 周期的循环，于是设置页「定时任务排程」的全部控件在桌面端空转——最典型的是
    /// 「活跃地图」从未执行，用户在官网对照连登热力图发现始终没点亮。
    ///
    /// 可证伪性：从 [`background_tasks`] 移除任一 `Scheduled`（或整段六类循环）会让本用例变红。
    #[test]
    fn background_tasks_cover_all_scheduled_tasks() {
        let tasks = background_tasks();
        for task in modules::schedule::ScheduleTask::all() {
            assert!(
                tasks.contains(&BackgroundTask::Scheduled(task)),
                "桌面端后台任务表缺少定时任务「{}」——它在此进程里永远不会执行",
                task.as_str()
            );
        }
        assert!(
            tasks.contains(&BackgroundTask::StartupMaintenance),
            "注册表缺少启动补跑任务"
        );
        assert!(
            tasks.contains(&BackgroundTask::AutoRotate),
            "注册表缺少自动轮换任务"
        );
    }
}
