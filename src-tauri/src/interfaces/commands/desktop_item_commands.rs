use crate::application::desktop_item_service;
use crate::domain::desktop::{DesktopItem, DesktopSnapshot};

/// 获取当前桌面文件快照，前端会基于这些真实文件维护 Box 映射和设置页展示。
#[tauri::command]
pub fn get_desktop_snapshot() -> Result<DesktopSnapshot, String> {
    desktop_item_service::get_desktop_snapshot()
}

/// 按路径解析任意磁盘文件元信息，让 Box 可以收纳桌面目录之外的项目。
#[tauri::command]
pub fn get_desktop_items_by_paths(paths: Vec<String>) -> Result<Vec<DesktopItem>, String> {
    desktop_item_service::get_desktop_items_by_paths(&paths)
}

/// 使用系统默认程序打开桌面项目，保持快捷方式、文件夹和普通文件与 Windows Explorer 一致。
#[tauri::command]
pub fn open_desktop_item(path: String) -> Result<(), String> {
    desktop_item_service::open_desktop_item(path)
}

/// 在桌面项目上弹出 Windows Shell 原生右键菜单，菜单项和执行逻辑全部交给系统。
#[tauri::command]
pub fn show_native_item_context_menu(
    window: tauri::WebviewWindow,
    path: String,
    screen_x: i32,
    screen_y: i32,
) -> Result<(), String> {
    desktop_item_service::show_native_item_context_menu(window, path, screen_x, screen_y)
}
