//! Desktop tray: menu-bar icon, dock visibility, lightweight mode, and check-in.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;
use tauri::menu::{CheckMenuItem, Menu, MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, Runtime, WebviewWindowBuilder, Window, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use buddy_switch_core::modules::{checkin, update};

const TRAY_ID: &str = "main-menu-bar";
const MAIN_WINDOW_LABEL: &str = "main";
/// 展示名统一为 `Buddy Switch`（带空格）；构建标识仍是 `BuddySwitch`
/// （productName / .app / 安装包文件名），两者刻意不同名，详见 README 的命名约定。
const DEFAULT_TOOLTIP: &str = "Buddy Switch";
const CHECKIN_TOOLTIP_RESTORE_SECS: u64 = 8;

/// 系统自启注册的启动参数：仅携带该精确参数的启动进入静默托盘模式。
pub const SILENT_STARTUP_ARG: &str = "--hidden";

static LIGHTWEIGHT_MODE: AtomicBool = AtomicBool::new(false);
static CHECKIN_BUSY: AtomicBool = AtomicBool::new(false);
static TOOLTIP_GENERATION: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "macos")]
static DOCK_ICON_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn setup(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = build_tray_menu(app)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_icon())
        // template image 是 macOS 菜单栏独有的语义（系统只取 alpha 通道着色）；
        // 在 Windows / Linux 上它是 no-op，RGB 会原样生效——所以彩色素材（见
        // `color_icon`）必须在这个开关为 false 的平台上使用，反之亦然。
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(DEFAULT_TOOLTIP)
        .menu(&menu)
        // macOS 惯例是左键展开菜单；Windows / Linux 惯例是左键=主操作（唤出主窗口），
        // 菜单交给右键。两个开关必须与下面的 `on_tray_icon_event` 保持一致。
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_tray_icon_event(|tray, event| {
            // macOS 左键必须继续展开菜单——这是 template 图标的既定行为，
            // 也是上游专门修过的回归点，所以这里先按平台短路。
            if cfg!(target_os = "macos") {
                return;
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open-main-window" => show_main_window(app),
            "open-github" => open_github(app),
            "checkin-all" => start_checkin_all(app),
            "lightweight-mode" => toggle_lightweight(app),
            "quit-app" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
        apply_dock_visible(window.app_handle(), false);
        emit_main_window_visible(window.app_handle(), false);
    }
}

/// 判断本次启动是否携带精确的 `--hidden` 参数（系统自启触发）。
///
/// 必须整参相等，禁止子串匹配，避免 `--hidden-x`、`x--hidden` 等误入静默模式。
pub fn is_silent_startup(args: impl IntoIterator<Item = impl AsRef<str>>) -> bool {
    args.into_iter()
        .any(|arg| arg.as_ref() == SILENT_STARTUP_ARG)
}

/// 在事件循环呈现应用前决定首次启动的主窗口可见性。
///
/// `main` 窗口由 `tauri.conf.json` 配置创建为不可见，此处做出第一次
/// show / hide 决策，静默启动不会先闪出主窗口：
///
/// - 普通启动（无 `--hidden`）：走既有 `show_main_window` 路径，
///   先恢复 Regular / Dock，再 show / unminimize / focus。
/// - 静默启动（`--hidden`）：窗口保持隐藏，隐藏 Dock / 任务栏入口，
///   只保留托盘；不设置 `LIGHTWEIGHT_MODE`（WebView 仍然存在）。
///
/// 之后从托盘「打开主界面」仍走 `show_main_window`，与隐藏窗口完全一致。
pub fn setup_startup_visibility<R: Runtime>(app: &AppHandle<R>, silent: bool) {
    if silent {
        apply_dock_visible(app, false);
        emit_main_window_visible(app, false);
    } else {
        show_main_window(app);
    }
}

fn emit_main_window_visible<R: Runtime>(app: &AppHandle<R>, visible: bool) {
    let _ = app.emit("main-window-visible", visible);
}

/// Keep the tray process alive when the last window is destroyed (lightweight mode).
///
/// `code == None` is Tauri's runtime exit after zero windows remain.
/// `code == Some(_)` is an explicit `app.exit()` / restart — let those through.
pub fn on_run_event(event: RunEvent) {
    if let RunEvent::ExitRequested { api, code, .. } = event {
        if should_keep_tray_alive(code) {
            api.prevent_exit();
        }
    }
}

fn should_keep_tray_alive(code: Option<i32>) -> bool {
    code.is_none()
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    // Recreate if the WebView is gone even when the flag is already false
    // (e.g. destroy() completed after a failed lightweight toggle).
    if LIGHTWEIGHT_MODE.load(Ordering::Acquire)
        || app.get_webview_window(MAIN_WINDOW_LABEL).is_none()
    {
        exit_lightweight(app);
        return;
    }
    apply_dock_visible(app, true);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    emit_main_window_visible(app, true);
}

#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows")),
    allow(unused_variables)
)]
fn apply_dock_visible<R: Runtime>(app: &AppHandle<R>, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let policy = if visible {
            ActivationPolicy::Regular
        } else {
            ActivationPolicy::Accessory
        };
        let _ = app.set_dock_visibility(visible);
        let _ = app.set_activation_policy(policy);
        if visible {
            restore_macos_dock_icon(app);
        } else {
            DOCK_ICON_GENERATION.fetch_add(1, Ordering::AcqRel);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
            let _ = window.set_skip_taskbar(!visible);
        }
    }
}

