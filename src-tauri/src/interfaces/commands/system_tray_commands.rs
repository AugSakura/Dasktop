use crate::application::system_tray_service;
use crate::interfaces::tray;
use tauri::AppHandle;

/// 读取系统开机自启状态；状态来源是官方 autostart 插件，不写入前端 SQLite 设置表。
#[tauri::command]
pub fn is_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    system_tray_service::resolve_autostart_enabled(&app)
}

/// 切换系统开机自启状态，并同步托盘菜单与设置页开关。
#[tauri::command]
pub fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let next_enabled = system_tray_service::set_autostart_enabled(&app, enabled)?;
    tray::sync_autostart_state(&app, next_enabled);
    Ok(next_enabled)
}

/// 同步托盘菜单里的桌面接管勾选状态，设置值仍由前端 Store 持久化。
#[tauri::command]
pub fn set_tray_desktop_takeover_checked(app: AppHandle, enabled: bool) -> Result<bool, String> {
    tray::set_desktop_takeover_checked(&app, enabled)
}
