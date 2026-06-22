/**
 * 原生 Box 面板默认尺寸和最小尺寸统一在这里维护，Store 与 Win32 控制器保持一致
 */
export const BOX_WINDOW_SIZE = {
  default: {
    width: 320,
    height: 320,
  },
  min: {
    width: 240,
    height: 240,
  },
} as const;

/**
 * Box 自身的默认交互偏好只随单个窗口保存，不进入全局 app_settings
 */
export const BOX_DEFAULT_STATE = {
  collapsed: false,
  locked: false,
  titleOpacity: 100,
  titlePosition: "top",
} as const;

/**
 * Box 闲置可见度使用百分比表达；0 表示鼠标未进入 Box 区域时整体不可见
 */
export const BOX_TITLE_OPACITY = {
  max: 100,
  min: 0,
  step: 1,
} as const;

/**
 * 新建 Box 使用固定起点和错位步长，避免连续创建时窗口完全重叠
 */
export const BOX_WINDOW_PLACEMENT = {
  initialX: 96,
  initialY: 96,
  cascadeStep: 28,
} as const;

/**
 * 设置窗聚焦同步需要避开标题栏拖动首帧，延迟值集中维护便于后续调优
 */
export const SETTINGS_WINDOW_SYNC_TIMING = {
  focusSyncDelayMs: 260,
  dragReleaseFallbackMs: 1200,
} as const;

/**
 * 主题过渡只在主动切换时短暂启用，避免启动和拖动时出现多余动画
 */
export const THEME_TRANSITION_CONFIG = {
  className: "theme-transition",
  timeoutMs: 260,
} as const;
