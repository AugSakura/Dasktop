/**
 * 桌面文件类型由后端扫描真实桌面目录得到，前端只负责展示与分组映射
 */
export type DesktopItemKind = "file" | "folder" | "shell" | "shortcut" | "unknown";

/**
 * 桌面项目来源用于区分真实文件和 Windows Shell 虚拟对象，Box 仍统一用 path 作为映射键
 */
export type DesktopItemSource = "fileSystem" | "shell";

/**
 * Box 图标名称显示模式由全局设置控制，避免不同窗口之间出现同一文件命名规则不一致
 */
export type DesktopNameDisplayMode = "full" | "hideShortcutExtension" | "hideAllExtensions";

/**
 * 桌面项目模型，path 是 Box 映射和 Explorer 图标坐标同步的稳定主键
 */
export interface DesktopItem {
  id: string;
  name: string;
  path: string;
  extension: string | null;
  kind: DesktopItemKind;
  source: DesktopItemSource;
  shellId: string | null;
  /**
   * Windows Shell 解析出的原生图标，Box 和桌面列表复用同一份数据以避免拖入后视觉不一致
   */
  iconDataUrl: string | null;
}
