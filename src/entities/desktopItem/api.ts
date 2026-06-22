import { invoke } from "@tauri-apps/api/core";
import type { DesktopItem } from "@/entities/desktopItem/types";
import type { DesktopSnapshot, NativeDesktopState } from "@/entities/desktopBox/types";

/**
 * 读取真实桌面目录快照；真实文件永远不在前端侧移动或改名
 */
export function getDesktopSnapshot(): Promise<DesktopSnapshot> {
  return invoke<DesktopSnapshot>("get_desktop_snapshot");
}

/**
 * 按任意真实路径解析项目元信息，支持非桌面目录文件拖入 Box 后继续显示原生图标
 */
export function getDesktopItemsByPaths(paths: string[]): Promise<DesktopItem[]> {
  return invoke<DesktopItem[]>("get_desktop_items_by_paths", { paths });
}

/**
 * 使用系统默认程序打开桌面项目；前端不判断文件类型，统一交给后端 Shell 入口处理
 */
export function openDesktopItem(path: string): Promise<void> {
  return invoke("open_desktop_item", { path });
}

/**
 * 在指定屏幕坐标弹出 Windows Shell 原生右键菜单，菜单内容和命令执行均由系统接管
 */
export function showNativeItemContextMenu(
  path: string,
  screenX: number,
  screenY: number,
): Promise<void> {
  return invoke("show_native_item_context_menu", {
    path,
    screenX: Math.round(screenX),
    screenY: Math.round(screenY),
  });
}

/**
 * 切换 Fences 模式原生桌面接管层，失败时后端会恢复 Explorer 图标显示
 */
export function setDesktopTakeoverEnabled(enabled: boolean): Promise<void> {
  return invoke("set_desktop_takeover_enabled", {
    enabled,
  });
}

/**
 * 将前端 Store 的持久化状态同步给原生桌面层，Rust 侧只保留运行时副本
 */
export function syncNativeDesktopState(state: NativeDesktopState): Promise<void> {
  return invoke("sync_native_desktop_state", {
    state,
  });
}

/**
 * 请求原生桌面层重新绘制，用于显示器、DPI 和快照变化后的兜底刷新
 */
export function refreshNativeDesktopLayer(): Promise<void> {
  return invoke("refresh_native_desktop_layer");
}

