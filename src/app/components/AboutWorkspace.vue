<script setup lang="ts">
import { onMounted, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { AGENT_IDS, AGENT_LABELS } from "@shared/lib/agent-labels";
import { extractErrorMessage } from "@shared/lib/errors";
import ConfirmDialog from "@shared/ui/ConfirmDialog.vue";
import { clearSessionCaches } from "../../features/sessions/api";
import "./about-workspace.css";

// 关于页是左栏的顶层页(与配置与分发/模型配置/历史会话平级),页面内不放返回:
// 换页由左栏承担,和用量统计那种"从会话列表钻进去的子页"不同。
// 版本只有一个来源:tauri.conf.json(productName/version),不在这里另存常量。
const version = ref<string | null>(null);

onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    // 取不到版本(非 Tauri 环境)时留占位:关于页不该因为这个调用失败而打不开。
  }
});

// 工具名取自共享常量表,不在这里另写一份拼写。
const toolNames = AGENT_IDS.map((id) => AGENT_LABELS[id] ?? id).join(" / ");

// 强制清空会话缓存:连持久层(目录行、摘要、用量)一起清,是怀疑缓存本身
// 时的逃生通道。代价是下次加载全量重建,大库要等一会——列表与用量页的
// 加载耗时指示正好能看到进度。
const clearDialogOpen = ref(false);
const clearing = ref(false);
const clearError = ref<string | null>(null);
const clearedNotice = ref(false);

function openClearDialog() {
  clearError.value = null;
  clearDialogOpen.value = true;
}

function closeClearDialog() {
  if (clearing.value) {
    return;
  }
  clearDialogOpen.value = false;
  clearError.value = null;
}

async function clearCaches() {
  clearing.value = true;
  clearError.value = null;
  try {
    await clearSessionCaches();
    clearDialogOpen.value = false;
    clearedNotice.value = true;
  } catch (caught) {
    // 失败留在对话框内,用户可就地重试或取消。
    clearError.value = extractErrorMessage(caught, "清空缓存失败");
  } finally {
    clearing.value = false;
  }
}
</script>

<template>
  <div class="about-workspace">
    <header class="about-header">
      <h2 class="about-header__title">关于 Reins</h2>
    </header>

    <div class="about-body">
      <p class="about-body__version">版本 {{ version ?? "未知" }}</p>
      <p class="about-body__desc">
        本地桌面工具：浏览各 AI 编码工具的历史会话、把 Skills 与 MCP 配置分发到多个
        agent target、集中管理聚合提供商的模型配置并一键应用。
      </p>
      <p class="about-body__tools">{{ toolNames }}</p>

      <section class="about-cache">
        <p class="about-cache__hint">
          会话列表与用量统计依赖本地缓存加速。怀疑缓存结果不对时可以强制清空:
          下次加载会全量重建,转录多的话需要等待一段时间。
        </p>
        <button
          class="secondary-button"
          type="button"
          @click="openClearDialog"
        >
          强制清空缓存
        </button>
        <p v-if="clearedNotice" class="about-cache__done">
          缓存已清空,下次加载会话时将全量重建。
        </p>
      </section>
    </div>

    <ConfirmDialog
      :confirm-label="clearing ? '清空中...' : '确认清空'"
      :error="clearError"
      :loading="clearing"
      :open="clearDialogOpen"
      cancel-label="取消"
      description="会清空各来源的目录、摘要与用量缓存,原始会话文件不受影响;下次加载将全量重建,耗时随会话总量增长。"
      eyebrow="缓存清空"
      title="强制清空会话缓存？"
      title-id="about-clear-cache-dialog-title"
      @close="closeClearDialog"
      @confirm="clearCaches"
    />
  </div>
</template>
