<script setup lang="ts">
import DialogShell from "@shared/ui/DialogShell.vue";
import type { TimelineItem } from "../timeline-group";
import SubagentFlow from "./SubagentFlow.vue";

// family 聚合型子代理（Codex/Claude Code/OpenCode）的过程弹窗：
// 入口行（SubagentGroupRow）与顶部 agent tab 复用同一实例，
// 内容由 SessionDetail 按需从后端取该 agent 的完整消息。
type SubagentGroupDialogProps = {
  open: boolean;
  label: string;
  loading: boolean;
  error: string | null;
  items: TimelineItem[];
};

defineProps<SubagentGroupDialogProps>();

const emit = defineEmits<{ close: [] }>();

const titleId = "subagent-group-dialog-title";
</script>

<template>
  <DialogShell
    :open="open"
    :title="`子代理 · ${label}`"
    :title-id="titleId"
    dialog-class-name="subagent-run-dialog"
    @close="emit('close')"
  >
    <p v-if="loading" class="muted-text">正在加载子代理内容...</p>
    <p v-else-if="error" class="error-text">{{ error }}</p>
    <template v-else>
      <p class="muted-text subagent-group-dialog-meta">{{ items.length }} 条消息</p>
      <SubagentFlow :items="items" />
      <p v-if="items.length === 0" class="muted-text">该子代理暂无内容。</p>
    </template>
  </DialogShell>
</template>
