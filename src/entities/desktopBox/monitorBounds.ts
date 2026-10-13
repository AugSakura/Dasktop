import { availableMonitors, primaryMonitor } from "@tauri-apps/api/window";

/**
 * 位置夹取和可见性判断只需要几何字段，DesktopBox 与窗口帧都可以直接复用
 */
export interface BoxMonitorBounds {
  height: number;
  width: number;
  x: number;
  y: number;
}

/**
 * 逻辑坐标工作区，Box 布局在数据库和 Store 里都按逻辑坐标保存
 */
interface LogicalWorkArea {
  height: number;
  width: number;
  x: number;
  y: number;
}

/**
 * 显示器查询结果同时给出全部工作区和主屏工作区，夹取时优先保留 Box 所在屏幕
 */
interface MonitorWorkAreaLayout {
  areas: LogicalWorkArea[];
  primaryArea: LogicalWorkArea | null;
}

/**
 * 一次性读取全部显示器工作区并换算成逻辑坐标，避免每个调用方重复处理 Tauri 的物理像素
 */
async function resolveMonitorWorkAreaLayout(): Promise<MonitorWorkAreaLayout> {
  const [monitors, primary] = await Promise.all([availableMonitors(), primaryMonitor()]);

  const toLogicalWorkArea = (monitor: {
    scaleFactor: number;
    workArea: { position: { x: number; y: number }; size: { width: number; height: number } };
  }): LogicalWorkArea => {
    // 个别环境可能返回 0 或非法缩放系数，兜底成 1 避免坐标被除成 Infinity
    const scaleFactor = monitor.scaleFactor > 0 ? monitor.scaleFactor : 1;

    return {
      x: monitor.workArea.position.x / scaleFactor,
      y: monitor.workArea.position.y / scaleFactor,
      width: monitor.workArea.size.width / scaleFactor,
      height: monitor.workArea.size.height / scaleFactor,
    };
  };

  return {
    areas: monitors.map(toLogicalWorkArea),
    primaryArea: primary ? toLogicalWorkArea(primary) : null,
  };
}

/**
 * 把 Box 夹取回某个可见显示器的工作区，供数据库残留屏幕外坐标或拔掉显示器后自愈使用。
 *
 * 优先保留 Box 当前所在的显示器，完全在屏幕外时退回主屏；工作区比 Box 还小时贴左上角，
 * 保证任何配置下 Box 都有一块可见区域。读取不到显示器时原样返回，不误伤正常布局。
 */
export async function clampBoxBoundsToVisibleMonitor(
  bounds: BoxMonitorBounds,
): Promise<BoxMonitorBounds> {
  const { areas, primaryArea } = await resolveMonitorWorkAreaLayout();
  if (areas.length === 0) {
    return bounds;
  }

  const targetArea =
    areas.find(
      (area) =>
        bounds.x >= area.x &&
        bounds.x < area.x + area.width &&
        bounds.y >= area.y &&
        bounds.y < area.y + area.height,
    ) ??
    primaryArea ??
    areas[0];
  // 工作区比 Box 尺寸小时 maxX 退化成 workX，夹取结果贴工作区左上角
  const maxX = Math.max(targetArea.x, targetArea.x + targetArea.width - bounds.width);
  const maxY = Math.max(targetArea.y, targetArea.y + targetArea.height - bounds.height);

  return {
    ...bounds,
    x: Math.round(Math.min(Math.max(bounds.x, targetArea.x), maxX)),
    y: Math.round(Math.min(Math.max(bounds.y, targetArea.y), maxY)),
  };
}

/**
 * 判断 Box 是否与任一显示器工作区相交，用于拦截不应该落库的窗口位置。
 *
 * Windows 最小化会把窗口临时挪到 (-32000, -32000) 这类幽灵坐标并触发移动事件，
 * 显示器拔插后原位置也会落到屏幕外；这两种坐标一旦写入数据库，
 * 重启后 Box 会永远停在屏幕外。允许部分超出，只拒绝完全不可见的位置。
 */
export async function isBoxBoundsInsideAnyMonitor(bounds: BoxMonitorBounds): Promise<boolean> {
  const { areas } = await resolveMonitorWorkAreaLayout();
  if (areas.length === 0) {
    // 读取不到显示器信息时不拦截，避免把正常位置误判成屏幕外
    return true;
  }

  return areas.some(
    (area) =>
      bounds.x + bounds.width > area.x &&
      bounds.x < area.x + area.width &&
      bounds.y + bounds.height > area.y &&
      bounds.y < area.y + area.height,
  );
}
