<script setup lang="ts">
import { Monitor, Moon, RotateCcw, Sun } from "@lucide/vue";
import type { Component } from "vue";
import {
  APP_SETTING_NUMBER_LIMITS,
  DEFAULT_APP_SETTINGS,
  type AppSettingNumberKey,
} from "@/entities/appSettings/defaults";
import type { AppSettings, ThemeMode } from "@/entities/appSettings/types";
import SegmentedControl from "@/shared/ui/SegmentedControl.vue";

/**
 * 主题分段选项只服务当前外观面板，内联后避免为单一展示数据单独跳转文件
 */
const THEME_SEGMENT_OPTIONS: Array<{
  icon: Component;
  label: string;
  value: ThemeMode;
}> = [
  { icon: Sun, label: "浅色", value: "light" },
  { icon: Monitor, label: "跟随系统", value: "system" },
  { icon: Moon, label: "深色", value: "dark" },
];

/**
 * Box 外观数值项排除吸附距离，避免把窗口行为参数混入外观面板
 */
type BoxVisualSettingKey = Exclude<AppSettingNumberKey, "snapThreshold">;

/**
 * Box 外观调节项随外观面板渲染，保证主题、透明度和视觉密度配置入口一致
 */
const BOX_VISUAL_SETTING_CONTROLS: Array<{
  description: string;
  key: BoxVisualSettingKey;
  label: string;
  max: number;
  min: number;
  step: number;
  unit: string;
}> = [
  {
    key: "boxBackgroundOpacity",
    label: "背景透明度",
    description: "控制 Box 背景与桌面壁纸的融合程度",
    ...APP_SETTING_NUMBER_LIMITS.boxBackgroundOpacity,
  },
  {
    key: "boxCollapseAnimationMs",
    label: "收缩动画速度",
    description: "调整 Box 自动收起和展开的动画时长",
    ...APP_SETTING_NUMBER_LIMITS.boxCollapseAnimationMs,
  },
  {
    key: "boxIconSize",
    label: "图标大小",
    description: "调整 Box 内项目图标的显示尺寸",
    ...APP_SETTING_NUMBER_LIMITS.boxIconSize,
  },
  {
    key: "boxLabelTextSize",
    label: "文字大小",
    description: "调整文件名文字大小，适配不同分辨率",
    ...APP_SETTING_NUMBER_LIMITS.boxLabelTextSize,
  },
  {
    key: "boxIconGapX",
    label: "横向间距",
    description: "调整图标列之间的水平距离",
    ...APP_SETTING_NUMBER_LIMITS.boxIconGapX,
  },
  {
    key: "boxIconGapY",
    label: "纵向间距",
    description: "调整图标行之间的垂直距离",
    ...APP_SETTING_NUMBER_LIMITS.boxIconGapY,
  },
  {
    key: "boxFilenameWidth",
    label: "文件名宽度",
    description: "控制文件名换行宽度，长名称会在此范围内显示",
    ...APP_SETTING_NUMBER_LIMITS.boxFilenameWidth,
  },
  {
    key: "boxCornerRadius",
    label: "圆角大小",
    description: "调整 Box 面板和图标悬停区域的圆角",
    ...APP_SETTING_NUMBER_LIMITS.boxCornerRadius,
  },
];

/**
 * 外观面板承载主题和 Box 视觉密度设置，文件名显示规则交给设置页集中管理
 */
const props = defineProps<{
  /**
   * 设置页父级统一下发内容宽度，避免各面板各自维护页面密度
   */
  panelWidth: string;
  settings: AppSettings;
}>();

const emit = defineEmits<{
  boxVisualSettingChange: [key: BoxVisualSettingKey, value: number];
  boxThemeChange: [theme: ThemeMode];
  settingsThemeChange: [theme: ThemeMode];
}>();

/**
 * Box 外观滑块统一通过数值设置事件保存，避免面板直接了解数据库细节
 */
function emitBoxVisualSettingChange(key: BoxVisualSettingKey, event: Event): void {
  emit("boxVisualSettingChange", key, Number((event.target as HTMLInputElement).value));
}

/**
 * 单行重置只恢复当前设置项，避免用户调整多个视觉参数后被整体覆盖
 */
function resetBoxVisualSetting(key: BoxVisualSettingKey): void {
  emit("boxVisualSettingChange", key, DEFAULT_APP_SETTINGS[key]);
}
</script>

