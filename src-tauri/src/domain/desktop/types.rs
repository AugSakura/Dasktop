use serde::{Deserialize, Serialize};

/// 桌面文件类型只描述渲染语义，不改变真实文件行为
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopItemKind {
    File,
    Folder,
    Shell,
    Shortcut,
    Unknown,
}

/// 桌面项目来源决定后端使用文件系统路径还是 Shell PIDL 执行打开、右键和拖放解析
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopItemSource {
    FileSystem,
    Shell,
}

/// 桌面图标模型，path 是 Box 映射和 Explorer 坐标同步的稳定主键
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopItem {
    pub id: String,
    pub name: String,
    pub path: String,
    pub extension: Option<String>,
    pub kind: DesktopItemKind,
    pub icon_data_url: Option<String>,
    pub source: DesktopItemSource,
    pub shell_id: Option<String>,
}

/// 桌面快照由真实桌面目录扫描得出，前端据此重绘 UI
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSnapshot {
    pub desktop_path: String,
    pub items: Vec<DesktopItem>,
}

/// Explorer-native 桌面模式下的完整运行时状态，由前端持久化源同步给 Win32 控制器
///
/// 控制器只持有该结构的内存副本，用来绘制 Box 背景并读写 Explorer 自己的桌面图标坐标。
/// 真实文件位置仍保持不变，所有 Box 映射、布局和图标位置写入继续由前端 Store 与 SQLite 负责。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeDesktopState {
    pub items: Vec<DesktopItem>,
    pub boxes: Vec<NativeBoxState>,
    pub box_items: Vec<NativeBoxItem>,
    pub icon_positions: Vec<NativeIconPosition>,
    pub display: NativeDesktopDisplaySettings,
}

/// 原生 Box 在物理屏幕坐标系中的布局和展示偏好
///
/// 这些字段来自前端持久化状态，Rust 只根据它们绘制和命中测试，不直接写数据库。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeBoxState {
    pub collapsed: bool,
    pub id: String,
    pub locked: bool,
    pub theme: String,
    pub title: String,
    pub title_opacity: i32,
    pub title_position: NativeBoxTitlePosition,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Box 标题位置直接对应前端设置，避免原生层硬编码上下布局
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeBoxTitlePosition {
    Top,
    Bottom,
}

/// Box 与桌面项目的映射，原生层按该表决定 Explorer 图标应停靠在哪个 Box 区域
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeBoxItem {
    pub box_id: String,
    pub item_path: String,
    pub order_index: i32,
}

/// 图标位置所属容器，桌面和 Box 共享同一套位置模型
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeIconContainerKind {
    Desktop,
    Box,
}

/// 图标持久化位置，使用项目稳定路径键和物理屏幕坐标保存
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeIconPosition {
    pub item_path: String,
    pub container_kind: NativeIconContainerKind,
    pub container_id: Option<String>,
    pub monitor_id: Option<String>,
    pub x: i32,
    pub y: i32,
    pub order_index: Option<i32>,
}

/// 原生 Box 背景和 Explorer 图标停靠布局所需的显示参数，来自用户设置而非硬编码在 Win32 控制器中
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeDesktopDisplaySettings {
    pub icon_size: i32,
    pub label_text_size: f32,
    pub label_width: i32,
    pub icon_gap_x: i32,
    pub icon_gap_y: i32,
    pub corner_radius: i32,
    pub box_background_opacity: i32,
    pub show_item_labels: bool,
    pub show_shortcut_arrow: bool,
}
