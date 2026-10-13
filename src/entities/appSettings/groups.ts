import { Monitor, Moon, Sun } from "@lucide/vue";
import type { Component } from "vue";
import {
  APP_SETTING_KEYS,
  APP_SETTING_NUMBER_LIMITS,
  type AppSettingBooleanKey,
  type AppSettingNumberKey,
} from "@/entities/appSettings/defaults";
import type {
  BoxConflictPolicy,
  BoxDeletePolicy,
  BoxDropAction,
  ThemeMode,
} from "@/entities/appSettings/types";
import type { DesktopNameDisplayMode } from "@/entities/desktopItem/types";

/**
 * 主题分段选项由设置页多个面板共享，集中维护能避免同一枚举出现不同文案。
 */
export const THEME_SEGMENT_OPTIONS: Array<{
  icon: Component;
  label: string;
  value: ThemeMode;
}> = [
  { icon: Sun, label: "浅色", value: "light" },
  { icon: Monitor, label: "跟随系统", value: "system" },
  { icon: Moon, label: "深色", value: "dark" },
];

/**
 * 删除策略文案按风险从低到高排列，让用户清楚删除 Box 时真实文件的去向。
 */
export const BOX_DELETE_POLICY_OPTIONS: Array<{
  label: string;
  value: BoxDeletePolicy;
}> = [
  { label: "移回桌面", value: "moveContentsToDesktop" },
  { label: "保留文件夹", value: "keepFolder" },
  { label: "进回收站", value: "recycleFolder" },
];

/**
 * 拖拽策略文案从“是否改变原文件”切入，帮助用户理解复制、移动和映射的真实差异。
 */
export const BOX_DROP_ACTION_OPTIONS: Array<{
  label: string;
  value: BoxDropAction;
}> = [
  { label: "移动", value: "move" },
  { label: "复制", value: "copy" },
  { label: "映射", value: "map" },
];

/**
 * 同名策略默认使用自动重命名，既保留已有文件，也避免 Windows 冲突弹窗被 Box 遮挡。
 */
export const BOX_CONFLICT_POLICY_OPTIONS: Array<{
  label: string;
  value: BoxConflictPolicy;
}> = [
  { label: "自动重命名", value: "rename" },
  { label: "跳过同名", value: "skip" },
  { label: "替换已有", value: "replace" },
];

/**
 * 文件名显示选项只改变 Box 标签文本，不改变真实文件名和磁盘路径。
 */
export const NAME_DISPLAY_MODE_OPTIONS: Array<{
  label: string;
  value: DesktopNameDisplayMode;
}> = [
  { label: "完整", value: "full" },
  { label: "隐藏快捷方式", value: "hideShortcutExtension" },
  { label: "隐藏全部后缀", value: "hideAllExtensions" },
];

/**
 * 规则显示项属于 Box 使用行为，集中配置后由规则显示面板渲染。
 */
export const FILE_BEHAVIOR_SETTING_CONTROLS: Array<{
  description: string;
  key: AppSettingBooleanKey;
  label: string;
}> = [
  {
    key: APP_SETTING_KEYS.showItemLabels,
    label: "显示文件名",
    description: "关闭后 Box 内只显示图标，适合极简桌面",
  },
  {
    key: APP_SETTING_KEYS.showShortcutArrow,
    label: "显示快捷方式角标",
    description: "只影响 `.lnk` 和 `.url` 的视觉角标，不改变快捷方式文件",
  },
  {
    key: APP_SETTING_KEYS.autoHideNativeShellIcons,
    label: "收纳系统图标后隐藏原生图标",
    description: "此电脑、回收站等系统图标进入 Box 后，将从 Windows 桌面隐藏",
  },
  {
    key: APP_SETTING_KEYS.doubleClickOpenItems,
    label: "双击打开",
    description: "关闭后单击即可打开文件，拖拽仍需要先移动超过阈值",
  },
];

/**
 * 窗口行为开关与吸附距离相邻展示，便于用户理解 resize 和拖动都属于窗口手感设置。
 */
export const WINDOW_BEHAVIOR_SETTING_CONTROLS: Array<{
  description: string;
  key: AppSettingBooleanKey;
  label: string;
}> = [
  {
    key: APP_SETTING_KEYS.boxResizeGridEnabled,
    label: "按网格调整大小",
    description: "拖拽窗口边缘时吸附到完整图标行列，减少半截空位",
  },
];

