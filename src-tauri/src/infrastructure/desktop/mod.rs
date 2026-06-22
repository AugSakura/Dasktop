//! 桌面基础设施实现真实桌面扫描、系统 Shell 操作和 Windows 原生桌面接管。

mod desktop_item_scanner;
mod explorer_icon_visibility;
mod native_desktop;
mod native_desktop_log;
mod shell_item_actions;

pub use desktop_item_scanner::{resolve_shell_parsing_name, scan_desktop, scan_paths};
pub use explorer_icon_visibility::set_native_desktop_icons_hidden;
pub use native_desktop::{
    refresh_native_desktop_layer, set_desktop_takeover_enabled, shutdown_desktop_takeover,
    sync_native_desktop_state,
};
pub use native_desktop_log::log_native_desktop;
pub use shell_item_actions::{open_desktop_item, show_native_item_context_menu};
