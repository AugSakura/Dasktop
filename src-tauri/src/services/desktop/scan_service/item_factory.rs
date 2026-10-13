//! Box 文件项展示模型构建与排序逻辑。

use std::path::Path;

use crate::domain::desktop::{DesktopItem, DesktopItemKind, DesktopItemSource};
use crate::domain::filesystem::is_windows_shortcut_extension;
use crate::infrastructure::filesystem::naming;
use crate::infrastructure::windows::shell_icon;

use super::entry::FileItemCacheSignature;

/// 前端 DOM ID 不保留 Windows 路径分隔符和空格等字符，统一替换成安全下划线。
const ITEM_ID_REPLACEMENT_CHAR: char = '_';

/// 将真实文件系统路径转换成前端展示模型；打开、重命名和删除仍以后续路径操作为准。
pub(super) fn create_desktop_item(path: &Path, signature: &FileItemCacheSignature) -> DesktopItem {
    let path_text = naming::display_path(path);

    DesktopItem {
        extension: path
            .extension()
            .map(|value| value.to_string_lossy().to_string()),
        icon_data_url: shell_icon::resolve_item_icon_data_url(path),
        id: stable_item_id(&path_text),
        kind: resolve_item_kind(path, signature.is_dir),
        name: resolve_item_name(path),
        path: path_text,
        shell_id: None,
        source: DesktopItemSource::FileSystem,
    }
}

/// 文件夹优先、名称其次的排序接近 Explorer 默认直觉；缓存 key 避免 sort 比较时重复分配小写字符串。
pub(super) fn sort_desktop_items(items: &mut [DesktopItem]) {
    items.sort_by_cached_key(|item| {
        (
            !matches!(item.kind, DesktopItemKind::Folder),
            item.name.to_lowercase(),
            item.path.to_lowercase(),
        )
    });
}

/// 根目录这类路径没有 file_name，使用完整路径作为兜底展示名可以避免空白项目。
fn resolve_item_name(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| naming::display_path(path))
}

/// 文件类型只用于选择前端后备图标，不参与真实文件的打开方式判断；`.lnk` 和 `.url` 同属快捷方式家族。
fn resolve_item_kind(path: &Path, is_dir: bool) -> DesktopItemKind {
    if is_dir {
        return DesktopItemKind::Folder;
    }

    match path.extension().and_then(|value| value.to_str()) {
        Some(extension) if is_windows_shortcut_extension(extension) => DesktopItemKind::Shortcut,
        Some(_) => DesktopItemKind::File,
        None => DesktopItemKind::Unknown,
    }
}

/// 基于完整路径生成稳定 ID，避免重名文件在前端列表中互相覆盖。
fn stable_item_id(path_text: &str) -> String {
    path_text
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                ITEM_ID_REPLACEMENT_CHAR
            }
        })
        .collect()
}
