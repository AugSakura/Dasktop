//! Tauri 命令适配层只做参数接收和服务转发，避免把 Shell/Win32 细节暴露给前端入口。

pub mod desktop_item_commands;
pub mod native_desktop_commands;
pub mod system_tray_commands;
