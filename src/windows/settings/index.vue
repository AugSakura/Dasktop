<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Boxes, Info, MonitorCog, Palette } from "@lucide/vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";
import AboutPanel from "./components/AboutPanel.vue";
import AppearancePanel from "./components/AppearancePanel.vue";
import BoxDisplayPanel from "./components/BoxDisplayPanel.vue";
import BoxesPanel from "./components/BoxesPanel.vue";
import SettingsHeader from "./components/SettingsHeader.vue";
import SettingsSidebar from "./components/SettingsSidebar.vue";
import {
  isAutostartEnabled,
  setAutostartEnabled,
  setTrayDesktopTakeoverChecked,
} from "@/entities/appSettings/api";
import { useDesktopStore } from "@/entities/desktopBox/store";
import { SETTINGS_WINDOW_SYNC_TIMING } from "@/entities/desktopBox/layout";
import {
  listenAutostartChanged,
  listenTrayCreateBox,
  listenTrayToggleDesktopTakeover,
} from "@/shared/ipc/appTray";
import type { Component } from "vue";
import type { DesktopBox } from "@/entities/desktopBox/types";

/**
 * 设置页的导航键值统一收口，避免侧栏、标题和内容区域各自维护字符串
 */
type SettingsSection = "boxes" | "boxDisplay" | "appearance" | "about";

/**
 * 设置页左侧导航项，icon 使用组件类型以便保持 Lucide 图标风格一致
 */
interface SettingsNavItem {
  key: SettingsSection;
  label: string;
  icon: Component;
}

/**
 * 设置页内容宽度由父窗口统一定义，再下发给各面板以保持视觉密度一致
 */
const SETTINGS_PANEL_WIDTH = {
  default: "max-w-[780px]",
  wide: "max-w-[820px]",
} as const;

const currentWindow = getCurrentWindow();
const desktopStore = useDesktopStore();
const activeSection = ref<SettingsSection>("boxes");
const startupError = ref("");
const autostartEnabled = ref(false);
const unlistenFns: UnlistenFn[] = [];
/**
 * 设置窗获得焦点时不立刻刷新桌面快照，避免 Shell 缩略图扫描卡住标题栏拖动首帧
 */
const FOCUS_SYNC_DELAY_MS = SETTINGS_WINDOW_SYNC_TIMING.focusSyncDelayMs;
const DRAG_RELEASE_FALLBACK_MS = SETTINGS_WINDOW_SYNC_TIMING.dragReleaseFallbackMs;
let focusSyncTimer: ReturnType<typeof window.setTimeout> | null = null;
let isDraggingSettingsWindow = false;
let dragReleaseCleanup: (() => void) | null = null;

/**
 * 设置页只保留当前真实可用的配置入口，旧占位导航不再兼容
 */
const sections: SettingsNavItem[] = [
  { key: "boxes", label: "Box", icon: Boxes },
  { key: "boxDisplay", label: "设置", icon: MonitorCog },
  { key: "appearance", label: "外观", icon: Palette },
  { key: "about", label: "关于", icon: Info },
];

const activeTitle = computed(
  () => sections.find((section) => section.key === activeSection.value)?.label ?? "设置",
);
const totalItems = computed(() => desktopStore.totalBoxItems);
const boxItemCounts = computed(() =>
  Object.fromEntries(
    desktopStore.boxes.map((box) => [box.id, desktopStore.getBoxItemPaths(box.id).length]),
  ),
);

onMounted(async () => {
  await desktopStore.initialize();
  await syncAutostartEnabled();
  await syncTrayDesktopTakeover();
  unlistenFns.push(
    await listenAutostartChanged(({ payload }) => {
      autostartEnabled.value = payload;
    }),
  );
  unlistenFns.push(
    await listenTrayCreateBox(async () => {
      await createAndOpenBox();
    }),
  );
  unlistenFns.push(
    await listenTrayToggleDesktopTakeover(async ({ payload }) => {
      await updateDesktopTakeoverEnabled(payload);
    }),
  );
  unlistenFns.push(
    await currentWindow.onFocusChanged(({ payload }) => {
      if (!payload) {
        clearFocusSyncTimer();
        return;
      }

      scheduleFocusSync();
    }),
  );

  await currentWindow.hide();
});

/**
 * 组件销毁时注销窗口事件，避免开发热更新后重复刷新状态
 */
onUnmounted(() => {
  clearFocusSyncTimer();
  clearDragReleaseListeners();
  for (const unlisten of unlistenFns) {
    unlisten();
  }
});

/**
 * 新增 Box 后同步给原生控制器，真实显示由 Win32 桌面层负责而非 WebView 窗口。
 */
async function createAndOpenBox(): Promise<void> {
  await desktopStore.createBox();
}

