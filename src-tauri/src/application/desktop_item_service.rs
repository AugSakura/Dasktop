//! 桌面应用服务聚合扫描、打开和右键菜单能力，避免 Tauri 命令直接依赖 Shell/Win32 细节。

use crate::domain::desktop::{DesktopItem, DesktopSnapshot};
use crate::infrastructure::desktop;

/// 获取当前桌面快照，并把基础设施错误收敛为前端可展示的中文错误。
pub fn get_desktop_snapshot() -> Result<DesktopSnapshot, String> {
    desktop::scan_desktop().map_err(|error| error.to_string())
}

/// 按路径补齐桌面项目元信息，供 Box 收纳桌面目录之外的项目时复用。
pub fn get_desktop_items_by_paths(paths: &[String]) -> Result<Vec<DesktopItem>, String> {
    desktop::scan_paths(paths).map_err(|error| error.to_string())
}

/// 使用系统默认行为打开桌面项目，保留 Explorer 对快捷方式、Shell 项和普通文件的语义。
pub fn open_desktop_item(path: String) -> Result<(), String> {
    desktop::open_desktop_item(path)
}

/// 展示 Windows 原生右键菜单，菜单内容和命令执行都交给 Shell。
pub fn show_native_item_context_menu(
    window: tauri::WebviewWindow,
    path: String,
    screen_x: i32,
    screen_y: i32,
) -> Result<(), String> {
    desktop::show_native_item_context_menu(window, path, screen_x, screen_y)
}
