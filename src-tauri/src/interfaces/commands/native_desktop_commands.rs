use crate::application::native_desktop_service;
use crate::domain::desktop::NativeDesktopState;
use tauri::AppHandle;

/// 切换 Windows Explorer 原生桌面图标层可见性；真实文件和图标坐标不会被修改。
#[tauri::command]
pub fn set_native_desktop_icons_hidden(hidden: bool) -> Result<(), String> {
    native_desktop_service::set_native_desktop_icons_hidden(hidden)
}

/// 切换 Fences 模式的同进程原生桌面辅助层；Explorer 原生图标层始终保持可见。
#[tauri::command]
pub fn set_desktop_takeover_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    native_desktop_service::set_desktop_takeover_enabled(&app, enabled)
}

/// 将前端持久化源同步给原生桌面层，Rust 只保存运行时副本并据此绘制原生 Box。
#[tauri::command]
pub fn sync_native_desktop_state(app: AppHandle, state: NativeDesktopState) -> Result<(), String> {
    native_desktop_service::sync_native_desktop_state(&app, state)
}

/// 请求原生桌面层重新布局和重绘，用于显示器、DPI 或桌面快照变化后的兜底刷新。
#[tauri::command]
pub fn refresh_native_desktop_layer(app: AppHandle) -> Result<(), String> {
    native_desktop_service::refresh_native_desktop_layer(&app)
}
