<script setup lang="ts">
import { FolderPlus, Lock, LockOpen, RefreshCw, RotateCw, Trash2 } from "@lucide/vue";
import { useDesktopBoxDeleteConfirmation } from "@/entities/desktopBox/deleteConfirmation";
import type { DesktopBox } from "@/entities/desktopBox/types";

/**
 * Box 面板只做创建、同步和刷新入口，具体桌面行为由原生控制器处理
 */
defineProps<{
  boxes: DesktopBox[];
  boxItemCounts: Record<string, number>;
  /**
   * 设置页父级统一下发内容宽度，避免各面板各自维护页面密度
   */
  panelWidth: string;
  totalItems: number;
  unassignedItems: number;
}>();

const emit = defineEmits<{
  createBox: [];
  deleteBox: [box: DesktopBox];
  openBox: [box: DesktopBox];
  refresh: [];
  toggleBoxLocked: [box: DesktopBox];
}>();

const {
  clearBoxDeleteConfirmation,
  isConfirmingBoxDelete,
  requestBoxDeleteConfirmation,
} = useDesktopBoxDeleteConfirmation();

/**
 * 设置列表需要给空标题 Box 一个识别名称，真实 Box 标题仍保持用户保存的空文本
 */
function displayBoxTitle(box: DesktopBox): string {
  return box.title || "未命名 Box";
}

/**
 * 创建新 Box 属于非危险操作，执行前清掉任何悬挂的删除确认态
 */
function createBoxFromPanel(): void {
  clearBoxDeleteConfirmation();
  emit("createBox");
}

/**
 * 同步 Box 时恢复删除按钮普通态，避免用户返回设置页后还看到旧确认状态
 */
function syncBoxFromPanel(box: DesktopBox): void {
  clearBoxDeleteConfirmation();
  emit("openBox", box);
}

/**
 * 锁定切换和删除无关，点击后取消二次确认可以降低误删概率
 */
function toggleBoxLockedFromPanel(box: DesktopBox): void {
  clearBoxDeleteConfirmation();
  emit("toggleBoxLocked", box);
}

/**
 * 删除按钮采用二段式交互，第一次点击只切换按钮状态，第二次点击才向父级发出删除事件
 */
function deleteBoxFromPanel(box: DesktopBox): void {
  if (!requestBoxDeleteConfirmation(box)) {
    return;
  }

  emit("deleteBox", box);
}

/**
 * 刷新桌面文件前重置危险操作状态，避免刷新后列表变化但确认态仍指向旧 Box
 */
function refreshFromPanel(): void {
  clearBoxDeleteConfirmation();
  emit("refresh");
}
</script>