<template>
  <section class="mx-auto w-full px-8 py-7" :class="panelWidth">
    <div class="mb-6">
      <h1 class="text-[24px] font-semibold tracking-[0] text-[#17181c] dark:text-[#f4f4f5]">外观</h1>
      <p class="mt-1 text-[13px] text-[#6f7480] dark:text-[#a7abb5]">调整设置窗和 Box 的显示偏好</p>
    </div>

    <div class="overflow-hidden rounded-[8px] border border-[#dfe2e8] bg-[#ffffff] shadow-[0_10px_28px_rgba(20,24,32,0.06)] dark:border-[#292c34] dark:bg-[#181a20] dark:shadow-none">
      <div class="grid min-h-[76px] grid-cols-[1fr_auto] items-center gap-6 px-5">
        <div class="min-w-0 pr-4">
          <h2 class="text-[13px] font-semibold text-[#202229] dark:text-[#f4f4f5]">设置页主题</h2>
          <p class="mt-1 text-[12px] leading-5 text-[#707684] dark:text-[#9ca0aa]">只影响当前设置窗口的明暗显示</p>
        </div>

        <SegmentedControl
          :model-value="props.settings.settingsTheme"
          :options="THEME_SEGMENT_OPTIONS"
          @change="emit('settingsThemeChange', $event)"
        />
      </div>

      <div class="h-px bg-[#e7e9ee] dark:bg-[#292c34]" />

      <div class="grid min-h-[76px] grid-cols-[1fr_auto] items-center gap-6 px-5">
        <div class="min-w-0 pr-4">
          <h2 class="text-[13px] font-semibold text-[#202229] dark:text-[#f4f4f5]">Box 面板主题</h2>
          <p class="mt-1 text-[12px] leading-5 text-[#707684] dark:text-[#9ca0aa]">只影响桌面上的原生 Box 面板</p>
        </div>

        <SegmentedControl
          :model-value="props.settings.boxTheme"
          :options="THEME_SEGMENT_OPTIONS"
          @change="emit('boxThemeChange', $event)"
        />
      </div>
    </div>

    <div class="mt-4 overflow-hidden rounded-[8px] border border-[#dfe2e8] bg-[#ffffff] shadow-[0_10px_28px_rgba(20,24,32,0.06)] dark:border-[#292c34] dark:bg-[#181a20] dark:shadow-none">
      <div
        v-for="(control, index) in BOX_VISUAL_SETTING_CONTROLS"
        :key="control.key"
      >
        <div class="grid min-h-[84px] grid-cols-[1fr_300px] items-center gap-6 px-5">
          <div class="min-w-0 pr-4">
            <h2 class="text-[13px] font-semibold text-[#202229] dark:text-[#f4f4f5]">{{ control.label }}</h2>
            <p class="mt-1 text-[12px] leading-5 text-[#707684] dark:text-[#9ca0aa]">{{ control.description }}</p>
          </div>

          <div class="flex items-center gap-3">
            <input
              class="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-[#d8dbe3] accent-[#ff5c5c] dark:bg-[#333640] dark:accent-[#ff6b6b]"
              :max="control.max"
              :min="control.min"
              :step="control.step"
              type="range"
              :value="props.settings[control.key]"
              @input="emitBoxVisualSettingChange(control.key, $event)"
            />
            <span class="w-[70px] shrink-0 whitespace-nowrap text-right text-[13px] font-medium text-[#555b66] dark:text-[#c7cad1]">
              {{ props.settings[control.key] }} {{ control.unit }}
            </span>
            <button
              :aria-label="`重置${control.label}`"
              class="grid size-8 shrink-0 place-items-center rounded-[6px] text-[#68707d] transition-colors hover:bg-[#eef0f4] hover:text-[#17181c] disabled:cursor-default disabled:opacity-35 disabled:hover:bg-transparent disabled:hover:text-[#68707d] dark:text-[#a7abb5] dark:hover:bg-[#242730] dark:hover:text-[#f4f4f5] dark:disabled:hover:bg-transparent dark:disabled:hover:text-[#a7abb5]"
              :disabled="props.settings[control.key] === DEFAULT_APP_SETTINGS[control.key]"
              :title="`重置${control.label}`"
              type="button"
              @click="resetBoxVisualSetting(control.key)"
            >
              <RotateCcw :size="15" />
            </button>
          </div>
        </div>

        <div
          v-if="index < BOX_VISUAL_SETTING_CONTROLS.length - 1"
          class="h-px bg-[#e7e9ee] dark:bg-[#292c34]"
        />
      </div>
    </div>

  </section>
</template>