/**
 * Box 收缩时序项独立成组，避免动画和延迟参数混入尺寸、圆角等视觉密度配置。
 */
export type BoxCollapseTimingSettingKey = Extract<
  AppSettingNumberKey,
  | "boxCollapseAnimationMs"
  | "boxCollapseDelayMs"
  | "boxIdleOpacityHideAnimationMs"
  | "boxIdleOpacityShowAnimationMs"
>;

/**
 * Box 外观数值项排除吸附距离和收缩时序，避免把窗口行为参数混入视觉密度卡片。
 */
export type BoxVisualSettingKey = Exclude<
  AppSettingNumberKey,
  "snapThreshold" | BoxCollapseTimingSettingKey
>;

/**
 * 外观面板当前承载 Box 视觉和收缩手感两类数值项，统一类型便于复用滑块保存事件。
 */
export type BoxAppearanceNumberSettingKey = BoxVisualSettingKey | BoxCollapseTimingSettingKey;

/**
 * Box 收缩时序调节项集中渲染在独立卡片，便于用户把动画速度和离开后的等待时间一起理解。
 */
export const BOX_COLLAPSE_TIMING_SETTING_CONTROLS: Array<{
  description: string;
  key: BoxCollapseTimingSettingKey;
  label: string;
  max: number;
  min: number;
  step: number;
  unit: string;
}> = [
  {
    key: "boxCollapseAnimationMs",
    label: "收缩动画速度",
    description: "调整 Box 自动收起和展开的动画时长",
    ...APP_SETTING_NUMBER_LIMITS.boxCollapseAnimationMs,
  },
  {
    key: "boxCollapseDelayMs",
    label: "离开后延迟收缩",
    description: "鼠标离开 Box 后等待多久再开始自动收起",
    ...APP_SETTING_NUMBER_LIMITS.boxCollapseDelayMs,
  },
  {
    key: "boxIdleOpacityShowAnimationMs",
    label: "可见度淡入时长",
    description: "Box 从闲置透明状态恢复到完全可见时的动画时间",
    ...APP_SETTING_NUMBER_LIMITS.boxIdleOpacityShowAnimationMs,
  },
  {
    key: "boxIdleOpacityHideAnimationMs",
    label: "可见度淡出时长",
    description: "Box 收缩后回到闲置透明状态时的动画时间",
    ...APP_SETTING_NUMBER_LIMITS.boxIdleOpacityHideAnimationMs,
  },
];

/**
 * Box 外观调节项随外观面板渲染，保证主题、透明度和视觉密度配置入口一致。
 */
export const BOX_VISUAL_SETTING_CONTROLS: Array<{
  description: string;
  key: BoxVisualSettingKey;
  label: string;
  max: number;
  min: number;
  step: number;
  unit: string;
}> = [
  {
    key: "boxBackgroundOpacity",
    label: "背景透明度",
    description: "控制 Box 背景与桌面壁纸的融合程度",
    ...APP_SETTING_NUMBER_LIMITS.boxBackgroundOpacity,
  },
  {
    key: "boxCornerRadius",
    label: "圆角大小",
    description: "调整 Box 面板和文件图标悬停区域的圆角",
    ...APP_SETTING_NUMBER_LIMITS.boxCornerRadius,
  },
  {
    key: "boxIconSize",
    label: "图标大小",
    description: "调整 Box 内项目图标的显示尺寸",
    ...APP_SETTING_NUMBER_LIMITS.boxIconSize,
  },
  {
    key: "boxLabelTextSize",
    label: "文字大小",
    description: "调整文件名文字大小，适配不同分辨率",
    ...APP_SETTING_NUMBER_LIMITS.boxLabelTextSize,
  },
  {
    key: "boxIconGapX",
    label: "横向间距",
    description: "调整图标列之间的水平距离",
    ...APP_SETTING_NUMBER_LIMITS.boxIconGapX,
  },
  {
    key: "boxIconGapY",
    label: "纵向间距",
    description: "调整图标行之间的垂直距离",
    ...APP_SETTING_NUMBER_LIMITS.boxIconGapY,
  },
  {
    key: "boxFilenameWidth",
    label: "文件名宽度",
    description: "控制文件名换行宽度，长名称会在此范围内显示",
    ...APP_SETTING_NUMBER_LIMITS.boxFilenameWidth,
  },
];