/**
 * 设置页里的锁定入口与 Box 更多菜单共用同一 Store 字段，确保窗口拖动和缩放行为一致
 */
async function toggleBoxLockedFromSettings(box: DesktopBox): Promise<void> {
  await desktopStore.updateBoxLocked(box.id, !box.locked);
}

/**
 * 设置页收到的删除事件已经由按钮完成二段式确认，这里只删除 Box 映射和布局。
 */
async function deleteBoxFromSettings(box: DesktopBox): Promise<void> {
  await desktopStore.deleteBox(box.id);
}

/**
 * Box 已由原生控制器常驻绘制，设置页里的“打开”动作只负责确认状态已经同步。
 */
async function focusNativeBoxFromSettings(_box: DesktopBox): Promise<void> {
  await desktopStore.refreshNativeDesktopRuntime();
}

/**
 * 开机自启以系统启动项为准，设置页每次需要展示时都重新读取，避免外部修改后状态滞后
 */
async function syncAutostartEnabled(): Promise<void> {
  try {
    autostartEnabled.value = await isAutostartEnabled();
  } catch (error) {
    desktopStore.lastError = error instanceof Error ? error.message : String(error);
  }
}

/**
 * 设置页切换自启后交给 Rust 同步托盘勾选状态，前端只保存后端确认后的真实结果
 */
async function updateAutostartEnabled(value: boolean): Promise<void> {
  try {
    autostartEnabled.value = await setAutostartEnabled(value);
  } catch (error) {
    desktopStore.lastError = error instanceof Error ? error.message : String(error);
    await syncAutostartEnabled();
  }
}

/**
 * 桌面接管模式的真实写入复用 Store，托盘只跟随最终偏好展示勾选状态
 */
async function updateDesktopTakeoverEnabled(value: boolean): Promise<void> {
  try {
    await desktopStore.updateDesktopTakeoverEnabled(value);
  } catch (error) {
    desktopStore.lastError = error instanceof Error ? error.message : String(error);
  } finally {
    await syncTrayDesktopTakeover();
  }
}

/**
 * 设置页初始化、聚焦刷新和托盘操作后都用当前 Store 状态回写托盘，避免两个入口显示不一致
 */
async function syncTrayDesktopTakeover(): Promise<void> {
  try {
    await setTrayDesktopTakeoverChecked(desktopStore.settings.desktopTakeoverEnabled);
  } catch (error) {
    desktopStore.lastError = error instanceof Error ? error.message : String(error);
  }
}

/**
 * 自定义标题栏通过 Tauri 转交拖动，保持无边框窗口仍可移动
 */
function startDragging(event: MouseEvent): void {
  if (event.button !== 0 || event.detail > 1) {
    return;
  }

  isDraggingSettingsWindow = true;
  clearFocusSyncTimer();
  bindDragReleaseListeners();
  void currentWindow.startDragging();
}

/**
 * 焦点同步延迟执行，让用户点击标题栏拖动时先进入系统拖动流程，再刷新 SQLite 和桌面快照
 */
function scheduleFocusSync(): void {
  clearFocusSyncTimer();
  focusSyncTimer = window.setTimeout(() => {
    focusSyncTimer = null;
    void syncSettingsWindowState();
  }, FOCUS_SYNC_DELAY_MS);
}

/**
 * 聚焦后同步持久化状态和桌面快照；拖动中收到兜底触发时继续后延，避免刷新抢占拖动
 */
async function syncSettingsWindowState(): Promise<void> {
  if (isDraggingSettingsWindow) {
    scheduleFocusSync();
    return;
  }

  await desktopStore.reloadPersistedState();
  await syncAutostartEnabled();
  await syncTrayDesktopTakeover();
  await desktopStore.refreshSnapshot(false);
}

/**
 * 清理焦点同步计时器，避免隐藏或拖动窗口时仍然启动一次昂贵的桌面扫描
 */
function clearFocusSyncTimer(): void {
  if (!focusSyncTimer) {
    return;
  }

  window.clearTimeout(focusSyncTimer);
  focusSyncTimer = null;
}

/**
 * 监听拖动释放后再恢复设置同步；原生拖动吞掉释放事件时用短兜底恢复
 */
function bindDragReleaseListeners(): void {
  clearDragReleaseListeners();

  const fallbackTimer = window.setTimeout(finishDragging, DRAG_RELEASE_FALLBACK_MS);
  function finishDragging(): void {
    isDraggingSettingsWindow = false;
    clearDragReleaseListeners();
    scheduleFocusSync();
  }

  window.addEventListener("mouseup", finishDragging, { capture: true, once: true });
  window.addEventListener("pointerup", finishDragging, { capture: true, once: true });
  document.addEventListener("mouseup", finishDragging, { capture: true, once: true });
  document.addEventListener("pointerup", finishDragging, { capture: true, once: true });
  dragReleaseCleanup = () => {
    window.clearTimeout(fallbackTimer);
    window.removeEventListener("mouseup", finishDragging, { capture: true });
    window.removeEventListener("pointerup", finishDragging, { capture: true });
    document.removeEventListener("mouseup", finishDragging, { capture: true });
    document.removeEventListener("pointerup", finishDragging, { capture: true });
  };
}

