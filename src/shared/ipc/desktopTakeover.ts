import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 原生桌面层请求打开项目，前端收到后复用现有 Shell 打开命令
 */
export const DESKTOP_TAKEOVER_OPEN_EVENT = "dasktop://desktop-takeover-open-item";

/**
 * 原生桌面层请求弹出项目右键菜单，菜单仍由 Rust Shell `IContextMenu` 实现
 */
export const DESKTOP_TAKEOVER_CONTEXT_MENU_EVENT =
  "dasktop://desktop-takeover-context-menu";

/**
 * 原生桌面层把项目拖入 Box 时发出的事件，前端负责写入 Box 映射
 */
export const DESKTOP_TAKEOVER_DROP_TO_BOX_EVENT =
  "dasktop://desktop-takeover-drop-to-box";

/**
 * Box 或原生桌面层把项目落回桌面时发出的事件，前端负责删除映射并保存落点
 */
export const DESKTOP_TAKEOVER_DROP_TO_DESKTOP_EVENT =
  "dasktop://desktop-takeover-drop-to-desktop";

/**
 * 原生 Box 标题栏拖动完成后发出的布局变更事件，前端负责保存到 SQLite
 */
export const DESKTOP_TAKEOVER_BOX_MOVED_EVENT =
  "dasktop://desktop-takeover-box-moved";

/**
 * 原生桌面层请求刷新快照或背景菜单时，前端可触发重新扫描
 */
export const DESKTOP_TAKEOVER_REFRESH_EVENT =
  "dasktop://desktop-takeover-refresh-request";

/**
 * 原生桌面控制器启动或刷新失败时上报真实错误，设置页负责展示并保持开关状态可恢复
 */
export const NATIVE_DESKTOP_ERROR_EVENT = "dasktop://native-desktop-error";

export interface DesktopTakeoverItemActionPayload {
  itemPath: string;
  screenX: number;
  screenY: number;
}

export interface DesktopTakeoverDropToBoxPayload {
  boxId: string;
  itemPath: string;
  screenX: number;
  screenY: number;
}

export interface DesktopTakeoverDropToDesktopPayload {
  sourceBoxId: string | null;
  itemPath: string;
  x: number;
  y: number;
}

export interface DesktopTakeoverBoxMovedPayload {
  boxId: string;
  x: number;
  y: number;
}

export interface NativeDesktopErrorPayload {
  message: string;
}

/**
 * 监听原生桌面层打开项目请求；处理方必须保证不存在真实文件移动副作用
 */
export async function listenDesktopTakeoverOpen(
  handler: (event: Event<DesktopTakeoverItemActionPayload>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<DesktopTakeoverItemActionPayload>(DESKTOP_TAKEOVER_OPEN_EVENT, handler);
}

/**
 * 监听原生桌面层右键菜单请求，坐标已经是屏幕物理坐标
 */
export async function listenDesktopTakeoverContextMenu(
  handler: (event: Event<DesktopTakeoverItemActionPayload>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<DesktopTakeoverItemActionPayload>(DESKTOP_TAKEOVER_CONTEXT_MENU_EVENT, handler);
}

/**
 * 监听原生桌面图标拖入 Box 请求，由 Store 写入映射后再同步接管状态
 */
export async function listenDesktopTakeoverDropToBox(
  handler: (event: Event<DesktopTakeoverDropToBoxPayload>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<DesktopTakeoverDropToBoxPayload>(DESKTOP_TAKEOVER_DROP_TO_BOX_EVENT, handler);
}

/**
 * 监听图标落回桌面请求，删除 Box 映射和保存位置必须作为同一个用户动作处理
 */
export async function listenDesktopTakeoverDropToDesktop(
  handler: (event: Event<DesktopTakeoverDropToDesktopPayload>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<DesktopTakeoverDropToDesktopPayload>(
    DESKTOP_TAKEOVER_DROP_TO_DESKTOP_EVENT,
    handler,
  );
}

/**
 * 监听原生 Box 标题拖动结果；位置坐标使用屏幕物理坐标，和 Store 中的 Box 布局保持一致
 */
export async function listenDesktopTakeoverBoxMoved(
  handler: (event: Event<DesktopTakeoverBoxMovedPayload>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<DesktopTakeoverBoxMovedPayload>(DESKTOP_TAKEOVER_BOX_MOVED_EVENT, handler);
}

/**
 * 监听原生桌面层刷新请求，前端重新扫描后会把最新状态同步回控制器
 */
export async function listenDesktopTakeoverRefresh(
  handler: (event: Event<Record<string, never>>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<Record<string, never>>(DESKTOP_TAKEOVER_REFRESH_EVENT, handler);
}

/**
 * 监听原生桌面控制器错误，避免后端只打印 stderr 导致用户不知道接管失败原因
 */
export async function listenNativeDesktopError(
  handler: (event: Event<NativeDesktopErrorPayload>) => void | Promise<void>,
): Promise<UnlistenFn> {
  return listen<NativeDesktopErrorPayload>(NATIVE_DESKTOP_ERROR_EVENT, handler);
}