/// Re-apply the Dock icon after returning to Regular.
///
/// `TransformProcessType` rebuilds the Dock tile from the running executable.
/// Packaged `.app` binaries have no icon of their own, and `tauri dev` is named
/// `exec`, so both need an explicit restore. Tauri only sets
/// `setApplicationIconImage` once on `RunEvent::Ready` (dev only).
/// `TransformProcessType` is asynchronous, so we also re-apply after a short delay.
///
/// Do **not** feed the raw `icon.png` here: it is a full-bleed opaque square.
/// `setApplicationIconImage` then bypasses the system squircle, which is why the
/// Dock icon lost rounded corners and changed size after “打开主窗口”.
#[cfg(target_os = "macos")]
fn restore_macos_dock_icon<R: Runtime>(app: &AppHandle<R>) {
    let generation = DOCK_ICON_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    apply_macos_app_icon();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if DOCK_ICON_GENERATION.load(Ordering::Acquire) != generation {
            return;
        }
        let _ = app.run_on_main_thread(move || {
            if DOCK_ICON_GENERATION.load(Ordering::Acquire) == generation {
                apply_macos_app_icon();
            }
        });
    });
}

#[cfg(target_os = "macos")]
fn apply_macos_app_icon() {
    use objc2::AllocAnyThread;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    // SAFETY: tray / window events and run_on_main_thread all run on the AppKit main thread.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);

    if let Some(icon) = macos_bundle_dock_icon() {
        unsafe { app.setApplicationIconImage(Some(&icon)) };
        return;
    }

    // `tauri dev` has no `.app` bundle; keep the PNG fallback so Dock is not "exec".
    const APP_ICON_PNG: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/icon.png"));
    let data = NSData::with_bytes(APP_ICON_PNG);
    let Some(icon) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    unsafe { app.setApplicationIconImage(Some(&icon)) };
}

/// Finder-composited app icon (system squircle already applied).
#[cfg(target_os = "macos")]
fn macos_bundle_dock_icon() -> Option<objc2::rc::Retained<objc2_app_kit::NSImage>> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let bundle = app_bundle_path_from_exe(&exe)?;
    let path = NSString::from_str(&bundle.to_string_lossy());
    Some(NSWorkspace::sharedWorkspace().iconForFile(&path))
}

/// `Foo.app/Contents/MacOS/binary` → `Foo.app`.
#[cfg(any(target_os = "macos", test))]
fn app_bundle_path_from_exe(exe: &std::path::Path) -> Option<&std::path::Path> {
    let macos_dir = exe.parent()?;
    if macos_dir.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents = macos_dir.parent()?;
    if contents.file_name()?.to_str()? != "Contents" {
        return None;
    }
    let bundle = contents.parent()?;
    (bundle.extension()?.to_str()? == "app").then_some(bundle)
}

fn toggle_lightweight<R: Runtime>(app: &AppHandle<R>) {
    if LIGHTWEIGHT_MODE.load(Ordering::Acquire) {
        exit_lightweight(app);
    } else {
        enter_lightweight(app);
    }
}

