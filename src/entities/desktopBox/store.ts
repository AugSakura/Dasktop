import { ref } from "vue";
import { defineStore } from "pinia";
import { getDesktopSnapshot } from "@/entities/desktopItem/api";
import { syncShellDesktopIconVisibility } from "@/entities/desktopItem/shellDesktopIconVisibilitySync";
import {
  chooseCollectionRootFolder,
  createBoxFolder,
  deleteBoxFolder,
  migrateBoxFolder,
  openBoxFolder,
} from "@/entities/desktopBox/api";
import {
  deleteBoxRecord,
  initializeStorage,
  loadBoxes,
  loadSettings,
  saveBox,
  saveSetting,
} from "@/shared/storage/database";
import {
  APP_SETTING_KEYS,
  DEFAULT_APP_SETTINGS,
  sanitizeNumberAppSetting,
  type AppSettingBooleanKey,
  type AppSettingNumberKey,
} from "@/entities/appSettings/defaults";
import {
  BOX_DEFAULT_STATE,
  BOX_TITLE_OPACITY,
  BOX_WINDOW_PLACEMENT,
  BOX_WINDOW_SIZE,
  THEME_TRANSITION_CONFIG,
} from "@/entities/desktopBox/layout";
import {
  listenDesktopStateChanged,
  notifyDesktopStateChanged,
  requestDesktopStartupSnapshot,
  type DesktopStartupSnapshot,
  type DesktopStateChangeScope,
} from "@/shared/ipc/desktop";
import {
  clampBoxBoundsToVisibleMonitor,
  isBoxBoundsInsideAnyMonitor,
} from "@/entities/desktopBox/monitorBounds";
import type {
  AppSettings,
  BoxDeletePolicy,
  BoxDropAction,
  BoxConflictPolicy,
  ThemeMode,
} from "@/entities/appSettings/types";
import type { DesktopNameDisplayMode } from "@/entities/desktopItem/types";
import type { DesktopBox, DesktopBoxTitlePosition } from "@/entities/desktopBox/types";

/**
 * 更新 Box 时允许调用方控制是否清洗尺寸和广播事件，缩放等高频场景需要避免跨窗口刷新抖动
 */
interface UpdateBoxOptions {
  broadcast?: boolean;
  sanitize?: boolean;
}

/**
 * 迁移结果用于设置页反馈真实移动数量，调用方无需理解每个 Box 的文件路径细节
 */
interface BoxFolderMigrationSummary {
  migratedCount: number;
  skippedCount: number;
}

/**
 * 记录一次已完成的 Box 文件夹迁移，失败回滚时需要同时恢复真实文件夹和数据库路径。
 */
interface MigratedBoxFolder {
  boxId: string;
  nextFolderPath: string;
  previousFolderPath: string;
}

/**
 * 桌面 Store 维护真实文件夹 Box 的窗口状态和全局偏好，文件内容由 Windows Explorer 原生视图管理
 */
