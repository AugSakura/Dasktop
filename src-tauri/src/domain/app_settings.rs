//! 应用设置契约常量；当前设置由前端 SQLite 插件写入，Rust 侧保留同名契约避免后续重复硬编码

/// 应用设置表名，必须与前端 `APP_SETTINGS_STORAGE.tables.appSettings` 保持一致
pub const APP_SETTINGS_TABLE: &str = "app_settings";

/// 应用设置键名，必须与前端 `APP_SETTING_KEYS` 保持一致
pub mod keys {
    /// Box 背景透明度百分比
    pub const BOX_BACKGROUND_OPACITY: &str = "boxBackgroundOpacity";
    /// Box 收缩和展开动画持续时间
    pub const BOX_COLLAPSE_ANIMATION_MS: &str = "boxCollapseAnimationMs";
    /// Box 整体和图标命中区域圆角
    pub const BOX_CORNER_RADIUS: &str = "boxCornerRadius";
    /// Box 文件名换行宽度
    pub const BOX_FILENAME_WIDTH: &str = "boxFilenameWidth";
    /// Box 图标横向间距
    pub const BOX_ICON_GAP_X: &str = "boxIconGapX";
    /// Box 图标纵向间距
    pub const BOX_ICON_GAP_Y: &str = "boxIconGapY";
    /// Box 图标显示尺寸
    pub const BOX_ICON_SIZE: &str = "boxIconSize";
    /// Box 文件名字号
    pub const BOX_LABEL_TEXT_SIZE: &str = "boxLabelTextSize";
    /// 原生 Box 面板主题模式
    pub const BOX_THEME: &str = "boxTheme";
    /// Box 内项目是否需要双击打开
    pub const DOUBLE_CLICK_OPEN_ITEMS: &str = "doubleClickOpenItems";
    /// Box 内文件名后缀显示策略
    pub const NAME_DISPLAY_MODE: &str = "nameDisplayMode";
    /// 是否启用 Dasktop 的原生桌面接管模式
    pub const DESKTOP_TAKEOVER_ENABLED: &str = "desktopTakeoverEnabled";
    /// 设置页主题模式
    pub const SETTINGS_THEME: &str = "settingsTheme";
    /// Box 内是否显示项目名称
    pub const SHOW_ITEM_LABELS: &str = "showItemLabels";
    /// Box 内是否显示快捷方式箭头标记
    pub const SHOW_SHORTCUT_ARROW: &str = "showShortcutArrow";
    /// Box 吸附阈值
    pub const SNAP_THRESHOLD: &str = "snapThreshold";
    /// Box 是否吸附屏幕或其他 Box 边缘
    pub const SNAP_TO_EDGES: &str = "snapToEdges";
}