fn enter_lightweight<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        if window.destroy().is_err() {
            // CheckMenuItem may have already toggled visually; restore it.
            refresh_tray_menu(app);
            return;
        }
    }
    apply_dock_visible(app, false);
    LIGHTWEIGHT_MODE.store(true, Ordering::Release);
    refresh_tray_menu(app);
}

fn exit_lightweight<R: Runtime>(app: &AppHandle<R>) {
    // Regular / dock first so a from_config window is not created while Accessory.
    apply_dock_visible(app, true);
    if app.get_webview_window(MAIN_WINDOW_LABEL).is_none() {
        let Some(config) = app
            .config()
            .app
            .windows
            .iter()
            .find(|window| window.label == MAIN_WINDOW_LABEL)
            .cloned()
        else {
            apply_dock_visible(app, false);
            refresh_tray_menu(app);
            return;
        };
        if WebviewWindowBuilder::from_config(app, &config)
            .and_then(|builder| builder.build())
            .is_err()
        {
            apply_dock_visible(app, false);
            refresh_tray_menu(app);
            return;
        }
    }
    // Window exists now: Windows skip_taskbar was a no-op before recreate.
    apply_dock_visible(app, true);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    emit_main_window_visible(app, true);
    LIGHTWEIGHT_MODE.store(false, Ordering::Release);
    refresh_tray_menu(app);
}

fn open_github<R: Runtime>(app: &AppHandle<R>) {
    let url = format!(
        "https://github.com/{}/{}",
        update::GITHUB_OWNER,
        update::GITHUB_REPO
    );
    let _ = app.opener().open_url(url, None::<&str>);
}

struct CheckinBusyGuard<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> Drop for CheckinBusyGuard<R> {
    fn drop(&mut self) {
        CHECKIN_BUSY.store(false, Ordering::Release);
        refresh_tray_menu(&self.app);
    }
}

fn start_checkin_all<R: Runtime>(app: &AppHandle<R>) {
    if crate::is_screenshot_demo() {
        set_tray_tooltip(app, "README 截图演示模式");
        return;
    }
    if CHECKIN_BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    bump_tooltip_generation();
    refresh_tray_menu(app);
    set_tray_tooltip(app, "正在签到…");

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _busy = CheckinBusyGuard { app: app.clone() };
        let payload = checkin::run_checkin_all().await;
        let text = format_checkin_tooltip(&payload);
        if checkin_succeeded(&payload) {
            notify_checkin(&app, &text);
        } else {
            let generation = bump_tooltip_generation();
            set_tray_tooltip(&app, &text);
            restore_tooltip_after(app, generation);
        }
    });
}

fn notify_checkin<R: Runtime>(app: &AppHandle<R>, body: &str) {
    let _ = app
        .notification()
        .builder()
        .title(DEFAULT_TOOLTIP)
        .body(body)
        .show();
}

fn checkin_succeeded(value: &Value) -> bool {
    let Some(accounts) = value.get("accounts").and_then(Value::as_array) else {
        return false;
    };
    !accounts.is_empty()
        && accounts.iter().all(|account| {
            matches!(
                account.get("result").and_then(Value::as_str),
                Some("success" | "already")
            )
        })
}

fn restore_tooltip_after<R: Runtime>(app: AppHandle<R>, generation: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(CHECKIN_TOOLTIP_RESTORE_SECS)).await;
        if TOOLTIP_GENERATION.load(Ordering::Acquire) == generation {
            set_tray_tooltip(&app, DEFAULT_TOOLTIP);
        }
    });
}

fn bump_tooltip_generation() -> u64 {
    TOOLTIP_GENERATION.fetch_add(1, Ordering::AcqRel) + 1
}

fn set_tray_tooltip<R: Runtime>(app: &AppHandle<R>, text: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(text));
    }
}

