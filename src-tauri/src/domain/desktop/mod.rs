//! 桌面领域模型定义前端持久化状态与原生接管运行时之间的稳定数据契约。

mod types;

pub use types::{
    DesktopItem, DesktopItemKind, DesktopItemSource, DesktopSnapshot, NativeBoxItem,
    NativeBoxState, NativeBoxTitlePosition, NativeDesktopDisplaySettings, NativeDesktopState,
    NativeIconContainerKind, NativeIconPosition,
};

/// 统一归一化桌面项目主键，避免 Windows 路径大小写和分隔符差异导致 Box 映射失效。
pub fn normalize_item_key(path: &str) -> String {
    path.trim().replace('/', "\\").to_lowercase()
}
