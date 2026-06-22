//! 原生桌面接管基础设施入口，负责组合运行时、状态模型、事件契约和平台实现。

mod events;
mod model;
mod runtime;
#[cfg(target_os = "windows")]
mod windows;

pub use runtime::{
    refresh_native_desktop_layer, set_desktop_takeover_enabled, shutdown_desktop_takeover,
    sync_native_desktop_state,
};