fn refresh_tray_menu<R: Runtime>(app: &AppHandle<R>) {
    let Ok(menu) = build_tray_menu(app) else {
        return;
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn build_tray_menu<R: Runtime, M: Manager<R>>(app: &M) -> tauri::Result<Menu<R>> {
    let open_item = MenuItem::with_id(app, "open-main-window", "打开主界面", true, None::<&str>)?;
    let github_item = MenuItem::with_id(app, "open-github", "打开 GitHub", true, None::<&str>)?;
    let checked_in = checkin::all_accounts_checked_in_today();
    let (checkin_label, checkin_enabled) = if CHECKIN_BUSY.load(Ordering::Acquire) {
        ("一键签到", false)
    } else if checked_in {
        ("已签到", false)
    } else {
        ("一键签到", true)
    };
    let checkin_item = MenuItem::with_id(
        app,
        "checkin-all",
        checkin_label,
        checkin_enabled,
        None::<&str>,
    )?;
    let lightweight_item = CheckMenuItem::with_id(
        app,
        "lightweight-mode",
        "轻量模式",
        true,
        LIGHTWEIGHT_MODE.load(Ordering::Acquire),
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit-app", "退出应用", true, None::<&str>)?;

    MenuBuilder::new(app)
        .item(&open_item)
        .item(&github_item)
        .item(&checkin_item)
        .separator()
        .item(&lightweight_item)
        .separator()
        .item(&quit_item)
        .build()
}

/// 当前平台要用的托盘图标。
///
/// macOS 菜单栏是 template image：系统只取 alpha、颜色由菜单栏前景色着色
/// ⇒ 用 36x36 纯白剪影（RGB 被忽略，深浅色模式都自适应）。
/// Windows / Linux 没有 template 语义，RGB 原样生效 ⇒ 用 32x32 彩色素材，
/// 否则浅色任务栏下白色剪影几乎不可见。
///
/// 这里刻意用运行时 `cfg!` 而不是 `#[cfg]`：两个素材函数必须在**所有**平台都能
/// 编译，否则单测没法在 macOS 上验证彩色变体（反向同理）。
fn tray_icon() -> tauri::image::Image<'static> {
    if cfg!(target_os = "macos") {
        menu_bar_icon()
    } else {
        color_icon()
    }
}

/// macOS 菜单栏 template image：36x36 纯白剪影，颜色交给系统。
fn menu_bar_icon() -> tauri::image::Image<'static> {
    const ICON: &[u8; 36 * 36 * 4] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/icons/tray-icon-template.rgba"
    ));
    tauri::image::Image::new(ICON, 36, 36)
}

/// Windows / Linux 通知区图标：32x32，RGB 取自源图真实颜色、alpha 为同一掩码。
fn color_icon() -> tauri::image::Image<'static> {
    const COLOR_ICON: &[u8; 32 * 32 * 4] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/icons/tray-icon-color.rgba"
    ));
    tauri::image::Image::new(COLOR_ICON, 32, 32)
}

fn format_checkin_tooltip(value: &Value) -> String {
    if value.get("status").and_then(Value::as_str) == Some("skipped")
        && value.get("reason").and_then(Value::as_str) == Some("already_running")
    {
        return "签到任务正在进行，请稍后再试".to_string();
    }
    let Some(accounts) = value.get("accounts").and_then(Value::as_array) else {
        return "没有可签到的账号".to_string();
    };
    if accounts.is_empty() {
        return "没有可签到的账号".to_string();
    }

    let mut ok = 0;
    let mut already = 0;
    let mut err = 0;
    for account in accounts {
        match account.get("result").and_then(Value::as_str) {
            Some("success") => ok += 1,
            Some("already") => already += 1,
            Some("error") => err += 1,
            _ => {}
        }
    }
    format!("签到完成：成功 {ok}，已签 {already}，失败 {err}")
}

#[cfg(test)]
mod tests {
    use super::{
        color_icon, format_checkin_tooltip, is_silent_startup, menu_bar_icon,
        should_keep_tray_alive,
    };
    use serde_json::json;

    #[test]
    fn runtime_exit_with_no_code_keeps_tray() {
        assert!(should_keep_tray_alive(None));
        assert!(!should_keep_tray_alive(Some(0)));
    }

    #[test]
    fn menu_bar_icon_has_transparency_and_antialiasing() {
        let icon = menu_bar_icon();
        assert_eq!((icon.width(), icon.height()), (36, 36));
        assert!(icon.rgba().chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(icon
            .rgba()
            .chunks_exact(4)
            .any(|pixel| (1..=254).contains(&pixel[3])));
        // macOS 只取 alpha，RGB 必须保持纯白；这条也顺手挡住「把彩色素材接到
        // template 上」的误改。
        assert!(icon
            .rgba()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0)
            .all(|pixel| pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255));
    }

