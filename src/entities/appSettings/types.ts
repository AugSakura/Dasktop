import type { DesktopNameDisplayMode } from "@/entities/desktopItem/types";

/**
 * 设置页只保留浅色、跟随系统和深色三种主题，避免与当前 UI 产生不一致的旧状态
 */
export type ThemeMode = "light" | "system" | "dark";

/**
 * 应用设置只保存当前版本真实使用的偏好，旧外观参数不再兼容
 */
export interface AppSettings {
  /**
   * 设置窗口独立使用的明暗主题，不影响原生桌面上的 Box 面板
   */
  settingsTheme: ThemeMode;
  /**
   * 原生 Box 面板独立使用的明暗主题，便于整理区域和设置页采用不同外观
   */
  boxTheme: ThemeMode;
  /**
   * Box resize 时是否按完整图标行列调整尺寸，关闭后允许连续像素级缩放
   */
  boxResizeGridEnabled: boolean;
  snapToEdges: boolean;
  snapThreshold: number;
  /**
   * 是否启用 Dasktop 的原生桌面接管模式
   */
  desktopTakeoverEnabled: boolean;
  showItemLabels: boolean;
  showShortcutArrow: boolean;
  doubleClickOpenItems: boolean;
  nameDisplayMode: DesktopNameDisplayMode;
  /**
   * Box 背景透明度使用百分比保存，便于设置页直接用滑块表达
   */
  boxBackgroundOpacity: number;
  /**
   * Box 收缩和展开动画的持续时间，数值越小动画速度越快
   */
  boxCollapseAnimationMs: number;
  /**
   * Box 内系统图标的最大显示尺寸，真实图像仍由 Windows Shell 解析
   */
  boxIconSize: number;
  /**
   * Box 内文件名的字号，和图标尺寸独立配置以适配不同桌面密度
   */
  boxLabelTextSize: number;
  /**
   * Box 图标列之间的横向间距，控制整理面板的信息密度
   */
  boxIconGapX: number;
  /**
   * Box 图标行之间的纵向间距，避免长文件名换行后互相遮挡
   */
  boxIconGapY: number;
  /**
   * Box 文件名的换行宽度，不改变真实文件名
   */
  boxFilenameWidth: number;
  /**
   * Box 面板和图标命中区域的圆角，保持桌面组件视觉一致
   */
  boxCornerRadius: number;
}
