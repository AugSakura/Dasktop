import Database from "@tauri-apps/plugin-sql";
import type { AppSettings } from "@/entities/appSettings/types";
import type {
  DesktopBox,
  DesktopBoxItem,
  DesktopBoxTitlePosition,
} from "@/entities/desktopBox/types";
import {
  APP_SETTINGS_STORAGE,
  DEFAULT_APP_SETTINGS,
  DESKTOP_NAME_DISPLAY_MODES,
  SUPPORTED_APP_SETTING_KEYS,
  THEME_MODES,
  APP_SETTING_KEYS,
  APP_SETTING_NUMBER_LIMITS,
  sanitizeNumberAppSetting,
  type AppSettingNumberKey,
} from "@/entities/appSettings/defaults";
import { BOX_DEFAULT_STATE, BOX_TITLE_OPACITY } from "@/entities/desktopBox/layout";

let databasePromise: Promise<Database> | null = null;
/**
 * 数值型设置键从统一范围配置派生，数据库读取时不再手写多处分支
 */
const numericSettingKeys = Object.keys(APP_SETTING_NUMBER_LIMITS) as AppSettingNumberKey[];
const DESKTOP_BOX_TITLE_POSITIONS = ["top", "bottom"] as const;

/**
 * SQLite 连接复用可以避免拖拽过程中反复打开数据库
 */
async function getDatabase(): Promise<Database> {
  if (!databasePromise) {
    databasePromise = Database.load(APP_SETTINGS_STORAGE.databaseUrl);
  }

  return databasePromise;
}

/**
 * 初始化 V1 需要的最小表结构；分组只保存映射，不触碰真实桌面文件
 */
export async function initializeStorage(): Promise<void> {
  const database = await getDatabase();

  await database.execute(`
    CREATE TABLE IF NOT EXISTS ${APP_SETTINGS_STORAGE.tables.boxes} (
      id TEXT PRIMARY KEY,
      title TEXT NOT NULL,
      collapsed INTEGER NOT NULL DEFAULT 0,
      locked INTEGER NOT NULL DEFAULT 0,
      title_opacity INTEGER NOT NULL DEFAULT 100,
      title_position TEXT NOT NULL DEFAULT 'top',
      x INTEGER NOT NULL,
      y INTEGER NOT NULL,
      width INTEGER NOT NULL,
      height INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    )
  `);

  await database.execute(`
    CREATE TABLE IF NOT EXISTS ${APP_SETTINGS_STORAGE.tables.boxItems} (
      box_id TEXT NOT NULL,
      item_path TEXT NOT NULL,
      order_index INTEGER NOT NULL DEFAULT 0,
      updated_at INTEGER NOT NULL,
      PRIMARY KEY (box_id, item_path),
      FOREIGN KEY (box_id) REFERENCES ${APP_SETTINGS_STORAGE.tables.boxes}(id) ON DELETE CASCADE
    )
  `);

  await database.execute(`
    CREATE TABLE IF NOT EXISTS ${APP_SETTINGS_STORAGE.tables.appSettings} (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL,
      updated_at INTEGER NOT NULL
    )
  `);
}

/**
 * 读取所有 Box 布局和映射；真实桌面文件仍由桌面快照提供
 */
export async function loadBoxes(): Promise<DesktopBox[]> {
  const database = await getDatabase();
  const rows = await database.select<Array<Record<string, unknown>>>(`
    SELECT id, title, collapsed, locked, title_opacity, title_position, x, y, width, height
    FROM ${APP_SETTINGS_STORAGE.tables.boxes}
    ORDER BY updated_at ASC
  `);

  return rows.map((row) => ({
    collapsed: sanitizeDesktopBoxBoolean(row.collapsed),
    id: String(row.id),
    locked: sanitizeDesktopBoxBoolean(row.locked),
    title: String(row.title),
    titleOpacity: sanitizeDesktopBoxTitleOpacity(row.title_opacity),
    titlePosition: sanitizeDesktopBoxTitlePosition(row.title_position),
    x: Number(row.x),
    y: Number(row.y),
    width: Number(row.width),
    height: Number(row.height),
  }));
}

/**
 * 读取 Box 和文件路径的关联表；Store 会基于它计算每个 Box 的项目列表和统计信息
 */
export async function loadBoxItems(): Promise<DesktopBoxItem[]> {
  const database = await getDatabase();
  const rows = await database.select<Array<Record<string, unknown>>>(`
    SELECT box_id, item_path, order_index
    FROM ${APP_SETTINGS_STORAGE.tables.boxItems}
    ORDER BY box_id ASC, order_index ASC, updated_at ASC
  `);

  return rows.map((row) => ({
    boxId: String(row.box_id),
    itemPath: String(row.item_path),
    orderIndex: Number(row.order_index),
  }));
}

