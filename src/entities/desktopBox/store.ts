import { computed, ref } from "vue";
import { defineStore } from "pinia";
import {
  refreshNativeDesktopLayer,
  getDesktopItemsByPaths,
  getDesktopSnapshot,
  openDesktopItem,
  setDesktopTakeoverEnabled,
  showNativeItemContextMenu,
  syncNativeDesktopState,
} from "@/entities/desktopItem/api";
import {
  assignBoxItems,
  deleteBoxItem,
  deleteBoxRecord,
  initializeStorage,
  loadBoxItems,
  loadBoxes,
  loadSettings,
  saveBox,
  saveBoxItemOrder,
  saveSetting,
} from "@/shared/storage/database";
import {
  APP_SETTING_KEYS,
  DEFAULT_APP_SETTINGS,
  sanitizeNumberAppSetting,
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
  type DesktopStateChangeScope,
} from "@/shared/ipc/desktop";
import type { AppSettings, ThemeMode } from "@/entities/appSettings/types";
import type {
  DesktopBox,
  DesktopBoxItem,
  DesktopBoxItemDropPlacement,
  DesktopBoxTitlePosition,
  NativeIconPosition,
} from "@/entities/desktopBox/types";
import type { DesktopItem, DesktopNameDisplayMode } from "@/entities/desktopItem/types";
import {
  listenDesktopTakeoverBoxMoved,
  listenDesktopTakeoverContextMenu,
  listenDesktopTakeoverDropToBox,
  listenDesktopTakeoverDropToDesktop,
  listenDesktopTakeoverOpen,
  listenDesktopTakeoverRefresh,
  listenNativeDesktopError,
} from "@/shared/ipc/desktopTakeover";

/**
 * 更新 Box 时允许调用方控制是否清洗尺寸和广播事件，缩放等高频场景需要避免跨窗口刷新抖动
 */
interface UpdateBoxOptions {
  broadcast?: boolean;
  sanitize?: boolean;
}

/**
 * 桌面 Store 只维护 Box 映射和展示偏好，不移动、改名或删除真实桌面文件
 */
