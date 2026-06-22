import type { DesktopNameDisplayMode } from "@/entities/desktopItem/types";
import type { AppSettings, ThemeMode } from "./types";

/**
 * 数值型设置键由 AppSettings 自动推导，新增数值设置时会被类型系统要求补齐范围
 */
export type AppSettingNumberKey = {
  [Key in keyof AppSettings]: AppSettings[Key] extends number ? Key : never;
}[keyof AppSettings];

/**
 * 应用设置表和键名统一收口，避免 Store、数据库和设置面板各自硬编码字符串
 */
export const APP_SETTINGS_STORAGE = {
  databaseUrl: "sqlite:dasktop.db",
  tables: {
    appSettings: "app_settings",
    boxItems: "box_items",
    boxes: "boxes",
  },
} as const;

/**
 * 设置键名使用对象常量承载，调用保存逻辑时始终复用同一份字段定义
 */
export const APP_SETTING_KEYS = {
  boxBackgroundOpacity: "boxBackgroundOpacity",
  boxCollapseAnimationMs: "boxCollapseAnimationMs",
  boxCornerRadius: "boxCornerRadius",
  boxFilenameWidth: "boxFilenameWidth",
  boxIconGapX: "boxIconGapX",
  boxIconGapY: "boxIconGapY",
  boxIconSize: "boxIconSize",
  boxLabelTextSize: "boxLabelTextSize",
  boxResizeGridEnabled: "boxResizeGridEnabled",
  boxTheme: "boxTheme",
  desktopTakeoverEnabled: "desktopTakeoverEnabled",
  doubleClickOpenItems: "doubleClickOpenItems",
  nameDisplayMode: "nameDisplayMode",
  settingsTheme: "settingsTheme",
  showItemLabels: "showItemLabels",
  showShortcutArrow: "showShortcutArrow",
  snapThreshold: "snapThreshold",
  snapToEdges: "snapToEdges",
} as const satisfies Record<keyof AppSettings, keyof AppSettings>;

/**
 * 默认设置只包含当前版本真实生效的字段，旧外观参数不再兼容
 */
export const DEFAULT_APP_SETTINGS: AppSettings = {
  settingsTheme: "system",
  boxTheme: "system",
  boxResizeGridEnabled: true,
  // Explorer 原生模式只创建 Box 背景并协调桌面图标坐标，新安装时直接启用才能呈现 Fences 类体验。
  desktopTakeoverEnabled: true,
  snapToEdges: true,
  snapThreshold: 20,
  showItemLabels: true,
  showShortcutArrow: true,
  doubleClickOpenItems: true,
  nameDisplayMode: "full",
  boxBackgroundOpacity: 70,
  boxCollapseAnimationMs: 420,
  boxIconSize: 44,
  boxLabelTextSize: 12,
  boxIconGapX: 4,
  boxIconGapY: 4,
  boxFilenameWidth: 72,
  boxCornerRadius: 8,
};

/**
 * 数值设置的输入范围和持久化校验共用同一份配置，防止 UI 能写入数据库不接受的值
 */
export const APP_SETTING_NUMBER_LIMITS = {
  boxBackgroundOpacity: { min: 0, max: 100, step: 1, unit: "%" },
  boxCollapseAnimationMs: { min: 120, max: 600, step: 10, unit: "ms" },
  boxCornerRadius: { min: 0, max: 24, step: 1, unit: "px" },
  boxFilenameWidth: { min: 56, max: 180, step: 1, unit: "px" },
  boxIconGapX: { min: 0, max: 32, step: 1, unit: "px" },
  boxIconGapY: { min: 0, max: 32, step: 1, unit: "px" },
  boxIconSize: { min: 28, max: 96, step: 1, unit: "px" },
  boxLabelTextSize: { min: 9, max: 16, step: 0.5, unit: "px" },
  snapThreshold: { min: 8, max: 64, step: 1, unit: "px" },
} as const satisfies Record<
  AppSettingNumberKey,
  {
    max: number;
    min: number;
    step: number;
    unit: string;
  }
>;

/**
 * 支持的设置键由默认设置派生，新增设置时只需要补齐默认值和校验逻辑
 */
export const SUPPORTED_APP_SETTING_KEYS = Object.values(APP_SETTING_KEYS);

/**
 * 明暗主题枚举和 UI 选项共用同一份值，避免设置读取接受 UI 不存在的状态
 */
export const THEME_MODES = ["light", "system", "dark"] as const satisfies readonly ThemeMode[];

/**
 * Box 文件名显示模式集中定义，设置页和数据库校验使用同一组可选值
 */
export const DESKTOP_NAME_DISPLAY_MODES = [
  "full",
  "hideShortcutExtension",
  "hideAllExtensions",
] as const satisfies readonly DesktopNameDisplayMode[];

/**
 * 数值设置统一做范围夹取和小数精度归一，避免无效历史数据撑破布局
 */
export function sanitizeNumberAppSetting<Key extends AppSettingNumberKey>(
  key: Key,
  value: unknown,
): AppSettings[Key] {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return DEFAULT_APP_SETTINGS[key] as AppSettings[Key];
  }

  const limit = APP_SETTING_NUMBER_LIMITS[key];
  const clampedValue = Math.min(Math.max(value, limit.min), limit.max);
  const decimalPlaces = getDecimalPlaces(limit.step);

  return Number(clampedValue.toFixed(decimalPlaces)) as AppSettings[Key];
}

/**
 * 步长决定持久化精度，0.5 这类设置需要保留一位小数
 */
function getDecimalPlaces(step: number): number {
  const [, decimal = ""] = String(step).split(".");

  return decimal.length;
}
