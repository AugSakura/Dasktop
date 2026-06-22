//! 系统托盘入口负责应用级命令分发，不直接读写 Box 数据

use tauri::menu::{CheckMenuItem, MenuBuilder, MenuEvent};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Wry};

use crate::application::system_tray_service;

const TRAY_ID: &str = "dasktop-tray";
const MENU_ID_SETTINGS: &str = "settings";
const MENU_ID_CREATE_BOX: &str = "create-box";
const MENU_ID_AUTOSTART: &str = "autostart";
const MENU_ID_DESKTOP_TAKEOVER: &str = "desktop-takeover";
const MENU_ID_QUIT: &str = "quit";

/// 托盘请求创建 Box 的前端事件；main WebView 作为隐藏控制器负责落库并同步原生层
pub const TRAY_CREATE_BOX_EVENT: &str = "dasktop://tray-create-box";
/// 自启状态变化事件用于同步隐藏设置窗里的开关状态
pub const AUTOSTART_CHANGED_EVENT: &str = "dasktop://autostart-changed";
/// 托盘请求切换桌面接管模式，由前端 Store 继续负责持久化和状态同步
pub const TRAY_TOGGLE_DESKTOP_TAKEOVER_EVENT: &str = "dasktop://tray-toggle-desktop-takeover";

/// 托盘菜单中需要跨入口同步的可变控件状态
pub struct AppTrayState {
    autostart_item: CheckMenuItem<Wry>,
    desktop_takeover_item: CheckMenuItem<Wry>,
}

/// 初始化系统托盘图标和右键菜单；菜单只分发动作，具体 Box 生命周期仍由前端统一处理
pub fn setup_app_tray(app: &App) -> tauri::Result<()> {
    let autostart_enabled =
        system_tray_service::resolve_autostart_enabled(app.handle()).unwrap_or(false);
    let autostart_item = CheckMenuItem::with_id(
        app,
        MENU_ID_AUTOSTART,
        "开机自启",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let desktop_takeover_item = CheckMenuItem::with_id(
        app,
        MENU_ID_DESKTOP_TAKEOVER,
        "桌面接管模式",
        true,
        true,
        None::<&str>,
    )?;
    let menu = MenuBuilder::new(app)
        .text(MENU_ID_SETTINGS, "设置")
        .text(MENU_ID_CREATE_BOX, "新增 Box")
        .separator()
        .item(&autostart_item)
        .item(&desktop_takeover_item)
        .separator()
        .text(MENU_ID_QUIT, "关闭")
        .build()?;
    let autostart_item_for_menu = autostart_item.clone();
    let desktop_takeover_item_for_menu = desktop_takeover_item.clone();
    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Dasktop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app_handle, event| {
            handle_tray_menu_event(
                app_handle,
                &event,
                &autostart_item_for_menu,
                &desktop_takeover_item_for_menu,
            );
        })
        .on_tray_icon_event(|tray, event| {
            if is_left_click_release(&event) {
                show_settings_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray_builder = tray_builder.icon(icon.clone());
    }

    tray_builder.build(app)?;
    app.manage(AppTrayState {
        autostart_item,
        desktop_takeover_item,
    });
    Ok(())
}

/// 同步托盘中“桌面接管模式”勾选状态；真实偏好仍以设置页 Store 为准
pub fn set_desktop_takeover_checked(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let tray_state = app
        .try_state::<AppTrayState>()
        .ok_or_else(|| "系统托盘尚未初始化，无法同步桌面接管状态".to_string())?;
    tray_state
        .desktop_takeover_item
        .set_checked(enabled)
        .map_err(|error| format!("无法同步托盘桌面接管状态：{error}"))?;

    Ok(enabled)
}

fn handle_tray_menu_event(
    app_handle: &AppHandle,
    event: &MenuEvent,
    autostart_item: &CheckMenuItem<Wry>,
    desktop_takeover_item: &CheckMenuItem<Wry>,
) {
    match event.id().as_ref() {
        MENU_ID_SETTINGS => show_settings_window(app_handle),
        MENU_ID_CREATE_BOX => request_create_box(app_handle),
        MENU_ID_AUTOSTART => {
            let next_enabled = autostart_item.is_checked().unwrap_or(false);
            if let Err(error) = system_tray_service::set_autostart_enabled(app_handle, next_enabled)
            {
                eprintln!("{error}");
                if let Ok(current_enabled) =
                    system_tray_service::resolve_autostart_enabled(app_handle)
                {
                    sync_autostart_state(app_handle, current_enabled);
                }
            } else {
                sync_autostart_state(app_handle, next_enabled);
            }
        }
        MENU_ID_DESKTOP_TAKEOVER => {
            request_toggle_desktop_takeover(
                app_handle,
                desktop_takeover_item.is_checked().unwrap_or(false),
            );
        }
        MENU_ID_QUIT => request_graceful_exit(app_handle),
        _ => {}
    }
}

/// 托盘“关闭”是真正退出应用，需要让设置 WebViewWindow 先走正常销毁链路
///
/// Windows WebView2/Chromium 在进程退出时会注销 `Chrome_WidgetWin_0` 等内部窗口类
/// 如果直接调用 `AppHandle::exit`，隐藏设置窗可能尚未收到 `Destroyed` 事件，
/// 底层清理就会和窗口销毁交错，从而打印 class unregister 失败日志。这里逐个
/// 关闭现有 WebViewWindow，让 Tauri 在最后一个窗口销毁后自然触发退出；最终桌面
/// 图标恢复仍由 `RunEvent::ExitRequested` 兜底执行
fn request_graceful_exit(app_handle: &AppHandle) {
    let webview_windows = app_handle.webview_windows();
    if webview_windows.is_empty() {
        app_handle.exit(0);
        return;
    }

    for (label, webview_window) in webview_windows {
        if let Err(error) = webview_window.close() {
            eprintln!("failed to close webview window {label} before exit: {error}");
        }
    }
}

fn is_left_click_release(event: &TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
    )
}

fn show_settings_window(app_handle: &AppHandle) {
    if let Some(settings_window) = app_handle.get_webview_window("main") {
        if let Err(error) = settings_window.unminimize() {
            eprintln!("failed to unminimize settings window: {error}");
        }
        if let Err(error) = settings_window.show() {
            eprintln!("failed to show settings window: {error}");
        }
        if let Err(error) = settings_window.set_focus() {
            eprintln!("failed to focus settings window: {error}");
        }
    }
}

fn request_create_box(app_handle: &AppHandle) {
    if let Err(error) = app_handle.emit_to("main", TRAY_CREATE_BOX_EVENT, ()) {
        eprintln!("failed to request tray box creation: {error}");
    }
}

fn request_toggle_desktop_takeover(app_handle: &AppHandle, enabled: bool) {
    if let Err(error) = app_handle.emit_to("main", TRAY_TOGGLE_DESKTOP_TAKEOVER_EVENT, enabled) {
        eprintln!("failed to request desktop takeover toggle: {error}");
    }
}

/// 同步托盘菜单和设置页中的自启状态，保证所有入口看到同一份系统状态。
pub fn sync_autostart_state(app: &AppHandle, enabled: bool) {
    if let Some(tray_state) = app.try_state::<AppTrayState>() {
        if let Err(error) = tray_state.autostart_item.set_checked(enabled) {
            eprintln!("failed to sync tray autostart check state: {error}");
        }
    }
    if let Err(error) = app.emit_to("main", AUTOSTART_CHANGED_EVENT, enabled) {
        eprintln!("failed to emit autostart state change: {error}");
    }
}