/**
 * 拖动释放监听每次只保留一组，防止多次按住标题栏后重复安排同步
 */
function clearDragReleaseListeners(): void {
  dragReleaseCleanup?.();
  dragReleaseCleanup = null;
}

/**
 * 最小化只作用于设置窗口，桌面上的 Box 会继续保持显示
 */
function minimizeSettings(): void {
  void currentWindow.minimize();
}

/**
 * 最大化只作用于设置窗口，不改变任何 Box 的桌面位置和尺寸
 */
function toggleSettingsMaximize(): void {
  void currentWindow.toggleMaximize();
}

/**
 * 关闭设置页时隐藏主窗口，Box 右键菜单仍可重新唤起设置
 */
function closeSettings(): void {
  void currentWindow.hide();
}
</script>

<template>
  <main class="h-screen w-screen overflow-hidden bg-transparent text-[#17181c] dark:text-[#f4f4f5]">
    <section class="flex h-full w-full overflow-hidden bg-[#f7f7f9] shadow-[0_16px_36px_rgba(20,24,32,0.16)] ring-1 ring-inset ring-[#d7dae2] dark:bg-[#15161b] dark:shadow-[0_16px_36px_rgba(0,0,0,0.34)] dark:ring-[#2b2e36]">
      <SettingsSidebar
        :active-section="activeSection"
        :sections="sections"
        @section-change="activeSection = $event"
      />

      <section class="flex min-w-0 flex-1 flex-col bg-[#fbfbfd] dark:bg-[#101116]">
        <SettingsHeader
          :title="activeTitle"
          @close="closeSettings"
          @drag-start="startDragging"
          @minimize="minimizeSettings"
          @toggle-maximize="toggleSettingsMaximize"
        />

        <div class="dasktop-scrollarea min-h-0 flex-1 overflow-auto">
          <div
            v-if="startupError"
            class="mx-8 mt-6 rounded-[12px] border border-[#f0c7c7] bg-[#fff6f6] px-4 py-3 text-[13px] text-[#9f1d1d] dark:border-[#5b2a2d] dark:bg-[#281619] dark:text-[#ffb4b4]"
          >
            Box 启动失败：{{ startupError }}
          </div>

          <AppearancePanel
            v-if="activeSection === 'appearance'"
            :panel-width="SETTINGS_PANEL_WIDTH.default"
            :settings="desktopStore.settings"
            @box-visual-setting-change="desktopStore.updateNumberSetting"
            @box-theme-change="desktopStore.updateBoxTheme"
            @settings-theme-change="desktopStore.updateSettingsTheme"
          />
          <BoxDisplayPanel
            v-else-if="activeSection === 'boxDisplay'"
            :autostart-enabled="autostartEnabled"
            :panel-width="SETTINGS_PANEL_WIDTH.default"
            :settings="desktopStore.settings"
            @autostart-enabled-change="updateAutostartEnabled"
            @box-resize-grid-enabled-change="desktopStore.updateBoxResizeGridEnabled"
            @double-click-open-items-change="desktopStore.updateDoubleClickOpenItems"
            @item-labels-change="desktopStore.updateShowItemLabels"
            @name-display-mode-change="desktopStore.updateNameDisplayMode"
            @desktop-takeover-enabled-change="updateDesktopTakeoverEnabled"
            @show-shortcut-arrow-change="desktopStore.updateShowShortcutArrow"
            @snap-threshold-change="desktopStore.updateSnapThreshold"
            @snap-to-edges-change="desktopStore.updateSnapToEdges"
          />
          <BoxesPanel
            v-else-if="activeSection === 'boxes'"
            :boxes="desktopStore.boxes"
            :box-item-counts="boxItemCounts"
            :panel-width="SETTINGS_PANEL_WIDTH.wide"
            :total-items="totalItems"
            :unassigned-items="desktopStore.unassignedItems.length"
            @create-box="createAndOpenBox"
            @delete-box="deleteBoxFromSettings"
            @open-box="focusNativeBoxFromSettings"
            @refresh="desktopStore.refreshSnapshot"
            @toggle-box-locked="toggleBoxLockedFromSettings"
          />
          <AboutPanel
            v-else-if="activeSection === 'about'"
            :desktop-path="desktopStore.desktopPath"
            :panel-width="SETTINGS_PANEL_WIDTH.default"
          />
        </div>
      </section>
    </section>
  </main>
</template>
