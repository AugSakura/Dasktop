import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 托盘菜单请求隐藏 main WebView 创建 Box，前端写入 SQLite 后同步给原生桌面层
 */
export const TRAY_CREATE_BOX_EVENT = "dasktop://tray-create-box";

/**
 * 系统自启状态可能由托盘或设置页修改，通过事件让所有设置入口显示同一状态
 */
export const AUTOSTART_CHANGED_EVENT = "dasktop://autostart-changed";

/**
 * 托盘菜单请求切换桌面接管模式，最终写入仍由设置 Store 完成
 */
export const TRAY_TOGGLE_DESKTOP_TAKEOVER_EVENT =
  "dasktop://tray-toggle-desktop-takeover";

/**
 * 监听托盘创建 Box 请求；处理方应创建 Box 并触发原生控制器重绘
 */
export async function listenTrayCreateBox(
  handler: (event: Event<Record<string, never>>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<Record<string, never>>(TRAY_CREATE_BOX_EVENT, handler);
}

/**
 * 监听系统自启状态变化，避免托盘菜单和设置页开关互相滞后
 */
export async function listenAutostartChanged(
  handler: (event: Event<boolean>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<boolean>(AUTOSTART_CHANGED_EVENT, handler);
}

/**
 * 监听托盘桌面接管请求；payload 是托盘当前勾选后的目标状态
 */
export async function listenTrayToggleDesktopTakeover(
  handler: (event: Event<boolean>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<boolean>(TRAY_TOGGLE_DESKTOP_TAKEOVER_EVENT, handler);
}
