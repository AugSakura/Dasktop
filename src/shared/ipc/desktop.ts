import { emit, listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 桌面状态同步事件用于连接设置页、托盘和原生控制器，真实数据仍以 SQLite 为准
 */
export const DESKTOP_STATE_CHANGED_EVENT = "dasktop-desktop-state-changed";

/**
 * 状态变更范围用于接收方判断刷新成本，设置变更只需要重读偏好，Box 变更才需要同步布局映射
 */
export type DesktopStateChangeScope = "settings" | "boxes" | "desktop";

/**
 * 跨窗口事件必须带上来源标识，防止当前窗口收到自己广播后重复读取数据库造成交互抖动
 */
export interface DesktopStateChangedPayload {
  sourceId: string;
  scope: DesktopStateChangeScope;
}

/**
 * 广播桌面扩展状态变更，真实数据仍以 SQLite 为准，事件只负责提示其他窗口重新读取
 */
export async function notifyDesktopStateChanged(payload: DesktopStateChangedPayload): Promise<void> {
  await emit(DESKTOP_STATE_CHANGED_EVENT, payload);
}

/**
 * 监听其他窗口的桌面状态变更，回调内部应从持久化层读取最新数据而不是信任事件载荷
 */
export async function listenDesktopStateChanged(
  handler: (event: Event<DesktopStateChangedPayload>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<DesktopStateChangedPayload>(DESKTOP_STATE_CHANGED_EVENT, handler);
}
