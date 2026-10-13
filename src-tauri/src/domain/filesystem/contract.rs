//! 文件系统契约集中表达跨模块共享的文件类型约定。

/// Windows 快捷方式扩展名，Box 映射模式和 Shell 图标解析都依赖该类型判断。
pub const WINDOWS_SHORTCUT_EXTENSION: &str = "lnk";

/// Windows Internet 快捷方式扩展名，Steam 等平台用它承载协议跳转，桌面交互和 `.lnk` 一致。
pub const WINDOWS_URL_SHORTCUT_EXTENSION: &str = "url";

/// Windows 快捷方式文件名后缀，生成映射文件时避免重复拼写 `.lnk`。
pub const WINDOWS_SHORTCUT_SUFFIX: &str = ".lnk";

/// 判断扩展名是否属于 Windows 快捷方式家族；`.lnk` 和 `.url` 在桌面上承载相同的跳转语义，
/// Explorer 也对两者一视同仁地显示快捷方式角标，比较时按大小写不敏感处理。
pub fn is_windows_shortcut_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case(WINDOWS_SHORTCUT_EXTENSION)
        || extension.eq_ignore_ascii_case(WINDOWS_URL_SHORTCUT_EXTENSION)
}