export const useDesktopStore = defineStore("desktop", () => {
  const desktopPath = ref("");
  const boxes = ref<DesktopBox[]>([]);
  const isLoading = ref(true);
  const isInitialized = ref(false);
  const lastError = ref("");
  const settings = ref<AppSettings>({ ...DEFAULT_APP_SETTINGS });
  const storeInstanceId = crypto.randomUUID();
  /**
   * Box 内容窗和 Box 菜单窗都应使用窗口主题，设置页继续使用设置页主题
   */
  const routeSearchParams = new URLSearchParams(window.location.search);
  const isBoxWindowContext = routeSearchParams.has("boxId") || routeSearchParams.has("boxMenu");
  let stateListenerRegistered = false;
  let themeTransitionTimer: ReturnType<typeof window.setTimeout> | null = null;

  /**
   * 初始化桌面路径、Box 布局和设置；同一窗口生命周期内只执行一次
   */
  async function initialize(): Promise<void> {
    if (isInitialized.value) {
      return;
    }

    isLoading.value = true;
    lastError.value = "";

    try {
      await initializeStorage();
      registerStateListener();
      const [snapshot, savedBoxes, savedSettings] = await Promise.all([
        getDesktopSnapshot(),
        loadBoxes(),
        loadSettings(),
      ]);

      desktopPath.value = snapshot.desktopPath;
      settings.value = savedSettings;
      applyCurrentWindowTheme();
      const sanitizedBoxes = savedBoxes.map((box) => sanitizeBoxSize(box));
      await Promise.all(sanitizedBoxes.map((box) => saveBox(box)));
      boxes.value = await clampLoadedBoxesToMonitors(sanitizedBoxes);
      await syncShellDesktopIconVisibilityForCurrentSettings();
      isInitialized.value = true;
    } catch (error) {
      lastError.value = error instanceof Error ? error.message : String(error);
    } finally {
      isLoading.value = false;
    }
  }

  /**
   * Box 窗口启动时优先读取主窗口准备好的快照，避免每个窗口重复读取 SQLite
   */
  async function initializeFromStartupSnapshot(token: string): Promise<boolean> {
    if (isInitialized.value) {
      return true;
    }

    const snapshot = await requestDesktopStartupSnapshot(token);
    if (!snapshot) {
      return false;
    }

    isLoading.value = true;
    lastError.value = "";

    try {
      registerStateListener();
      hydrateStartupSnapshot(snapshot);
      isInitialized.value = true;
      return true;
    } catch (error) {
      lastError.value = error instanceof Error ? error.message : String(error);
      return false;
    } finally {
      isLoading.value = false;
    }
  }

  /**
   * 设置页已经持有完整桌面状态，批量打开 Box 前导出给新窗口复用
   */
  function createStartupSnapshot(): DesktopStartupSnapshot {
    return {
      boxes: boxes.value.map((box) => ({ ...box })),
      desktopPath: desktopPath.value,
      settings: { ...settings.value },
    };
  }

  /**
   * Box 菜单窗口只需要 Box 几何状态和主题设置，避免打开菜单时扫描桌面目录造成明显延迟
   */
  async function initializeBoxMenu(): Promise<void> {
    if (isInitialized.value) {
      return;
    }

    isLoading.value = true;
    lastError.value = "";

    try {
      await initializeStorage();
      registerStateListener();
      const [savedBoxes, savedSettings] = await Promise.all([loadBoxes(), loadSettings()]);

      boxes.value = savedBoxes.map((box) => sanitizeBoxSize(box));
      settings.value = savedSettings;
      applyCurrentWindowTheme();
      isInitialized.value = true;
    } catch (error) {
      lastError.value = error instanceof Error ? error.message : String(error);
    } finally {
      isLoading.value = false;
    }
  }

  /**
   * 主动刷新桌面路径，用于删除 Box 默认移回桌面时拿到最新目标目录
   */
  async function refreshSnapshot(shouldBroadcast = true): Promise<void> {
    const snapshot = await getDesktopSnapshot();
    desktopPath.value = snapshot.desktopPath;

    if (shouldBroadcast) {
      await broadcastStateChanged("desktop");
    }
  }

  /**
   * 设置窗重新获得焦点时从 SQLite 读取最新状态，解决多个独立 Box 窗口各自持有 Store 的问题
   */
  async function reloadPersistedState(): Promise<void> {
    const [savedBoxes, savedSettings] = await Promise.all([loadBoxes(), loadSettings()]);

    const previousTheme = getCurrentWindowTheme();
    boxes.value = await clampLoadedBoxesToMonitors(savedBoxes.map((box) => sanitizeBoxSize(box)));
    settings.value = savedSettings;
    applyCurrentWindowTheme(previousTheme !== getCurrentWindowTheme());
  }

  /**
   * 跨窗口监听只注册一次，接收通知后从 SQLite 重新读取，避免信任可能过期的事件载荷
   */
  function registerStateListener(): void {
    if (stateListenerRegistered) {
      return;
    }

    stateListenerRegistered = true;
    void listenDesktopStateChanged(async ({ payload }) => {
      if (payload.sourceId === storeInstanceId || !isInitialized.value) {
        return;
      }

      await reloadPersistedState();
      if (payload.scope === "desktop") {
        await refreshSnapshot(false);
      }
    }).catch((error) => {
      stateListenerRegistered = false;
      lastError.value = error instanceof Error ? error.message : String(error);
    });
  }

  /**
   * 状态广播只作为失效通知使用，接收方会自行读取数据库中的最新状态
   */
  async function broadcastStateChanged(scope: DesktopStateChangeScope): Promise<void> {
    await notifyDesktopStateChanged({
      sourceId: storeInstanceId,
      scope,
    });
  }

  /**
   * 创建一个新的真实文件夹 Box；没有收纳根目录时先让用户选择根目录
   */
  async function createBox(title = "新建 Box"): Promise<DesktopBox> {
    const rootPath = await ensureCollectionRootPath();
    const boxId = crypto.randomUUID();
    const folderPath = await createBoxFolder(rootPath, `box_${boxId.replace(/-/g, "")}`);
    const nextBox = sanitizeBoxSize({
      id: boxId,
      title,
      folderPath,
      ...BOX_DEFAULT_STATE,
      x: BOX_WINDOW_PLACEMENT.initialX + boxes.value.length * BOX_WINDOW_PLACEMENT.cascadeStep,
      y: BOX_WINDOW_PLACEMENT.initialY + boxes.value.length * BOX_WINDOW_PLACEMENT.cascadeStep,
      ...BOX_WINDOW_SIZE.default,
    });

    boxes.value.push(nextBox);
    await saveBox(nextBox);
    await broadcastStateChanged("boxes");
    return nextBox;
  }

  /**
   * 收纳根目录为空时打开系统文件夹选择器；用户取消时中断创建，避免生成不可追踪目录
   */
  async function ensureCollectionRootPath(): Promise<string> {
    const currentRootPath = settings.value.collectionRootPath.trim();
    if (currentRootPath) {
      return currentRootPath;
    }

    const selectedRootPath = await chooseCollectionRootFolder();
    if (!selectedRootPath) {
      throw new Error("已取消选择收纳位置，未创建 Box");
    }

    await updateCollectionRootPath(selectedRootPath);
    return selectedRootPath;
  }

  /**
   * 更新 Box 的布局或展示名，尺寸默认会经过最小可操作范围保护
   */
  async function updateBox(nextBox: DesktopBox, options: UpdateBoxOptions = {}): Promise<void> {
    const index = boxes.value.findIndex((box) => box.id === nextBox.id);
    if (index === -1) {
      return;
    }

    const { broadcast = true, sanitize = true } = options;
    const normalizedBox = sanitize ? sanitizeBoxSize(nextBox) : nextBox;
    // 屏幕外坐标一律不落库：最小化幽灵坐标和拔掉显示器后的位置一旦写入，重启后 Box 将永远不可见
    const safeBox = (await isBoxBoundsInsideAnyMonitor(normalizedBox))
      ? normalizedBox
      : { ...normalizedBox, x: boxes.value[index].x, y: boxes.value[index].y };
    boxes.value[index] = safeBox;
    await saveBox(safeBox);

    if (broadcast) {
      await broadcastStateChanged("boxes");
    }
  }

  /**
   * 删除 Box 前先按当前策略处理真实文件夹；用户在 Shell 确认中取消时不会删除数据库记录
   */
  async function deleteBox(boxId: string): Promise<void> {
    const targetBox = boxes.value.find((box) => box.id === boxId);
    if (!targetBox) {
      return;
    }

    /**
     * Box 菜单窗口为了启动快只加载布局和设置，删除前需要按需补齐默认策略使用的桌面目录。
     */
    if (!desktopPath.value.trim()) {
      const snapshot = await getDesktopSnapshot();
      desktopPath.value = snapshot.desktopPath;
    }

    await deleteBoxFolder(
      targetBox.folderPath,
      desktopPath.value,
      settings.value.boxDeletePolicy,
      settings.value.boxConflictPolicy,
    );
    boxes.value = boxes.value.filter((box) => box.id !== boxId);
    await deleteBoxRecord(boxId);
    await syncShellDesktopIconVisibilityForCurrentSettings();
    await broadcastStateChanged("boxes");
  }

  /**
   * 更新主题时同步根节点标识，确保设置页和 Box 窗口一起响应
   */
  async function updateSettingsTheme(theme: ThemeMode): Promise<void> {
    settings.value.settingsTheme = theme;
    if (!isBoxWindowContext) {
      applyTheme(theme, true);
    }
    await saveSetting(APP_SETTING_KEYS.settingsTheme, theme);
    await broadcastStateChanged("settings");
  }

  /**
   * Box 窗口主题只影响桌面上的 Box，设置页切换时通过广播让各 Box 自己应用
   */
  async function updateBoxTheme(theme: ThemeMode): Promise<void> {
    settings.value.boxTheme = theme;
    if (isBoxWindowContext) {
      applyTheme(theme, true);
    }
    await saveSetting(APP_SETTING_KEYS.boxTheme, theme);
    await broadcastStateChanged("settings");
  }

  /**
   * 收纳根目录只影响后续新建 Box，不迁移或重命名已有 Box 的真实文件夹
   */
  async function updateCollectionRootPath(value: string): Promise<void> {
    settings.value.collectionRootPath = value.trim();
    await saveSetting(APP_SETTING_KEYS.collectionRootPath, settings.value.collectionRootPath);
    await broadcastStateChanged("settings");
  }

  /**
   * 打开当前收纳根目录，帮助用户确认新建 Box 和迁移后的真实落盘位置。
   */
  async function openCollectionRootPath(): Promise<void> {
    const rootPath = settings.value.collectionRootPath.trim();
    if (!rootPath) {
      throw new Error("请先选择收纳位置");
    }

    await openBoxFolder(rootPath);
  }

  /**
   * 判断 Box 是否已经位于当前收纳根目录下，设置页据此决定迁移按钮是否可用。
   */
  function getMigratableBoxes(): DesktopBox[] {
    const rootPath = settings.value.collectionRootPath.trim();
    if (!rootPath) {
      return [];
    }

    return boxes.value.filter((box) => !isPathUnderRoot(box.folderPath, rootPath));
  }

  /**
   * 将现有 Box 文件夹迁移到当前收纳根目录；失败时恢复已写入的数据库路径，避免 UI 指向半迁移状态。
   */
  async function migrateExistingBoxFolders(): Promise<BoxFolderMigrationSummary> {
    const rootPath = settings.value.collectionRootPath.trim();
    if (!rootPath) {
      throw new Error("请先选择收纳位置");
    }

    const migrationTargets = getMigratableBoxes();
    if (migrationTargets.length === 0) {
      return {
        migratedCount: 0,
        skippedCount: boxes.value.length,
      };
    }

    const migratedBoxes: MigratedBoxFolder[] = [];
    try {
      for (const targetBox of migrationTargets) {
        const nextFolderPath = await migrateBoxFolder(targetBox.folderPath, rootPath);
        const nextBox = {
          ...targetBox,
          folderPath: nextFolderPath,
        };
        await updateBox(nextBox, { broadcast: false, sanitize: false });
        migratedBoxes.push({
          boxId: targetBox.id,
          nextFolderPath,
          previousFolderPath: targetBox.folderPath,
        });
      }
    } catch (error) {
      await restoreMigratedBoxFolders(migratedBoxes, error);
    }

    await broadcastStateChanged("boxes");
    return {
      migratedCount: migratedBoxes.length,
      skippedCount: boxes.value.length - migratedBoxes.length,
    };
  }

  /**
   * 批量迁移失败时按完成顺序倒序搬回真实文件夹，确保数据库路径不指向已经迁走的位置。
   */
  async function restoreMigratedBoxFolders(
    migratedBoxes: MigratedBoxFolder[],
    originalError: unknown,
  ): Promise<never> {
    for (const migratedBox of migratedBoxes.reverse()) {
      const currentBox = boxes.value.find((box) => box.id === migratedBox.boxId);
      if (!currentBox) {
        continue;
      }

      try {
        const previousParentPath = getWindowsParentPath(migratedBox.previousFolderPath);
        const rollbackSourcePath = currentBox.folderPath || migratedBox.nextFolderPath;
        await migrateBoxFolder(rollbackSourcePath, previousParentPath);
        await updateBox(
          {
            ...currentBox,
            folderPath: migratedBox.previousFolderPath,
          },
          { broadcast: false, sanitize: false },
        );
      } catch (rollbackError) {
        throw new Error(
          `迁移失败，且 Box 文件夹回滚失败：${toErrorMessage(rollbackError)}；原始错误：${toErrorMessage(originalError)}`,
        );
      }
    }

    throw originalError instanceof Error ? originalError : new Error(String(originalError));
  }

  /**
   * 删除策略直接关系真实文件处理，保存时只接受设置面板提供的枚举值
   */
  async function updateBoxDeletePolicy(value: BoxDeletePolicy): Promise<void> {
    settings.value.boxDeletePolicy = value;
    await saveSetting(APP_SETTING_KEYS.boxDeletePolicy, value);
    await broadcastStateChanged("settings");
  }

  /**
   * 同名策略会影响复制、移动和删除 Box 时移回桌面的真实落盘结果，必须持久化后广播到所有 Box。
   */
  async function updateBoxConflictPolicy(value: BoxConflictPolicy): Promise<void> {
    settings.value.boxConflictPolicy = value;
    await saveSetting(APP_SETTING_KEYS.boxConflictPolicy, value);
    await broadcastStateChanged("settings");
  }

  /**
   * 拖入策略会触发真实文件操作，保存时只接受设置面板提供的枚举值
   */
  async function updateBoxDropAction(value: BoxDropAction): Promise<void> {
    settings.value.boxDropAction = value;
    await saveSetting(APP_SETTING_KEYS.boxDropAction, value);
    await broadcastStateChanged("settings");
  }

  /**
   * 拖出策略会把 Box 内文件处理到桌面目录，和拖入策略分开保存避免双向整理互相影响
   */
  async function updateBoxDragOutAction(value: BoxDropAction): Promise<void> {
    settings.value.boxDragOutAction = value;
    await saveSetting(APP_SETTING_KEYS.boxDragOutAction, value);
    await broadcastStateChanged("settings");
  }

  /**
   * 边缘吸附开关立即持久化，避免下次启动恢复旧交互习惯
   */
  async function updateSnapToEdges(enabled: boolean): Promise<void> {
    settings.value.snapToEdges = enabled;
    await saveSetting(APP_SETTING_KEYS.snapToEdges, enabled);
    await broadcastStateChanged("settings");
  }

  /**
   * 吸附阈值使用像素保存，和 Tauri 窗口移动事件的坐标体系保持一致
   */
  async function updateSnapThreshold(threshold: number): Promise<void> {
    await updateNumberSetting(APP_SETTING_KEYS.snapThreshold, threshold);
  }

  /**
   * 数值类设置统一做范围校验和跨窗口广播，避免每个滑块各自复制保存逻辑
   */
  async function updateNumberSetting<Key extends AppSettingNumberKey>(
    key: Key,
    value: AppSettings[Key],
  ): Promise<void> {
    const nextValue = sanitizeNumberAppSetting(key, value);

    settings.value = {
      ...settings.value,
      [key]: nextValue,
    };
    await saveSetting(APP_SETTING_KEYS[key], nextValue);
    await broadcastStateChanged("settings");
  }

  /**
   * 布尔类外观设置统一保存，避免每个开关各自写数据库和跨窗口广播。
   */
  async function updateBooleanSetting<Key extends AppSettingBooleanKey>(
    key: Key,
    value: AppSettings[Key],
  ): Promise<void> {
    settings.value = {
      ...settings.value,
      [key]: Boolean(value),
    };
    await saveSetting(APP_SETTING_KEYS[key], settings.value[key]);
    if (key === APP_SETTING_KEYS.autoHideNativeShellIcons) {
      await syncShellDesktopIconVisibilityForCurrentSettings();
    }
    await broadcastStateChanged("settings");
  }

  /**
   * 系统桌面图标同步失败只写入错误提示，避免启动或设置切换时中断 Box 状态加载。
   */
  async function syncShellDesktopIconVisibilityForCurrentSettings(): Promise<void> {
    try {
      await syncShellDesktopIconVisibility({
        autoHideEnabled: settings.value.autoHideNativeShellIcons,
        forceRestoreManagedIcons: !settings.value.autoHideNativeShellIcons,
      });
    } catch (error) {
      lastError.value = error instanceof Error ? error.message : String(error);
    }
  }

  /**
   * 文件名显示模式只影响 Box 标签文本，保存后广播给所有已打开 Box 立即重绘。
   */
  async function updateNameDisplayMode(value: DesktopNameDisplayMode): Promise<void> {
    settings.value.nameDisplayMode = value;
    await saveSetting(APP_SETTING_KEYS.nameDisplayMode, value);
    await broadcastStateChanged("settings");
  }

  /**
   * Box 标题位置属于单个 Box 的布局属性，修改时只保存当前窗口，不影响其他 Box
   */
  async function updateBoxTitlePosition(
    boxId: string,
    value: DesktopBoxTitlePosition,
  ): Promise<void> {
    const targetBox = boxes.value.find((item) => item.id === boxId);
    if (!targetBox || targetBox.titlePosition === value) {
      return;
    }

    await updateBox({
      ...targetBox,
      titlePosition: value,
    });
  }

  /**
   * 收缩模式是单个 Box 的行为偏好，开启后窗口会在鼠标离开时只保留标题
   */
  async function updateBoxCollapsed(boxId: string, value: boolean): Promise<void> {
    const targetBox = boxes.value.find((item) => item.id === boxId);
    if (!targetBox || targetBox.collapsed === value) {
      return;
    }

    await updateBox({
      ...targetBox,
      collapsed: value,
    });
  }

  /**
   * 锁定只影响当前 Box 的移动和缩放，内部文件操作仍由 Explorer 原生视图处理
   */
  async function updateBoxLocked(boxId: string, value: boolean): Promise<void> {
    const targetBox = boxes.value.find((item) => item.id === boxId);
    if (!targetBox || targetBox.locked === value) {
      return;
    }

    await updateBox({
      ...targetBox,
      locked: value,
    });
  }

  /**
   * 闲置可见度独立于 Box 背景透明度，只影响鼠标未进入 Box 区域时的整体透明度
   */
  async function updateBoxTitleOpacity(boxId: string, value: number): Promise<void> {
    const targetBox = boxes.value.find((item) => item.id === boxId);
    const nextOpacity = sanitizeBoxTitleOpacity(value);
    if (!targetBox || targetBox.titleOpacity === nextOpacity) {
      return;
    }

    await updateBox({
      ...targetBox,
      titleOpacity: nextOpacity,
    });
  }

  /**
   * Box 的尺寸与本地偏好一起规整，避免非法透明度或过小窗口撑破桌面组件
   */
  function sanitizeBoxSize(box: DesktopBox): DesktopBox {
    return {
      ...box,
      collapsed: Boolean(box.collapsed),
      folderPath: String(box.folderPath ?? ""),
      width: Math.max(box.width, BOX_WINDOW_SIZE.min.width),
      height: Math.max(box.height, BOX_WINDOW_SIZE.min.height),
      locked: Boolean(box.locked),
      titleOpacity: sanitizeBoxTitleOpacity(box.titleOpacity),
    };
  }

  /**
   * 把加载到的 Box 位置夹取回当前可见显示器，让旧版本残留的屏幕外坐标自愈。
   *
   * 历史版本曾把 Windows 最小化产生的幽灵坐标写进数据库，重启后 Box 会全部停在屏幕外；
   * 这里只修正内存布局不回写数据库，显示器拔掉后重新插回时 Box 仍能回到原来的屏幕。
   */
  async function clampLoadedBoxesToMonitors(boxes: DesktopBox[]): Promise<DesktopBox[]> {
    return Promise.all(
      boxes.map(async (box) => {
        const clampedBounds = await clampBoxBoundsToVisibleMonitor(box);

        return { ...box, x: clampedBounds.x, y: clampedBounds.y };
      }),
    );
  }

  /**
   * 闲置可见度在 Store 层统一夹取，数据库和菜单滑块都复用相同边界
   */
  function sanitizeBoxTitleOpacity(value: number): number {
    if (!Number.isFinite(value)) {
      return BOX_DEFAULT_STATE.titleOpacity;
    }

    return Math.round(
      Math.min(Math.max(value, BOX_TITLE_OPACITY.min), BOX_TITLE_OPACITY.max),
    );
  }

  /**
   * 收纳根目录判断只用于 UI 和迁移过滤，真实迁移安全检查仍由 Rust 侧基于文件系统路径完成。
   */
  function isPathUnderRoot(candidatePath: string, rootPath: string): boolean {
    const normalizedCandidate = normalizeWindowsPath(candidatePath);
    const normalizedRoot = normalizeWindowsPath(rootPath);
    if (!normalizedCandidate || !normalizedRoot) {
      return false;
    }

    return (
      normalizedCandidate === normalizedRoot ||
      normalizedCandidate.startsWith(`${normalizedRoot}\\`)
    );
  }

  /**
   * 前端路径比较按 Windows 桌面目标处理，统一斜杠和大小写即可避免显示层误判。
   */
  function normalizeWindowsPath(path: string): string {
    return path
      .trim()
      .replace(/\//gu, "\\")
      .replace(/\\+$/u, "")
      .toLocaleLowerCase();
  }

  /**
   * 迁移回滚需要把物理目录搬回原父目录；前端只做路径拆分，最终合法性仍由 Rust 文件系统检查。
   */
  function getWindowsParentPath(path: string): string {
    const normalizedPath = path.trim().replace(/\//gu, "\\").replace(/\\+$/u, "");
    const separatorIndex = normalizedPath.lastIndexOf("\\");
    if (separatorIndex <= 0) {
      throw new Error("无法解析 Box 原收纳目录");
    }
    if (/^[A-Za-z]:\\/u.test(normalizedPath) && separatorIndex === 2) {
      return normalizedPath.slice(0, separatorIndex + 1);
    }

    return normalizedPath.slice(0, separatorIndex);
  }

  /**
   * 错误提示统一在 Store 层转成文本，避免 unknown 直接拼接成没有信息量的对象字符串。
   */
  function toErrorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  /**
   * Box 收缩动画时长从设置读取，确保所有窗口动画节奏一致
   */
  function getBoxCollapseAnimationMs(): number {
    return sanitizeNumberAppSetting(
      APP_SETTING_KEYS.boxCollapseAnimationMs,
      settings.value.boxCollapseAnimationMs,
    );
  }

  /**
   * 鼠标离开后的收缩等待时间从设置读取，所有 Box 窗口共享同一套手感配置。
   */
  function getBoxCollapseDelayMs(): number {
    return sanitizeNumberAppSetting(
      APP_SETTING_KEYS.boxCollapseDelayMs,
      settings.value.boxCollapseDelayMs,
    );
  }

  /**
   * 闲置可见度淡出动画由全局设置控制，保证自动收缩后的透明过渡节奏可调。
   */
  function getBoxIdleOpacityHideAnimationMs(): number {
    return sanitizeNumberAppSetting(
      APP_SETTING_KEYS.boxIdleOpacityHideAnimationMs,
      settings.value.boxIdleOpacityHideAnimationMs,
    );
  }

  /**
   * 闲置可见度淡入动画由全局设置控制，保证鼠标进入 Box 后的恢复速度可调。
   */
  function getBoxIdleOpacityShowAnimationMs(): number {
    return sanitizeNumberAppSetting(
      APP_SETTING_KEYS.boxIdleOpacityShowAnimationMs,
      settings.value.boxIdleOpacityShowAnimationMs,
    );
  }

  /**
   * 跟随系统只影响渲染标识，不把解析后的明暗值写回数据库
   */
  function applyTheme(theme: ThemeMode, shouldAnimate = false): void {
    const root = document.documentElement;
    const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    const resolvedTheme = theme === "system" ? (systemDark ? "dark" : "light") : theme;

    if (shouldAnimate) {
      enableThemeTransition(root);
    }

    root.dataset.themeMode = theme;
    root.dataset.theme = resolvedTheme;
    root.classList.toggle("dark", resolvedTheme === "dark");
  }

  /**
   * 当前窗口按自身角色应用对应主题，避免设置页主题误影响 Box 窗口
   */
  function applyCurrentWindowTheme(shouldAnimate = false): void {
    applyTheme(getCurrentWindowTheme(), shouldAnimate);
  }

  /**
   * 设置页和 Box 窗口共享 Store，但 URL 中的 boxId 能稳定区分当前渲染目标
   */
  function getCurrentWindowTheme(): ThemeMode {
    return isBoxWindowContext ? settings.value.boxTheme : settings.value.settingsTheme;
  }

  /**
   * 快照水合只填充内存状态，不写数据库；真实持久化仍由主窗口初始化和后续操作负责
   */
  function hydrateStartupSnapshot(snapshot: DesktopStartupSnapshot): void {
    desktopPath.value = snapshot.desktopPath;
    settings.value = snapshot.settings;
    boxes.value = snapshot.boxes.map((box) => sanitizeBoxSize(box));
    applyCurrentWindowTheme();
  }

  /**
   * 主题动画尊重系统的减少动态效果设置，只做短时颜色过渡，不影响拖动和缩放性能
   */
  function enableThemeTransition(root: HTMLElement): void {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      return;
    }

    if (themeTransitionTimer) {
      window.clearTimeout(themeTransitionTimer);
    }

    root.classList.add(THEME_TRANSITION_CONFIG.className);
    /**
     * 强制浏览器先计算一次旧主题样式，随后切换 dark class 时才能拿到明确的过渡起点
     */
    void root.offsetHeight;
    themeTransitionTimer = window.setTimeout(() => {
      root.classList.remove(THEME_TRANSITION_CONFIG.className);
      themeTransitionTimer = null;
    }, THEME_TRANSITION_CONFIG.timeoutMs);
  }

  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (getCurrentWindowTheme() === "system") {
      applyCurrentWindowTheme(true);
    }
  });

  return {
    boxes,
    createBox,
    createStartupSnapshot,
    deleteBox,
    desktopPath,
    getBoxCollapseAnimationMs,
    getBoxCollapseDelayMs,
    getBoxIdleOpacityHideAnimationMs,
    getBoxIdleOpacityShowAnimationMs,
    initialize,
    initializeFromStartupSnapshot,
    initializeBoxMenu,
    isInitialized,
    isLoading,
    lastError,
    reloadPersistedState,
    refreshSnapshot,
    settings,
    getMigratableBoxes,
    updateBox,
    updateBoxCollapsed,
    updateBoxDeletePolicy,
    updateBoxConflictPolicy,
    updateBoxDragOutAction,
    updateBoxDropAction,
    updateBoxLocked,
    updateBoxTheme,
    updateBoxTitleOpacity,
    updateBoxTitlePosition,
    updateCollectionRootPath,
    migrateExistingBoxFolders,
    openCollectionRootPath,
    updateBooleanSetting,
    updateNameDisplayMode,
    updateNumberSetting,
    updateSettingsTheme,
    updateSnapThreshold,
    updateSnapToEdges,
  };
});
