import { invoke } from "@tauri-apps/api/core";

/**
 * 读取系统级开机自启状态；该状态以操作系统启动项为准，不落入 SQLite 设置表
 */
export function isAutostartEnabled(): Promise<boolean> {
  return invoke<boolean>("is_autostart_enabled");
}

/**
 * 切换系统级开机自启状态，后端会同步托盘勾选状态并通知设置页刷新
 */
export function setAutostartEnabled(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_autostart_enabled", { enabled });
}

/**
 * 同步托盘菜单的桌面接管勾选状态；偏好本身仍由桌面 Store 保存
 */
export function setTrayDesktopTakeoverChecked(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_tray_desktop_takeover_checked", { enabled });
}