/**
 * 保存单个 Box 状态；文件映射由 box_items 关联表独立维护，避免布局更新误写项目列表
 */
export async function saveBox(box: DesktopBox): Promise<void> {
  const database = await getDatabase();

  await database.execute(
    `
      INSERT INTO ${APP_SETTINGS_STORAGE.tables.boxes} (id, title, collapsed, locked, title_opacity, title_position, x, y, width, height, updated_at)
      VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
      ON CONFLICT(id) DO UPDATE SET
        title = excluded.title,
        collapsed = excluded.collapsed,
        locked = excluded.locked,
        title_opacity = excluded.title_opacity,
        title_position = excluded.title_position,
        x = excluded.x,
        y = excluded.y,
        width = excluded.width,
        height = excluded.height,
        updated_at = excluded.updated_at
    `,
    [
      box.id,
      box.title,
      box.collapsed ? 1 : 0,
      box.locked ? 1 : 0,
      sanitizeDesktopBoxTitleOpacity(box.titleOpacity),
      box.titlePosition,
      box.x,
      box.y,
      box.width,
      box.height,
      Date.now(),
    ],
  );
}

/**
 * 批量将桌面项目重新归入目标 Box；同一路径会先从其他 Box 移除，再写入目标关联
 */
export async function assignBoxItems(boxId: string, itemPaths: string[]): Promise<void> {
  const database = await getDatabase();
  const normalizedBoxId = typeof boxId === "string" ? boxId.trim() : "";
  const uniqueItemPaths = Array.from(new Set(itemPaths)).filter(Boolean);

  if (!normalizedBoxId) {
    throw new Error("无法收纳项目：目标 Box 无效");
  }
  if (uniqueItemPaths.length === 0) {
    return;
  }

  const orderRows = await database.select<Array<Record<string, unknown>>>(
    `SELECT MAX(order_index) AS max_order FROM ${APP_SETTINGS_STORAGE.tables.boxItems} WHERE box_id = $1`,
    [normalizedBoxId],
  );
  let nextOrderIndex = Number(orderRows[0]?.max_order ?? -1) + 1;

  for (const itemPath of uniqueItemPaths) {
    await database.execute(
      `DELETE FROM ${APP_SETTINGS_STORAGE.tables.boxItems} WHERE item_path = $1`,
      [itemPath],
    );
    await database.execute(
      `
        INSERT INTO ${APP_SETTINGS_STORAGE.tables.boxItems} (box_id, item_path, order_index, updated_at)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT(box_id, item_path) DO UPDATE SET
          order_index = excluded.order_index,
          updated_at = excluded.updated_at
      `,
      [normalizedBoxId, itemPath, nextOrderIndex, Date.now()],
    );
    nextOrderIndex += 1;
  }
}

/**
 * 保存单个 Box 内的手动排序，只更新关联表顺序，不触碰真实桌面文件
 */
export async function saveBoxItemOrder(boxId: string, itemPaths: string[]): Promise<void> {
  const database = await getDatabase();

  for (const [orderIndex, itemPath] of itemPaths.entries()) {
    await database.execute(
      `
        UPDATE ${APP_SETTINGS_STORAGE.tables.boxItems}
        SET order_index = $1, updated_at = $2
        WHERE box_id = $3 AND item_path = $4
      `,
      [orderIndex, Date.now(), boxId, itemPath],
    );
  }
}

/**
 * 从 Box 中移除单个映射，真实桌面项目继续保留在 Windows 桌面目录
 */
export async function deleteBoxItem(boxId: string, itemPath: string): Promise<void> {
  const database = await getDatabase();

  await database.execute(
    `DELETE FROM ${APP_SETTINGS_STORAGE.tables.boxItems} WHERE box_id = $1 AND item_path = $2`,
    [boxId, itemPath],
  );
}

/**
 * 删除 Box 记录只影响 Dasktop 映射，不删除桌面文件本体
 */
export async function deleteBoxRecord(boxId: string): Promise<void> {
  const database = await getDatabase();

  await database.execute(
    `DELETE FROM ${APP_SETTINGS_STORAGE.tables.boxItems} WHERE box_id = $1`,
    [boxId],
  );
  await database.execute(`DELETE FROM ${APP_SETTINGS_STORAGE.tables.boxes} WHERE id = $1`, [boxId]);
}

/**
 * 读取当前设置，并过滤掉旧版本残留或非法的设置值
 */