    /// 本次缺陷的护栏：Windows / Linux 没有 template 语义、RGB 原样生效，
    /// 退回「全白剪影」实现会让最后一条断言变红。
    #[test]
    fn color_icon_is_32px_with_source_colors() {
        let icon = color_icon();
        assert_eq!((icon.width(), icon.height()), (32, 32));
        assert!(icon.rgba().chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(icon.rgba().chunks_exact(4).any(|pixel| {
            if pixel[3] == 0 {
                return false;
            }
            let rgb = &pixel[..3];
            let max = rgb.iter().max().copied().unwrap_or(0);
            let min = rgb.iter().min().copied().unwrap_or(0);
            max - min > 12
        }));
    }

    #[test]
    fn silent_startup_matches_exact_hidden_arg() {
        assert!(is_silent_startup(["--hidden"]));
        assert!(is_silent_startup(["buddy-switch-rust", "--hidden"]));
        assert!(is_silent_startup(["buddy-switch-rust", "--hidden", "--debug"]));
    }

    #[test]
    fn silent_startup_rejects_unrelated_and_substring_args() {
        assert!(!is_silent_startup(Vec::<&str>::new()));
        assert!(!is_silent_startup(["buddy-switch-rust"]));
        assert!(!is_silent_startup(["buddy-switch-rust", "--debug"]));
        assert!(!is_silent_startup(["buddy-switch-rust", "--hidden=true"]));
        assert!(!is_silent_startup(["buddy-switch-rust", "-hidden"]));
        assert!(!is_silent_startup(["buddy-switch-rust", "--hidden-x"]));
        assert!(!is_silent_startup(["buddy-switch-rust", "x--hidden"]));
    }

    #[test]
    fn tooltip_when_no_accounts() {
        assert_eq!(
            format_checkin_tooltip(&json!({"accounts": []})),
            "没有可签到的账号"
        );
    }

    #[test]
    fn tooltip_when_accounts_missing() {
        assert_eq!(format_checkin_tooltip(&json!({})), "没有可签到的账号");
    }

    #[test]
    fn tooltip_reports_overlapping_checkin_as_busy() {
        assert_eq!(
            format_checkin_tooltip(&json!({
                "accounts": [],
                "status": "skipped",
                "reason": "already_running"
            })),
            "签到任务正在进行，请稍后再试"
        );
    }

    #[test]
    fn tooltip_summarizes_success_already_error() {
        let payload = json!({
            "accounts": [
                {"result": "success"},
                {"result": "success"},
                {"result": "already"},
                {"result": "error"},
                {"result": "error"},
                {"result": "error"}
            ]
        });
        assert_eq!(
            format_checkin_tooltip(&payload),
            "签到完成：成功 2，已签 1，失败 3"
        );
    }

    #[test]
    fn checkin_succeeded_requires_all_ok() {
        use super::checkin_succeeded;
        assert!(!checkin_succeeded(&json!({})));
        assert!(!checkin_succeeded(&json!({"accounts": []})));
        assert!(checkin_succeeded(&json!({
            "accounts": [{"result": "success"}, {"result": "already"}]
        })));
        assert!(!checkin_succeeded(&json!({
            "accounts": [{"result": "success"}, {"result": "error"}]
        })));
    }

    #[test]
    fn app_bundle_path_from_packaged_exe() {
        use std::path::Path;
        let exe = Path::new("/Applications/BuddySwitch.app/Contents/MacOS/buddy-switch-rust");
        assert_eq!(
            super::app_bundle_path_from_exe(exe),
            Some(Path::new("/Applications/BuddySwitch.app"))
        );
    }

    #[test]
    fn app_bundle_path_none_outside_app_bundle() {
        use std::path::Path;
        assert!(super::app_bundle_path_from_exe(Path::new("/tmp/exec")).is_none());
        assert!(
            super::app_bundle_path_from_exe(Path::new("/Users/x/target/debug/buddy-switch-rust"))
                .is_none()
        );
        assert!(super::app_bundle_path_from_exe(Path::new(
            "/Applications/BuddySwitch.app/Contents/Resources/icon.icns"
        ))
        .is_none());
    }

    #[test]
    fn tooltip_ignores_unknown_results() {
        let payload = json!({
            "accounts": [
                {"result": "success"},
                {"result": "skipped"},
                {"result": null}
            ]
        });
        assert_eq!(
            format_checkin_tooltip(&payload),
            "签到完成：成功 1，已签 0，失败 0"
        );
    }
}
