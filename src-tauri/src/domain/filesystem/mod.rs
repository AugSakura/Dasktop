//! 文件系统领域契约，表达跨模块共享且不依赖具体系统 API 的文件类型约定。

pub mod contract;

pub use contract::{
    is_windows_shortcut_extension, WINDOWS_SHORTCUT_EXTENSION, WINDOWS_SHORTCUT_SUFFIX,
};
