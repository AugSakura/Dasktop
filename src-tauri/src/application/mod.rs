//! 应用服务层负责业务编排，Tauri 命令和托盘入口只通过这里调用后端能力。

pub mod desktop_item_service;
pub mod native_desktop_service;
pub mod system_tray_service;
