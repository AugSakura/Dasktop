/**
 * 设置页只保留浅色、跟随系统和深色三种主题，避免与当前 UI 产生不一致的旧状态
 */
export type ThemeMode = "light" | "system" | "dark";

/**
 * 删除文件夹型 Box 时的文件处理策略，危险操作交给 Windows Shell 保留系统确认和撤销能力
 */
export type BoxDeletePolicy = "moveContentsToDesktop" | "keepFolder" | "recycleFolder";

/**
 * 外部文件拖入 Box 后的处理方式；映射会创建快捷方式，避免移动真实文件位置。
 */
export type BoxDropAction = "copy" | "move" | "map";

/**
 * Box 文件传输遇到同名目标时的处理方式；默认自动重命名以避免系统冲突弹窗。
 */
export type BoxConflictPolicy = "rename" | "skip" | "replace";

/**
 * 应用设置只保存文件夹型 Box 真实使用的偏好，旧版参数不再兼容
 */
export interface AppSettings {
  /**
   * 设置窗口独立使用的明暗主题，不再影响桌面上的 Box 窗口
   */
  settingsTheme: ThemeMode;
  /**
   * Box 窗口独立使用的明暗主题，便于整理面板和设置页采用不同外观
   */
  boxTheme: ThemeMode;
  /**
   * 新建 Box 文件夹的根目录；修改后只影响后续新建 Box，不移动已有 Box 文件夹
   */
  collectionRootPath: string;
  /**
   * 删除 Box 时如何处理对应的真实收纳文件夹
   */
  boxDeletePolicy: BoxDeletePolicy;
  /**
   * 外部文件拖入 Box 后执行的真实文件操作
   */
  boxDropAction: BoxDropAction;
  /**
   * Box 内文件拖出到桌面后执行的真实文件操作；与拖入策略分开保存以适配不同整理习惯
   */
  boxDragOutAction: BoxDropAction;
  /**
   * Box 文件复制、移动或映射时遇到同名目标的处理策略
   */
  boxConflictPolicy: BoxConflictPolicy;
  /**
   * 是否显示 Box 内文件名标签；隐藏后 Box 更接近纯图标工作区
   */
  showItemLabels: boolean;
  /**
   * 快捷方式是否显示角标，关闭后仍不改变真实 `.lnk`、`.url` 文件
   */
  showShortcutArrow: boolean;
  /**
   * 系统桌面图标进入 Box 后是否自动隐藏 Windows 原生桌面上的同名入口
   */
  autoHideNativeShellIcons: boolean;
  /**
   * 是否使用双击打开文件；关闭后单击即可打开，更适合触控板快速整理
   */
  doubleClickOpenItems: boolean;
  /**
   * 文件名展示规则只影响标签文本，不改变真实文件名
   */
  nameDisplayMode: DesktopNameDisplayMode;
  /**
   * Box 拖动时是否吸附屏幕或其他 Box 边缘
   */
  snapToEdges: boolean;
  /**
   * 手动调整 Box 尺寸时是否按当前图标网格吸附，保证窗口边界落在完整行列上
   */
  boxResizeGridEnabled: boolean;
  /**
   * 吸附阈值使用逻辑像素保存，和窗口拖动坐标保持同一体系
   */
  snapThreshold: number;
  /**
   * Box 背景透明度使用百分比保存，便于设置页直接用滑块表达
   */
  boxBackgroundOpacity: number;
  /**
   * Box 收缩和展开动画的持续时间，数值越小动画速度越快
   */
  boxCollapseAnimationMs: number;
  /**
   * 鼠标离开 Box 后等待多久再收缩，给用户移动到菜单或边缘操作留出缓冲
   */
  boxCollapseDelayMs: number;
  /**
   * Box 从闲置透明状态恢复到完全可见时的淡入动画时长
   */
  boxIdleOpacityShowAnimationMs: number;
  /**
   * Box 回到闲置透明状态时的淡出动画时长
   */
  boxIdleOpacityHideAnimationMs: number;
  /**
   * Box 内系统图标的显示尺寸，真实图像仍由 Windows Shell 解析
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
import type { DesktopNameDisplayMode } from "@/entities/desktopItem/types";
