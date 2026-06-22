//! 原生桌面接管应用服务负责隔离 Tauri 入口和 Win32 控制器线程。

use crate::domain::desktop::NativeDesktopState;
use crate::infrastructure::desktop;
use tauri::AppHandle;

/// 切换 Windows Explorer 桌面图标层可见性，主要用于异常恢复和兼容旧设置入口。
pub fn set_native_desktop_icons_hidden(hidden: bool) -> Result<(), String> {
    desktop::set_native_desktop_icons_hidden(hidden)
}

/// 开启或关闭 Dasktop 原生桌面接管层。
pub fn set_desktop_takeover_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    desktop::set_desktop_takeover_enabled(app, enabled)
}

/// 将前端持久化源同步给原生接管运行时，Rust 只保留内存态副本。
pub fn sync_native_desktop_state(app: &AppHandle, state: NativeDesktopState) -> Result<(), String> {
    desktop::sync_native_desktop_state(app, state)
}

/// 主动刷新原生桌面层，用于 Explorer 重启、设置变化或重新扫描后的兜底同步。
pub fn refresh_native_desktop_layer(app: &AppHandle) -> Result<(), String> {
    desktop::refresh_native_desktop_layer(app)
}

/// 应用退出时关闭接管线程并恢复 Explorer 图标层。
pub fn shutdown_desktop_takeover() {
    desktop::shutdown_desktop_takeover();
}