<template>
  <section class="mx-auto w-full px-8 py-7" :class="panelWidth">
    <div class="mb-6 flex items-end justify-between gap-6">
      <div class="min-w-0">
        <h1 class="text-[24px] font-semibold tracking-[0] text-[#17181c] dark:text-[#f4f4f5]">Box</h1>
        <p class="mt-1 text-[13px] text-[#6f7480] dark:text-[#a7abb5]">管理桌面上的原生 Box 面板</p>
      </div>

      <button
        class="inline-flex h-9 items-center gap-2 rounded-[9px] bg-[#ff5c5c] px-4 text-[13px] font-semibold text-white shadow-[0_10px_24px_rgba(255,92,92,0.24)] transition-colors hover:bg-[#ee4d4d]"
        type="button"
        @click="createBoxFromPanel"
      >
        <FolderPlus :size="16" />
        新增 Box
      </button>
    </div>

    <div class="mb-4 grid grid-cols-3 gap-3">
      <div class="rounded-[12px] border border-[#dfe2e8] bg-[#ffffff] px-4 py-3 dark:border-[#292c34] dark:bg-[#181a20]">
        <div class="text-[20px] font-semibold text-[#17181c] dark:text-[#f4f4f5]">{{ boxes.length }}</div>
        <div class="mt-1 text-[12px] text-[#707684] dark:text-[#9ca0aa]">Box</div>
      </div>
      <div class="rounded-[12px] border border-[#dfe2e8] bg-[#ffffff] px-4 py-3 dark:border-[#292c34] dark:bg-[#181a20]">
        <div class="text-[20px] font-semibold text-[#17181c] dark:text-[#f4f4f5]">{{ totalItems }}</div>
        <div class="mt-1 text-[12px] text-[#707684] dark:text-[#9ca0aa]">已收纳</div>
      </div>
      <div class="rounded-[12px] border border-[#dfe2e8] bg-[#ffffff] px-4 py-3 dark:border-[#292c34] dark:bg-[#181a20]">
        <div class="text-[20px] font-semibold text-[#17181c] dark:text-[#f4f4f5]">{{ unassignedItems }}</div>
        <div class="mt-1 text-[12px] text-[#707684] dark:text-[#9ca0aa]">桌面未分组</div>
      </div>
    </div>

    <div class="overflow-hidden rounded-[14px] border border-[#dfe2e8] bg-[#ffffff] shadow-[0_10px_28px_rgba(20,24,32,0.06)] dark:border-[#292c34] dark:bg-[#181a20] dark:shadow-none">
      <article
        v-for="box in boxes"
        :key="box.id"
        class="grid min-h-[64px] w-full grid-cols-[1fr_auto] items-center gap-5 border-b border-[#eceef3] px-5 text-left transition-colors last:border-b-0 hover:bg-[#f6f7fa] dark:border-[#292c34] dark:hover:bg-[#202229]"
      >
        <button
          class="grid min-w-0 py-3 text-left"
          type="button"
          @click="syncBoxFromPanel(box)"
        >
          <strong class="block truncate text-[13px] font-semibold text-[#202229] dark:text-[#f4f4f5]">{{ displayBoxTitle(box) }}</strong>
          <span class="mt-1 block text-[12px] text-[#707684] dark:text-[#9ca0aa]">
            {{ box.locked ? "已锁定" : "可移动" }} · {{ box.width }} × {{ box.height }} · {{ boxItemCounts[box.id] ?? 0 }} 个项目
          </span>
        </button>

        <div class="flex items-center gap-1">
          <button
            :aria-label="box.locked ? '解除锁定 Box' : '锁定 Box'"
            class="grid size-8 place-items-center rounded-[7px] text-[#68707d] transition-colors hover:bg-[#eef0f4] hover:text-[#17181c] dark:text-[#a7abb5] dark:hover:bg-[#242730] dark:hover:text-[#f4f4f5]"
            :title="box.locked ? '解除锁定 Box' : '锁定 Box'"
            type="button"
            @click="toggleBoxLockedFromPanel(box)"
          >
            <LockOpen v-if="box.locked" :size="15" />
            <Lock v-else :size="15" />
          </button>
          <button
            aria-label="同步 Box 到桌面"
            class="grid size-8 place-items-center rounded-[7px] text-[#68707d] transition-colors hover:bg-[#eef0f4] hover:text-[#17181c] dark:text-[#a7abb5] dark:hover:bg-[#242730] dark:hover:text-[#f4f4f5]"
            title="同步 Box 到桌面"
            type="button"
            @click="syncBoxFromPanel(box)"
          >
            <RotateCw :size="15" />
          </button>
          <span class="flex h-8 w-[78px] justify-end">
            <button
              :aria-label="isConfirmingBoxDelete(box) ? '确认删除 Box' : '删除 Box'"
              class="inline-flex h-8 items-center justify-center rounded-[7px] transition-colors"
              :class="
                isConfirmingBoxDelete(box)
                  ? 'w-[78px] gap-1.5 bg-red-600 px-2 text-[12px] font-medium text-white hover:bg-red-700 dark:bg-red-500 dark:hover:bg-red-600'
                  : 'w-8 text-red-500 hover:bg-[#fff0f0] hover:text-red-600 dark:text-red-400 dark:hover:bg-[#3a2528]'
              "
              :title="isConfirmingBoxDelete(box) ? '再次点击确认删除，真实文件不会被删除' : '删除 Box'"
              type="button"
              @click="deleteBoxFromPanel(box)"
            >
              <Trash2 :size="15" />
              <span v-if="isConfirmingBoxDelete(box)">确认</span>
            </button>
          </span>
        </div>
      </article>

      <div v-if="boxes.length === 0" class="px-5 py-10 text-center">
        <p class="text-[13px] font-semibold text-[#202229] dark:text-[#f4f4f5]">还没有 Box</p>
        <p class="mt-1 text-[12px] text-[#707684] dark:text-[#9ca0aa]">点击右上角创建第一个桌面 Box</p>
      </div>
    </div>

    <button
      class="mt-4 inline-flex h-9 items-center gap-2 rounded-[9px] border border-[#dfe2e8] bg-[#ffffff] px-4 text-[13px] font-medium text-[#555b66] transition-colors hover:bg-[#f6f7fa] hover:text-[#17181c] dark:border-[#292c34] dark:bg-[#181a20] dark:text-[#a7abb5] dark:hover:bg-[#202229] dark:hover:text-[#f4f4f5]"
      type="button"
      @click="refreshFromPanel"
    >
      <RefreshCw :size="15" />
      刷新桌面文件
    </button>
  </section>
</template>
