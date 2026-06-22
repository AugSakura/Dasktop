use serde::Serialize;

/// 原生层把项目拖入 Box 时发出的事件，前端收到后写入 `box_items`。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeDesktopDropToBox {
    pub box_id: String,
    pub item_path: String,
    pub screen_x: i32,
    pub screen_y: i32,
}

/// 原生层把项目落回桌面时发出的事件，前端收到后删除映射并保存落点。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeDesktopDropToDesktop {
    pub source_box_id: Option<String>,
    pub item_path: String,
    pub x: i32,
    pub y: i32,
}

/// 原生 Box 标题栏拖动完成后发出的布局事件，前端负责持久化并回灌最新运行时状态。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeDesktopBoxMoved {
    pub box_id: String,
    pub x: i32,
    pub y: i32,
}

/// 原生接管失败事件，用于设置页展示真实错误并保持 Explorer 图标可见。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeDesktopError {
    pub message: String,
}

/// Explorer 图标落入 Box 后通知前端写入映射的事件名。
pub(crate) const NATIVE_DESKTOP_DROP_TO_BOX_EVENT: &str = "dasktop://desktop-takeover-drop-to-box";

/// Explorer 图标被拖出 Box 后通知前端删除映射的事件名。
pub(crate) const NATIVE_DESKTOP_DROP_TO_DESKTOP_EVENT: &str =
    "dasktop://desktop-takeover-drop-to-desktop";

/// 原生 Box 拖动结束后通知前端持久化新位置的事件名。
pub(crate) const NATIVE_DESKTOP_BOX_MOVED_EVENT: &str = "dasktop://desktop-takeover-box-moved";

/// 原生接管异常需要透传给前端设置页展示的事件名。
pub(crate) const NATIVE_DESKTOP_ERROR_EVENT: &str = "dasktop://native-desktop-error";