export const useDesktopStore = defineStore("desktop", () => {
  const desktopPath = ref("");
  const items = ref<DesktopItem[]>([]);
  const desktopItemPathKeys = ref<Set<string>>(new Set());
  const boxes = ref<DesktopBox[]>([]);
  const boxItems = ref<DesktopBoxItem[]>([]);
  const desktopItemPositions = ref<NativeIconPosition[]>([]);
  const isLoading = ref(true);
  const isInitialized = ref(false);
  const lastError = ref("");
  const settings = ref<AppSettings>({ ...DEFAULT_APP_SETTINGS });
  const storeInstanceId = crypto.randomUUID();
  let stateListenerRegistered = false;
  let desktopTakeoverListenerRegistered = false;
  let themeTransitionTimer: ReturnType<typeof window.setTimeout> | null = null;

  /**
   * 桌面目录项目与额外拖入的任意路径分开计算，避免外部磁盘文件被误当成桌面未分组项目
   */
  const desktopItems = computed(() =>
    items.value.filter((item) => desktopItemPathKeys.value.has(normalizeItemPathKey(item.path))),
  );

  /**
   * 桌面视图只显示真实桌面目录中尚未归入 Box 的项目；拖入 Box 后通过映射过滤实现“桌面消失”
   */
  const unassignedItems = computed(() => {
    const assignedPathKeys = new Set(
      boxItems.value.map((boxItem) => normalizeItemPathKey(boxItem.itemPath)),
    );

    return desktopItems.value.filter(
      (item) => !assignedPathKeys.has(normalizeItemPathKey(item.path)),
    );
  });

  /**
   * 所有 Box 的项目总数直接来自关联表，避免窗口布局数据承担统计职责
   */
  const totalBoxItems = computed(() => boxItems.value.length);

  /**
   * 通过真实路径查找桌面项目，保证 Box 映射不依赖易变化的展示名称
   */
  function findItem(path: string): DesktopItem | undefined {
    const pathKey = normalizeItemPathKey(path);

    return items.value.find((item) => normalizeItemPathKey(item.path) === pathKey);
  }

  /**
   * 通过关联表读取目标 Box 的路径列表，保持窗口布局对象只承载几何信息
   */
  function getBoxItemPaths(boxId: string): string[] {
    return boxItems.value
      .filter((boxItem) => boxItem.boxId === boxId)
      .sort((left, right) => left.orderIndex - right.orderIndex)
      .map((boxItem) => boxItem.itemPath);
  }

  /**
   * Box 渲染时从桌面快照反查项目详情，桌面文件不存在时自动不显示悬空映射
   */
  function getBoxItems(boxId: string): DesktopItem[] {
    return getBoxItemPaths(boxId)
      .map((path) => findItem(path))
      .filter((item) => item !== undefined);
  }

  /**
   * 初始化桌面快照、Box 布局和设置；同一窗口生命周期内只执行一次
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
      const [snapshot, savedBoxes, savedBoxItems, savedSettings] = await Promise.all([
        getDesktopSnapshot(),
        loadBoxes(),
        loadBoxItems(),
        loadSettings(),
      ]);

      desktopPath.value = snapshot.desktopPath;
      desktopItemPathKeys.value = new Set(
        snapshot.items.map((item) => normalizeItemPathKey(item.path)),
      );
      items.value = snapshot.items;
      settings.value = savedSettings;
      registerDesktopTakeoverListeners();
      applyCurrentWindowTheme();
      boxes.value = (savedBoxes.length > 0 ? savedBoxes : [createDefaultBox()]).map((box) =>
        sanitizeBoxSize(box),
      );
      boxItems.value = savedBoxItems;
      await ensureItemsAvailable(savedBoxItems.map((boxItem) => boxItem.itemPath));

      await applyDesktopTakeoverMode();
      await Promise.all(boxes.value.map((box) => saveBox(box)));
      isInitialized.value = true;
    } catch (error) {
      lastError.value = error instanceof Error ? error.message : String(error);
    } finally {
      isLoading.value = false;
    }
  }

  /**
   * 主动刷新桌面目录快照，用于用户手动同步新增或删除的桌面文件
   */
  async function refreshSnapshot(shouldBroadcast = true): Promise<void> {
    const snapshot = await getDesktopSnapshot();
    applyDesktopSnapshot(snapshot);
    await ensureItemsAvailable(boxItems.value.map((boxItem) => boxItem.itemPath));

    await applyDesktopTakeoverMode();

    if (shouldBroadcast) {
      await broadcastStateChanged("desktop");
    }
  }

  /**
   * 设置窗重新获得焦点时从 SQLite 读取最新状态，避免托盘或原生事件写入后页面状态滞后
   */
  async function reloadPersistedState(): Promise<void> {
    const [savedBoxes, savedBoxItems, savedSettings] = await Promise.all([
      loadBoxes(),
      loadBoxItems(),
      loadSettings(),
    ]);

    const previousTheme = settings.value.settingsTheme;
    boxes.value = savedBoxes.map((savedBox) => sanitizeBoxSize(savedBox));
    boxItems.value = savedBoxItems;
    await ensureItemsAvailable(savedBoxItems.map((boxItem) => boxItem.itemPath));
    settings.value = savedSettings;
    applyCurrentWindowTheme(previousTheme !== settings.value.settingsTheme);
    await applyDesktopTakeoverMode();
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
   * 创建一个新的独立 Box，默认位置错开，避免覆盖现有窗口
   */
  async function createBox(title = "新建 Box"): Promise<DesktopBox> {
    const nextBox = sanitizeBoxSize({
      id: crypto.randomUUID(),
      title,
      ...BOX_DEFAULT_STATE,
      x: BOX_WINDOW_PLACEMENT.initialX + boxes.value.length * BOX_WINDOW_PLACEMENT.cascadeStep,
      y: BOX_WINDOW_PLACEMENT.initialY + boxes.value.length * BOX_WINDOW_PLACEMENT.cascadeStep,
      ...BOX_WINDOW_SIZE.default,
    });

    boxes.value.push(nextBox);
    await saveBox(nextBox);
    /**
     * 状态广播不会回流到当前 Store 实例，新增 Box 后必须立即把本窗口内存态同步给原生接管层。
     * 否则 SQLite 已经落库但 Win32 控制器仍持有旧快照，用户会看到“新增 Box”按钮没有任何反应。
     */
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("boxes");
    return nextBox;
  }

  /**
   * 更新 Box 的布局或映射，尺寸默认会经过最小可操作范围保护
   */
  async function updateBox(nextBox: DesktopBox, options: UpdateBoxOptions = {}): Promise<void> {
    const index = boxes.value.findIndex((box) => box.id === nextBox.id);
    if (index === -1) {
      return;
    }

    const { broadcast = true, sanitize = true } = options;
    const normalizedBox = sanitize ? sanitizeBoxSize(nextBox) : nextBox;
    boxes.value[index] = normalizedBox;
    await saveBox(normalizedBox);

    if (broadcast) {
      await broadcastStateChanged("boxes");
    }
    await syncDesktopTakeoverRuntime();
  }

  /**
   * 文件只会被重新映射到目标 Box，不会移动、改名或删除真实桌面文件
   */
  async function assignItemToBox(itemPath: string, boxId: string): Promise<void> {
    await assignItemsToBox([itemPath], boxId);
  }

  /**
   * 批量映射用于原生文件拖放，保证一次 Drop 只触发一次数据库写入
   */
  async function assignItemsToBox(itemPaths: string[], boxId: string): Promise<void> {
    const uniqueItemPaths = normalizeUniqueItemPaths(itemPaths);
    if (!boxId) {
      lastError.value = "无法收纳项目：目标 Box 无效";
      return;
    }
    if (uniqueItemPaths.length === 0) {
      return;
    }

    await assignBoxItems(boxId, uniqueItemPaths);
    const uniqueItemPathKeys = new Set(
      uniqueItemPaths.map((itemPath) => normalizeItemPathKey(itemPath)),
    );
    boxItems.value = boxItems.value.filter(
      (boxItem) => !uniqueItemPathKeys.has(normalizeItemPathKey(boxItem.itemPath)),
    );
    const nextOrderStart = boxItems.value.filter((boxItem) => boxItem.boxId === boxId).length;
    boxItems.value.push(
      ...uniqueItemPaths.map((itemPath, index) => ({
        boxId,
        itemPath,
        orderIndex: nextOrderStart + index,
      })),
    );
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("boxes");
  }

  /**
   * Box 内拖拽排序只重排关联表，不移动真实桌面文件位置
   */
  async function reorderBoxItem(
    boxId: string,
    draggedItemPath: string,
    targetItemPath: string,
    placement: DesktopBoxItemDropPlacement,
  ): Promise<void> {
    if (draggedItemPath === targetItemPath) {
      return;
    }

    const currentPaths = getBoxItemPaths(boxId);
    const draggedIndex = currentPaths.indexOf(draggedItemPath);
    const targetIndex = currentPaths.indexOf(targetItemPath);
    if (draggedIndex === -1 || targetIndex === -1) {
      return;
    }

    currentPaths.splice(draggedIndex, 1);
    const nextTargetIndex = currentPaths.indexOf(targetItemPath);
    const insertIndex = placement === "after" ? nextTargetIndex + 1 : nextTargetIndex;

    currentPaths.splice(insertIndex, 0, draggedItemPath);
    applyBoxItemOrder(boxId, currentPaths);
    await saveBoxItemOrder(boxId, currentPaths);
    await broadcastStateChanged("boxes");
  }

  /**
   * 拖到 Box 空白区域时把项目放到末尾，贴近 Windows 桌面图标的手动排列习惯
   */
  async function moveBoxItemToEnd(boxId: string, itemPath: string): Promise<void> {
    const currentPaths = getBoxItemPaths(boxId);
    if (!currentPaths.includes(itemPath) || currentPaths[currentPaths.length - 1] === itemPath) {
      return;
    }

    const nextPaths = currentPaths.filter((path) => path !== itemPath);
    nextPaths.push(itemPath);
    applyBoxItemOrder(boxId, nextPaths);
    await saveBoxItemOrder(boxId, nextPaths);
    await broadcastStateChanged("boxes");
  }

  /**
   * 从 Box 中删除单个映射，用于图标拖出窗口后恢复到桌面未归类列表
   */
  async function removeItemFromBox(boxId: string, itemPath: string): Promise<void> {
    boxItems.value = boxItems.value.filter(
      (boxItem) => !(boxItem.boxId === boxId && boxItem.itemPath === itemPath),
    );
    await deleteBoxItem(boxId, itemPath);
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("boxes");
  }

  /**
   * Box 图标拖回桌面时删除映射并保存释放点，原生桌面层随后会在该位置重绘项目
   */
  async function moveItemFromBoxToDesktop(
    boxId: string,
    itemPath: string,
    screenX: number,
    screenY: number,
  ): Promise<void> {
    saveDesktopItemPosition(itemPath, screenX, screenY);
    await removeItemFromBox(boxId, itemPath);
  }

  /**
   * 本地先更新排序，避免拖放后等待 SQLite 写入才刷新图标位置
   */
  function applyBoxItemOrder(boxId: string, orderedPaths: string[]): void {
    const orderMap = new Map(orderedPaths.map((path, index) => [path, index]));
    boxItems.value = boxItems.value.map((boxItem) =>
      boxItem.boxId === boxId && orderMap.has(boxItem.itemPath)
        ? {
            ...boxItem,
            orderIndex: orderMap.get(boxItem.itemPath) ?? boxItem.orderIndex,
          }
        : boxItem,
    );
  }

  /**
   * 原生拖入的路径可能来自任意磁盘，先让后端按 Windows Shell 解析出图标和名称，再写入 Box 映射
   */
  async function assignDroppedPathsToBox(paths: string[], boxId: string): Promise<void> {
    const acceptedPaths = normalizeUniqueItemPaths(paths);

    await ensureItemsAvailable(acceptedPaths);

    await assignItemsToBox(acceptedPaths.filter((path) => findItem(path)), boxId);
  }

  /**
   * 删除 Box 只删除分组映射，真实桌面文件继续保留在 Explorer 管理下
   */
  async function deleteBox(boxId: string): Promise<void> {
    boxes.value = boxes.value.filter((box) => box.id !== boxId);
    boxItems.value = boxItems.value.filter((boxItem) => boxItem.boxId !== boxId);
    await deleteBoxRecord(boxId);
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("boxes");
  }

  /**
   * Drop 事件只接受路径文本，避免把浏览器临时拖拽数据写入映射
   */
  async function handleItemDrop(event: DragEvent, boxId: string): Promise<void> {
    const itemPath = event.dataTransfer?.getData("text/plain");
    if (!itemPath) {
      return;
    }

    await assignItemToBox(itemPath, boxId);
  }

  /**
   * 首次启动时创建一个默认 Box，让用户能立即看到桌面扩展本体
   */
  function createDefaultBox(): DesktopBox {
    return {
      ...BOX_DEFAULT_STATE,
      id: crypto.randomUUID(),
      title: "工作",
      x: BOX_WINDOW_PLACEMENT.initialX,
      y: BOX_WINDOW_PLACEMENT.initialY,
      ...BOX_WINDOW_SIZE.default,
    };
  }

  /**
   * 更新设置页主题时同步根节点标识；原生 Box 主题通过运行时状态同步给 Win32 控制器
   */
  async function updateSettingsTheme(theme: ThemeMode): Promise<void> {
    settings.value.settingsTheme = theme;
    applyTheme(theme, true);
    await saveSetting(APP_SETTING_KEYS.settingsTheme, theme);
    await broadcastStateChanged("settings");
  }

  /**
   * Box 主题只影响原生桌面上的 Box 面板，修改后立即同步给 Win32 控制器重绘
   */
  async function updateBoxTheme(theme: ThemeMode): Promise<void> {
    settings.value.boxTheme = theme;
    await saveSetting(APP_SETTING_KEYS.boxTheme, theme);
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("settings");
  }

  /**
   * Box 缩放网格化只影响窗口尺寸交互，不改变现有 Box 内文件映射
   */
  async function updateBoxResizeGridEnabled(enabled: boolean): Promise<void> {
    settings.value.boxResizeGridEnabled = enabled;
    await saveSetting(APP_SETTING_KEYS.boxResizeGridEnabled, enabled);
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("settings");
  }

  /**
   * 边缘吸附开关立即持久化，避免下次启动恢复旧交互习惯
   */
  async function updateSnapToEdges(enabled: boolean): Promise<void> {
    settings.value.snapToEdges = enabled;
    await saveSetting(APP_SETTING_KEYS.snapToEdges, enabled);
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("settings");
  }

  /**
   * 吸附阈值使用像素保存，和 Tauri 窗口移动事件的坐标体系保持一致
   */
  async function updateSnapThreshold(threshold: number): Promise<void> {
    await updateNumberSetting(APP_SETTING_KEYS.snapThreshold, threshold);
  }

  /**
   * 桌面接管模式会进入 Win32 原生桌面层，必须等后端确认成功后再持久化开启状态。
   *
   * 如果先写入 SQLite 再启动原生层，启动失败或访问违例会让下次应用启动自动重试接管，
   * 用户会陷入“打开即崩”的循环。关闭接管则优先落库，确保异常退出后 Explorer 图标可恢复。
   */
  async function updateDesktopTakeoverEnabled(value: boolean): Promise<void> {
    if (!value) {
      settings.value.desktopTakeoverEnabled = false;
      await saveSetting(APP_SETTING_KEYS.desktopTakeoverEnabled, false);
      await applyDesktopTakeoverMode();
      await broadcastStateChanged("settings");
      return;
    }

    const previousValue = settings.value.desktopTakeoverEnabled;
    settings.value.desktopTakeoverEnabled = true;
    try {
      await applyDesktopTakeoverMode();
      await saveSetting(APP_SETTING_KEYS.desktopTakeoverEnabled, true);
      await broadcastStateChanged("settings");
    } catch (error) {
      settings.value.desktopTakeoverEnabled = previousValue;
      await setDesktopTakeoverEnabled(false);
      throw error;
    }
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
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("settings");
  }

  /**
   * 图标名称显示属于纯渲染偏好，不影响 Box 与真实文件的映射关系
   */
  async function updateShowItemLabels(value: boolean): Promise<void> {
    settings.value.showItemLabels = value;
    await saveSetting(APP_SETTING_KEYS.showItemLabels, value);
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("settings");
  }

  /**
   * 快捷方式箭头只影响 Dasktop 的视觉提示，不改 Windows 快捷方式文件本身
   */
  async function updateShowShortcutArrow(value: boolean): Promise<void> {
    settings.value.showShortcutArrow = value;
    await saveSetting(APP_SETTING_KEYS.showShortcutArrow, value);
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("settings");
  }

  /**
   * 打开方式只影响 Dasktop 图标的点击交互，不改变系统默认打开程序
   */
  async function updateDoubleClickOpenItems(value: boolean): Promise<void> {
    settings.value.doubleClickOpenItems = value;
    await saveSetting(APP_SETTING_KEYS.doubleClickOpenItems, value);
    await syncDesktopTakeoverRuntime();
    await broadcastStateChanged("settings");
  }

  /**
   * 文件名显示模式用于统一控制 Box 内后缀呈现，避免同一路径在多个 Box 中展示不一致
   */
  async function updateNameDisplayMode(value: DesktopNameDisplayMode): Promise<void> {
    settings.value.nameDisplayMode = value;
    await saveSetting(APP_SETTING_KEYS.nameDisplayMode, value);
    await syncDesktopTakeoverRuntime();
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
   * 锁定只影响当前 Box 的移动和缩放，内部图标拖拽、打开和右键菜单仍保持可用
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
      width: Math.max(box.width, BOX_WINDOW_SIZE.min.width),
      height: Math.max(box.height, BOX_WINDOW_SIZE.min.height),
      locked: Boolean(box.locked),
      titleOpacity: sanitizeBoxTitleOpacity(box.titleOpacity),
    };
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
   * Box 收缩动画时长从设置读取，确保所有窗口动画节奏一致
   */
  function getBoxCollapseAnimationMs(): number {
    return sanitizeNumberAppSetting(
      APP_SETTING_KEYS.boxCollapseAnimationMs,
      settings.value.boxCollapseAnimationMs,
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
   * 设置页按自身主题渲染，原生 Box 的明暗主题不再通过 WebView DOM 生效
   */
  function applyCurrentWindowTheme(shouldAnimate = false): void {
    applyTheme(settings.value.settingsTheme, shouldAnimate);
  }

  /**
   * 刷新桌面快照时保留已经收纳的外部磁盘项目，避免非桌面文件在下次刷新后从 Box 中消失
   */
  function applyDesktopSnapshot(snapshot: { desktopPath: string; items: DesktopItem[] }): void {
    const nextDesktopPathKeys = new Set(
      snapshot.items.map((item) => normalizeItemPathKey(item.path)),
    );
    const assignedPathKeys = new Set(
      boxItems.value.map((boxItem) => normalizeItemPathKey(boxItem.itemPath)),
    );
    const assignedExternalItems = items.value.filter((item) => {
      const itemPathKey = normalizeItemPathKey(item.path);

      return assignedPathKeys.has(itemPathKey) && !nextDesktopPathKeys.has(itemPathKey);
    });

    desktopPath.value = snapshot.desktopPath;
    desktopItemPathKeys.value = nextDesktopPathKeys;
    mergeKnownItems([...snapshot.items, ...assignedExternalItems]);
  }

  /**
   * 按路径补齐项目详情，支持用户把任意磁盘上的文件拖入 Box 后仍复用 Windows Shell 图标
   */
  async function ensureItemsAvailable(paths: string[]): Promise<void> {
    const missingPaths = normalizeUniqueItemPaths(paths).filter((path) => !findItem(path));
    if (missingPaths.length === 0) {
      return;
    }

    const resolvedItems = await getDesktopItemsByPaths(missingPaths);

    mergeKnownItems(resolvedItems);
  }

  /**
   * 项目列表用路径做稳定主键合并，后端重新解析出的图标可以覆盖旧缓存
   */
  function mergeKnownItems(nextItems: DesktopItem[]): void {
    const itemMap = new Map(items.value.map((item) => [normalizeItemPathKey(item.path), item]));

    for (const item of nextItems) {
      itemMap.set(normalizeItemPathKey(item.path), item);
    }

    items.value = Array.from(itemMap.values());
  }

  /**
   * Windows 路径比较不区分大小写，统一归一化后可避免同一路径重复进入 Box
   */
  function normalizeUniqueItemPaths(paths: string[]): string[] {
    const seenPathKeys = new Set<string>();
    const uniquePaths: string[] = [];

    for (const path of paths) {
      const pathKey = normalizeItemPathKey(path);
      if (!pathKey || seenPathKeys.has(pathKey)) {
        continue;
      }

      seenPathKeys.add(pathKey);
      uniquePaths.push(path);
    }

    return uniquePaths;
  }

  /**
   * 前后端都以原始路径落库，归一化仅用于运行时去重和查找，不改变真实文件路径
   */
  function normalizeItemPathKey(path: string): string {
    return path.trim().replace(/\//g, "\\").toLowerCase();
  }

  /**
   * 桌面接管模式由前端状态驱动：先同步完整状态，再让 Rust 原生层创建 Box 背景并协调 Explorer 图标坐标
   */
  async function applyDesktopTakeoverMode(): Promise<void> {
    if (!settings.value.desktopTakeoverEnabled) {
      await setDesktopTakeoverEnabled(false);
      return;
    }

    await syncDesktopTakeoverRuntime();
    await setDesktopTakeoverEnabled(true);
  }

  /**
   * 原生层只持有运行时副本，任何 Box 映射、布局或显示设置变化后都需要同步最新状态。
   * 这里必须同步桌面目录全集，由 Rust 根据 Box 映射过滤；若提前传入未分配列表，图标拖回桌面后会因为二次过滤而无法重绘。
   */
  async function syncDesktopTakeoverRuntime(): Promise<void> {
    if (!settings.value.desktopTakeoverEnabled) {
      return;
    }

    await syncNativeDesktopState({
      boxItems: boxItems.value.map((boxItem) => ({ ...boxItem })),
      boxes: boxes.value.map((box) => ({
        ...box,
        theme: settings.value.boxTheme,
      })),
      display: {
        boxBackgroundOpacity: settings.value.boxBackgroundOpacity,
        cornerRadius: settings.value.boxCornerRadius,
        iconGapX: settings.value.boxIconGapX,
        iconGapY: settings.value.boxIconGapY,
        iconSize: settings.value.boxIconSize,
        labelTextSize: settings.value.boxLabelTextSize,
        labelWidth: settings.value.boxFilenameWidth,
        showItemLabels: settings.value.showItemLabels,
        showShortcutArrow: settings.value.showShortcutArrow,
      },
      iconPositions: desktopItemPositions.value,
      items: desktopItems.value,
    });
  }

  /**
   * 设置页需要一个显式入口把当前 Box 布局推给原生控制器，不再通过打开 WebView 窗口体现 Box。
   */
  async function refreshNativeDesktopRuntime(): Promise<void> {
    await syncDesktopTakeoverRuntime();
    await refreshNativeDesktopLayer();
  }

  /**
   * 原生桌面事件只应由 main 控制窗口处理，避免每个 Box 窗口重复写数据库或弹菜单
   */
  function registerDesktopTakeoverListeners(): void {
    if (desktopTakeoverListenerRegistered) {
      return;
    }

    desktopTakeoverListenerRegistered = true;
    void listenDesktopTakeoverOpen(async ({ payload }) => {
      await openDesktopItem(payload.itemPath);
    }).catch(registerDesktopTakeoverListenerFailure);
    void listenDesktopTakeoverContextMenu(async ({ payload }) => {
      const targetItem = findItem(payload.itemPath);
      if (!targetItem) {
        return;
      }

      await showNativeItemContextMenu(targetItem.path, payload.screenX, payload.screenY);
    }).catch(registerDesktopTakeoverListenerFailure);
    void listenDesktopTakeoverDropToBox(async ({ payload }) => {
      await assignDroppedPathsToBox([payload.itemPath], payload.boxId);
    }).catch(registerDesktopTakeoverListenerFailure);
    void listenDesktopTakeoverDropToDesktop(async ({ payload }) => {
      if (payload.sourceBoxId) {
        await removeItemFromBox(payload.sourceBoxId, payload.itemPath);
      }
      saveDesktopItemPosition(payload.itemPath, payload.x, payload.y);
      await syncDesktopTakeoverRuntime();
    }).catch(registerDesktopTakeoverListenerFailure);
    void listenDesktopTakeoverBoxMoved(async ({ payload }) => {
      const targetBox = boxes.value.find((box) => box.id === payload.boxId);
      if (!targetBox || targetBox.locked) {
        return;
      }

      await updateBox({
        ...targetBox,
        x: Math.round(payload.x),
        y: Math.round(payload.y),
      });
    }).catch(registerDesktopTakeoverListenerFailure);
    void listenDesktopTakeoverRefresh(async () => {
      await refreshSnapshot(false);
      await refreshNativeDesktopLayer();
    }).catch(registerDesktopTakeoverListenerFailure);
    void listenNativeDesktopError(({ payload }) => {
      lastError.value = payload.message;
    }).catch(registerDesktopTakeoverListenerFailure);
  }

  /**
   * 监听注册失败需要允许下次初始化重试，否则 main 窗口早期异常会让原生层永久失联
   */
  function registerDesktopTakeoverListenerFailure(error: unknown): void {
    desktopTakeoverListenerRegistered = false;
    lastError.value = error instanceof Error ? error.message : String(error);
  }

  /**
   * 桌面落点先保存在运行时内存中，确保拖出 Box 后本次会话立即按落点重绘
   */
  function saveDesktopItemPosition(itemPath: string, x: number, y: number): void {
    const itemPathKey = normalizeItemPathKey(itemPath);
    const nextPosition: NativeIconPosition = {
      containerId: null,
      containerKind: "desktop",
      itemPath,
      monitorId: null,
      orderIndex: null,
      x: Math.round(x),
      y: Math.round(y),
    };

    desktopItemPositions.value = [
      ...desktopItemPositions.value.filter(
        (position) => normalizeItemPathKey(position.itemPath) !== itemPathKey,
      ),
      nextPosition,
    ];
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
    if (settings.value.settingsTheme === "system") {
      applyCurrentWindowTheme(true);
    }
  });

  return {
    assignItemToBox,
    assignDroppedPathsToBox,
    boxes,
    createBox,
    deleteBox,
    desktopItems,
    desktopPath,
    findItem,
    getBoxCollapseAnimationMs,
    getBoxItemPaths,
    getBoxItems,
    handleItemDrop,
    initialize,
    isInitialized,
    isLoading,
    items,
    lastError,
    moveBoxItemToEnd,
    moveItemFromBoxToDesktop,
    reloadPersistedState,
    removeItemFromBox,
    reorderBoxItem,
    refreshSnapshot,
    refreshNativeDesktopRuntime,
    settings,
    boxItems,
    totalBoxItems,
    unassignedItems,
    updateBox,
    updateBoxCollapsed,
    updateBoxLocked,
    updateBoxResizeGridEnabled,
    updateBoxTheme,
    updateBoxTitleOpacity,
    updateBoxTitlePosition,
    updateDoubleClickOpenItems,
    updateNameDisplayMode,
    updateDesktopTakeoverEnabled,
    updateNumberSetting,
    updateSettingsTheme,
    updateShowItemLabels,
    updateShowShortcutArrow,
    updateSnapThreshold,
    updateSnapToEdges,
  };
});