export async function loadSettings(): Promise<AppSettings> {
  const database = await getDatabase();
  const rows = await database.select<Array<Record<string, unknown>>>(`
    SELECT key, value FROM ${APP_SETTINGS_STORAGE.tables.appSettings}
  `);
  const settings = { ...DEFAULT_APP_SETTINGS };
  const supportedKeys = new Set<keyof AppSettings>(SUPPORTED_APP_SETTING_KEYS);

  for (const row of rows) {
    const key = String(row.key) as keyof AppSettings;
    if (supportedKeys.has(key)) {
      const value = JSON.parse(String(row.value));
      settings[key] = sanitizeSettingValue(key, value) as never;
    }
  }

  return settings;
}

/**
 * 设置读取只接受当前版本定义的值，旧主题和旧字段不会被迁移回新状态
 */
function sanitizeSettingValue<Key extends keyof AppSettings>(
  key: Key,
  value: unknown,
): AppSettings[Key] {
  if (key === APP_SETTING_KEYS.settingsTheme || key === APP_SETTING_KEYS.boxTheme) {
    return (isThemeMode(value) ? value : DEFAULT_APP_SETTINGS[key]) as AppSettings[Key];
  }

  if (
    key === APP_SETTING_KEYS.boxResizeGridEnabled ||
    key === APP_SETTING_KEYS.desktopTakeoverEnabled ||
    key === APP_SETTING_KEYS.snapToEdges ||
    key === APP_SETTING_KEYS.showItemLabels ||
    key === APP_SETTING_KEYS.showShortcutArrow ||
    key === APP_SETTING_KEYS.doubleClickOpenItems
  ) {
    return (typeof value === "boolean" ? value : DEFAULT_APP_SETTINGS[key]) as AppSettings[Key];
  }

  if (key === APP_SETTING_KEYS.nameDisplayMode) {
    return (isDesktopNameDisplayMode(value)
      ? value
      : DEFAULT_APP_SETTINGS.nameDisplayMode) as AppSettings[Key];
  }

  if (isNumericSettingKey(key)) {
    return sanitizeNumberAppSetting(key, value) as AppSettings[Key];
  }

  return DEFAULT_APP_SETTINGS[key];
}

/**
 * 类型守卫把普通设置键收窄到数值设置键，便于复用统一的范围校验函数
 */
function isNumericSettingKey(key: keyof AppSettings): key is AppSettingNumberKey {
  return numericSettingKeys.some((settingKey) => settingKey === key);
}

/**
 * 主题枚举只允许当前三种值，避免历史状态继续污染 UI
 */
function isThemeMode(value: unknown): value is AppSettings["settingsTheme"] {
  return THEME_MODES.some((themeMode) => themeMode === value);
}

/**
 * 文件名显示模式只接受当前版本提供的三个选项，避免历史字符串污染渲染逻辑
 */
function isDesktopNameDisplayMode(value: unknown): value is AppSettings["nameDisplayMode"] {
  return DESKTOP_NAME_DISPLAY_MODES.some((displayMode) => displayMode === value);
}

/**
 * Box 标题位置只接受当前菜单提供的上下两种布局
 */
function isDesktopBoxTitlePosition(value: unknown): value is DesktopBoxTitlePosition {
  return DESKTOP_BOX_TITLE_POSITIONS.some((position) => position === value);
}

/**
 * SQLite 布尔值以 0/1 保存，读取时只接受明确开启状态，避免脏值误锁定窗口
 */
function sanitizeDesktopBoxBoolean(value: unknown): boolean {
  return value === 1 || value === "1" || value === true;
}

/**
 * Box 标题位置保存在布局表中，非法值回退到默认上方，避免窗口渲染出现无序 order
 */
function sanitizeDesktopBoxTitlePosition(value: unknown): DesktopBoxTitlePosition {
  return isDesktopBoxTitlePosition(value) ? value : BOX_DEFAULT_STATE.titlePosition;
}

/**
 * 闲置可见度只允许 0-100 的百分比，防止菜单滑块和渲染样式出现不同步
 */
function sanitizeDesktopBoxTitleOpacity(value: unknown): number {
  const numericValue = Number(value);
  if (!Number.isFinite(numericValue)) {
    return BOX_DEFAULT_STATE.titleOpacity;
  }

  return Math.round(
    Math.min(Math.max(numericValue, BOX_TITLE_OPACITY.min), BOX_TITLE_OPACITY.max),
  );
}

/**
 * 按键保存设置，保持设置项独立更新，避免整行 JSON 合并冲突
 */
export async function saveSetting<Key extends keyof AppSettings>(
  key: Key,
  value: AppSettings[Key],
): Promise<void> {
  const database = await getDatabase();

  await database.execute(
    `
      INSERT INTO ${APP_SETTINGS_STORAGE.tables.appSettings} (key, value, updated_at)
      VALUES ($1, $2, $3)
      ON CONFLICT(key) DO UPDATE SET
        value = excluded.value,
        updated_at = excluded.updated_at
    `,
    [key, JSON.stringify(value), Date.now()],
  );
}
